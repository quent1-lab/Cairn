//! Banc d'**acceptation** : rejoue, sur plusieurs seeds et deux scènes, les
//! critères mesurables des phases 2 à 5 du BRIEF (§9), et dit pour chacun ce
//! qu'il en est. Il ne corrige rien ; il sert de juge à chaque chantier, pour
//! ne plus régresser sans le voir.
//!
//! Deux horizons :
//! - **1 an** : contrôle après chaque chantier — phase 2, formation des clans,
//!   tensions, curiosité ;
//! - **5 ans** : jalon avant de reporter un gros chantier — les critères à
//!   événements rares (scission, effondrement, feu) ont alors des cas.
//!
//! Les critères à 100 et 500 ans sont **hors de portée** : le banc en donne une
//! projection (croissance annuelle, feu par clan et par an), jamais un verdict.
//! Le déterminisme, la surchasse, l'échange d'informations et l'inclusion de la
//! carte mentale sont couverts par des tests unitaires.
//!
//! Usage :
//!   cargo run --release -p cairn-sim --example acceptation -- \
//!       [années] [jour_de_départ] [seeds, ex. 42,7,1337,2024] [fils]
//!
//! Défauts : 1 an, départ d'été (jour 135), seeds 42,7,1337,2024, 2 fils.
//! Scènes : tempéré (40 agents, celle de `chronicle`) et froid (60 agents,
//! celle d'`etincelle`).

use std::collections::{BTreeMap, BTreeSet};

use cairn_core::{TICKS_PER_DAY, WorldSeed, km_to_tiles};
use cairn_sim::demography::Traits;
use cairn_sim::tech::TechEventKind;
use cairn_sim::{
    Activity, AgentId, Behavior, ClanEventKind, ClanMembership, Memory, Physiology, Sim, TaskKind,
    fauna, scenario,
};
use cairn_worldgen::Biome;

/// Ce qu'un run rapporte, critère par critère.
#[derive(Default, Debug, Clone)]
struct Report {
    label: String,
    years: f64,
    pop0: usize,
    pop_end: usize,
    /// Population à chaque fin d'année.
    pop_years: Vec<usize>,
    deaths: usize,
    births: usize,
    // Phase 2 : heures où un besoin presse, et celles passées à y répondre.
    thirst_h: u64,
    thirst_ok: u64,
    hunger_h: u64,
    hunger_ok: u64,
    cold_h: u64,
    cold_ok: u64,
    // Les mêmes réponses quand le besoin est bas : le point de comparaison.
    calm_h: u64,
    calm_drink: u64,
    calm_eat: u64,
    calm_shelter: u64,
    // Phase 3 : curiosité contre territoire connu (adultes vivants en fin de run).
    curiosity_corr: f64,
    // Phase 4.
    clans_formed: usize,
    first_clan_day: Option<u64>,
    fissions: usize,
    merges: usize,
    /// Tailles des clans, relevées une fois par jour.
    clan_sizes: Vec<usize>,
    hunger_collapses: usize,
    collapse_survivors: usize,
    collapse_survivors_regrouped: usize,
    tension_max: f32,
    tense_pair_days: u64,
    pair_days: u64,
    // Phase 5.
    discoveries: usize,
    forgotten: usize,
    fire_clans: usize,
    clans_end: usize,
    bronze: usize,
    techs: BTreeSet<String>,
    // Soif : morts de déshydratation, dont enfants, dont affamés (faim au
    // maximum la veille de la mort) ; refus de trajet faute de budget.
    thirst_deaths: usize,
    thirst_children: usize,
    thirst_starving: usize,
    /// Nourrissons (âge d'allaitement) morts de soif : orphelins, mère sans
    /// lait (affamée), autre.
    infant_orphan: usize,
    infant_dry_mother: usize,
    infant_other: usize,
    path_denied: u64,
    // Débit : ticks par seconde murale, année par année (instruments compris).
    tps_years: Vec<f64>,
    // Mortalité par âge : morts par classe d'âge et par cause, jours-personnes
    // vécus dans chaque classe (le dénominateur d'un taux).
    deaths_by_age: [[usize; CAUSES.len()]; AGE_CLASSES.len()],
    person_days: [u64; AGE_CLASSES.len()],
    // Maladie : morts, dont affamés l'heure d'avant (faim > 0,7) ;
    // heures-personnes malades contre vécues, enfants (< 15 ans) et adultes.
    disease_deaths: usize,
    disease_hungry: usize,
    sick_h: [u64; 2],
    lived_h: [u64; 2],
}

