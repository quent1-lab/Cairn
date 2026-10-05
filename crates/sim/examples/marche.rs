//! Banc de la **marche** (chantier de la marche, 2026-10-05) : pourquoi les
//! adultes marchent-ils 33 à 40 km par jour (Hadza : 5,8 km pour les femmes,
//! 11,4 pour les hommes, Pontzer et al. 2012) ?
//!
//! Il ne corrige rien. Il ventile, heure par heure, ce que font les adultes :
//! l'activité selon l'heure du jour (le cycle jour/nuit est-il vécu ?), et la
//! tâche qui fait marcher. Scènes et départ d'`acceptation`.
//!
//! Usage :
//!   cargo run --release -p cairn-sim --example marche -- [jours] [jour_de_départ] [seeds] [fils]
//!
//! Défauts : 90 jours, départ d'été (jour 135), seeds 42,7,1337,2024, 2 fils.

use std::collections::BTreeMap;

use cairn_core::{TICKS_PER_DAY, WorldSeed, km_to_tiles};
use cairn_sim::{Activity, Behavior, Demographics, Sex, Sim, fauna, scenario};
use cairn_worldgen::Biome;

const ACTIVITIES: [Activity; 10] = [
    Activity::Sleeping,
    Activity::Idle,
    Activity::Walking,
    Activity::Eating,
    Activity::Drinking,
    Activity::Sheltering,
    Activity::Hunting,
    Activity::Farming,
    Activity::Fighting,
    Activity::Idle, // bouche-trou, jamais compté deux fois (voir `slot`)
];

fn slot(a: Activity) -> usize {
    ACTIVITIES.iter().position(|x| *x == a).unwrap_or(1)
}

#[derive(Default, Clone)]
struct Report {
    label: String,
    /// [sexe][heure][activité] : heures-adulte.
    by_hour: Vec<Vec<[u64; 10]>>,
    /// [sexe] tâche → heures de marche.
    walk_task: [BTreeMap<String, u64>; 2],
    adult_h: [u64; 2],
    /// Régime : latitude du foyer (°), heures de jour au départ et à la fin.
    lat: f64,
    day_hours: (usize, usize),
    /// Heures-adulte dans le noir (lumière nulle au foyer) ; dont endormis.
    dark_h: u64,
    dark_sleep_h: u64,
    /// Heures-adulte de sommeil, dont dans le noir.
    sleep_h: u64,
    /// Chasse : têtes prélevées par les humains ; jours-adulte passés au moins
    /// une heure à chasser (quête, piste, approche) ; jours-adulte vécus.
    kills: f32,
    hunter_days: u64,
    adult_days: u64,
    /// Gibier : troupeaux à moins de 10 km du foyer au départ et à la fin ;
    /// déplacement net médian (km) des troupeaux présents du début à la fin.
    herds_near: (usize, usize),
    herd_net_km: f64,
    /// Herbivores, toutes têtes, au départ et à la fin (règle 5 : la faune).
    herbivores: (f32, f32),
}

