//! Couche 4 du pipeline : l'humidité, par advection le long du vent.
//!
//! Pour connaître l'humidité en un point, on remonte le vent sur quelques
//! milliers de tuiles (d'où vient l'air ?), puis on rejoue le trajet dans le
//! sens du vent en transportant une masse d'humidité :
//!
//! - au-dessus de l'**océan**, l'air se recharge (évaporation, saturante) ;
//! - au-dessus des **terres**, il se décharge lentement (pluie de base) ;
//! - à chaque **montée de relief**, il se décharge brutalement (pluie
//!   orographique) — l'air qui franchit une chaîne arrive sec de l'autre
//!   côté : c'est l'**ombre pluviométrique**, et c'est elle qui place les
//!   déserts derrière les montagnes.
//!
//! Aucun bruit dédié : l'humidité est entièrement dérivée des couches
//! altitude et vent. Coût : `steps` évaluations d'altitude par requête —
//! cher ; sera calculé sur macro-grille et mis en cache au chunking.

use std::cell::RefCell;

use cairn_core::scale::km_to_tiles;

use crate::altitude::AltitudeField;
use crate::wind::WindField;

thread_local! {
    // Tampon de travail de l'advection, réutilisé d'un appel à l'autre pour
    // éviter une allocation tas par échantillon (le rendu du client en
    // produirait des dizaines de milliers par image). `thread_local!` donne un
    // tampon propre à chaque thread — donc pas de partage à synchroniser — et
    // son contenu est intégralement réécrit à chaque appel, sans effet sur le
    // déterminisme. Le `const { }` rend l'initialiseur constant (idiome depuis
    // Rust 1.59, plus léger qu'un `lazy` à la première lecture).
    static PATH_ELEVATIONS: RefCell<Vec<f64>> = const { RefCell::new(Vec::new()) };
}

pub struct HumidityConfig {
    /// Nombre de pas de la remontée au vent.
    pub steps: usize,
    /// Longueur d'un pas en tuiles (portée totale = steps × step_tiles × |vent|).
    pub step_tiles: f64,
    /// Recharge par pas au-dessus de l'océan (fraction du déficit).
    pub ocean_evaporation: f64,
    /// Recharge par pas au-dessus des terres (évapotranspiration).
    pub land_evaporation: f64,
    /// Décharge de base par pas au-dessus des terres.
    pub base_rainout: f64,
    /// Décharge supplémentaire par unité d'élévation gravie en un pas.
    pub orographic_rainout: f64,
    /// Montée minimale (par pas) pour déclencher la pluie orographique :
    /// filtre les ondulations du bruit de relief, sinon chaque bosse
    /// essore l'air et l'ombre pluviométrique se noie dans une aridité
    /// générale.
    pub orographic_threshold: f64,
    /// Diffusion latérale : nombre de trajets parallèles de chaque côté,
    /// décalés perpendiculairement au vent, dont on moyenne l'humidité.
    /// 0 = trajet unique (calcul brut). Lisse les stries d'advection sans
    /// écraser le signal directionnel (le lissage est perpendiculaire au
    /// vent, jamais le long). Coût ×(1 + 2·lateral_samples).
    pub lateral_samples: usize,
    /// Écart entre trajets parallèles, en tuiles.
    pub lateral_step: f64,
}

impl Default for HumidityConfig {
    fn default() -> Self {
        Self {
            // Portée = steps × step_tiles ≈ 256 km : distance de pénétration
            // de l'humidité océanique dans les terres (intérieurs profonds
            // secs). Le pas de 4 km reste assez fin pour résoudre les
            // barrières montagneuses (larges de dizaines de km) — l'ombre
            // pluviométrique en dépend.
            steps: 64,
            step_tiles: km_to_tiles(4.0),
            ocean_evaporation: 0.15,
            land_evaporation: 0.015,
            base_rainout: 0.01,
            orographic_rainout: 2.5,
            orographic_threshold: 0.01,
            // Diffusion latérale : 5 trajets sur ±8 km. L'écart doit être à
            // l'échelle des features de terrain (relief ~150 km) pour lisser
            // les stries d'advection ; un écart sous-pixel ne fait rien.
            // La vraie diffusion sur grille (moins chère) viendra avec le
            // précalcul macro-grille du chunking.
            lateral_samples: 2,
            lateral_step: km_to_tiles(4.0),
        }
    }
}

