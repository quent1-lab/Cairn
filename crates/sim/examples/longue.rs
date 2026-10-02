//! Banc **long, sans horizon** : la scène du banc `acceptation`, lancée jusqu'à
//! ce qu'elle cale. Il sert trois fois à la fois : valider les chantiers faits
//! dans la durée, voir venir les suivants, suivre le débit et la mémoire.
//!
//! Lecture seule : rien de ce qu'il relève ne touche la trajectoire.
//!
//! Une ligne CSV par jour de jeu, vidée à chaque ligne (un arrêt brutal ne perd
//! rien), et un résumé lisible par année sur la sortie standard.
//!
//! Arrêts (aucun ne dépend de la simulation, seulement de la machine) :
//! - **débit** sous `STOP_TPS` sur les 30 derniers jours de jeu — à 1 tps, une
//!   année prend 2,4 h : la dérive est alors acquise, rien de plus à apprendre ;
//! - **mémoire** au-delà de `STOP_RSS_KB` (la machine a 2,9 Go, deux runs) ;
//! - **extinction** de la population.
//!
//! Usage : `cargo run --release -p cairn-sim --example longue -- [seed] [tempere|froid] [jour_de_départ] [out.csv]`

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::Write;

use cairn_core::{TICKS_PER_DAY, WorldSeed, km_to_tiles};
use cairn_sim::exposure::{Exposure, Exposures};
use cairn_sim::fauna::Herd;
use cairn_sim::social::BOND_THRESHOLD;
use cairn_sim::tech::TechEventKind;
use cairn_sim::{ClanEventKind, ClanMembership, DeathCause, Physiology, Position, Sim, fauna, scenario};
use cairn_worldgen::Biome;

const STOP_TPS: f64 = 1.0;
const STOP_RSS_KB: u64 = 1_400_000;
const WINDOW_DAYS: usize = 30;
/// Côté des mailles qui comptent l'aire occupée par la faune (comme `derive`).
const ZONE_KM: f64 = 2.0;

