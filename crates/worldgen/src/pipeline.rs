//! Assemblage du pipeline : chaque couche s'appuie sur les précédentes
//! (BRIEF §2.2). `WorldGen` détient toutes les couches et expose les
//! requêtes point à point.

use cairn_core::WorldSeed;

use crate::altitude::{AltitudeConfig, AltitudeField};
use crate::biomes::Biome;
use crate::geology::{Deposit, Geology, GeologyConfig, RockType};
use crate::humidity::{self, HumidityConfig};
use crate::temperature::{TemperatureConfig, TemperatureField};
use crate::wind::{WindConfig, WindField};

#[derive(Default)]
pub struct WorldGenConfig {
    pub altitude: AltitudeConfig,
    pub temperature: TemperatureConfig,
    pub wind: WindConfig,
    pub humidity: HumidityConfig,
    pub geology: GeologyConfig,
}

pub struct WorldGen {
    pub altitude: AltitudeField,
    pub temperature: TemperatureField,
    pub wind: WindField,
    pub geology: Geology,
    humidity_cfg: HumidityConfig,
}

impl WorldGen {
    pub fn new(seed: WorldSeed) -> Self {
        Self::with_config(seed, WorldGenConfig::default())
    }

    pub fn with_config(seed: WorldSeed, cfg: WorldGenConfig) -> Self {
        Self {
            altitude: AltitudeField::with_config(seed, cfg.altitude),
            temperature: TemperatureField::with_config(seed, cfg.temperature),
            wind: WindField::new(cfg.wind),
            geology: Geology::with_config(seed, cfg.geology),
            humidity_cfg: cfg.humidity,
        }
    }

    /// Élévation normalisée : [-1, 0) océan, (0, 1] terres.
    pub fn elevation(&self, x: i64, y: i64) -> f64 {
        self.altitude.elevation(x, y)
    }

    /// Température moyenne annuelle en °C. `elevation` est fournie par
    /// l'appelant pour ne pas la recalculer.
    pub fn mean_temperature(&self, x: i64, y: i64, elevation: f64) -> f64 {
        self.temperature.mean_temperature(x, y, elevation)
    }

    /// Humidité de l'air en (x, y), dans [0, 1]. Coûteux (remontée au
    /// vent) : voir [`crate::humidity`].
    pub fn humidity(&self, x: i64, y: i64) -> f64 {
        humidity::humidity(&self.altitude, &self.wind, &self.humidity_cfg, x, y)
    }

    /// Biome en (x, y) — évalue tout le pipeline. Si l'appelant a déjà
    /// élévation, température et humidité, utiliser [`Biome::classify`].
    pub fn biome(&self, x: i64, y: i64) -> Biome {
        let e = self.elevation(x, y);
        let t = self.mean_temperature(x, y, e);
        let h = self.humidity(x, y);
        Biome::classify(e, t, h)
    }

    /// Type de roche en (x, y).
    pub fn rock_type(&self, x: i64, y: i64) -> RockType {
        self.geology.rock_type(x, y)
    }

    /// Gisement en (x, y). `elevation` est fournie par l'appelant.
    pub fn deposit(&self, x: i64, y: i64, elevation: f64) -> Deposit {
        self.geology.deposit(x, y, elevation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humidite_deterministe_et_bornee() {
        let a = WorldGen::new(WorldSeed(42));
        let b = WorldGen::new(WorldSeed(42));
        for i in 0..50i64 {
            let (x, y) = (i * 331, i * 173 - 4000);
            let h = a.humidity(x, y);
            assert!((0.0..=1.0).contains(&h));
            assert_eq!(h.to_bits(), b.humidity(x, y).to_bits());
        }
    }

    #[test]
    fn l_ocean_est_plus_humide_que_l_interieur_des_terres() {
        let world = WorldGen::new(WorldSeed(42));
        let (mut ocean, mut inland) = (Vec::new(), Vec::new());
        // Balayage à l'échelle continentale : le pas de ~40 km sur 60×60
        // points couvre ~2400 km, assez pour que l'intérieur profond dépasse
        // la portée d'advection (~160 km) et soit réellement sec.
        let step = cairn_core::scale::km_to_tiles(40.0) as i64;
        let half = 30 * step;
        for i in 0..60i64 {
            for j in 0..60i64 {
                let (x, y) = (i * step - half, j * step - half);
                let e = world.elevation(x, y);
                let h = world.humidity(x, y);
                if e <= 0.0 {
                    ocean.push(h);
                } else if e > 0.15 {
                    inland.push(h);
                }
            }
        }
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        let (m_ocean, m_inland) = (mean(&ocean), mean(&inland));
        assert!(
            m_ocean > m_inland + 0.1,
            "océan {m_ocean:.2}, terres hautes {m_inland:.2}"
        );
    }
}
