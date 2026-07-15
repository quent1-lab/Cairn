//! Génération procédurale du monde : altitude, température, vent, humidité,
//! puis (à venir) hydrologie, biomes, géologie — chaque couche s'appuyant
//! sur les précédentes (BRIEF §2.2). [`WorldGen`] assemble le pipeline.

pub mod altitude;
pub mod biomes;
pub mod fbm;
pub mod humidity;
pub mod latitude;
pub mod pipeline;
pub mod temperature;
pub mod wind;

pub use altitude::{AltitudeConfig, AltitudeField};
pub use biomes::Biome;
pub use fbm::Fbm;
pub use humidity::HumidityConfig;
pub use pipeline::{WorldGen, WorldGenConfig};
pub use temperature::{TemperatureConfig, TemperatureField};
pub use wind::{WindConfig, WindField};

/// Salts des couches de bruit : chaque couche dérive sa seed de la seed du
/// monde via `WorldSeed::derive(salt)`. Centralisés ici pour garantir leur
/// unicité — deux couches partageant un salt seraient corrélées.
pub(crate) mod salt {
    pub const CONTINENTS: u64 = 1;
    pub const RELIEF: u64 = 2;
    pub const TEMPERATURE: u64 = 3;
}
