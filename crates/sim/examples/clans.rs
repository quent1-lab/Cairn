//! Banc **D2** : pourquoi un clan se dissout, et revient-il ?
//!
//! Lecture seule. À chaque dissolution (`ClanEventKind::Dissolved`), juste
//! après la passe de détection qui l'a prononcée, on rejoue à l'extérieur les
//! portes de `social::detect_clans` sur les anciens membres : graphe des liens
//! ≥ `BOND_THRESHOLD`, composante qui en contient le plus, puis taille et
//! co-résidence. La cause retenue est la première porte fermée :
//!
//! - **morts** : la majorité des anciens membres est morte ;
//! - **sans lien** : la majorité des survivants n'a plus aucun lien ≥ seuil ;
//! - **trop petit** : la composante dominante a moins de 8 membres ;
//! - **dispersé** : moins de 70 % de la composante dans 4,5 km de son centroïde ;
//! - **autre** : la composante passerait ces portes (scission par modularité,
//!   ou groupe absorbé sans majorité).
//!
//! Puis on suit les survivants : combien de jours avant qu'un clan en
//! rassemble au moins la moitié (le même groupe revenu sous un autre nom) ?
//!
//! Usage : `cargo run --release -p cairn-sim --example clans -- [années] [jour_de_départ] [seeds] [fils]`

use std::collections::{BTreeMap, BTreeSet};

use cairn_core::{TICKS_PER_DAY, WorldSeed, km_to_tiles};
use cairn_sim::social::{BOND_THRESHOLD, RESIDENCE_RADIUS_TILES};
use cairn_sim::{ClanEventKind, Sim, fauna, scenario};
use cairn_worldgen::Biome;

const CAUSES: [&str; 5] = ["morts", "sans lien", "trop petit", "dispersé", "autre"];
/// Délai de retour suivi au plus (jours).
const RETURN_HORIZON_DAYS: u64 = 30;

#[derive(Default)]
struct Report {
    label: String,
    formed: usize,
    dissolved: usize,
    merged: usize,
    causes: [usize; 5],
    /// Taille de la composante dominante, pour les « trop petit ».
    small_sizes: BTreeMap<usize, usize>,
    /// Fraction résidente, pour les « dispersé » (somme, pour la moyenne).
    scattered_frac: f64,
    /// Pour les « dispersé » : composante / clan, et fraction résidente des
    /// seuls anciens membres vivants autour de leur propre centroïde.
    scattered_ratio: Vec<f64>,
    scattered_core: Vec<f64>,
    /// Taille du clan à sa dissolution (somme) et son âge en jours.
    size_sum: usize,
    ages: Vec<u64>,
    /// Délai de retour (jours) ; `None` = pas revenu sous l'horizon.
    returns: Vec<Option<u64>>,
    /// Jours-personnes en clan, et part dans un clan de plus de 30 jours.
    member_days: u64,
    member_days_old: u64,
    /// Liens ≥ seuil par membre de clan, et densité interne (paires liées /
    /// paires possibles), relevés chaque jour.
    degree: Vec<f64>,
    density: Vec<f64>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let years: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let start_day: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(135);
    let seeds: Vec<u64> = args
        .next()
        .map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect())
        .unwrap_or_else(|| vec![42, 7, 1337, 2024]);
    let threads: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(2).max(1);
    let mut jobs: Vec<(u64, bool)> = Vec::new();
    for &cold in &[false, true] {
        for &seed in &seeds {
            jobs.push((seed, cold));
        }
    }
    println!("# clans — {years} an(s), départ au jour {start_day}, seeds {seeds:?}, {} runs", jobs.len());
    let started = std::time::Instant::now();
    let queue = std::sync::Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>());
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let job = queue.lock().unwrap().pop();
                let Some((idx, (seed, cold))) = job else { break };
                let r = run(seed, cold, years, start_day);
                eprintln!("  fini : {} ({:.0} s)", r.label, started.elapsed().as_secs_f64());
                results.lock().unwrap().push((idx, r));
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|(i, _)| *i);
    for (_, r) in &results {
        print_report(r);
    }
}

