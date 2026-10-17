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
//! - **extinction** de la population ;
//! - **horizon** en années, s'il est donné (5ᵉ argument).
//!
//! Usage : `cargo run --release -p cairn-sim --example longue -- [seed] [tempere|froid] [jour_de_départ] [out.csv] [années]`

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::Write;

use cairn_core::{TICKS_PER_DAY, WorldSeed, km_to_tiles, tiles_to_km};
use cairn_sim::exposure::{Exposure, Exposures};
use cairn_sim::fauna::Herd;
use cairn_sim::social::BOND_THRESHOLD;
use cairn_sim::tech::TechEventKind;
use cairn_sim::{Activity, AgentId, Behavior, ClanEventKind, ClanMembership, DeathCause, Demographics, Physiology, Position, Sim, fauna, scenario};
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
    let horizon_days: Option<u64> = args.next().and_then(|s| s.parse::<u64>().ok()).map(|y| y * 360);

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
    let mut camp_prev: BTreeMap<u64, (f64, f64)> = BTreeMap::new();
    let causes_head: Vec<String> = CAUSES.iter().map(|c| format!("morts_{c:?}")).collect();
    writeln!(
        csv,
        "jour,an,pop,naissances,morts,{},clans,en_clan,taille_med,taille_max,clans_formes,clans_dissous,clans_absorbes,liens_med,tension_max,paires_tendues,faim_moy,faim_p90,expo_feu,expo_cuivre,techs_vivantes,decouvertes,oublis,troupeaux,tetes,meutes,predateurs,mailles_faune,chunks,tps_jour,tps_30j,rss_mo,morts_nourrissons,morts_enfants,age_med_deces_10a,age_med_vivants,disp_med_km,disp_max_km,tailles_clans,chasse_tetes,marche_h_adulte,noir_dormi,troupeaux_10km,foyers_km_jour,foyer_depart_km,inter_clans_km,huttes_chef",
        causes_head.join(",")
    )
    .unwrap();
    println!("# longue — seed {seed}, scène {scene}, départ jour {start_day}, {n_agents} agents ; CSV {out}");
    println!("# arrêt : débit < {STOP_TPS} tps sur {WINDOW_DAYS} j, mémoire > {} Mo, ou extinction", STOP_RSS_KB / 1000);

    let started = std::time::Instant::now();
    let mut window: VecDeque<f64> = VecDeque::new(); // secondes par jour de jeu
    let (mut formed, mut dissolved, mut merged) = (0usize, 0usize, 0usize);
    let mut seen_events = 0usize;
    // Âge au décès : date de naissance des vivants de la veille.
    let mut born: BTreeMap<u64, i64> = BTreeMap::new();
    let mut seen_deaths = 0usize;
    let (mut dead_infants, mut dead_children) = (0usize, 0usize);
    let mut death_ages: VecDeque<(u64, f64)> = VecDeque::new(); // (jour, âge) sur 10 ans
    let mut day = 0u64;
    let reason = loop {
        let t0 = std::time::Instant::now();
        // La chasse, la marche et la nuit (CHA, MAR) : relevées heure par heure.
        let hunted_before = sim.hunted_head;
        let (mut adult_h, mut walk_h, mut dark_h, mut dark_sleep_h) = (0u64, 0u64, 0u64, 0u64);
        for _ in 0..TICKS_PER_DAY {
            sim.step();
            let dark = sim.climate.light(home.1, sim.time) == 0.0;
            for (_, (demo, behavior)) in sim.agents.query::<(&Demographics, &Behavior)>().iter() {
                if !demo.is_adult(sim.time.tick) {
                    continue;
                }
                adult_h += 1;
                walk_h += u64::from(behavior.activity == Activity::Walking);
                if dark {
                    dark_h += 1;
                    dark_sleep_h += u64::from(behavior.activity == Activity::Sleeping);
                }
            }
        }
        let hunted_day = sim.hunted_head - hunted_before;
        let walk_per_adult = walk_h as f64 / (adult_h as f64 / TICKS_PER_DAY as f64).max(1e-9);
        let dark_slept = dark_sleep_h as f64 / dark_h.max(1) as f64;
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

        for d in &sim.deaths[seen_deaths..] {
            if let Some(&b) = born.get(&d.agent.0) {
                let age = (d.tick as i64 - b) as f64 / (360.0 * TICKS_PER_DAY as f64);
                dead_infants += usize::from(age < 3.0);
                dead_children += usize::from(age < 15.0);
                death_ages.push_back((day, age));
            }
        }
        seen_deaths = sim.deaths.len();
        while death_ages.front().is_some_and(|(d, _)| *d + 3600 < day) {
            death_ages.pop_front();
        }
        let mut ages_dead: Vec<f64> = death_ages.iter().map(|(_, a)| *a).collect();
        ages_dead.sort_by(f64::total_cmp);
        born = sim.agents.query::<(&AgentId, &Demographics)>().iter().map(|(_, (id, d))| (id.0, d.born_tick)).collect();
        let mut ages_alive: Vec<f64> =
            sim.agents.query::<&Demographics>().iter().map(|(_, d)| d.age_years(sim.time.tick)).collect();
        ages_alive.sort_by(f64::total_cmp);
        let pts: Vec<(f64, f64)> = sim.agents.query::<&Position>().iter().map(|(_, p)| (p.x, p.y)).collect();
        let n = pts.len().max(1) as f64;
        let c = (pts.iter().map(|p| p.0).sum::<f64>() / n, pts.iter().map(|p| p.1).sum::<f64>() / n);
        let mut dist: Vec<f64> = pts.iter().map(|p| cairn_core::tiles_to_km((p.0 - c.0).hypot(p.1 - c.1))).collect();
        dist.sort_by(f64::total_cmp);
        let med = |v: &[f64]| v.get(v.len() / 2).copied().unwrap_or(0.0);

        let pop = sim.population();
        let mut by_cause: BTreeMap<String, usize> = BTreeMap::new();
        for d in &sim.deaths {
            *by_cause.entry(format!("{:?}", d.cause)).or_default() += 1;
        }
        let causes: Vec<String> =
            CAUSES.iter().map(|c| by_cause.get(&format!("{c:?}")).copied().unwrap_or(0).to_string()).collect();

        let mut sizes: Vec<usize> = sim.clans.iter().map(|c| c.members.len()).collect();
        sizes.sort_unstable();
        // Vraie médiane (moyenne des deux du milieu quand le nombre est pair).
        let size_med = match sizes.len() {
            0 => 0.0,
            n if n % 2 == 1 => sizes[n / 2] as f64,
            n => (sizes[n / 2 - 1] + sizes[n / 2]) as f64 / 2.0,
        };
        let sizes_str: Vec<String> = sizes.iter().map(|x| x.to_string()).collect();
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
        let near_herds = sim
            .fauna
            .query::<(&Herd, &Position)>()
            .iter()
            .filter(|(_, (h, p))| h.anchor.is_none() && (p.x - home.0 as f64).hypot(p.y - home.1 as f64) < km_to_tiles(10.0))
            .count();
        let side = km_to_tiles(ZONE_KM);
        let zones: BTreeSet<(i64, i64)> = sim
            .fauna
            .query::<(&Herd, &Position)>()
            .iter()
            .map(|(_, (_, p))| ((p.x / side).floor() as i64, (p.y / side).floor() as i64))
            .collect();

        // MAR-6b : le chemin des foyers de clan depuis hier (km, somme sur les
        // clans), la distance médiane des foyers au point de départ, la plus
        // petite distance entre deux foyers, et les huttes du chef debout.
        let mut camp_step_km = 0.0;
        for c in &sim.clans {
            if let Some(p) = camp_prev.get(&c.id.0) {
                camp_step_km += tiles_to_km((p.0 - c.home.0).hypot(p.1 - c.home.1));
            }
        }
        camp_prev = sim.clans.iter().map(|c| (c.id.0, c.home)).collect();
        let mut from_start: Vec<f64> = sim
            .clans
            .iter()
            .map(|c| tiles_to_km((c.home.0 - home.0 as f64).hypot(c.home.1 - home.1 as f64)))
            .collect();
        from_start.sort_by(f64::total_cmp);
        let mut inter = f64::NAN;
        for i in 0..sim.clans.len() {
            for j in (i + 1)..sim.clans.len() {
                let (a, b) = (sim.clans[i].home, sim.clans[j].home);
                let d = tiles_to_km((a.0 - b.0).hypot(a.1 - b.1));
                if inter.is_nan() || d < inter {
                    inter = d;
                }
            }
        }
        let chief_huts = sim.structures.iter().filter(|s| s.kind == cairn_sim::StructureKind::ChiefHut).count();
        writeln!(
            csv,
            "{day},{:.3},{pop},{},{},{},{},{in_clan},{},{},{formed},{dissolved},{merged},{},{tension_max:.3},{tense},{hunger_mean:.3},{hunger_p90:.3},{fire},{copper},{},{disc},{forgot},{herds},{heads:.0},{packs},{predators:.0},{},{},{tps_day:.1},{tps_win:.1},{},{dead_infants},{dead_children},{:.1},{:.1},{:.2},{:.2},{},{hunted_day:.2},{walk_per_adult:.2},{dark_slept:.3},{near_herds},{camp_step_km:.3},{:.2},{inter:.2},{chief_huts}",
            day as f64 / 360.0,
            sim.births.len(),
            sim.deaths.len(),
            causes.join(","),
            sim.clans.len(),
            size_med,
            sizes.last().copied().unwrap_or(0),
            links.get(links.len() / 2).copied().unwrap_or(0),
            sim.known_techs.len(),
            zones.len(),
            sim.world.loaded(),
            rss_kb / 1000,
            med(&ages_dead),
            med(&ages_alive),
            med(&dist),
            dist.last().copied().unwrap_or(0.0),
            sizes_str.join(";"),
            from_start.get(from_start.len() / 2).copied().unwrap_or(f64::NAN),
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
        if horizon_days.is_some_and(|h| day >= h) {
            break "horizon atteint";
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
