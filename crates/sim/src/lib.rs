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
//!
//! Et depuis la Phase 3, **le nombre** : la population se reproduit
//! ([`demography`]) — traits hérités, gestation, enfance portée puis suivie,
//! sénescence — et c'est la démographie, pas le spawn, qui fait durer une
//! peuplade.

pub mod agent;
pub mod brain;
pub mod chronicle;
pub mod chunk;
pub mod combat;
pub mod commerce;
pub mod climate;
pub mod curves;
pub mod demography;
pub mod ecology;
pub mod exposure;
pub mod fauna;
pub mod fire;
pub mod memory;
pub mod names;
pub mod pastoral;
pub mod pathfind;
pub mod pressure;
pub mod scenario;
pub mod sim;
pub mod skills;
pub mod social;
pub mod structures;
pub mod tech;
pub mod tile;
pub mod world;

pub use agent::{Activity, AgentId, Behavior, DeathCause, Physiology, Position, Task, TaskKind, Wound};
pub use brain::Motivation;
pub use chronicle::{Event, EventKind};
pub use climate::Season;
pub use combat::{Clash, Engagement};
pub use chunk::{CHUNK_AREA, CHUNK_SIZE, Chunk, ChunkCoord};
pub use commerce::Expedition;
pub use climate::Climate;
pub use demography::{Demographics, HumanView, Kinship, Pregnancy, Sex, Traits};
pub use exposure::{Exposure, Exposures};
pub use fire::Fire;
pub use fauna::{FaunaId, Herd, HerdState, Pack, PackView, Species};
pub use memory::Memory;
pub use pressure::ClanPressure;
pub use sim::{BirthRecord, DeathRecord, Sim};
pub use skills::Skills;
pub use social::{Clan, ClanDiagnostics, ClanEvent, ClanEventKind, ClanId, ClanMembership, ClanRelations, SocialGraph};
pub use structures::{Structure, StructureKind};
pub use tech::{
    Age, EnvCond, Knowledge, Tech, TechEvent, TechEventKind, TechId, TechPressure, TechSkill,
    TechTree,
};
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
    /// Population initiale : sexe, âge et traits des fondateurs.
    pub const DEMOGRAPHY: u64 = 1005;
    /// Tirages quotidiens de conception.
    pub const CONCEPTION: u64 = 1006;
    /// Hérédité : sexe de l'enfant et mutations des traits à la naissance.
    pub const HEREDITY: u64 = 1007;
    /// Sénescence : tirages quotidiens de mortalité par âge.
    pub const SENESCENCE: u64 = 1008;
    /// Exploration : choix de l'azimut vers l'inconnu.
    pub const EXPLORE: u64 = 1009;
    /// Immigration de gibier : tirage du site candidat quotidien.
    pub const IMMIGRATION: u64 = 1010;
    /// Immigration de prédateurs : réensemencement des meutes près du gibier.
    pub const PREDATOR_IMMIGRATION: u64 = 1011;
    /// Insight : tirage quotidien de découverte technologique (Phase 5).
    pub const INSIGHT: u64 = 1012;
    /// Diffusion : tirage de transmission d'une tech entre agents proches.
    pub const DIFFUSION: u64 = 1013;
    /// Feux de forêt : tirage quotidien d'un départ de feu et de son site.
    pub const FIRE: u64 = 1014;
    /// Combat homme↔prédateur : la part d'incertitude de la riposte des meutes.
    pub const COMBAT: u64 = 1015;
    /// Noms propres des humains et des clans (Phase 6, la Chronique). Ce flux
    /// ne pilote **aucun** comportement — un nom n'a pas d'effet — mais il a
    /// son salt comme les autres : deux consommateurs qui le partageraient
    /// seraient corrélés le jour où l'un des deux compterait vraiment.
    pub const NAMES: u64 = 1016;
}
