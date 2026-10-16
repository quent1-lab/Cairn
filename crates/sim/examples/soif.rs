//! Banc de la **soif des petits** (MAR-6c, 2026-10-16) : quand un enfant de
//! 1 à 4 ans a soif à en mourir, que fait-il, que fait sa mère, et où est
//! l'eau ?
//!
//! Il ne corrige rien. Chaque heure, pour chaque enfant de 1 à 4 ans dont la
//! soif est saturée (≥ 0,95), il relève : sa tâche, celle de sa mère (ou
//! « sans mère »), la distance à sa mère, la distance à la source la plus
//! proche qu'il connaît (ou « aucune »), et la distance au foyer de son clan.
//! Puis les morts de soif des 1-4 ans, avec les mêmes relevés à leur dernière
//! heure. Scènes et départ d'`acceptation`.
//!
//! Usage :
//!   cargo run --release -p cairn-sim --example soif -- [jours] [jour_de_départ] [seeds] [fils]
//!
//! Défauts : 3 ans, départ d'été (jour 135), seeds 42,7,1337,2024, 1 fil.

use std::collections::BTreeMap;

use cairn_core::{TICKS_PER_DAY, WorldSeed, km_to_tiles, tiles_to_km};
use cairn_sim::{AgentId, Behavior, ClanMembership, DeathCause, Demographics, Kinship, Memory, Physiology, Position, Sim, fauna, scenario};
use cairn_worldgen::Biome;

#[derive(Default, Clone)]
struct Report {
    label: String,
    /// Heures-enfant en soif saturée, par (tâche de l'enfant, tâche de la mère).
    by_task: BTreeMap<(String, String), u64>,
    /// Heures-enfant en soif saturée, par tranche de distance (km) à la mère,
    /// à la source connue la plus proche, au foyer.
    to_mother: BTreeMap<String, u64>,
    to_spring: BTreeMap<String, u64>,
    to_home: BTreeMap<String, u64>,
    /// Heures-enfant en soif saturée, par âge (moins de 3 ans : porté, ne boit
    /// que le lait ; 3-4 ans : délibère) et par état de la mère (réserves,
    /// santé).
    by_age: BTreeMap<String, u64>,
    mother_state: BTreeMap<String, u64>,
    /// Morts de soif des 1-4 ans : la dernière heure relevée.
    deaths: Vec<String>,
    child_h: u64,
    /// Relevé de minuit des mères d'un enfant de moins de 3 ans : jours-mère,
    /// dont réserves vides ; charge portée (somme, points) ; faim (somme).
    mother_days: u64,
    mother_empty: u64,
    mother_load: f64,
    mother_hunger: f64,
}

fn bucket(km: Option<f64>) -> String {
    match km {
        None => "aucune".into(),
        Some(k) if k < 0.1 => "< 0,1".into(),
        Some(k) if k < 0.5 => "0,1-0,5".into(),
        Some(k) if k < 1.0 => "0,5-1".into(),
        Some(k) if k < 2.0 => "1-2".into(),
        Some(k) if k < 5.0 => "2-5".into(),
        Some(_) => "≥ 5".into(),
    }
}

fn task_name(b: &Behavior) -> String {
    b.task.map_or("(aucune)".into(), |t| format!("{:?}", t.kind).split('(').next().unwrap_or("").to_string())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let days: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(3 * 360);
    let start_day: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(135);
    let seeds: Vec<u64> = args
        .next()
        .map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect())
        .unwrap_or_else(|| vec![42, 7, 1337, 2024]);
    let threads: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(1).max(1);
    let mut jobs = Vec::new();
    for &cold in &[false, true] {
        for &seed in &seeds {
            jobs.push((seed, cold));
        }
    }
    println!("# soif — {days} jours, départ au jour {start_day}, seeds {seeds:?}, {} runs", jobs.len());
    let queue = std::sync::Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>());
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let job = queue.lock().unwrap().pop();
                let Some((idx, (seed, cold))) = job else { break };
                let r = run(seed, cold, days, start_day);
                eprintln!("  fini : {}", r.label);
                results.lock().unwrap().push((idx, r));
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|(i, _)| *i);
    print(&results.into_iter().map(|(_, r)| r).collect::<Vec<_>>());
}

