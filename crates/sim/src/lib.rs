//! La simulation : le monde chunké mutable, et depuis la Phase 2, **la vie**
//! qui l'habite.
//!
//! Côté espace : un store de tuiles chunké (BRIEF §2.1), pont entre le
//! `worldgen` (baseline pur, immuable, fonction de la seed) et l'état
//! simulé — biomasse consommée, fertilité dégradée, et à venir structures,
//! revendications. Un chunk non modifié se décharge sans regret (il est
//! régénérable) ; un chunk modifié est retenu.
//!
//! Côté vie : des agents à physiologie complète (faim, soif, fatigue, froid,
//! santé, mort), une utility AI minimale (manger, boire, dormir, s'abriter,
//! errer), une écologie à croissance logistique, un climat instantané
//! (saisons, jour/nuit) — le tout dans une boucle à tick fixe déterministe
//! ([`Sim`]).

pub mod agent;
pub mod brain;
pub mod chunk;
pub mod climate;
pub mod curves;
pub mod ecology;
pub mod fauna;
pub mod scenario;
pub mod sim;
pub mod tile;
pub mod world;

pub use agent::{Activity, AgentId, Behavior, DeathCause, Physiology, Position, Task, TaskKind};
pub use chunk::{CHUNK_AREA, CHUNK_SIZE, Chunk, ChunkCoord};
pub use climate::Climate;
pub use fauna::{FaunaId, Herd, HerdState, Pack};
pub use sim::{DeathRecord, Sim};
pub use tile::{Tile, TileFlags};
pub use world::World;

/// Salts des flux aléatoires de la simulation, dérivés de la seed du monde
/// via `WorldSeed::derive`. Plage 1000+ : les salts 1–14 appartiennent au
/// worldgen (voir `cairn-worldgen/src/lib.rs`) et ne doivent jamais être
/// réutilisés ici — deux consommateurs partageant un salt seraient corrélés.
pub(crate) mod salt {
    /// Sources d'eau douce (proxy d'hydrologie, voir `chunk::is_spring`).
    pub const SPRINGS: u64 = 1000;
    /// Décisions des agents (softmax de l'utility AI).
    pub const DECISIONS: u64 = 1001;
    /// Repousse de la biomasse (arrondi stochastique).
    pub const ECOLOGY: u64 = 1002;
    /// Errance : direction des déplacements exploratoires.
    pub const WANDER: u64 = 1003;
    /// Faune : dispersion des migrations et des scissions de troupeaux.
    pub const FAUNA: u64 = 1004;
}
