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
use cairn_sim::demography::{self, Demographics, Kinship, Sex};
use cairn_sim::{AgentId, Behavior, DeathCause, Knowledge, Memory, Physiology, Position, Sim, fauna, food_stats, scenario};
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
    // D11 — la soif : le dernier état connu de chaque assoiffé (soif >= 0,95),
    // pour décrire ceux qui en meurent (l'agent mort n'est plus interrogeable).
    let mut parched: BTreeMap<u64, (String, Option<f64>, f64, (f64, f64), f32)> = BTreeMap::new();
    let mut thirst_deaths: Vec<(String, Option<f64>, f64, (f64, f64), f32)> = Vec::new();
    let mut seen_deaths_t = 0usize;
    // Qui meurt, et à quel âge : l'âge de chaque vivant, relevé chaque heure,
    // sert d'âge au décès (l'agent mort n'est plus interrogeable).
    let mut ages: BTreeMap<u64, f64> = BTreeMap::new();
    let mut deaths_by_age: BTreeMap<(&'static str, String), u32> = BTreeMap::new();
    // Trace heure par heure des assoiffés (soif > 0,7), pour les premiers morts.
    let mut traces: BTreeMap<u64, std::collections::VecDeque<String>> = BTreeMap::new();
    let mut traces_printed = 0;
    // D12 — la natalité : jours-femme féconde par première porte fermée.
    let mut gates: BTreeMap<&'static str, u64> = BTreeMap::new();
    // D13 — la violence : chaque jour, pour chaque paire de clans, le critère de
    // rareté de `social::update_relations` recalculé à part (gibier à 4,5 km du
    // milieu des foyers, par bouche, contre 3 têtes), et la tension.
    // (paires-jours, en contact, rares, tension ≥ 0,4, somme des tensions)
    let mut pairs = (0u64, 0u64, 0u64, 0u64, 0.0f64);
    let mut seen_events = 0usize;
    for day in 1..=total_days {
        if first_clan_day.is_none() && !sim.clans.is_empty() {
            first_clan_day = Some(day - 1);
            println!("      premier clan au jour {} après l'arrivée", day - 1);
        }
        for _ in 0..TICKS_PER_DAY {
            sim.step();
            let tick_now = sim.time.tick;
            for (_, (id, pos, phys, behavior, mem, demo)) in sim
                .agents
                .query::<(&AgentId, &Position, &Physiology, &Behavior, &Memory, &Demographics)>()
                .iter()
            {
                ages.insert(id.0, demo.age_years(tick_now));
                if phys.thirst > 0.7 {
                    let tr = traces.entry(id.0).or_default();
                    let (tk, dist) = behavior.task.map_or(("—".to_string(), -1.0), |t| {
                        (
                            format!("{:?}", t.kind).split('(').next().unwrap_or("").to_string(),
                            (t.target.0 as f64 - pos.x).hypot(t.target.1 as f64 - pos.y) * 2.0,
                        )
                    });
                    let spring = mem.nearest_known_spring((pos.x, pos.y));
                    let water_between = spring.is_some_and(|sp| {
                        let (dx, dy) = (sp.0 as f64 - pos.x, sp.1 as f64 - pos.y);
                        let d = dx.hypot(dy).max(1.0);
                        (0..(d / 8.0) as i64).any(|k| {
                            let f = k as f64 * 8.0 / d;
                            sim.world.worldgen().elevation((pos.x + dx * f) as i64, (pos.y + dy * f) as i64) <= 0.0
                        })
                    });
                    tr.push_back(format!(
                        "h{} {tk} {:?} cible {dist:.0} m soif {:.2} faim {:.2} santé {:.2} fatigue {:.2} | source connue à {} m, eau entre deux : {water_between}, sources connues {}",
                        tick_now % 24, behavior.activity, phys.thirst, phys.hunger, phys.health, phys.fatigue,
                        spring.map_or(-1.0, |sp| (sp.0 as f64 - pos.x).hypot(sp.1 as f64 - pos.y) * 2.0) as i64,
                        mem.springs.len()
                    ));
                    if tr.len() > 48 {
                        tr.pop_front();
                    }
                } else {
                    traces.remove(&id.0);
                }
                if phys.thirst >= 0.95 {
                    let task = behavior.task.map_or("—".to_string(), |t| {
                        format!("{:?}", t.kind).split('(').next().unwrap_or("").to_string()
                    });
                    let known = mem.nearest_known_spring((pos.x, pos.y)).map(|s| {
                        cairn_core::tiles_to_km((s.0 as f64 - pos.x).hypot(s.1 as f64 - pos.y))
                    });
                    parched.insert(id.0, (task, known, demo.age_years(tick_now), (pos.x, pos.y), phys.hunger));
                }
            }
            for d in &sim.deaths[seen_deaths_t..] {
                let age = ages.get(&d.agent.0).copied().unwrap_or(-1.0);
                let class = match age {
                    a if a < 0.0 => "?",
                    a if a < 3.0 => "nourrisson",
                    a if a < 14.0 => "enfant",
                    a if a < 50.0 => "adulte",
                    _ => "âgé",
                };
                *deaths_by_age.entry((class, format!("{:?}", d.cause))).or_insert(0) += 1;
            }
            for d in &sim.deaths[seen_deaths_t..] {
                if d.cause == DeathCause::Dehydration && traces_printed < 3 {
                    traces_printed += 1;
                    println!("      TRACE de l'agent {} (mort de soif) :", d.agent.0);
                    for l in traces.get(&d.agent.0).into_iter().flatten() {
                        println!("        {l}");
                    }
                }
                if d.cause == DeathCause::Dehydration {
                    let snap = parched.remove(&d.agent.0).unwrap_or((
                        "?".to_string(),
                        None,
                        -1.0,
                        (d.pos.0 as f64, d.pos.1 as f64),
                        -1.0,
                    ));
                    thirst_deaths.push(snap);
                }
            }
            seen_deaths_t = sim.deaths.len();
            for (_, (phys, behavior)) in sim.agents.query::<(&Physiology, &Behavior)>().iter() {
                if phys.hunger > 0.5 {
                    // Ce que l'heure a réellement été : l'activité du tick, et pour
                    // une marche, ce vers quoi on marchait. (La tâche relevée après
                    // le tick est vide dès qu'elle s'est achevée dans l'heure : un
                    // premier relevé comptait ainsi « Rien » un tiers du temps.)
                    let kind = match behavior.activity {
                        cairn_sim::Activity::Walking => format!(
                            "Marche→{}",
                            behavior.task.map_or("?".to_string(), |t| {
                                format!("{:?}", t.kind).split('(').next().unwrap_or("").to_string()
                            })
                        ),
                        a => format!("{a:?}"),
                    };
                    *hungry_tasks.entry(kind).or_insert(0) += 1;
                }
            }
        }
        human_days += sim.population() as f64;
        // Les paires de clans, une fois par jour.
        {
            let residence = cairn_sim::social::RESIDENCE_RADIUS_TILES;
            let herds: Vec<(f64, f64, f32)> = sim
                .fauna
                .query::<(&fauna::Herd, &Position)>()
                .iter()
                .map(|(_, (h, p))| (p.x, p.y, h.population))
                .collect();
            for i in 0..sim.clans.len() {
                for j in (i + 1)..sim.clans.len() {
                    let (a, b) = (&sim.clans[i], &sim.clans[j]);
                    pairs.0 += 1;
                    let dist = (a.home.0 - b.home.0).hypot(a.home.1 - b.home.1);
                    let contact = dist <= 2.0 * residence;
                    if contact {
                        pairs.1 += 1;
                        let mid = ((a.home.0 + b.home.0) / 2.0, (a.home.1 + b.home.1) / 2.0);
                        let game: f32 = herds
                            .iter()
                            .filter(|h| (h.0 - mid.0).hypot(h.1 - mid.1) <= residence)
                            .map(|h| h.2)
                            .sum();
                        let mouths = (a.members.len() + b.members.len()).max(1) as f32;
                        if game / mouths < 3.0 {
                            pairs.2 += 1;
                        }
                    }
                    let t = sim.clan_relations.tension_between(a.id, b.id);
                    if t >= 0.4 {
                        pairs.3 += 1;
                    }
                    pairs.4 += f64::from(t);
                }
            }
        }
        // Les portes de la conception, relevées une fois par jour.
        {
            let tick_now = sim.time.tick;
            let mut nursing: std::collections::BTreeSet<u64> = Default::default();
            let mut males: Vec<(f64, f64)> = Vec::new();
            for (_, (pos, phys, demo, kin)) in
                sim.agents.query::<(&Position, &Physiology, &Demographics, &Kinship)>().iter()
            {
                if demo.is_infant(tick_now)
                    && let Some(m) = kin.mother
                {
                    nursing.insert(m.0);
                }
                if demo.sex == Sex::Male
                    && demography::MALE_FERTILE_YEARS.contains(&demo.age_years(tick_now))
                    && phys.health > 0.3
                {
                    males.push((pos.x, pos.y));
                }
            }
            let r2 = demography::MATE_RADIUS_TILES * demography::MATE_RADIUS_TILES;
            for (_, (id, pos, phys, demo)) in
                sim.agents.query::<(&AgentId, &Position, &Physiology, &Demographics)>().iter()
            {
                if demo.sex != Sex::Female
                    || !demography::FEMALE_FERTILE_YEARS.contains(&demo.age_years(tick_now))
                {
                    continue;
                }
                let gate = if demo.pregnancy.is_some() {
                    "enceinte"
                } else if nursing.contains(&id.0) {
                    "allaite"
                } else if phys.health <= 0.6 {
                    "santé"
                } else if phys.hunger >= 0.85 {
                    "faim"
                } else if phys.thirst >= 0.9 {
                    "soif"
                } else if !males.iter().any(|m| (m.0 - pos.x).powi(2) + (m.1 - pos.y).powi(2) <= r2) {
                    "sans homme à 400 m"
                } else {
                    "féconde"
                };
                *gates.entry(gate).or_insert(0) += 1;
            }
        }
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
            .take(7)
            .map(|(v, k)| format!("{k} {:.0}%", 100.0 * *v as f64 / tasks_total.max(1) as f64))
            .collect();
        let pct_of = |k: &str| 100.0 * *hungry_tasks.get(k).unwrap_or(&0) as f64 / tasks_total.max(1) as f64;
        let (track_pct, hunt_pct) = (pct_of("Marche→Track"), pct_of("Hunting") + pct_of("Marche→Hunt"));
        let remembering = sim
            .agents
            .query::<&cairn_sim::Memory>()
            .iter()
            .filter(|(_, m)| m.game.is_some())
            .count();
        hungry_tasks.clear();
        let (fed, ev) = food_stats::take();
        let total: f64 = fed[0] + fed[1] + fed[4] + fed[6] + fed[7] + fed[8];
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
        {
            let total: u64 = gates.values().sum();
            if total > 0 {
                let mut v: Vec<(u64, &str)> = gates.iter().map(|(k, n)| (*n, *k)).collect();
                v.sort_by(|a, b| b.0.cmp(&a.0));
                let txt: Vec<String> =
                    v.iter().map(|(n, k)| format!("{k} {:.0}%", 100.0 * *n as f64 / total as f64)).collect();
                let open = *gates.get("féconde").unwrap_or(&0) as f64;
                println!(
                    "      natalité : {total} jours-femme féconde — {} ; conceptions attendues {:.1}, naissances {births}",
                    txt.join(", "),
                    open * demography::CONCEPTION_DAILY_P
                );
            }
            gates.clear();
            {
                let (mut raids, mut killed, mut mutual) = (0u32, 0usize, 0u32);
                for e in &sim.chronicle[seen_events..] {
                    if let cairn_sim::EventKind::Raid { casualties, mutual: m, .. } = e.kind {
                        raids += 1;
                        killed += casualties;
                        mutual += u32::from(m);
                    }
                }
                seen_events = sim.chronicle.len();
                if pairs.0 > 0 || raids > 0 {
                    println!(
                        "      violence : {} paires-jours de clans, {:.0} % en contact, {:.0} % en contact ET rares, {:.0} % au-dessus du seuil de razzia (tension moyenne {:.2}) ; {raids} affrontements ({mutual} mêlées), {killed} tués",
                        pairs.0,
                        100.0 * pairs.1 as f64 / pairs.0.max(1) as f64,
                        100.0 * pairs.2 as f64 / pairs.0.max(1) as f64,
                        100.0 * pairs.3 as f64 / pairs.0.max(1) as f64,
                        pairs.4 / pairs.0.max(1) as f64,
                    );
                }
                pairs = (0, 0, 0, 0, 0.0);
            }
            if !deaths_by_age.is_empty() {
                let txt: Vec<String> =
                    deaths_by_age.iter().map(|((c, cause), n)| format!("{c} {cause} {n}")).collect();
                println!("      morts par âge : {}", txt.join(", "));
                deaths_by_age.clear();
            }
            if !thirst_deaths.is_empty() {
                let mut known: Vec<f64> = thirst_deaths.iter().filter_map(|t| t.1).collect();
                known.sort_by(f64::total_cmp);
                let never = thirst_deaths.iter().filter(|t| t.1.is_none()).count();
                let kids = thirst_deaths.iter().filter(|t| t.2 >= 0.0 && t.2 < 14.0).count();
                let mut tasks: BTreeMap<&str, u32> = BTreeMap::new();
                for t in &thirst_deaths {
                    *tasks.entry(t.0.as_str()).or_insert(0) += 1;
                }
                let hum: f64 = thirst_deaths
                    .iter()
                    .map(|t| sim.world.worldgen().humidity(t.3.0 as i64, t.3.1 as i64))
                    .sum::<f64>()
                    / thirst_deaths.len() as f64;
                let starving = thirst_deaths.iter().filter(|t| t.4 >= 0.99).count();
                println!("      soif : dont {starving} mourants qui avaient aussi faim à 1,0 (affamés comptés « soif »)");
                println!(
                    "      soif : {} morts — source connue la plus proche : médiane {}, aucune connue {} ; enfants {} ; humidité moyenne {:.2} ; tâches {:?}",
                    thirst_deaths.len(),
                    known.get(known.len() / 2).map_or("—".to_string(), |k| format!("{k:.1} km")),
                    never,
                    kids,
                    hum,
                    tasks
                );
                thirst_deaths.clear();
            }
        }
        if pop > 0 {
            println!(
                "      mangé : {:.2} point de faim par personne et par jour (besoin 0,50) — cueillette {:.2}, chasse {:.2}, porté {:.2}, part {:.2}, stock {:.2}, lait {:.2}",
                total / hd, fed[0] / hd, fed[1] / hd, fed[8] / hd, fed[7] / hd, fed[4] / hd, fed[6] / hd
            );
            println!(
                "      chasse : {} h de chasse, {} h à portée, {} prises — {:.1} % des heures à portée, {:.3} prise par heure de chasse",
                ev[4], ev[5], ev[3],
                100.0 * ev[5] as f64 / ev[4].max(1) as f64,
                ev[3] as f64 / ev[4].max(1) as f64
            );
            println!(
                "      gibier : {track_pct:.1} % des heures d'affamés à pister, {hunt_pct:.1} % à chasser ; {:.0} % se souviennent d'un troupeau",
                100.0 * remembering as f64 / pop as f64
            );
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