/// Classes d'âge du tableau de mortalité : bornes inférieures, en années.
const AGE_CLASSES: [(f64, &str); 6] =
    [(0.0, "<1"), (1.0, "1-4"), (5.0, "5-14"), (15.0, "15-39"), (40.0, "40-59"), (60.0, "60+")];

fn age_class(age_years: f64) -> usize {
    AGE_CLASSES.iter().rposition(|(lo, _)| age_years >= *lo).unwrap_or(0)
}

const CAUSES: [&str; 8] = ["faim", "soif", "froid", "âge", "préd", "mal", "viol", "foud"];

fn cause_index(c: cairn_sim::DeathCause) -> usize {
    use cairn_sim::DeathCause::*;
    match c {
        Starvation => 0,
        Dehydration => 1,
        Hypothermia => 2,
        OldAge => 3,
        Predation => 4,
        Disease => 5,
        Violence => 6,
        Lightning => 7,
    }
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
    println!(
        "# acceptation — {years} an(s), départ au jour {start_day}, seeds {seeds:?}, {} runs, {threads} fil(s)",
        jobs.len()
    );
    let started = std::time::Instant::now();
    let queue = std::sync::Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>());
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let job = queue.lock().unwrap().pop();
                let Some((idx, (seed, cold))) = job else { break };
                let report = run(seed, cold, years, start_day);
                eprintln!("  fini : {} ({:.0} s)", report.label, started.elapsed().as_secs_f64());
                results.lock().unwrap().push((idx, report));
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|(i, _)| *i);
    let reports: Vec<Report> = results.into_iter().map(|(_, r)| r).collect();
    print_table(&reports, years);
}

