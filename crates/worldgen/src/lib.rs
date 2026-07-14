//! Génération procédurale du monde : altitude, puis (à venir) température,
//! vent, humidité, hydrologie, biomes, géologie — chaque couche s'appuyant
//! sur les précédentes (BRIEF §2.2).

pub mod altitude;
pub mod fbm;

pub use altitude::{AltitudeConfig, AltitudeField};
pub use fbm::Fbm;