fn run(seed: u64, cold: bool, years: u64, start_day: u64) -> Report {
    // Même scène que le banc `acceptation`.
    let n_agents = if cold { 60 } else { 40 };
    let mut sim = Sim::new(WorldSeed(seed), 16384);
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
    let mut r = Report { label: format!("{} {seed}", if cold { "froid" } else { "tempéré" }), ..Report::default() };
    // Clans de l'heure d'avant : (id, membres, fondé au tick).
    let mut prev: Vec<(u64, Vec<u64>, u64)> = Vec::new();
    let mut seen = 0usize;
    // Survivants à suivre : (tick de dissolution, membres vivants).
    let mut following: Vec<(u64, Vec<u64>)> = Vec::new();

    for _day in 0..years * 360 {
        for _ in 0..TICKS_PER_DAY {
            sim.step();
            if sim.clan_events.len() == seen {
                continue;
            }
            let humans = sim.human_views();
            let alive: BTreeSet<u64> = humans.iter().map(|h| h.id.0).collect();
            let pos: BTreeMap<u64, (f64, f64)> = humans.iter().map(|h| (h.id.0, h.pos)).collect();
            let edges: Vec<(u64, u64)> =
                sim.social.bonds.iter().filter(|&(_, &w)| w >= BOND_THRESHOLD).map(|(&k, _)| k).collect();
            for e in &sim.clan_events[seen..] {
                match e.kind {
                    ClanEventKind::Formed => r.formed += 1,
                    ClanEventKind::Merged { .. } => r.merged += 1,
                    ClanEventKind::Dissolved => {
                        r.dissolved += 1;
                        let Some((_, members, founded)) = prev.iter().find(|(id, ..)| *id == e.clan.0) else {
                            continue;
                        };
                        r.size_sum += members.len();
                        r.ages.push(sim.time.tick.saturating_sub(*founded) / TICKS_PER_DAY);
                        let living: Vec<u64> = members.iter().copied().filter(|m| alive.contains(m)).collect();
                        let cause = classify(members, &living, &edges, &pos, &mut r);
                        r.causes[cause] += 1;
                        if !living.is_empty() {
                            following.push((sim.time.tick, living));
                        }
                    }
                }
            }
            seen = sim.clan_events.len();
            prev = sim
                .clans
                .iter()
                .map(|c| (c.id.0, c.members.iter().map(|m| m.0).collect(), c.founded_tick))
                .collect();
        }
        // Une fois par jour : retours, et jours-personnes en clan.
        let now = sim.time.tick;
        for c in &sim.clans {
            r.member_days += c.members.len() as u64;
            if now.saturating_sub(c.founded_tick) > 30 * TICKS_PER_DAY {
                r.member_days_old += c.members.len() as u64;
            }
        }
        let strong: BTreeSet<(u64, u64)> =
            sim.social.bonds.iter().filter(|&(_, &w)| w >= BOND_THRESHOLD).map(|(&k, _)| k).collect();
        for c in &sim.clans {
            let ids: Vec<u64> = c.members.iter().map(|m| m.0).collect();
            let n = ids.len();
            let mut inside = 0usize;
            for (i, a) in ids.iter().enumerate() {
                for b in &ids[i + 1..] {
                    inside += usize::from(strong.contains(&((*a).min(*b), (*a).max(*b))));
                }
            }
            for a in &ids {
                r.degree.push(strong.iter().filter(|(x, y)| x == a || y == a).count() as f64);
            }
            if n > 1 {
                r.density.push(inside as f64 / (n * (n - 1) / 2) as f64);
            }
        }
        following.retain(|(t, living)| {
            let back = sim.clans.iter().any(|c| {
                c.founded_tick > *t - 1 && {
                    let n = living.iter().filter(|m| c.members.iter().any(|a| a.0 == **m)).count();
                    2 * n >= living.len()
                }
            });
            let days = (now - t) / TICKS_PER_DAY;
            if back {
                r.returns.push(Some(days));
                false
            } else if days >= RETURN_HORIZON_DAYS {
                r.returns.push(None);
                false
            } else {
                true
            }
        });
    }
    r
}