/// Un run : la scène, puis l'observation heure par heure et jour par jour.
fn run(seed: u64, cold: bool, years: u64, start_day: u64) -> Report {
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

    let mut r = Report {
        label: format!("{} {seed}", if cold { "froid" } else { "tempéré" }),
        years: years as f64,
        pop0: placed,
        ..Report::default()
    };
    let fire = sim.tech_tree.id_of("fire_mastery");
    let bronze = sim.tech_tree.id_of("bronze");
    // Clans de l'heure précédente : (id, membres, faim moyenne).
    let mut prev: Vec<(u64, Vec<u64>, f32)> = Vec::new();
    let mut seen_events = 0usize;
    // Survivants d'un effondrement par la faim, à revoir 30 jours plus tard.
    let mut to_follow: Vec<(u64, Vec<u64>)> = Vec::new();

    // Assoiffés au bord de la mort, à l'heure d'avant : (faim, adulte).
    // (faim, adulte, état du nourrisson : 0 = pas nourrisson, 1 = orphelin,
    // 2 = mère sans lait, 3 = autre).
    let mut parched: BTreeMap<u64, (f32, bool, u8)> = BTreeMap::new();
    let mut seen_deaths = 0usize;
    // Tick de naissance de chaque humain ayant vécu : l'âge au décès.
    let mut born: BTreeMap<u64, i64> = sim
        .agents
        .query::<(&AgentId, &cairn_sim::Demographics)>()
        .iter()
        .map(|(_, (id, d))| (id.0, d.born_tick))
        .collect();
    let mut seen_births = 0usize;
    let mut prev_hunger: BTreeMap<u64, f32> = BTreeMap::new();
    let total_days = years * 360;
    let mut year_started = std::time::Instant::now();
    for day in 1..=total_days {
        for _ in 0..TICKS_PER_DAY {
            sim.step();
            // — Phase 2 : répond-on au besoin qui presse ?
            for (_, (phys, behavior)) in sim.agents.query::<(&Physiology, &Behavior)>().iter() {
                let walking_to = |kinds: &[TaskKind]| {
                    behavior.activity == Activity::Walking
                        && behavior.task.is_some_and(|t| kinds.contains(&t.kind))
                };
                let drinking = behavior.activity == Activity::Drinking || walking_to(&[TaskKind::Drink]);
                let eating = matches!(behavior.activity, Activity::Eating | Activity::Hunting)
                    || walking_to(&[
                        TaskKind::Forage,
                        TaskKind::Hunt,
                        TaskKind::Track,
                        TaskKind::SeekGame,
                        TaskKind::EatFromStock,
                    ]);
                let sheltering = behavior.activity == Activity::Sheltering || walking_to(&[TaskKind::Shelter]);
                if phys.thirst < 0.3 && phys.hunger < 0.3 && phys.cold < 0.1 {
                    r.calm_h += 1;
                    r.calm_drink += u64::from(drinking);
                    r.calm_eat += u64::from(eating);
                    r.calm_shelter += u64::from(sheltering);
                }
                if phys.thirst > 0.7 {
                    r.thirst_h += 1;
                    if behavior.activity == Activity::Drinking || walking_to(&[TaskKind::Drink]) {
                        r.thirst_ok += 1;
                    }
                }
                if phys.hunger > 0.7 {
                    r.hunger_h += 1;
                    if matches!(behavior.activity, Activity::Eating | Activity::Hunting)
                        || walking_to(&[
                            TaskKind::Forage,
                            TaskKind::Hunt,
                            TaskKind::Track,
                            TaskKind::SeekGame,
                            TaskKind::EatFromStock,
                        ])
                    {
                        r.hunger_ok += 1;
                    }
                }
                if phys.cold > 0.5 {
                    r.cold_h += 1;
                    if behavior.activity == Activity::Sheltering || walking_to(&[TaskKind::Shelter]) {
                        r.cold_ok += 1;
                    }
                }
            }
            for b in &sim.births[seen_births..] {
                born.insert(b.child.0, b.tick as i64);
            }
            seen_births = sim.births.len();
            for d in &sim.deaths[seen_deaths..] {
                if let Some(&b) = born.get(&d.agent.0) {
                    let age = (d.tick as i64 - b) as f64 / cairn_core::TICKS_PER_YEAR as f64;
                    r.deaths_by_age[age_class(age)][cause_index(d.cause)] += 1;
                }
                if d.cause == cairn_sim::DeathCause::Disease {
                    r.disease_deaths += 1;
                    r.disease_hungry += usize::from(prev_hunger.get(&d.agent.0).is_some_and(|&h| h > 0.7));
                }
                if d.cause == cairn_sim::DeathCause::Dehydration {
                    r.thirst_deaths += 1;
                    if let Some(&(hunger, adult, infant)) = parched.get(&d.agent.0) {
                        r.thirst_children += usize::from(!adult);
                        r.thirst_starving += usize::from(hunger >= 0.99);
                        match infant {
                            1 => r.infant_orphan += 1,
                            2 => r.infant_dry_mother += 1,
                            3 => r.infant_other += 1,
                            _ => {}
                        }
                    }
                }
            }
            seen_deaths = sim.deaths.len();
            let health_by_id: BTreeMap<u64, f32> = sim
                .agents
                .query::<(&AgentId, &Physiology)>()
                .iter()
                .map(|(_, (id, p))| (id.0, p.health))
                .collect();
            parched = sim
                .agents
                .query::<(&AgentId, &Physiology, &cairn_sim::Demographics, &cairn_sim::Kinship)>()
                .iter()
                .filter(|(_, (_, p, _, _))| p.thirst >= 0.99)
                .map(|(_, (id, p, demo, kin))| {
                    let infant = if !demo.is_infant(sim.time.tick) {
                        0
                    } else {
                        // Mère « sans lait » : réserves sous la moitié (le lait suit la santé).
                        match kin.mother.and_then(|m| health_by_id.get(&m.0)) {
                            None => 1,
                            Some(&h) if h < cairn_sim::demography::MILK_TO_SPARE_HEALTH => 2,
                            Some(_) => 3,
                        }
                    };
                    (id.0, (p.hunger, demo.is_adult(sim.time.tick), infant))
                })
                .collect();
            // — Phase 4 : naissances et morts de clans, lues contre l'heure d'avant.
            let alive_clans: BTreeSet<u64> = sim.clans.iter().map(|c| c.id.0).collect();
            for e in &sim.clan_events[seen_events..] {
                match e.kind {
                    ClanEventKind::Formed => {
                        r.clans_formed += 1;
                        r.first_clan_day.get_or_insert(day);
                        // Scission : la majorité du nouveau clan vient d'un clan
                        // qui existe toujours.
                        if let Some(c) = sim.clans.iter().find(|c| c.id == e.clan) {
                            let parent_alive = prev.iter().any(|(pid, members, _)| {
                                alive_clans.contains(pid)
                                    && *pid != e.clan.0
                                    && c.members.iter().filter(|m| members.contains(&m.0)).count() * 2
                                        > c.members.len()
                            });
                            if parent_alive {
                                r.fissions += 1;
                            }
                        }
                    }
                    ClanEventKind::Merged { .. } => r.merges += 1,
                    ClanEventKind::Dissolved => {
                        if let Some((_, members, hunger)) = prev.iter().find(|(pid, ..)| *pid == e.clan.0)
                            && *hunger > 0.6
                        {
                            r.hunger_collapses += 1;
                            to_follow.push((sim.time.tick + 30 * TICKS_PER_DAY, members.clone()));
                        }
                    }
                }
            }
            seen_events = sim.clan_events.len();
            let hunger_of: BTreeMap<u64, f32> = sim
                .agents
                .query::<(&AgentId, &Physiology)>()
                .iter()
                .map(|(_, (id, p))| (id.0, p.hunger))
                .collect();
            for (_, (demo, ill)) in sim.agents.query::<(&cairn_sim::Demographics, &cairn_sim::Illness)>().iter() {
                let k = usize::from(demo.is_adult(sim.time.tick));
                r.lived_h[k] += 1;
                r.sick_h[k] += u64::from(ill.active.iter().any(Option::is_some));
            }
            prev = sim
                .clans
                .iter()
                .map(|c| {
                    let members: Vec<u64> = c.members.iter().map(|m| m.0).collect();
                    let h = members.iter().filter_map(|m| hunger_of.get(m)).sum::<f32>()
                        / members.len().max(1) as f32;
                    (c.id.0, members, h)
                })
                .collect();
            prev_hunger = hunger_of;
        }
        if day % 360 == 0 {
            r.pop_years.push(sim.population());
            r.tps_years.push((360 * TICKS_PER_DAY) as f64 / year_started.elapsed().as_secs_f64());
            year_started = std::time::Instant::now();
        }
        // Survivants des effondrements, 30 jours après : en clan ou non ?
        let now = sim.time.tick;
        let membership: BTreeMap<u64, bool> = sim
            .agents
            .query::<(&AgentId, &ClanMembership)>()
            .iter()
            .map(|(_, (id, m))| (id.0, m.0.is_some()))
            .collect();
        to_follow.retain(|(due, members)| {
            if now < *due {
                return true;
            }
            for m in members {
                if let Some(&in_clan) = membership.get(m) {
                    r.collapse_survivors += 1;
                    if in_clan {
                        r.collapse_survivors_regrouped += 1;
                    }
                }
            }
            false
        });
        for (_, demo) in sim.agents.query::<&cairn_sim::Demographics>().iter() {
            r.person_days[age_class(demo.age_years(sim.time.tick))] += 1;
        }
        r.clan_sizes.extend(sim.clans.iter().map(|c| c.members.len()));
        // Tensions entre clans, une fois par jour.
        for i in 0..sim.clans.len() {
            for j in (i + 1)..sim.clans.len() {
                let t = sim.clan_relations.tension_between(sim.clans[i].id, sim.clans[j].id);
                r.pair_days += 1;
                r.tension_max = r.tension_max.max(t);
                if t >= 0.4 {
                    r.tense_pair_days += 1;
                }
            }
        }
    }

    r.pop_end = sim.population();
    r.path_denied = sim.path_denied;
    r.deaths = sim.deaths.len();
    r.births = sim.births.len();
    // Phase 3 : curiosité contre territoire connu.
    let mut pts: Vec<(f64, f64)> = Vec::new();
    for (_, (traits, mem, demo)) in
        sim.agents.query::<(&Traits, &Memory, &cairn_sim::Demographics)>().iter()
    {
        if demo.is_adult(sim.time.tick) {
            pts.push((f64::from(traits.curiosity), mem.known.len() as f64));
        }
    }
    r.curiosity_corr = pearson(&pts);
    // Phase 5.
    for e in &sim.tech_events {
        match e.kind {
            TechEventKind::Discovered => {
                r.discoveries += 1;
                r.techs.insert(sim.tech_tree.get(e.tech).name.clone());
                if Some(e.tech) == bronze {
                    r.bronze += 1;
                }
            }
            TechEventKind::Forgotten => r.forgotten += 1,
        }
    }
    r.clans_end = sim.clans.len();
    if let Some(fire) = fire {
        r.fire_clans = sim.clans.iter().filter(|c| sim.clan_corpus(c.id).contains(&fire)).count();
    }
    r
}