/// Humidité de l'air arrivant en (x, y), dans [0, 1].
///
/// Moyenne de plusieurs advections parallèles décalées perpendiculairement
/// au vent local (voir [`HumidityConfig::lateral_samples`]) : c'est cette
/// diffusion transverse qui gomme les stries que produirait un trajet
/// unique, sans mélanger les bandes climatiques le long du vent.
pub fn humidity(
    altitude: &AltitudeField,
    wind: &WindField,
    cfg: &HumidityConfig,
    x: i64,
    y: i64,
) -> f64 {
    if cfg.lateral_samples == 0 {
        return advect(altitude, wind, cfg, x as f64, y as f64);
    }
    // Perpendiculaire au vent local, normalisée. En zone de calme (vent nul)
    // les décalages s'effondrent sur un point : pas de lissage, cohérent.
    let (vx, vy) = wind.wind(x, y);
    let mag = (vx * vx + vy * vy).sqrt();
    let (perp_x, perp_y) = if mag > 1e-9 {
        (-vy / mag, vx / mag)
    } else {
        (0.0, 0.0)
    };
    let k = cfg.lateral_samples as i64;
    let mut sum = 0.0;
    let mut count = 0.0;
    for j in -k..=k {
        let offset = j as f64 * cfg.lateral_step;
        let sx = x as f64 + perp_x * offset;
        let sy = y as f64 + perp_y * offset;
        sum += advect(altitude, wind, cfg, sx, sy);
        count += 1.0;
    }
    sum / count
}

/// Une advection unique : remontée au vent puis transport de l'humidité.
fn advect(altitude: &AltitudeField, wind: &WindField, cfg: &HumidityConfig, x: f64, y: f64) -> f64 {
    PATH_ELEVATIONS.with(|scratch| {
        let elevations = &mut *scratch.borrow_mut();
        elevations.clear();

        // 1. Remontée au vent : on retrace le chemin qu'a suivi l'air pour
        // arriver ici, en relevant l'altitude à chaque pas. Le déplacement est
        // proportionnel au vent : dans les zones de calme, l'air ne vient pas
        // de loin — les terres y restent sèches. On mémorise les altitudes
        // (et non les positions) : c'est tout ce dont le rejeu a besoin, et le
        // tampon reste réutilisable tel quel.
        let (mut px, mut py) = (x, y);
        elevations.push(altitude.elevation(px.round() as i64, py.round() as i64));
        for _ in 0..cfg.steps {
            let (vx, vy) = wind.wind(px.round() as i64, py.round() as i64);
            px -= vx * cfg.step_tiles;
            py -= vy * cfg.step_tiles;
            elevations.push(altitude.elevation(px.round() as i64, py.round() as i64));
        }

        // 2. Rejeu du trajet dans le sens du vent (des altitudes les plus en
        // amont vers le point courant), humidité transportée.
        let mut moisture: f64 = 0.0;
        let mut prev_elevation: f64 = 0.0;
        for &e in elevations.iter().rev() {
            if e <= 0.0 {
                moisture += cfg.ocean_evaporation * (1.0 - moisture);
            } else {
                moisture += cfg.land_evaporation * (1.0 - moisture);
                let uplift = (e - prev_elevation.max(0.0) - cfg.orographic_threshold).max(0.0);
                let rainout = (cfg.base_rainout + cfg.orographic_rainout * uplift).min(1.0);
                moisture -= moisture * rainout;
            }
            prev_elevation = e;
        }
        moisture.clamp(0.0, 1.0)
    })
}
