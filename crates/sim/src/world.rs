//! Le gestionnaire de chunks : génère à la demande, garde en mémoire un
//! nombre borné de chunks, décharge les moins récemment utilisés (LRU).
//!
//! C'est la pièce qui rend le monde **infini de fait mais borné en mémoire**
//! (BRIEF §2.1). L'éviction est sûre tant que les chunks sont du baseline pur :
//! décharger puis régénérer redonne l'identique. Quand l'état mutable arrivera
//! (Phase 2+), un chunk modifié devra être persisté avant éviction — sinon on
//! perd les changements. Marqueur laissé pour ce moment-là.

use std::collections::BTreeMap;

use cairn_core::WorldSeed;
use cairn_worldgen::WorldGen;

use crate::chunk::{CHUNK_SIZE, Chunk, ChunkCoord};
use crate::tile::Tile;

pub struct World {
    worldgen: WorldGen,
    /// Chunks résidents. `BTreeMap` et non `HashMap` : ordre d'itération
    /// déterministe, exigence du projet.
    chunks: BTreeMap<ChunkCoord, Chunk>,
    /// Tick d'accès le plus récent par chunk, pour le LRU.
    last_access: BTreeMap<ChunkCoord, u64>,
    /// Horloge logique : incrémentée à chaque accès.
    clock: u64,
    /// Nombre maximal de chunks résidents.
    capacity: usize,
    /// Statistiques cumulées (observabilité de la mémoire).
    pub generated: u64,
    pub evicted: u64,
}

impl World {
    pub fn new(seed: WorldSeed, capacity: usize) -> Self {
        Self {
            worldgen: WorldGen::new(seed),
            chunks: BTreeMap::new(),
            last_access: BTreeMap::new(),
            clock: 0,
            capacity: capacity.max(1),
            generated: 0,
            evicted: 0,
        }
    }

    /// Nombre de chunks actuellement en mémoire.
    pub fn loaded(&self) -> usize {
        self.chunks.len()
    }

    /// Renvoie le chunk demandé, le générant s'il est absent. Marque l'accès
    /// (pour le LRU) et évince si la capacité est dépassée.
    pub fn chunk(&mut self, coord: ChunkCoord) -> &Chunk {
        self.clock += 1;
        if !self.chunks.contains_key(&coord) {
            let chunk = Chunk::generate(coord, &self.worldgen);
            self.chunks.insert(coord, chunk);
            self.generated += 1;
            self.evict_down_to_capacity(coord);
        }
        self.last_access.insert(coord, self.clock);
        &self.chunks[&coord]
    }

    /// Tuile à la coordonnée de tuile (x, y) — charge le chunk au besoin.
    /// Renvoie une copie : la tuile est `Copy`, pas besoin de garder le chunk
    /// emprunté.
    pub fn tile(&mut self, x: i64, y: i64) -> Tile {
        let (coord, lx, ly) = split(x, y);
        *self.chunk(coord).tile(lx, ly)
    }

    /// Évince les chunks les moins récemment accédés jusqu'à revenir sous la
    /// capacité. `keep` (celui qu'on vient de charger) n'est jamais évincé.
    fn evict_down_to_capacity(&mut self, keep: ChunkCoord) {
        while self.chunks.len() > self.capacity {
            // Victime = plus petit tick d'accès. Les ex æquo sont départagés
            // par l'ordre du BTreeMap → déterministe.
            let victim = self
                .last_access
                .iter()
                .filter(|(c, _)| **c != keep)
                .min_by_key(|(_, t)| **t)
                .map(|(c, _)| *c);
            match victim {
                Some(c) => {
                    self.chunks.remove(&c);
                    self.last_access.remove(&c);
                    self.evicted += 1;
                }
                None => break,
            }
        }
    }
}

/// Décompose une coordonnée de tuile en (chunk, position locale). `div_euclid`
/// et `rem_euclid` donnent le bon résultat pour x, y négatifs — sinon la tuile
/// (-1, -1) tomberait dans le mauvais chunk.
fn split(x: i64, y: i64) -> (ChunkCoord, usize, usize) {
    let coord = ChunkCoord {
        x: x.div_euclid(CHUNK_SIZE),
        y: y.div_euclid(CHUNK_SIZE),
    };
    let lx = x.rem_euclid(CHUNK_SIZE) as usize;
    let ly = y.rem_euclid(CHUNK_SIZE) as usize;
    (coord, lx, ly)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoupage_des_coordonnees_negatives() {
        assert_eq!(split(0, 0), (ChunkCoord { x: 0, y: 0 }, 0, 0));
        assert_eq!(split(63, 63), (ChunkCoord { x: 0, y: 0 }, 63, 63));
        assert_eq!(split(64, 64), (ChunkCoord { x: 1, y: 1 }, 0, 0));
        assert_eq!(split(-1, -1), (ChunkCoord { x: -1, y: -1 }, 63, 63));
        assert_eq!(split(-64, -65), (ChunkCoord { x: -1, y: -2 }, 0, 63));
    }

    #[test]
    fn la_memoire_reste_bornee() {
        let mut world = World::new(WorldSeed(42), 16);
        // On balaie bien plus de chunks que la capacité.
        for cy in 0..12 {
            for cx in 0..12 {
                world.chunk(ChunkCoord { x: cx, y: cy });
                assert!(world.loaded() <= 16, "capacité dépassée : {}", world.loaded());
            }
        }
        assert_eq!(world.generated, 144);
        assert!(world.evicted >= 144 - 16);
    }

    #[test]
    fn eviction_puis_regeneration_est_identique() {
        // Lire une tuile, la faire évincer en chargeant beaucoup d'autres
        // chunks, puis la relire : le monde est régénérable, donc identique.
        let mut world = World::new(WorldSeed(42), 4);
        let avant = world.tile(100, 200);
        for cx in 50..80 {
            world.chunk(ChunkCoord { x: cx, y: cx });
        }
        let apres = world.tile(100, 200);
        assert_eq!(avant, apres);
    }

    #[test]
    fn tuile_du_monde_colle_au_worldgen() {
        let mut world = World::new(WorldSeed(42), 64);
        let reference = WorldGen::new(WorldSeed(42));
        for &(x, y) in &[(0, 0), (-1, -1), (1000, -500), (63, 64)] {
            assert_eq!(world.tile(x, y).elevation, reference.elevation(x, y) as f32);
        }
    }
}