const CAUSES: [DeathCause; 8] = [
    DeathCause::Starvation,
    DeathCause::Dehydration,
    DeathCause::Hypothermia,
    DeathCause::OldAge,
    DeathCause::Predation,
    DeathCause::Disease,
    DeathCause::Lightning,
    DeathCause::Violence,
];

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(42);
    let cold = args.next().is_some_and(|s| s == "froid");
    let start_day: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(135);
    let scene = if cold { "froid" } else { "tempere" };
    let out = args.next().unwrap_or_else(|| format!("out/longue_{scene}_{seed}.csv"));

    // Même scène que `acceptation`.
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

    let mut csv = std::fs::File::create(&out).expect("fichier CSV");
    let causes_head: Vec<String> = CAUSES.iter().map(|c| format!("morts_{c:?}")).collect();
    writeln!(
        csv,
        "jour,an,pop,naissances,morts,{},clans,en_clan,taille_med,taille_max,clans_formes,clans_dissous,clans_absorbes,liens_med,tension_max,paires_tendues,faim_moy,faim_p90,expo_feu,expo_cuivre,techs_vivantes,decouvertes,oublis,troupeaux,tetes,meutes,predateurs,mailles_faune,chunks,tps_jour,tps_30j,rss_mo",
        causes_head.join(",")
    )
    .unwrap();
    println!("# longue — seed {seed}, scène {scene}, départ jour {start_day}, {n_agents} agents ; CSV {out}");
    println!("# arrêt : débit < {STOP_TPS} tps sur {WINDOW_DAYS} j, mémoire > {} Mo, ou extinction", STOP_RSS_KB / 1000);

    let started = std::time::Instant::now();
    let mut window: VecDeque<f64> = VecDeque::new(); // secondes par jour de jeu
    let (mut formed, mut dissolved, mut merged) = (0usize, 0usize, 0usize);
    let mut seen_events = 0usize;
    let mut day = 0u64;
    let reason = loop {
        let t0 = std::time::Instant::now();
        for _ in 0..TICKS_PER_DAY {
            sim.step();
        }
        day += 1;
        let secs = t0.elapsed().as_secs_f64();
        window.push_back(secs);
        if window.len() > WINDOW_DAYS {
            window.pop_front();
        }
        let tps_day = TICKS_PER_DAY as f64 / secs.max(1e-9);
        let tps_win = (window.len() as f64 * TICKS_PER_DAY as f64) / window.iter().sum::<f64>().max(1e-9);
        let rss_kb = rss_kb();

        for e in &sim.clan_events[seen_events..] {
            match e.kind {
                ClanEventKind::Formed => formed += 1,
                ClanEventKind::Dissolved => dissolved += 1,
                ClanEventKind::Merged { .. } => merged += 1,
            }
        }
        seen_events = sim.clan_events.len();

        let pop = sim.population();
        let mut by_cause: BTreeMap<String, usize> = BTreeMap::new();
        for d in &sim.deaths {
            *by_cause.entry(format!("{:?}", d.cause)).or_default() += 1;
        }
        let causes: Vec<String> =
            CAUSES.iter().map(|c| by_cause.get(&format!("{c:?}")).copied().unwrap_or(0).to_string()).collect();

        let mut sizes: Vec<usize> = sim.clans.iter().map(|c| c.members.len()).collect();
        sizes.sort_unstable();
        let in_clan = sim.agents.query::<&ClanMembership>().iter().filter(|(_, m)| m.0.is_some()).count();
        let mut degree: BTreeMap<u64, usize> = BTreeMap::new();
        for (&(a, b), &w) in &sim.social.bonds {
            if w >= BOND_THRESHOLD {
                *degree.entry(a).or_default() += 1;
                *degree.entry(b).or_default() += 1;
            }
        }
        let mut links: Vec<usize> = sim
            .clans
            .iter()
            .flat_map(|c| c.members.iter().map(|m| degree.get(&m.0).copied().unwrap_or(0)))
            .collect();
        links.sort_unstable();
        let (mut tension_max, mut tense) = (0.0f32, 0usize);
        for i in 0..sim.clans.len() {
            for j in (i + 1)..sim.clans.len() {
                let t = sim.clan_relations.tension_between(sim.clans[i].id, sim.clans[j].id);
                tension_max = tension_max.max(t);
                tense += usize::from(t >= 0.4);
            }
        }
        let mut hunger: Vec<f32> = sim.agents.query::<&Physiology>().iter().map(|(_, p)| p.hunger).collect();
        hunger.sort_by(f32::total_cmp);
        let hunger_mean = hunger.iter().sum::<f32>() / hunger.len().max(1) as f32;
        let hunger_p90 = hunger.get(hunger.len() * 9 / 10).copied().unwrap_or(0.0);
        let (mut fire, mut copper) = (0usize, 0usize);
        for (_, e) in sim.agents.query::<&Exposures>().iter() {
            fire += usize::from(e.has(Exposure::Fire));
            copper += usize::from(e.has(Exposure::Copper));
        }
        let (mut disc, mut forgot) = (0usize, 0usize);
        for e in &sim.tech_events {
            match e.kind {
                TechEventKind::Discovered => disc += 1,
                TechEventKind::Forgotten => forgot += 1,
            }
        }
        let (heads, predators, herds, packs) = sim.fauna_census();
        let side = km_to_tiles(ZONE_KM);
        let zones: BTreeSet<(i64, i64)> = sim
            .fauna
            .query::<(&Herd, &Position)>()
            .iter()
            .map(|(_, (_, p))| ((p.x / side).floor() as i64, (p.y / side).floor() as i64))
            .collect();

        writeln!(
            csv,
            "{day},{:.3},{pop},{},{},{},{},{in_clan},{},{},{formed},{dissolved},{merged},{},{tension_max:.3},{tense},{hunger_mean:.3},{hunger_p90:.3},{fire},{copper},{},{disc},{forgot},{herds},{heads:.0},{packs},{predators:.0},{},{},{tps_day:.1},{tps_win:.1},{}",
            day as f64 / 360.0,
            sim.births.len(),
            sim.deaths.len(),
            causes.join(","),
            sim.clans.len(),
            sizes.get(sizes.len() / 2).copied().unwrap_or(0),
            sizes.last().copied().unwrap_or(0),
            links.get(links.len() / 2).copied().unwrap_or(0),
            sim.known_techs.len(),
            zones.len(),
            sim.world.loaded(),
            rss_kb / 1000,
        )
        .unwrap();
        csv.flush().unwrap();

        if day % 360 == 0 {
            let techs: Vec<String> =
                sim.known_techs.iter().map(|t| sim.tech_tree.get(*t).name.clone()).collect();
            println!(
                "an {:>3} · {:>5.1} h · pop {pop} (nés {}, morts {}) · clans {} (méd. {}) · techs {:?} · faune {herds} trp / {heads:.0} têtes, {packs} meutes · tps 30 j {tps_win:.1} · {} Mo",
                day / 360,
                started.elapsed().as_secs_f64() / 3600.0,
                sim.births.len(),
                sim.deaths.len(),
                sim.clans.len(),
                sizes.get(sizes.len() / 2).copied().unwrap_or(0),
                techs,
                rss_kb / 1000,
            );
        }
        if pop == 0 {
            break "extinction de la population";
        }
        if window.len() == WINDOW_DAYS && tps_win < STOP_TPS {
            break "débit sous le seuil";
        }
        if rss_kb > STOP_RSS_KB {
            break "mémoire au-delà du seuil";
        }
    };
    println!(
        "ARRÊT au jour {day} (an {:.2}) après {:.1} h : {reason}",
        day as f64 / 360.0,
        started.elapsed().as_secs_f64() / 3600.0
    );
}

/// Mémoire résidente du processus, en ko (`/proc/self/status`, Linux).
fn rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS"))
                .and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse().ok()))
        })
        .unwrap_or(0)
}
