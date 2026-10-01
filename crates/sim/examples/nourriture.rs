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
//!       [seed] [années] [scène: tempere|froid] [capacité_chunks] [agents] [période_j] [jour_de_départ]
//!
//! Scènes : `tempere` est celle de `chronicle` (foyer tempéré, 40 agents),
//! `froid` celle d'`etincelle` (1-5 °C, hivers sous zéro, 60 agents).

use std::collections::BTreeMap;

use cairn_core::{TICKS_PER_DAY, WorldSeed, km_to_tiles};
use cairn_sim::{Behavior, DeathCause, Knowledge, Physiology, Position, Sim, fauna, food_stats, scenario};
use cairn_worldgen::Biome;

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
    #[allow(non_snake_case)]
    let PERIOD_DAYS: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(90);
    let start_day: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0);

    let mut sim = Sim::new(WorldSeed(seed), capacity);
    scenario::start_on_day(&mut sim, start_day);
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
         {years} ans, capacité {capacity}, période {PERIOD_DAYS} j, départ au jour {start_day}",
        tile.biome, tile.temperature,
    );
    println!(
        "{:>5} {:>4} {:>4} {:>4} {:>13} {:>15} | {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} | \
         {:>6} {:>5} {:>5} | {:>5} {:>4} {:>6} {:>7} | {:>7} {:>5} {:>5}",
        "jour", "pop", "nés", "morts", "faim/soif/aut", "faim moy/p90/max",
        "ceuil", "chsM", "chsP", "perdu", "stock", "chept", "lait", "part",
        "bio/h/j", "court", "tue/j",
        "zones", "max", "h/km²", "h/km²z", "gibier", "tps", "cons%",
    );

    let total_days = years * 360;
    let mut seen_deaths = 0usize;
    let mut seen_births = 0usize;
    let mut window = std::time::Instant::now();
    let mut human_days = 0.0f64;
    let day0 = sim.time.tick / TICKS_PER_DAY;
    // Ce que font les affamés (faim > 0,5), relevé chaque heure : la
    // dispersion se juge à ce qu'ils choisissent quand le pays ne répond plus.
    let mut hungry_tasks: BTreeMap<String, u64> = BTreeMap::new();
    let mut first_clan_day: Option<u64> = None;
    for day in 1..=total_days {
        if first_clan_day.is_none() && !sim.clans.is_empty() {
            first_clan_day = Some(day - 1);
            println!("      premier clan au jour {} après l'arrivée", day - 1);
        }
        for _ in 0..TICKS_PER_DAY {
            sim.step();
            for (_, (phys, behavior)) in sim.agents.query::<(&Physiology, &Behavior)>().iter() {
                if phys.hunger > 0.5 {
                    let kind = behavior.task.map_or("Rien".to_string(), |t| {
                        let k = format!("{:?}", t.kind);
                        k.split('(').next().unwrap_or("").to_string()
                    });
                    *hungry_tasks.entry(kind).or_insert(0) += 1;
                }
            }
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

        let preservation = sim.tech_tree.id_of("preservation");
        let preserving = sim
            .agents
            .query::<&Knowledge>()
            .iter()
            .filter(|(_, k)| preservation.is_some_and(|t| k.has(t)))
            .count() as u32;
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

        // Dispersion : distance moyenne au foyer de départ, et ce que la
        // maille la mieux pourvue à 6 km offre par tête face à la maille la
        // plus peuplée.
        let mut dist = 0.0;
        for (_, pos) in sim.agents.query::<&Position>().iter() {
            dist += (pos.x - home.0 as f64).hypot(pos.y - home.1 as f64);
        }
        let dist_km = cairn_core::tiles_to_km(dist / n as f64);
        let tick = sim.time.tick;
        let (busiest, busiest_n) =
            zones.iter().max_by_key(|(_, c)| **c).map(|(z, c)| (*z, *c)).unwrap_or(((0, 0), 0));
        let side = fauna::RANGE_ZONE_TILES;
        let center = |z: (i64, i64)| ((z.0 as f64 + 0.5) * side, (z.1 as f64 + 0.5) * side);
        let busy_kcal = sim.world.edible_kcal_peek(center(busiest), tick) / f64::from(busiest_n.max(1));
        let mut best_kcal = 0.0f64;
        for dz in -3..=3i64 {
            for dx in -3..=3i64 {
                let z = (busiest.0 + dx, busiest.1 + dz);
                let k = sim.world.edible_kcal_peek(center(z), tick);
                best_kcal = best_kcal.max(k);
            }
        }
        let tasks_total: u64 = hungry_tasks.values().sum();
        let mut tasks: Vec<(u64, String)> = hungry_tasks.iter().map(|(k, v)| (*v, k.clone())).collect();
        tasks.sort_by(|a, b| b.0.cmp(&a.0));
        let tasks_txt: Vec<String> = tasks
            .iter()
            .take(5)
            .map(|(v, k)| format!("{k} {:.0}%", 100.0 * *v as f64 / tasks_total.max(1) as f64))
            .collect();
        hungry_tasks.clear();
        let (fed, ev) = food_stats::take();
        let total: f64 = fed[0] + fed[1] + fed[4] + fed[6] + fed[7];
        let pct = |x: f64| 100.0 * x / total.max(1e-9);
        let hd = human_days.max(1.0);
        human_days = 0.0;
        println!(
            "{:>5} {:>4} {:>4} {:>4} {:>4}/{:>3}/{:>4} {:>5.2}/{:>4.2}/{:>4.2} | \
             {:>5.1} {:>5.1} {:>5.1} {:>5.1} {:>5.1} {:>5.1} {:>5.1} {:>5.1} | \
             {:>6.1} {:>4.0}% {:>5.2} | {:>5} {:>4} {:>6.2} {:>7.2} | {:>7.0} {:>5.1} {:>5.1}",
            day0 + day, pop, births, d_starv + d_thirst + d_other, d_starv, d_thirst, d_other,
            mean, p90, max,
            pct(fed[0]), pct(fed[1]), pct(fed[2]), pct(fed[3]), pct(fed[4]), pct(fed[5]),
            pct(fed[6]), pct(fed[7]),
            ev[0] as f64 / hd,
            100.0 * ev[2] as f64 / (ev[1].max(1)) as f64,
            ev[3] as f64 / PERIOD_DAYS as f64,
            zones.len(), zmax, dens, dens_max,
            sim.fauna_census().0, tps, 100.0 * f64::from(preserving) / n as f64,
        );
        let in_clan = sim
            .agents
            .query::<&cairn_sim::ClanMembership>()
            .iter()
            .filter(|(_, m)| m.0.is_some())
            .count();
        let bonds = sim
            .social
            .bonds
            .values()
            .filter(|w| **w >= cairn_sim::social::BOND_THRESHOLD)
            .count();
        if pop > 0 {
            println!(
                "      groupe : {} clan(s), {:.0} % en clan, {:.1} liens solides par personne",
                sim.clans.len(),
                100.0 * in_clan as f64 / pop as f64,
                2.0 * bonds as f64 / pop as f64,
            );
        }
        if tasks_total > 0 || pop > 0 {
            println!(
                "      disp {dist_km:.1} km · maille la plus peuplée {busiest_n} hab., {:.0} j de nourriture/tête · \
                 meilleure maille à 6 km {:.0} j pour {busiest_n} · affamés-heures {tasks_total} : {}",
                busy_kcal / 2_500.0,
                best_kcal / 2_500.0 / f64::from(busiest_n.max(1)),
                tasks_txt.join(", "),
            );
        }
    }
}