/// La première porte de `detect_clans` qui se ferme sur les anciens membres.
fn classify(
    members: &[u64],
    living: &[u64],
    edges: &[(u64, u64)],
    pos: &BTreeMap<u64, (f64, f64)>,
    r: &mut Report,
) -> usize {
    if 2 * living.len() < members.len() {
        return 0;
    }
    let bonded: BTreeSet<u64> = edges.iter().flat_map(|&(a, b)| [a, b]).collect();
    if 2 * living.iter().filter(|m| bonded.contains(m)).count() < living.len() {
        return 1;
    }
    // Composantes connexes du graphe entier (union-find maison, minuscule).
    let mut parent: BTreeMap<u64, u64> = BTreeMap::new();
    fn find(p: &mut BTreeMap<u64, u64>, x: u64) -> u64 {
        let px = *p.get(&x).unwrap_or(&x);
        if px == x {
            return x;
        }
        let root = find(p, px);
        p.insert(x, root);
        root
    }
    for &(a, b) in edges {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            parent.insert(ra.max(rb), ra.min(rb));
        }
    }
    let mut count: BTreeMap<u64, usize> = BTreeMap::new();
    for &m in living {
        if bonded.contains(&m) {
            *count.entry(find(&mut parent, m)).or_default() += 1;
        }
    }
    let Some((&root, _)) = count.iter().max_by_key(|&(root, n)| (*n, std::cmp::Reverse(*root))) else {
        return 1;
    };
    let component: Vec<u64> =
        bonded.iter().copied().filter(|&x| find(&mut parent, x) == root).collect();
    if component.len() < 8 {
        *r.small_sizes.entry(component.len()).or_default() += 1;
        return 2;
    }
    let pts: Vec<(f64, f64)> = component.iter().filter_map(|id| pos.get(id).copied()).collect();
    let n = pts.len().max(1) as f64;
    let (cx, cy) = pts.iter().fold((0.0, 0.0), |(sx, sy), &(x, y)| (sx + x / n, sy + y / n));
    let frac = pts.iter().filter(|&&(x, y)| (x - cx).hypot(y - cy) <= RESIDENCE_RADIUS_TILES).count() as f64 / n;
    if frac < 0.7 {
        r.scattered_frac += frac;
        r.scattered_ratio.push(component.len() as f64 / members.len() as f64);
        r.scattered_core.push(resident_fraction(living, pos));
        return 3;
    }
    4
}

fn resident_fraction(ids: &[u64], pos: &BTreeMap<u64, (f64, f64)>) -> f64 {
    let pts: Vec<(f64, f64)> = ids.iter().filter_map(|id| pos.get(id).copied()).collect();
    let n = pts.len().max(1) as f64;
    let (cx, cy) = pts.iter().fold((0.0, 0.0), |(sx, sy), &(x, y)| (sx + x / n, sy + y / n));
    pts.iter().filter(|&&(x, y)| (x - cx).hypot(y - cy) <= RESIDENCE_RADIUS_TILES).count() as f64 / n
}

fn median(v: &[f64]) -> f64 {
    let mut v = v.to_vec();
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(f64::NAN)
}

fn print_report(r: &Report) {
    let d = r.dissolved.max(1) as f64;
    println!("\n## {}", r.label);
    println!("  clans formés {} · dissous {} · absorbés {}", r.formed, r.dissolved, r.merged);
    if r.dissolved > 0 {
        let causes: Vec<String> = CAUSES
            .iter()
            .zip(r.causes)
            .map(|(name, n)| format!("{name} {n} ({:.0} %)", 100.0 * n as f64 / d))
            .collect();
        println!("  causes : {}", causes.join(" · "));
        println!("  taille de la composante (trop petit) : {:?}", r.small_sizes);
        if r.causes[3] > 0 {
            println!("  fraction résidente moyenne (dispersé) : {:.2}", r.scattered_frac / r.causes[3] as f64);
            let core_ok = r.scattered_core.iter().filter(|&&f| f >= 0.7).count();
            println!(
                "  dispersé : composante/clan médian {:.2} (max {:.1}) ; anciens membres seuls résidents (≥ 0,7) {}/{} ; leur fraction médiane {:.2}",
                median(&r.scattered_ratio),
                r.scattered_ratio.iter().copied().fold(0.0, f64::max),
                core_ok,
                r.scattered_core.len(),
                median(&r.scattered_core)
            );
        }
        let mut ages = r.ages.clone();
        ages.sort_unstable();
        let q = |p: f64| ages.get(((ages.len() as f64 - 1.0) * p).round() as usize).copied().unwrap_or(0);
        println!(
            "  clan dissous : {:.1} membres en moyenne ; âge (j) médian {} · q90 {} · max {}",
            r.size_sum as f64 / d,
            q(0.5),
            q(0.9),
            q(1.0)
        );
        let n = r.returns.len().max(1) as f64;
        let within = |days: u64| r.returns.iter().filter(|x| x.is_some_and(|v| v <= days)).count() as f64 / n;
        println!(
            "  retour du groupe (≥ moitié des survivants dans un clan neuf) : ≤ 1 j {:.0} % · ≤ 3 j {:.0} % · ≤ {RETURN_HORIZON_DAYS} j {:.0} % ({} suivis)",
            100.0 * within(1),
            100.0 * within(3),
            100.0 * within(RETURN_HORIZON_DAYS),
            r.returns.len()
        );
    }
    println!(
        "  membres de clan : liens forts par personne médiane {:.0} ; densité interne médiane {:.2}",
        median(&r.degree),
        median(&r.density)
    );
    println!(
        "  jours-personnes en clan : {} ; dont dans un clan de plus de 30 j : {:.0} %",
        r.member_days,
        100.0 * r.member_days_old as f64 / r.member_days.max(1) as f64
    );
}
