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

use crate::altitude::AltitudeField;
use crate::wind::WindField;

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
}

impl Default for HumidityConfig {
    fn default() -> Self {
        Self {
            steps: 64,
            step_tiles: 32.0,
            ocean_evaporation: 0.15,
            land_evaporation: 0.015,
            base_rainout: 0.01,
            orographic_rainout: 2.5,
            orographic_threshold: 0.01,
        }
    }
}

/// Humidité de l'air arrivant en (x, y), dans [0, 1].
pub fn humidity(
    altitude: &AltitudeField,
    wind: &WindField,
    cfg: &HumidityConfig,
    x: i64,
    y: i64,
) -> f64 {
    // 1. Remontée au vent : le chemin qu'a suivi l'air pour arriver ici.
    // Le déplacement est proportionnel au vent : dans les zones de calme,
    // l'air ne vient pas de loin — les terres y restent sèches.
    let mut path = Vec::with_capacity(cfg.steps + 1);
    let (mut px, mut py) = (x as f64, y as f64);
    path.push((px, py));
    for _ in 0..cfg.steps {
        let (vx, vy) = wind.wind(px.round() as i64, py.round() as i64);
        px -= vx * cfg.step_tiles;
        py -= vy * cfg.step_tiles;
        path.push((px, py));
    }

    // 2. Rejeu du trajet dans le sens du vent, humidité transportée.
    let mut moisture: f64 = 0.0;
    let mut prev_elevation: f64 = 0.0;
    for &(sx, sy) in path.iter().rev() {
        let e = altitude.elevation(sx.round() as i64, sy.round() as i64);
        if e <= 0.0 {
            moisture += cfg.ocean_evaporation * (1.0 - moisture);
        } else {
            moisture += cfg.land_evaporation * (1.0 - moisture);
            let uplift =
                (e - prev_elevation.max(0.0) - cfg.orographic_threshold).max(0.0);
            let rainout = (cfg.base_rainout + cfg.orographic_rainout * uplift).min(1.0);
            moisture -= moisture * rainout;
        }
        prev_elevation = e;
    }
    moisture.clamp(0.0, 1.0)
}
