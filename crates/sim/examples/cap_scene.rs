//! Les saisons de pousse autour des foyers des scènes de référence (CAP, POU) :
//! pour chaque seed et chaque scène, le biome du foyer, les jours de pousse par
//! an de sa maille, et la part des mailles à moins de 8 km qui poussent toute
//! l'année. Lecture seule : aucune simulation n'avance.
//!
//! Usage : `cargo run --release -p cairn-sim --example cap_scene -- [seeds]`

use cairn_sim::{fauna, scenario};

fn main() {
    let seeds: Vec<u64> = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "42,7,1337,2024".into())
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    let r = (cairn_core::km_to_tiles(8.0) / fauna::RANGE_ZONE_TILES).ceil() as i64;
    for seed in seeds {
        for cold in [false, true] {
            let (mut sim, home, _) = scenario::reference(seed, cold, 135);
            let biome = sim.world.tile(home.0, home.1).biome;
            let z0 = fauna::range_zone((home.0 as f64, home.1 as f64));
            let (wg, climate, range) = (sim.world.worldgen(), &sim.climate, &mut sim.rangeland);
            let days = range.growing_days(wg, climate, z0);
            let (mut all_year, mut n, mut sum) = (0, 0, 0u32);
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dy * dy > r * r {
                        continue;
                    }
                    let g = range.growing_days(wg, climate, (z0.0 + dx, z0.1 + dy));
                    n += 1;
                    sum += g;
                    if g >= 360 {
                        all_year += 1;
                    }
                }
            }
            println!(
                "seed {seed} {} : foyer en {biome:?}, {days} jours de pousse ; à 8 km : {all_year}/{n} mailles sans hiver, {} jours en moyenne",
                if cold { "froid" } else { "tempéré" },
                sum / n.max(1)
            );
        }
    }
}
