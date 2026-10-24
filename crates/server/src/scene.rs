//! La scène que le serveur fait vivre : **la même** que le banc d'acceptation
//! (`crates/sim/examples/acceptation.rs`, fonction `run`), pour que ce qu'on
//! regarde dans le navigateur soit ce qu'on a mesuré. Si l'une change,
//! l'autre doit suivre.

use cairn_core::{WorldSeed, km_to_tiles};
use cairn_sim::{Sim, fauna, scenario};
use cairn_worldgen::Biome;

/// Capacité du store de chunks, celle du banc.
const CHUNK_CAPACITY: usize = 16384;
/// Jour de départ, celui du banc : la fin du printemps.
const START_DAY: u64 = 135;

pub fn build(seed: u64, cold: bool) -> Sim {
    let n_agents = if cold { 60 } else { 40 };
    let mut sim = Sim::new(WorldSeed(seed), CHUNK_CAPACITY);
    scenario::start_on_day(&mut sim, START_DAY);
    let seed_point = (km_to_tiles(1500.0) as i64, km_to_tiles(2100.0) as i64);
    let home = if cold {
        scenario::find_home_where(
            &mut sim,
            seed_point,
            1.0..=5.0,
            &[Biome::TemperateForest, Biome::Taiga, Biome::Grassland],
        )
        .unwrap_or_else(|| scenario::find_home(&mut sim, seed_point))
    } else {
        scenario::find_home(&mut sim, seed_point)
    };
    scenario::populate(&mut sim, home, n_agents, 2);
    for i in 0..4i64 {
        let angle = i as f64 * 1.57;
        let (x, y) = (
            home.0 + (angle.cos() * km_to_tiles(4.0)) as i64,
            home.1 + (angle.sin() * km_to_tiles(4.0)) as i64,
        );
        if sim.world.tile(x, y).is_walkable() {
            sim.spawn_pack(x as f64, y as f64, fauna::PACK_START);
        }
    }
    sim
}
