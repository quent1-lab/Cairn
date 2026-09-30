//! Banc de l'**alimentation humaine** (défaut D10) : d'où les humains tirent-ils
//! leur nourriture, et quelque chose la limite-t-il ?
//!
//! Il ne corrige rien. Par saison (90 jours), il imprime la population, les
//! naissances et les morts par cause de la période, la faim (moyenne, 9ᵉ
//! décile, pire cas), la part de chaque voie d'alimentation dans la faim
//! retirée, ce que la cueillette prélève sur la flore, et la **densité** :
//! humains par maille de 2 km occupée (la maille de pâturage de la faune, ce
//! qui rend les deux comparables) et humains par km² occupé.
//!
//! Usage :
//!   cargo run --release -p cairn-sim --features food-stats --example nourriture -- \
//!       [seed] [années] [scène: tempere|froid] [capacité_chunks] [agents]
//!
//! Scènes : `tempere` est celle de `chronicle` (foyer tempéré, 40 agents),
//! `froid` celle d'`etincelle` (1-5 °C, hivers sous zéro, 60 agents).

use std::collections::BTreeMap;

use cairn_core::{TICKS_PER_DAY, WorldSeed, km_to_tiles};
use cairn_sim::{DeathCause, Physiology, Position, Sim, fauna, food_stats, scenario};
use cairn_worldgen::Biome;

const PERIOD_DAYS: u64 = 90;
/// Maille de comptage : celle de la faune (`fauna::range_zone`), 2 km.
const ZONE_KM2: f64 = 4.0;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(42);
    let years: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);
    let scene = args.next().unwrap_or_else(|| "tempere".into());
    let capacity: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(16384);
    let cold = scene == "froid";
    let n_agents: usize =
        args.next().and_then(|s| s.parse().ok()).unwrap_or(if cold { 60 } else { 40 });

    let mut sim = Sim::new(WorldSeed(seed), capacity);
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
    let placed = scenario::populate(&mut sim, home, n_agents, 2);
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
    let tile = sim.world.tile(home.0, home.1);
    println!(
        "# nourriture — seed {seed}, scène {scene}, foyer {:?} {:.1} °C, {placed} agents, \
         {years} ans, capacité {capacity}, période {PERIOD_DAYS} j",
        tile.biome, tile.temperature,
    );
    println!(
        "{:>5} {:>4} {:>4} {:>4} {:>13} {:>15} | {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} | \
         {:>6} {:>5} {:>5} | {:>5} {:>4} {:>6} {:>7} | {:>7} {:>5}",
        "jour", "pop", "nés", "morts", "faim/soif/aut", "faim moy/p90/max",
        "ceuil", "chsM", "chsP", "perdu", "stock", "chept", "lait",
        "bio/h/j", "court", "tue/j",
        "zones", "max", "h/km²", "h/km²z", "gibier", "tps",
    );

    let total_days = years * 360;
    let mut seen_deaths = 0usize;
    let mut seen_births = 0usize;
    let mut window = std::time::Instant::now();
    let mut human_days = 0.0f64;
    for day in 1..=total_days {
        for _ in 0..TICKS_PER_DAY {
            sim.step();
        }
        human_days += sim.population() as f64;
        if !day.is_multiple_of(PERIOD_DAYS) {
            continue;
        }
        let tps = (PERIOD_DAYS * TICKS_PER_DAY) as f64 / window.elapsed().as_secs_f64().max(1e-6);
        window = std::time::Instant::now();

        let births = sim.births.len() - seen_births;
        seen_births = sim.births.len();
        let (mut d_starv, mut d_thirst, mut d_other) = (0, 0, 0);
        for d in &sim.deaths[seen_deaths..] {
            match d.cause {
                DeathCause::Starvation => d_starv += 1,
                DeathCause::Dehydration => d_thirst += 1,
                _ => d_other += 1,
            }
        }
        seen_deaths = sim.deaths.len();

        let mut hunger: Vec<f32> = Vec::new();
        let mut zones: BTreeMap<(i64, i64), u32> = BTreeMap::new();
        for (_, (pos, phys)) in sim.agents.query::<(&Position, &Physiology)>().iter() {
            hunger.push(phys.hunger);
            *zones.entry(fauna::range_zone((pos.x, pos.y))).or_insert(0) += 1;
        }
        hunger.sort_by(f32::total_cmp);
        let n = hunger.len().max(1);
        let mean = hunger.iter().sum::<f32>() / n as f32;
        let p90 = hunger.get(n * 9 / 10).copied().unwrap_or(0.0);
        let max = hunger.last().copied().unwrap_or(0.0);
        let zmax = zones.values().copied().max().unwrap_or(0);
        let occupied_km2 = zones.len() as f64 * ZONE_KM2;
        let pop = sim.population();
        let dens = pop as f64 / occupied_km2.max(1e-9);
        let dens_max = f64::from(zmax) / ZONE_KM2;

        let (fed, ev) = food_stats::take();
        let total: f64 = fed[0] + fed[1] + fed[4] + fed[6];
        let pct = |x: f64| 100.0 * x / total.max(1e-9);
        let hd = human_days.max(1.0);
        human_days = 0.0;
        println!(
            "{:>5} {:>4} {:>4} {:>4} {:>4}/{:>3}/{:>4} {:>5.2}/{:>4.2}/{:>4.2} | \
             {:>5.1} {:>5.1} {:>5.1} {:>5.1} {:>5.1} {:>5.1} {:>5.1} | \
             {:>6.1} {:>4.0}% {:>5.2} | {:>5} {:>4} {:>6.2} {:>7.2} | {:>7.0} {:>5.1}",
            day, pop, births, d_starv + d_thirst + d_other, d_starv, d_thirst, d_other,
            mean, p90, max,
            pct(fed[0]), pct(fed[1]), pct(fed[2]), pct(fed[3]), pct(fed[4]), pct(fed[5]),
            pct(fed[6]),
            ev[0] as f64 / hd,
            100.0 * ev[2] as f64 / (ev[1].max(1)) as f64,
            ev[3] as f64 / PERIOD_DAYS as f64,
            zones.len(), zmax, dens, dens_max,
            sim.fauna_census().0, tps,
        );
    }
}