fn run(seed: u64, cold: bool, days: u64, start_day: u64) -> Report {
    let n_agents = if cold { 60 } else { 40 };
    let mut sim = Sim::new(WorldSeed(seed), 16384);
    scenario::start_on_day(&mut sim, start_day);
    let seed_point = (km_to_tiles(1500.0) as i64, km_to_tiles(2100.0) as i64);
    let home = if cold {
        scenario::find_home_where(&mut sim, seed_point, 1.0..=5.0, &[Biome::TemperateForest, Biome::Taiga, Biome::Grassland])
            .unwrap_or_else(|| scenario::find_home(&mut sim, seed_point))
    } else {
        scenario::find_home(&mut sim, seed_point)
    };
    scenario::populate(&mut sim, home, n_agents, 2);
    for i in 0..4i64 {
        let angle = i as f64 * 1.57;
        let (x, y) = (home.0 + (angle.cos() * km_to_tiles(4.0)) as i64, home.1 + (angle.sin() * km_to_tiles(4.0)) as i64);
        if sim.world.tile(x, y).is_walkable() {
            sim.spawn_pack(x as f64, y as f64, fauna::PACK_START);
        }
    }
    let mut r = Report { label: format!("{} {seed}", if cold { "froid" } else { "tempéré" }), ..Report::default() };
    let mut last: BTreeMap<u64, String> = BTreeMap::new();
    let mut seen_deaths = 0usize;
    for _ in 0..days * TICKS_PER_DAY {
        sim.step();
        let tick = sim.time.tick;
        // Les mères : position et tâche.
        let adults: BTreeMap<u64, ((f64, f64), String)> = sim
            .agents
            .query::<(&AgentId, &Position, &Behavior)>()
            .iter()
            .map(|(_, (id, p, b))| (id.0, ((p.x, p.y), task_name(b))))
            .collect();
        let bodies: BTreeMap<u64, (f32, f32, f32)> = sim
            .agents
            .query::<(&AgentId, &Physiology)>()
            .iter()
            .map(|(_, (id, p))| (id.0, (p.reserves_kcal, p.health, cairn_sim::demography::milk_flow(p))))
            .collect();
        let homes: BTreeMap<u64, (f64, f64)> = sim.clans.iter().map(|c| (c.id.0, c.home)).collect();
        for (_, (id, demo, kin, phys, pos, b, mem, cm)) in sim
            .agents
            .query::<(&AgentId, &Demographics, &Kinship, &Physiology, &Position, &Behavior, &Memory, &ClanMembership)>()
            .iter()
        {
            let age = demo.age_years(tick);
            if !(1.0..5.0).contains(&age) {
                continue;
            }
            r.child_h += 1;
            if phys.thirst < 0.95 {
                continue;
            }
            let mother = kin.mother.and_then(|m| adults.get(&m.0));
            let mother_task = mother.map_or("(sans mère)".to_string(), |m| m.1.clone());
            let d_mother = mother.map(|m| tiles_to_km((m.0.0 - pos.x).hypot(m.0.1 - pos.y)));
            let d_spring = mem
                .nearest_known_spring((pos.x, pos.y))
                .map(|s| tiles_to_km((s.0 as f64 - pos.x).hypot(s.1 as f64 - pos.y)));
            let d_home = cm.0.and_then(|c| homes.get(&c.0)).map(|h| tiles_to_km((h.0 - pos.x).hypot(h.1 - pos.y)));
            let task = task_name(b);
            let age_key = if age < 3.0 { "1-3 ans (porté)" } else { "3-4 ans" };
            *r.by_age.entry(age_key.into()).or_default() += 1;
            let ms = kin.mother.and_then(|m| bodies.get(&m.0)).map_or("(sans mère)".to_string(), |&(res, health, milk)| {
                format!(
                    "réserves {} ; santé {} ; lait {}",
                    if res <= 0.0 { "vides" } else { "non vides" },
                    if health < 0.5 { "< 0,5" } else { "≥ 0,5" },
                    if milk < 0.6 { "< 0,6" } else { "≥ 0,6" }
                )
            });
            *r.mother_state.entry(ms).or_default() += 1;
            *r.by_task.entry((task.clone(), mother_task.clone())).or_default() += 1;
            *r.to_mother.entry(bucket(d_mother)).or_default() += 1;
            *r.to_spring.entry(bucket(d_spring)).or_default() += 1;
            *r.to_home.entry(bucket(d_home)).or_default() += 1;
            last.insert(
                id.0,
                format!(
                    "{age:.1} an, {task} ; mère : {mother_task} à {} km ; source connue à {} km ; foyer à {} km",
                    bucket(d_mother),
                    bucket(d_spring),
                    bucket(d_home)
                ),
            );
        }
        if tick % TICKS_PER_DAY == 0 {
            let mothers: std::collections::BTreeSet<u64> = sim
                .agents
                .query::<(&Demographics, &Kinship)>()
                .iter()
                .filter(|(_, (d, _))| d.age_years(tick) < 3.0)
                .filter_map(|(_, (_, k))| k.mother.map(|m| m.0))
                .collect();
            for (_, (id, phys, carrying)) in sim.agents.query::<(&AgentId, &Physiology, &cairn_sim::agent::Carrying)>().iter() {
                if mothers.contains(&id.0) {
                    r.mother_days += 1;
                    r.mother_empty += u64::from(phys.reserves_kcal <= 0.0);
                    r.mother_load += f64::from(carrying.0);
                    r.mother_hunger += f64::from(phys.hunger);
                }
            }
        }
        for d in &sim.deaths[seen_deaths..] {
            if d.cause == DeathCause::Dehydration
                && let Some(s) = last.remove(&d.agent.0)
            {
                r.deaths.push(s);
            }
        }
        seen_deaths = sim.deaths.len();
    }
    r
}