fn herds_at(sim: &Sim) -> BTreeMap<u64, (f64, f64)> {
    sim.fauna
        .query::<(&cairn_sim::FaunaId, &cairn_sim::Herd, &cairn_sim::Position)>()
        .iter()
        .map(|(_, (id, _, p))| (id.0, (p.x, p.y)))
        .collect()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let days: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(90);
    let start_day: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(135);
    let seeds: Vec<u64> = args
        .next()
        .map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect())
        .unwrap_or_else(|| vec![42, 7, 1337, 2024]);
    let threads: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(2).max(1);
    let mut jobs = Vec::new();
    for &cold in &[false, true] {
        for &seed in &seeds {
            jobs.push((seed, cold));
        }
    }
    println!("# marche — {days} jours, départ au jour {start_day}, seeds {seeds:?}, {} runs", jobs.len());
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
    let reports: Vec<Report> = results.into_iter().map(|(_, r)| r).collect();
    print(&reports);
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
        let (x, y) = (
            home.0 + (angle.cos() * km_to_tiles(4.0)) as i64,
            home.1 + (angle.sin() * km_to_tiles(4.0)) as i64,
        );
        if sim.world.tile(x, y).is_walkable() {
            sim.spawn_pack(x as f64, y as f64, fauna::PACK_START);
        }
    }
    let day_hours = |sim: &Sim| {
        let day = sim.time.tick / TICKS_PER_DAY * TICKS_PER_DAY;
        (0..TICKS_PER_DAY)
            .filter(|h| sim.climate.sun_elevation_deg(home.1, cairn_core::SimTime { tick: day + h }) > 0.0)
            .count()
    };
    let mut r = Report {
        label: format!("{} {seed}", if cold { "froid" } else { "tempéré" }),
        by_hour: vec![vec![[0; 10]; 24]; 2],
        lat: sim.climate.latitude_deg(home.1),
        ..Report::default()
    };
    let first_day_hours = day_hours(&sim);
    let near = |m: &BTreeMap<u64, (f64, f64)>| {
        m.values().filter(|p| ((p.0 - home.0 as f64).hypot(p.1 - home.1 as f64)) < km_to_tiles(10.0)).count()
    };
    let herds0 = herds_at(&sim);
    r.herbivores.0 = sim.fauna_census().0;
    let mut hunted_today: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
    for _ in 0..days * TICKS_PER_DAY {
        sim.step();
        if sim.time.tick % TICKS_PER_DAY == 0 {
            r.hunter_days += hunted_today.len() as u64;
            hunted_today.clear();
            r.adult_days += sim
                .agents
                .query::<&Demographics>()
                .iter()
                .filter(|(_, d)| d.is_adult(sim.time.tick))
                .count() as u64;
        }
        for (_, (id, demo, behavior)) in sim.agents.query::<(&cairn_sim::AgentId, &Demographics, &Behavior)>().iter() {
            let hunting = behavior.task.is_some_and(|t| {
                matches!(t.kind, cairn_sim::TaskKind::Hunt | cairn_sim::TaskKind::Track | cairn_sim::TaskKind::SeekGame)
            });
            if hunting && demo.is_adult(sim.time.tick) {
                hunted_today.insert(id.0);
            }
        }
        let tick = sim.time.tick;
        let hour = (tick % TICKS_PER_DAY) as usize;
        let dark = sim.climate.light(home.1, sim.time) == 0.0;
        for (_, (demo, behavior)) in sim.agents.query::<(&Demographics, &Behavior)>().iter() {
            if !demo.is_adult(tick) {
                continue;
            }
            let s = usize::from(demo.sex == Sex::Male);
            r.adult_h[s] += 1;
            r.by_hour[s][hour][slot(behavior.activity)] += 1;
            let asleep = behavior.activity == Activity::Sleeping;
            r.sleep_h += u64::from(asleep);
            if dark {
                r.dark_h += 1;
                r.dark_sleep_h += u64::from(asleep);
            }
            if behavior.activity == Activity::Walking {
                let task = behavior.task.map_or("(aucune)".to_string(), |t| {
                    let name = format!("{:?}", t.kind);
                    name.split('(').next().unwrap_or("").to_string()
                });
                let key = if dark { format!("{task} (noir)") } else { task };
                *r.walk_task[s].entry(key).or_insert(0) += 1;
            }
        }
    }
    r.day_hours = (first_day_hours, day_hours(&sim));
    r.kills = sim.hunted_head;
    let herds1 = herds_at(&sim);
    let mut nets: Vec<f64> = herds0
        .iter()
        .filter_map(|(id, a)| herds1.get(id).map(|b| cairn_core::tiles_to_km((a.0 - b.0).hypot(a.1 - b.1))))
        .collect();
    nets.sort_by(f64::total_cmp);
    r.herd_net_km = nets.get(nets.len() / 2).copied().unwrap_or(f64::NAN);
    r.herds_near = (near(&herds0), near(&herds1));
    r.herbivores.1 = sim.fauna_census().0;
    r
}

