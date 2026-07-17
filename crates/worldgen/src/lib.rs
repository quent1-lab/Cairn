//! Génération procédurale du monde : altitude, température, vent, humidité,
//! puis (à venir) hydrologie, biomes, géologie — chaque couche s'appuyant
//! sur les précédentes (BRIEF §2.2). [`WorldGen`] assemble le pipeline.

// Le `thread_local!` d'humidité utilise déjà `const { }`, mais ce lint
// (clippy 1.97) se déclenche à tort sur la macro elle-même ; un `#[allow]`
// sur l'item ne couvre pas l'expansion, d'où l'allow au niveau du crate —
// même décision que dans `cairn-client`.
#![allow(clippy::missing_const_for_thread_local)]

pub mod altitude;
pub mod biomes;
pub mod fbm;
pub mod geology;
pub mod humidity;
pub mod hydrology;
pub mod latitude;
pub mod pipeline;
pub mod temperature;
pub mod wind;

pub use altitude::{AltitudeConfig, AltitudeField};
pub use biomes::Biome;
pub use fbm::Fbm;
pub use geology::{Deposit, Geology, GeologyConfig, RockType};
pub use humidity::HumidityConfig;
pub use hydrology::{FREEZE_FLOWING_C, FREEZE_STILL_C, Hydrology, HydrologyConfig, Region, Water};
pub use pipeline::{WorldGen, WorldGenConfig};
pub use temperature::{TemperatureConfig, TemperatureField};
pub use wind::{WindConfig, WindField};

/// Période de latitude par défaut (équateur → pôle → équateur), en tuiles.
/// **Partagée entre température et vent** : les deux couches dérivent de la
/// même latitude et doivent donc utiliser exactement cette valeur — d'où une
/// constante unique plutôt que deux champs de config indépendants.
/// 8000 km, soit une bande climatique (équateur → pôle) de 4000 km.
pub const DEFAULT_PLANET_PERIOD: f64 = cairn_core::scale::km_to_tiles(8000.0);

/// Salts des couches de bruit : chaque couche dérive sa seed de la seed du
/// monde via `WorldSeed::derive(salt)`. Centralisés ici pour garantir leur
/// unicité — deux couches partageant un salt seraient corrélées.
pub(crate) mod salt {
    pub const CONTINENTS: u64 = 1;
    pub const RELIEF: u64 = 2;
    pub const TEMPERATURE: u64 = 3;
}
