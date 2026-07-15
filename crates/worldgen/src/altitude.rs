//! Couche 1 du pipeline : l'altitude.
//!
//! Deux champs de bruit indépendants se combinent :
//! - la **continentalité** (très basse fréquence) décide océan ou terre ;
//! - le **relief** (fréquence moyenne, 6 octaves) sculpte le détail, mais ne
//!   s'exprime pleinement qu'à l'intérieur des terres — les côtes restent
//!   des plaines, les chaînes de montagnes sont continentales.

use cairn_core::WorldSeed;
use cairn_core::scale::km_to_tiles;

use crate::fbm::Fbm;
use crate::salt;

/// Paramètres de forme du terrain, regroupés pour pouvoir itérer visuellement
/// sans chercher des constantes éparpillées.
pub struct AltitudeConfig {
    pub continent_octaves: usize,
    /// Fréquence de la continentalité : ~1/taille d'un continent en tuiles.
    pub continent_frequency: f64,
    pub relief_octaves: usize,
    pub relief_frequency: f64,
    /// Décalage du niveau de la mer : plus il est haut, plus l'océan couvre.
    pub sea_bias: f64,
    /// Amplitude du relief partout (fonds marins, plaines côtières).
    pub relief_base: f64,
    /// Amplitude supplémentaire du relief à l'intérieur des terres.
    pub relief_mountain: f64,
}

impl Default for AltitudeConfig {
    fn default() -> Self {
        Self {
            // Continentalité : ~3000 km de longueur d'onde. 3 octaves + le
            // sea_bias donnent la connexité mesurée (example analyze) : un
            // continent principal par seed (médiane 24 % des terres, pire cas
            // 12 %), sans basculer vers la Pangée.
            continent_octaves: 3,
            continent_frequency: 1.0 / km_to_tiles(3000.0),
            // Relief : de ~150 km (massifs) à ~300 m (détail visible au zoom
            // village). 10 octaves pour couvrir tout ce spectre — c'est ce
            // qui remplit le vide entre continents et sol local maintenant
            // qu'on affiche jusqu'à l'échelle humaine.
            relief_octaves: 10,
            relief_frequency: 1.0 / km_to_tiles(150.0),
            sea_bias: 0.1,
            relief_base: 0.1,
            relief_mountain: 0.5,
        }
    }
}

pub struct AltitudeField {
    continents: Fbm,
    relief: Fbm,
    cfg: AltitudeConfig,
}

impl AltitudeField {
    pub fn new(seed: WorldSeed) -> Self {
        Self::with_config(seed, AltitudeConfig::default())
    }

    pub fn with_config(seed: WorldSeed, cfg: AltitudeConfig) -> Self {
        Self {
            continents: Fbm::new(
                seed.derive(salt::CONTINENTS),
                cfg.continent_octaves,
                cfg.continent_frequency,
            ),
            relief: Fbm::new(
                seed.derive(salt::RELIEF),
                cfg.relief_octaves,
                cfg.relief_frequency,
            ),
            cfg,
        }
    }

    /// Altitude normalisée en (x, y) : [-1, 0) océan, 0 niveau de la mer,
    /// (0, 1] terres émergées.
    pub fn elevation(&self, x: i64, y: i64) -> f64 {
        let (xf, yf) = (x as f64, y as f64);
        let c = self.continents.get(xf, yf) - self.cfg.sea_bias;
        let r = self.relief.get(xf, yf);
        let mountains = smoothstep(0.0, 0.4, c);
        let e = c + r * (self.cfg.relief_base + self.cfg.relief_mountain * mountains);
        e.clamp(-1.0, 1.0)
    }
}

/// Interpolation lisse : 0 avant `edge0`, 1 après `edge1`, cubique entre.
fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meme_seed_meme_monde_bit_a_bit() {
        let a = AltitudeField::new(WorldSeed(42));
        let b = AltitudeField::new(WorldSeed(42));
        for &(x, y) in &[(0, 0), (1_000, -500), (123_456, -654_321), (-1 << 40, 1 << 40)] {
            // Comparaison des représentations binaires : l'égalité flottante
            // exacte est précisément ce que le déterminisme exige.
            assert_eq!(a.elevation(x, y).to_bits(), b.elevation(x, y).to_bits());
        }
    }

    #[test]
    fn seeds_differentes_mondes_differents() {
        let a = AltitudeField::new(WorldSeed(1));
        let b = AltitudeField::new(WorldSeed(2));
        let differe = (0..100).any(|i| a.elevation(i * 37, i * 91) != b.elevation(i * 37, i * 91));
        assert!(differe);
    }

    #[test]
    fn le_monde_contient_terre_et_mer() {
        // Pas de ~60 km : 100 échantillons couvrent ~6000 km, soit plusieurs
        // continents — sinon la fenêtre tiendrait dans une seule masse et le
        // ratio serait 0 % ou 100 %.
        let field = AltitudeField::new(WorldSeed(42));
        let step = km_to_tiles(60.0) as i64;
        let echantillons: Vec<f64> = (0..10_000)
            .map(|i| field.elevation((i % 100) * step, (i / 100) * step))
            .collect();
        let terre = echantillons.iter().filter(|&&e| e > 0.0).count();
        let ratio = terre as f64 / echantillons.len() as f64;
        assert!(
            (0.15..0.60).contains(&ratio),
            "ratio de terres émergées hors limites : {ratio}"
        );
    }
}