fn pearson(pts: &[(f64, f64)]) -> f64 {
    let n = pts.len() as f64;
    if n < 3.0 {
        return f64::NAN;
    }
    let (mx, my) = (pts.iter().map(|p| p.0).sum::<f64>() / n, pts.iter().map(|p| p.1).sum::<f64>() / n);
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (x, y) in pts {
        sxy += (x - mx) * (y - my);
        sxx += (x - mx).powi(2);
        syy += (y - my).powi(2);
    }
    if sxx == 0.0 || syy == 0.0 { f64::NAN } else { sxy / (sxx * syy).sqrt() }
}

fn pct(a: u64, b: u64) -> String {
    if b == 0 { "—".into() } else { format!("{:.0} %", 100.0 * a as f64 / b as f64) }
}

/// Taux de mortalité en pour mille par an ; « — » sans exposition.
fn rate(deaths: usize, person_days: u64) -> String {
    if person_days == 0 { "—".into() } else { format!("{:.0}", 1000.0 * deaths as f64 * 360.0 / person_days as f64) }
}

/// Table de mortalité de tous les runs réunis : taux par classe d'âge, morts
/// par cause, et la probabilité de mourir avant 15 ans qu'impliquent les taux.
/// Les runs se somment : chacun est un tirage, l'agrégat n'efface pas le
/// détail par run imprimé plus haut.
fn print_mortality(rs: &[Report]) {
    println!("\nMortalité, {} runs réunis (régime : scènes du banc, effectifs de quelques centaines)", rs.len());
    println!("{:<8}{:>10}{:>7}{:>8}  {}", "âge", "années-p", "morts", "‰/an", CAUSES.map(|c| format!("{c:>6}")).join(""));
    let mut q15_hazard = 0.0;
    for (k, (_, name)) in AGE_CLASSES.iter().enumerate() {
        let pd: u64 = rs.iter().map(|r| r.person_days[k]).sum();
        let by_cause: Vec<usize> = (0..CAUSES.len()).map(|c| rs.iter().map(|r| r.deaths_by_age[k][c]).sum()).collect();
        let total: usize = by_cause.iter().sum();
        if k < 3 && pd > 0 {
            let width = AGE_CLASSES[k + 1].0 - AGE_CLASSES[k].0;
            q15_hazard += total as f64 * 360.0 / pd as f64 * width;
        }
        println!(
            "{name:<8}{:>10.0}{total:>7}{:>8}  {}",
            pd as f64 / 360.0,
            rate(total, pd),
            by_cause.iter().map(|n| format!("{n:>6}")).collect::<String>()
        );
    }
    println!("mourir avant 15 ans (impliqué par les taux) : {:.0} %", 100.0 * (1.0 - (-q15_hazard).exp()));
}

