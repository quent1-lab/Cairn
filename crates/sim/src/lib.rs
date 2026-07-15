//! Le monde de simulation : un store de tuiles **chunké**, généré à la
//! demande depuis la seed et déchargé quand inutilisé (BRIEF §2.1).
//!
//! C'est le pont entre le `worldgen` (baseline pur, immuable, fonction de la
//! seed) et la simulation à venir : chaque tuile matérialise le baseline puis
//! portera l'état mutable (fertilité dégradée, biomasse consommée, structures,
//! revendications…). Tant qu'aucune tuile n'est modifiée, décharger un chunk
//! et le régénérer redonne l'identique — c'est ce qui rend le monde infini
//! viable en mémoire bornée.

pub mod chunk;
pub mod tile;
pub mod world;

pub use chunk::{CHUNK_AREA, CHUNK_SIZE, Chunk, ChunkCoord};
pub use tile::{Tile, TileFlags};
pub use world::World;