fn print(rs: &[Report]) {
    let mut by_task: BTreeMap<(String, String), u64> = BTreeMap::new();
    let (mut m, mut s, mut h): (BTreeMap<String, u64>, BTreeMap<String, u64>, BTreeMap<String, u64>) = Default::default();
    let (mut ages, mut ms): (BTreeMap<String, u64>, BTreeMap<String, u64>) = Default::default();
    let mut total = 0u64;
    let mut child_h = 0u64;
    for r in rs {
        child_h += r.child_h;
        for (k, v) in &r.by_task {
            *by_task.entry(k.clone()).or_default() += v;
            total += v;
        }
        for (src, dst) in [(&r.to_mother, &mut m), (&r.to_spring, &mut s), (&r.to_home, &mut h), (&r.by_age, &mut ages), (&r.mother_state, &mut ms)] {
            for (k, v) in src {
                *dst.entry(k.clone()).or_default() += v;
            }
        }
    }
    println!("\n## heures-enfant (1-4 ans) en soif saturée : {total} sur {child_h} ({:.2} %)", 100.0 * total as f64 / child_h.max(1) as f64);
    println!("\n## par (tâche de l'enfant, tâche de la mère), les 15 premières");
    let mut v: Vec<_> = by_task.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));
    for ((c, mo), n) in v.iter().take(15) {
        println!("  {c:<16} {mo:<18}{:>6.1} %", 100.0 * **n as f64 / total.max(1) as f64);
    }
    for (title, map) in [("âge de l'enfant", &ages), ("état de la mère", &ms), ("distance à la mère", &m), ("distance à la source connue la plus proche", &s), ("distance au foyer", &h)] {
        println!("\n## {title} (km)");
        for (k, n) in map {
            println!("  {k:<10}{:>6.1} %", 100.0 * *n as f64 / total.max(1) as f64);
        }
    }
    let (md, me, ml, mh) = rs.iter().fold((0u64, 0u64, 0.0, 0.0), |a, r| (a.0 + r.mother_days, a.1 + r.mother_empty, a.2 + r.mother_load, a.3 + r.mother_hunger));
    println!(
        "\n## mères d'un enfant de moins de 3 ans, à minuit : {md} jours-mère ; réserves vides {:.2} % ; charge moyenne {:.2} point ; faim moyenne {:.3}",
        100.0 * me as f64 / md.max(1) as f64,
        ml / md.max(1) as f64,
        mh / md.max(1) as f64
    );
    for r in rs {
        println!(
            "  {:<14} réserves vides {:>5.2} % ; charge {:.2} ; faim {:.3}",
            r.label,
            100.0 * r.mother_empty as f64 / r.mother_days.max(1) as f64,
            r.mother_load / r.mother_days.max(1) as f64,
            r.mother_hunger / r.mother_days.max(1) as f64
        );
    }
    println!("\n## morts de soif des 1-4 ans, dernière heure relevée");
    for r in rs {
        for d in &r.deaths {
            println!("  {:<14} {d}", r.label);
        }
    }
}
