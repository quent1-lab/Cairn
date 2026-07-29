//! Banc de **calibrage** de la Phase 2 (BRIEF : « cette phase est un
//! calibrage »). On lâche la même population dans des foyers de climats
//! différents, sur plusieurs seeds, et on mesure la mortalité **par cause**.
//!
//! But : vérifier que le monde est « ni trop dur ni trop mou » sur une plage
//! de conditions — en particulier que le **froid tue** dans un foyer froid
//! (critère « s'abritent quand il fait froid ») sans exterminer, et que le
//! tempéré reste vivable. Le tempéré teste la soif/faim ; le froid ajoute
//! l'hypothermie.
//!
//! Usage : cargo run --release -p cairn-sim --example calibrate -- [jours] [agents]

use cairn_core::{TICKS_PER_DAY, WorldSeed, km_to_tiles};
use cairn_sim::{DeathCause, Sim};
use cairn_worldgen::{Biome, HumidityConfig, WorldGenConfig};

const CHUNK_CAPACITY: usize = 8192;

/// Config d'humidité rapide (comme le client) : le calibrage enchaîne des
/// années entières, la vitesse prime sur le détail des frontières humides.
fn fast_config() -> WorldGenConfig {
    WorldGenConfig {
        humidity: HumidityConfig { steps: 32, lateral_samples: 0, ..HumidityConfig::default() },
        ..WorldGenConfig::default()
    }
}

struct Climate {
    label: &'static str,
    /// Point de départ de la recherche (une latitude visée).
    seed_point: (i64, i64),
    temp: std::ops::RangeInclusive<f64>,
    biomes: &'static [Biome],
}

fn main() {
    let mut args = std::env::args().skip(1);
    let days: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(180);
    let agents: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(60);
    let n_seeds: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    let ticks = days * TICKS_PER_DAY;

    // Latitudes visées : tempéré ~mi-latitude, plus froid en montant vers le
    // pôle (à y = période/2 ≈ 2 000 000 tuiles).
    let climates = [
        Climate {
            label: "tempéré",
            seed_point: (0, km_to_tiles(2000.0) as i64),
            temp: 9.0..=18.0,
            biomes: &[Biome::Grassland, Biome::TemperateForest],
        },
        Climate {
            label: "frais",
            seed_point: (0, km_to_tiles(2500.0) as i64),
            temp: 3.0..=8.0,
            biomes: &[Biome::Grassland, Biome::Taiga, Biome::TemperateForest],
        },
        Climate {
            label: "froid",
            seed_point: (0, km_to_tiles(2900.0) as i64),
            temp: -5.0..=3.0,
            biomes: &[Biome::Taiga, Biome::Tundra, Biome::Grassland],
        },
    ];
    let all_seeds = [42u64, 7, 123, 2024, 5];
    let seeds = &all_seeds[..n_seeds.min(all_seeds.len())];

    println!("— Calibrage Phase 2 : {days} jours, {agents} agents —\n");
    println!(
        "{:>6} {:>9} {:>7} {:>6} | {:>6} {:>6} {:>7} {:>6} | {:>6} | verdict",
        "seed", "climat", "T°moy", "vivants", "†faim", "†soif", "†froid", "†tot", "abris%"
    );

    for &seed in seeds {
        for c in &climates {
            let mut sim = Sim::with_config(WorldSeed(seed), CHUNK_CAPACITY, fast_config());
            let home = cairn_sim::scenario::find_home_where(
                &mut sim,
                c.seed_point,
                c.temp.clone(),
                c.biomes,
            );
            let Some(home) = home else {
                println!("{seed:>6} {:>9}   (aucun foyer de ce climat trouvé)", c.label);
                continue;
            };
            let t_moy = {
                let wg = sim.world.worldgen();
                wg.mean_temperature(home.0, home.1, wg.elevation(home.0, home.1))
            };
            let placed = cairn_sim::scenario::populate(&mut sim, home, agents, 2);
            for _ in 0..ticks {
                sim.step();
            }
            let (starved, dehydrated, frozen) = death_counts(&sim);
            // Le nombre de décès enregistrés, pas `placed - population()` :
            // depuis la Phase 3, les naissances peuvent faire dépasser la
            // population de départ, et cette soustraction (usize) débordait.
            let dead = sim.deaths.len();
            let alive = sim.population();
            // Fraction du temps de vie passée à s'abriter : preuve du
            // comportement « s'abriter quand il fait froid ».
            let agent_ticks = (placed as u64 * ticks).max(1);
            let shelter_pct = 100.0 * sim.shelter_ticks as f64 / agent_ticks as f64;

            let verdict = if alive == 0 {
                "EXTINCTION (trop dur)"
            } else if dead == 0 {
                "aucune mort (trop mou)"
            } else {
                "OK : mortalité partielle"
            };
            println!(
                "{seed:>6} {:>9} {t_moy:>6.1} {alive:>6} | {starved:>6} {dehydrated:>6} {frozen:>7} {dead:>6} | {shelter_pct:>5.1} | {verdict}",
                c.label,
            );
        }
    }

    println!(
        "\nLecture : le tempéré doit survivre avec quelques morts ; le froid \n\
         doit produire des morts de froid (†froid > 0) sans exterminer."
    );
}

fn death_counts(sim: &Sim) -> (usize, usize, usize) {
    let mut c = (0, 0, 0);
    for d in &sim.deaths {
        match d.cause {
            DeathCause::Starvation => c.0 += 1,
            DeathCause::Dehydration => c.1 += 1,
            DeathCause::Hypothermia => c.2 += 1,
            // Ni la vieillesse ni la prédation ne sont un signal de calibrage
            // climatique.
            DeathCause::OldAge | DeathCause::Predation => {}
        }
    }
    c
}
