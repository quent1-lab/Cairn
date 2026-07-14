//! Génération procédurale du monde : altitude, température, puis (à venir)
//! vent, humidité, hydrologie, biomes, géologie — chaque couche s'appuyant
//! sur les précédentes (BRIEF §2.2).

pub mod altitude;
pub mod fbm;
pub mod temperature;

pub use altitude::{AltitudeConfig, AltitudeField};
pub use fbm::Fbm;
pub use temperature::{TemperatureConfig, TemperatureField};

/// Salts des couches de bruit : chaque couche dérive sa seed de la seed du
/// monde via `WorldSeed::derive(salt)`. Centralisés ici pour garantir leur
/// unicité — deux couches partageant un salt seraient corrélées.
pub(crate) mod salt {
    pub const CONTINENTS: u64 = 1;
    pub const RELIEF: u64 = 2;
    pub const TEMPERATURE: u64 = 3;
}