fn print_table(rs: &[Report], years: u64) {
    let col = |f: &dyn Fn(&Report) -> String| -> String {
        rs.iter().map(|r| format!("{:>12}", f(r))).collect::<Vec<_>>().join("")
    };
    let head = col(&|r| r.label.clone());
    println!("\n{:<44}{head}", "critère");
    let line = |name: &str, f: &dyn Fn(&Report) -> String| println!("{name:<44}{}", col(f));

    println!("— Phase 2 : la vie");
    line("survie (population départ → fin)", &|r| format!("{}→{}", r.pop0, r.pop_end));
    line("mortalité ni nulle ni totale", &|r| {
        if r.pop_end == 0 { "TOTALE".into() } else if r.deaths == 0 { "NULLE".into() } else { format!("{} morts", r.deaths) }
    });
    // Contraste : la réponse quand le besoin presse, contre la même réponse
    // quand tout va bien (un « 30 % » brut ne dit rien sans ce point de comparaison).
    let ratio = |ok: u64, h: u64, calm_ok: u64, calm_h: u64| -> String {
        if h == 0 || calm_h == 0 { return "—".into(); }
        let (a, b) = (ok as f64 / h as f64, calm_ok as f64 / calm_h as f64);
        format!("{:.0}% vs {:.0}%", 100.0 * a, 100.0 * b)
    };
    line("boire : assoiffé vs repu", &|r| ratio(r.thirst_ok, r.thirst_h, r.calm_drink, r.calm_h));
    line("manger/chasser : affamé vs repu", &|r| ratio(r.hunger_ok, r.hunger_h, r.calm_eat, r.calm_h));
    line("s'abriter : transi vs au chaud", &|r| ratio(r.cold_ok, r.cold_h, r.calm_shelter, r.calm_h));
    line("morts de soif (dont enfants ; dont affamés)", &|r| {
        format!("{} ({} ; {})", r.thirst_deaths, r.thirst_children, r.thirst_starving)
    });
    line("…nourrissons : orphelins ; mère sans lait ; autre", &|r| {
        format!("{} ; {} ; {}", r.infant_orphan, r.infant_dry_mother, r.infant_other)
    });
    line("morts de maladie (dont affamés)", &|r| format!("{} ({})", r.disease_deaths, r.disease_hungry));
    line("malades : part du temps, enfants ; adultes", &|r| {
        format!("{} ; {}", pct(r.sick_h[0], r.lived_h[0]), pct(r.sick_h[1], r.lived_h[1]))
    });
    line("trajets refusés faute de budget", &|r| r.path_denied.to_string());
    line("mortalité ‰/an : <1 ; 1-4 ; 5-14", &|r| {
        (0..3).map(|k| rate(r.deaths_by_age[k].iter().sum(), r.person_days[k])).collect::<Vec<_>>().join(";")
    });
    line("mortalité ‰/an : 15-39 ; 40-59 ; 60+", &|r| {
        (3..6).map(|k| rate(r.deaths_by_age[k].iter().sum(), r.person_days[k])).collect::<Vec<_>>().join(";")
    });
    println!("— Phase 3 : le nombre (projection, pas verdict)");
    // Après la cohorte des fondateurs (baby-boom puis allaitement) : de la fin
    // de l'an 2 à la fin du run, si le run dure au moins 5 ans.
    let late = |r: &Report| -> Option<f64> {
        if r.pop_years.len() < 5 || r.pop_end == 0 { return None; }
        let (a, n) = (r.pop_years[1] as f64, (r.pop_years.len() - 2) as f64);
        Some((r.pop_end as f64 / a.max(1.0)).powf(1.0 / n))
    };
    line("croissance après la cohorte (an 2 → fin)", &|r| late(r).map_or("(≥ 5 ans)".into(), |g| format!("{:+.1} %", 100.0 * (g - 1.0))));
    line("croissance annuelle (brute)", &|r| {
        if r.pop_end == 0 { "—".into() } else {
            format!("{:+.1} %", 100.0 * ((r.pop_end as f64 / r.pop0 as f64).powf(1.0 / r.years) - 1.0))
        }
    });
    line("projection à 100 ans (critère : 50 → 300)", &|r| {
        if r.years < 5.0 { "(≥ 5 ans)".into() } else if r.pop_end == 0 { "0".into() } else {
            let g = late(r).unwrap_or(1.0);
            let p = 50.0 * g.powf(100.0);
            if p > 99_999.0 { ">99 999".into() } else { format!("{p:.0}") }
        }
    });
    line("naissances", &|r| r.births.to_string());
    line("curiosité ↔ territoire connu (r)", &|r| format!("{:.2}", r.curiosity_corr));
    println!("— Phase 4 : le clan");
    line("clans formés (1er au jour…)", &|r| {
        format!("{} ({})", r.clans_formed, r.first_clan_day.map_or("—".into(), |d| d.to_string()))
    });
    line("scissions (un clan parent survit)", &|r| r.fissions.to_string());
    line("absorptions (un clan en rejoint un autre)", &|r| r.merges.to_string());
    line("taille des clans (médiane ; max)", &|r| {
        let mut v = r.clan_sizes.clone();
        v.sort_unstable();
        if v.is_empty() { "—".into() } else { format!("{} ; {}", v[v.len() / 2], v[v.len() - 1]) }
    });
    line("effondrements par la faim", &|r| r.hunger_collapses.to_string());
    line("…survivants regroupés à 30 j", &|r| {
        if r.collapse_survivors == 0 { "—".into() } else { format!("{}/{}", r.collapse_survivors_regrouped, r.collapse_survivors) }
    });
    line("tension (max ; jours-paires ≥ 0,4)", &|r| format!("{:.2} ; {}", r.tension_max, pct(r.tense_pair_days, r.pair_days)));
    println!("— Phase 5 : l'étincelle");
    line("découvertes / oublis", &|r| format!("{} / {}", r.discoveries, r.forgotten));
    line("clans qui ont le feu (fin)", &|r| format!("{}/{}", r.fire_clans, r.clans_end));
    line("bronze (doit supposer une route)", &|r| r.bronze.to_string());
    println!("— Débit (objectif : ≥ 20 tps chaque année ; instruments compris, un run par cœur)");
    line("débit plancher annuel (tps)", &|r| {
        r.tps_years.iter().copied().reduce(f64::min).map_or("—".into(), |t| format!("{t:.1}"))
    });
    line("…atteint l'année", &|r| {
        r.tps_years
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(b.1))
            .map_or("—".into(), |(i, _)| (i + 1).to_string())
    });
    line("techniques connues", &|r| r.techs.iter().map(|t| t.chars().take(4).collect::<String>()).collect::<Vec<_>>().join(","));
    let distinct: BTreeSet<(usize, usize, Vec<String>)> =
        rs.iter().map(|r| (r.pop_end, r.clans_formed, r.techs.iter().cloned().collect())).collect();
    println!(
        "\nDeux seeds, deux histoires : {} trajectoires distinctes sur {} runs.\n\
         Couverts par des tests unitaires : déterminisme, surchasse, échange d'informations, carte mentale incluse.\n\
         Hors de portée à {years} an(s) : 50 → 300 en 100 ans, feu chez un clan sur trois en 500 ans, chaîne feu → agriculture.",
        distinct.len(),
        rs.len()
    );
    print_mortality(rs);
    // Pic mémoire du processus entier (tous les fils ensemble).
    if let Ok(status) = std::fs::read_to_string("/proc/self/status")
        && let Some(l) = status.lines().find(|l| l.starts_with("VmHWM"))
    {
        println!("Mémoire, pic du processus (tous fils) : {}", l.trim_start_matches("VmHWM:").trim());
    }
}
