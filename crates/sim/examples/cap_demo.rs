//! Banc **démographique réduit** (chantier CAP, étape 3) : un troupeau agrégé
//! dans une maille de pâturage réelle (biome et climat d'une seed), sans
//! espace, sans prédateur, sans humain. Il appelle les mêmes fonctions que
//! `fauna::update_herds` — le fourrage de la maille (`Rangeland::graze`) et
//! la démographie (`herd_life_tick`, avec le signal de saison `Rangeland::spring`) — et fait tenir des décennies en secondes.
//!
//! Ce qu'il ne voit pas : les déplacements, la fuite, la prédation, la
//! migration, le partage d'une maille entre espèces.
//!
//! Usage : `cargo run --release -p cairn-sim --example cap_demo -- [années] [densité de départ /km²] [seeds] [biome à tracer]`
//! (avec un biome à tracer : une ligne tous les 10 jours de la dernière année — densité, réserves,
//! part de ration trouvée, fourrage sur pied).

use cairn_core::{SimTime, TICKS_PER_DAY, WorldSeed};
use cairn_sim::fauna::{self, Species};
use cairn_sim::{Sim, scenario};
use cairn_worldgen::Biome;

const ZONE_KM2: f32 = 4.0;

/// La maille la plus proche du point de référence dont le centre porte le biome.
fn find_zone(sim: &Sim, biome: Biome) -> Option<(i64, i64)> {
    let origin = scenario::reference_seed_point();
    let z0 = fauna::range_zone((origin.0 as f64, origin.1 as f64));
    let side = fauna::RANGE_ZONE_TILES;
    for r in 0..120i64 {
        for dx in -r..=r {
            for dy in -r..=r {
                if dx.abs() != r && dy.abs() != r {
                    continue;
                }
                let z = (z0.0 + dx, z0.1 + dy);
                let all = (0..4).all(|k| {
                    let x = (z.0 as f64 + 0.25 + 0.5 * (k % 2) as f64) * side;
                    let y = (z.1 as f64 + 0.25 + 0.5 * (k / 2) as f64) * side;
                    sim.world.worldgen().biome(x as i64, y as i64) == biome
                });
                if all {
                    return Some(z);
                }
            }
        }
    }
    None
}

fn main() {
    let mut args = std::env::args().skip(1);
    let years: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(50);
    let start: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(11.0);
    let seeds: Vec<u64> = args
        .next()
        .unwrap_or_else(|| "42,7,1337,2024".into())
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    let cases = [
        ("forêt", Biome::TemperateForest, Species::Deer),
        ("taïga", Biome::Taiga, Species::Reindeer),
        ("toundra", Biome::Tundra, Species::Reindeer),
        ("prairie", Biome::Grassland, Species::Aurochs),
        ("steppe", Biome::Steppe, Species::Gazelle),
        ("savane", Biome::Savanna, Species::Gazelle),
    ];
    let trace: Option<String> = args.next();
    println!("# cap_demo — {years} ans, départ {start} têtes/km², une maille de {ZONE_KM2} km², sans prédateur");
    println!("# densité min/méd/max de l'année ; « nourr. » = part de ration trouvée au plus bas de l'année");
    for (name, biome, species) in cases {
        if trace.as_deref().is_some_and(|t| t != name) {
            continue;
        }
        for &seed in &seeds {
            let mut sim = Sim::new(WorldSeed(seed), 64);
            let Some(zone) = find_zone(&sim, biome) else {
                println!("{name} seed {seed} : aucune maille");
                continue;
            };
            let ration = species.daily_ration_kg();
            let y = ((zone.1 as f64 + 0.5) * fauna::RANGE_ZONE_TILES) as i64;
            let seasons = sim.climate.season(y, SimTime { tick: 0 }).is_some();
            let (wg, climate, range) = (sim.world.worldgen(), &sim.climate, &mut sim.rangeland);
            // Capacité annuelle : production moyenne (kg/j sur 365 j) ramenée à l'année de 360 j.
            let capacity = range.production(wg, zone) * 365.0 / 360.0 / ration / ZONE_KM2;
            let mut herd = fauna::Herd::new(start * ZONE_KM2, species);
            let mut out = String::new();
            let mut tick = 0u64;
            for year in 1..=years {
                let mut d: Vec<f32> = Vec::with_capacity(360);
                let mut food_min = 1.0f32;
                for day in 0..360 {
                    if trace.is_some() && year == years && day % 10 == 0 {
                        let kg = range.peek_stock(wg, climate, zone, SimTime { tick });
                        let n = herd.population;
                        println!(
                            "  j{day:>3} densité {:.1} (jeunes {:.1}) réserves {:.3} fourrage {:.0} kg ({:.1} j de demande)",
                            n / ZONE_KM2,
                            herd.young / ZONE_KM2,
                            herd.condition,
                            kg,
                            kg / (n * ration).max(1e-6)
                        );
                    }
                    for _ in 0..TICKS_PER_DAY {
                        let food = range.graze(wg, climate, zone, SimTime { tick }, herd.population * ration);
                        food_min = food_min.min(food);
                        let spring = if tick % TICKS_PER_DAY == 0 {
                            range.spring(wg, climate, zone, tick / TICKS_PER_DAY)
                        } else {
                            match range.spring(wg, climate, zone, tick / TICKS_PER_DAY) {
                                fauna::Spring::Continuous => fauna::Spring::Continuous,
                                _ => fauna::Spring::None,
                            }
                        };
                        fauna::herd_life_tick(&mut herd, food, spring);
                        tick += 1;
                    }
                    d.push(herd.population / ZONE_KM2);
                }
                if year == 1 || year % 10 == 0 {
                    d.sort_by(f32::total_cmp);
                    out += &format!(" · an {year} : {:.1}/{:.1}/{:.1} nourr. {food_min:.2}", d[0], d[180], d[359]);
                }
            }
            println!(
                "{name} seed {seed} ({}, capacité annuelle {capacity:.1} /km², {}){out}",
                species.label(),
                if seasons { "saisons" } else { "sans saison" }
            );
        }
    }
}
