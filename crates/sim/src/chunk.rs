//! Le chunk : un carré de 64 × 64 tuiles, unité de génération et de
//! déchargement (BRIEF §2.1).

use cairn_worldgen::{Biome, WorldGen};

use crate::tile::{Tile, TileFlags, baseline_biomass, baseline_fertility};

/// Côté d'un chunk, en tuiles.
pub const CHUNK_SIZE: i64 = 64;
/// Nombre de tuiles par chunk.
pub const CHUNK_AREA: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize;

/// Coordonnées d'un chunk (et non d'une tuile) : le chunk (cx, cy) couvre les
/// tuiles [cx·64, cx·64+64) × [cy·64, cy·64+64). En `i64` : monde infini.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkCoord {
    pub x: i64,
    pub y: i64,
}

impl ChunkCoord {
    /// Coin supérieur-gauche du chunk, en coordonnées de tuiles.
    pub fn origin(self) -> (i64, i64) {
        (self.x * CHUNK_SIZE, self.y * CHUNK_SIZE)
    }
}

pub struct Chunk {
    pub coord: ChunkCoord,
    tiles: Vec<Tile>,
}

impl Chunk {
    /// Génère le chunk depuis le baseline procédural. Déterministe : même
    /// seed, même chunk, bit pour bit.
    ///
    /// L'humidité est échantillonnée aux **4 coins** puis interpolée
    /// bilinéairement : elle varie sur ~256 km, un chunk fait 128 m, donc la
    /// calculer par tuile gâcherait 4096× le travail. Les coins étant
    /// **partagés** avec les chunks voisins, le champ reste continu d'un chunk
    /// à l'autre — aucune couture.
    pub fn generate(coord: ChunkCoord, world: &WorldGen) -> Self {
        let (x0, y0) = coord.origin();
        let h = [
            world.humidity(x0, y0),
            world.humidity(x0 + CHUNK_SIZE, y0),
            world.humidity(x0, y0 + CHUNK_SIZE),
            world.humidity(x0 + CHUNK_SIZE, y0 + CHUNK_SIZE),
        ];

        let mut tiles = Vec::with_capacity(CHUNK_AREA);
        for ly in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let (x, y) = (x0 + lx, y0 + ly);
                let fx = lx as f64 / CHUNK_SIZE as f64;
                let fy = ly as f64 / CHUNK_SIZE as f64;
                let humidity = bilerp(h, fx, fy);

                let elevation = world.elevation(x, y);
                let temperature = world.mean_temperature(x, y, elevation);
                let biome = Biome::classify(elevation, temperature, humidity);

                let mut flags = TileFlags::default();
                if elevation <= 0.0 {
                    flags.set(TileFlags::WATER);
                    if matches!(biome, Biome::Coast) {
                        flags.set(TileFlags::COAST);
                    }
                    if temperature < cairn_worldgen::FREEZE_STILL_C {
                        flags.set(TileFlags::FROZEN);
                    }
                }

                tiles.push(Tile {
                    biome,
                    rock: world.rock_type(x, y),
                    deposit: world.deposit(x, y, elevation),
                    elevation: elevation as f32,
                    temperature: temperature as f32,
                    soil_fertility: baseline_fertility(biome),
                    biomass: baseline_biomass(biome),
                    flags,
                });
            }
        }
        Self { coord, tiles }
    }

    /// Tuile locale (lx, ly), avec 0 ≤ lx, ly < 64.
    pub fn tile(&self, lx: usize, ly: usize) -> &Tile {
        &self.tiles[ly * CHUNK_SIZE as usize + lx]
    }
}

/// Interpolation bilinéaire dans le carré unité entre 4 coins
/// `[haut-gauche, haut-droit, bas-gauche, bas-droit]`.
fn bilerp(c: [f64; 4], fx: f64, fy: f64) -> f64 {
    let top = c[0] + (c[1] - c[0]) * fx;
    let bottom = c[2] + (c[3] - c[2]) * fx;
    top + (bottom - top) * fy
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_core::WorldSeed;

    #[test]
    fn chunk_deterministe() {
        let wg = WorldGen::new(WorldSeed(42));
        let coord = ChunkCoord { x: 3, y: -7 };
        let a = Chunk::generate(coord, &wg);
        let b = Chunk::generate(coord, &wg);
        for i in 0..CHUNK_AREA {
            assert_eq!(a.tiles[i], b.tiles[i]);
        }
    }

    #[test]
    fn tuile_du_chunk_colle_au_worldgen() {
        // La matérialisation ne doit rien inventer : l'altitude d'une tuile
        // vaut exactement celle du worldgen au même endroit (garantie
        // « aucune couture »).
        let wg = WorldGen::new(WorldSeed(42));
        let coord = ChunkCoord { x: -2, y: 5 };
        let chunk = Chunk::generate(coord, &wg);
        let (x0, y0) = coord.origin();
        for &(lx, ly) in &[(0usize, 0usize), (63, 63), (17, 40)] {
            let (x, y) = (x0 + lx as i64, y0 + ly as i64);
            assert_eq!(chunk.tile(lx, ly).elevation, wg.elevation(x, y) as f32);
        }
    }
}