fn print(rs: &[Report]) {
    // Agrégat de tous les runs : la question est qualitative (que font-ils ?),
    // le détail par run suit pour la règle 7.
    let mut by_hour = vec![vec![[0u64; 10]; 24]; 2];
    let mut tasks: [BTreeMap<String, u64>; 2] = Default::default();
    let mut adult_h = [0u64; 2];
    for r in rs {
        for s in 0..2 {
            adult_h[s] += r.adult_h[s];
            for h in 0..24 {
                for a in 0..10 {
                    by_hour[s][h][a] += r.by_hour[s][h][a];
                }
            }
            for (k, v) in &r.walk_task[s] {
                *tasks[s].entry(k.clone()).or_insert(0) += v;
            }
        }
    }
    let names = ["dort", "repos", "marche", "cueille", "boit", "abri", "chasse", "cultive", "combat"];
    for (s, sex) in ["femmes", "hommes"].iter().enumerate() {
        let per_day = |n: u64| 24.0 * n as f64 / adult_h[s].max(1) as f64;
        println!("\n## {sex} adultes — heures par jour et par adulte, selon l'heure (tous runs)");
        println!("{:>5}{}", "h", names.map(|n| format!("{n:>8}")).join(""));
        for h in 0..24 {
            let total: u64 = by_hour[s][h].iter().take(9).sum();
            println!(
                "{h:>5}{}",
                (0..9).map(|a| format!("{:>7.0}%", 100.0 * by_hour[s][h][a] as f64 / total.max(1) as f64)).collect::<String>()
            );
        }
        let walk: u64 = (0..24).map(|h| by_hour[s][h][2]).sum();
        let sleep: u64 = (0..24).map(|h| by_hour[s][h][0]).sum();
        println!("marche : {:.1} h/j ({:.1} km/j) ; sommeil : {:.1} h/j", per_day(walk), 4.0 * per_day(walk), per_day(sleep));
        println!("marche par tâche (h/j) :");
        let mut v: Vec<_> = tasks[s].iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        for (k, n) in v {
            println!("  {k:<18}{:>6.2}", per_day(*n));
        }
    }
    let (k, hd, ad): (f32, u64, u64) = rs.iter().fold((0.0, 0, 0), |a, r| (a.0 + r.kills, a.1 + r.hunter_days, a.2 + r.adult_days));
    println!(
        "\n## chasse (tous runs) : {k:.0} têtes ; {hd} jours-chasseur sur {ad} jours-adulte ({:.0} %) ; {:.2} % de prise par jour-chasseur (Hadza : 1 à 3 %)",
        100.0 * hd as f64 / ad.max(1) as f64,
        100.0 * f64::from(k) / hd.max(1) as f64
    );
    println!("\n## par run : régime (latitude, heures de jour départ → fin) ; marche km/j femmes ; hommes ; sommeil dans le noir ; noir passé à dormir");
    for r in rs {
        let km = |s: usize| {
            let w: u64 = (0..24).map(|h| r.by_hour[s][h][2]).sum();
            4.0 * 24.0 * w as f64 / r.adult_h[s].max(1) as f64
        };
        println!(
            "  {:<14}{:>5.0}° {:>2} → {:>2} h{:>8.1} ;{:>6.1}{:>8.0} %{:>8.0} %   troupeaux à 10 km {} → {}, déplacement net médian {:.1} km, herbivores {:.0} → {:.0}",
            r.label,
            r.lat,
            r.day_hours.0,
            r.day_hours.1,
            km(0),
            km(1),
            100.0 * r.dark_sleep_h as f64 / r.sleep_h.max(1) as f64,
            100.0 * r.dark_sleep_h as f64 / r.dark_h.max(1) as f64,
            r.herds_near.0,
            r.herds_near.1,
            r.herd_net_km,
            r.herbivores.0,
            r.herbivores.1
        );
    }
}
