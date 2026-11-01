//! Banc de contact (D13, 2026-11-01) : deux foyers de la scène de référence
//! posés à quelques kilomètres, sur un horizon court, pour juger un mécanisme
//! de la violence **par exposition** et non par totaux de monde. Une run par
//! appel, une ligne CSV sur la sortie ; les répliques sont des seeds de monde,
//! comparées variante contre variante (même seed, même monde jusqu'au premier
//! coup).
//!
//! `cargo run --release -p cairn-sim --example contact -- <seed> [jours] [km] [agents par foyer] [tempere|froid] [jour de départ]`
//!
//! Colonnes : l'exposition (heures-paires à 500 m entre membres de clans en
//! tension ≥ 0,4 — elle ne dépend pas de la violence), le jour du premier coup
//! de raid, les coups de raid (dont sur des < 5 ans), les passes de défense,
//! les semaines-paires de clans avec au moins un coup, les morts violentes
//! (< 5 ans, adultes), les morts de < 1 an toutes causes, la population finale.

use std::collections::{BTreeMap, BTreeSet};

use cairn_core::{TICKS_PER_DAY, TICKS_PER_YEAR, WorldSeed, km_to_tiles, tiles_to_km};
use cairn_sim::{AgentId, ClanMembership, DeathCause, Demographics, Position, Sim, scenario};

/// Tension à partir de laquelle un voisin est un ennemi (`brain`).
const TENSION: f32 = 0.4;

/// Le second foyer : une terre avec une source à portée, à `km` du premier,
/// cherchée sur huit directions puis en s'éloignant par pas de 500 m
/// (`scenario::find_home` avance par anneaux de 20 km, trop large ici).
fn near_home(sim: &mut Sim, home: (i64, i64), km: f64) -> (i64, i64) {
    for extra in 0..20 {
        let r = km_to_tiles(km + 0.5 * f64::from(extra));
        for k in 0..8 {
            let a = f64::from(k) * std::f64::consts::FRAC_PI_4;
            let p = (home.0 + (r * a.cos()) as i64, home.1 + (r * a.sin()) as i64);
            if sim.world.worldgen().elevation(p.0, p.1) > 0.0 && sim.world.nearest_spring(p, 4).is_some() {
                return p;
            }
        }
    }
    panic!("pas de second foyer à moins de {} km", km + 10.0);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(42);
    let days: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(180);
    let km: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(3.0);
    let agents: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(30);
    let cold = args.next().is_some_and(|s| s == "froid");
    let start_day: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(135);

    let mut sim = Sim::new(WorldSeed(seed), scenario::REFERENCE_CAPACITY);
    scenario::start_on_day(&mut sim, start_day);
    let home = scenario::reference_home(&mut sim, cold, scenario::reference_seed_point());
    let second = near_home(&mut sim, home, km);
    for h in [home, second] {
        scenario::populate(&mut sim, h, agents, 2);
        scenario::reference_packs(&mut sim, h);
    }
    let d_km = tiles_to_km(((home.0 - second.0) as f64).hypot((home.1 - second.1) as f64));

    let start_tick = sim.time.tick;
    let mut born: BTreeMap<u64, i64> = sim
        .agents
        .query::<(&AgentId, &Demographics)>()
        .iter()
        .map(|(_, (id, d))| (id.0, d.born_tick))
        .collect();
    let (mut seen_births, mut seen_deaths) = (0usize, 0usize);
    let r2 = km_to_tiles(0.5).powi(2);
    let mut exposure: u64 = 0;
    let mut first_raid: Option<u64> = None;
    let mut prev_passes = 0u64;
    let mut raid_weeks: BTreeSet<(u64, u64, u64)> = BTreeSet::new();
    let (mut viol_child, mut viol_adult, mut infant_deaths) = (0u32, 0u32, 0u32);

    for _ in 0..days * TICKS_PER_DAY {
        sim.step();
        let tick = sim.time.tick;
        // Exposition : paires de membres de clans en tension, à 500 m.
        let people: Vec<((f64, f64), cairn_sim::ClanId)> = sim
            .agents
            .query::<(&Position, &ClanMembership)>()
            .iter()
            .filter_map(|(_, (p, m))| Some(((p.x, p.y), m.0?)))
            .collect();
        for (i, &(pa, ca)) in people.iter().enumerate() {
            for &(pb, cb) in &people[i + 1..] {
                if ca != cb
                    && sim.clan_relations.tension_between(ca, cb) >= TENSION
                    && (pa.0 - pb.0).powi(2) + (pa.1 - pb.1).powi(2) <= r2
                {
                    exposure += 1;
                }
            }
        }
        // Coups de raid du tick : la semaine et la paire de clans en tension.
        if sim.violence.raid_passes > prev_passes {
            first_raid.get_or_insert(tick);
            let clans: BTreeSet<u64> = people.iter().map(|p| p.1.0).collect();
            for &a in &clans {
                for &b in clans.range(a + 1..) {
                    if sim.clan_relations.tension_between(cairn_sim::ClanId(a), cairn_sim::ClanId(b)) >= TENSION {
                        raid_weeks.insert((a, b, tick / (7 * TICKS_PER_DAY)));
                    }
                }
            }
            prev_passes = sim.violence.raid_passes;
        }
        for b in &sim.births[seen_births..] {
            born.insert(b.child.0, b.tick as i64);
        }
        seen_births = sim.births.len();
        for d in &sim.deaths[seen_deaths..] {
            let age = born.get(&d.agent.0).map_or(30.0, |&b| (d.tick as i64 - b) as f64 / TICKS_PER_YEAR as f64);
            if age < 1.0 {
                infant_deaths += 1;
            }
            if d.cause == DeathCause::Violence {
                if age < 5.0 {
                    viol_child += 1;
                } else if age >= 15.0 {
                    viol_adult += 1;
                }
            }
        }
        seen_deaths = sim.deaths.len();
    }
    let v = &sim.violence;
    let pop = sim.agents.query::<&AgentId>().iter().count();
    println!("# seed,km,expo_h_paires,premier_coup_j,coups,coups_moins5,defenses,semaines_paires,viol_moins5,viol_adultes,morts_moins1,pop_fin");
    println!(
        "{seed},{d_km:.1},{exposure},{},{},{},{},{},{viol_child},{viol_adult},{infant_deaths},{pop}",
        first_raid.map_or("-".into(), |t| format!("{:.0}", (t - start_tick) as f64 / TICKS_PER_DAY as f64)),
        v.raid_passes,
        v.raid_passes_by_age[0],
        v.defense_passes,
        raid_weeks.len(),
    );
}
