//! Banc d'**analyse long terme** : lâche une population dans un foyer tempéré
//! et simule des décennies en journalisant *tout* ce qui est mesurable, un
//! échantillon par jour (ou tous les `sample_days`), dans un **CSV large** —
//! de quoi analyser après coup la calibration et la cohérence des
//! comportements (démographie, physiologie, faune, clans, structures,
//! tensions, dispersion, dérive génétique et culturelle…).
//!
//! Usage :
//!   cargo run --release -p cairn-sim --example chronicle -- \
//!       [seed] [années] [sample_days] [out.csv] [capacité_chunks] [agents]
//!
//! Défauts : seed 42, 50 ans, 1 jour d'échantillon, out/chronicle_<seed>.csv,
//! capacité 16384 chunks, 40 agents. Le CSV est vidé à chaque échantillon :
//! une exécution interrompue reste analysable jusqu'où elle est allée.

use std::io::Write;

use cairn_core::{TICKS_PER_DAY, WorldSeed};
use cairn_sim::{
    ClanMembership, DeathCause, Demographics, Memory, Physiology, Position, Sim, Skills,
    StructureKind, Traits, fauna, scenario,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(42);
    let years: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(50);
    let sample_days: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1).max(1);
    let out_path: String =
        args.next().unwrap_or_else(|| format!("out/chronicle_{seed}.csv"));
    let capacity: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(16384);
    let n_agents: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(40);

    // — Scène tempérée (prairie/forêt tempérée, 6–18 °C), même amorçage que le
    //   client (`scenario`), plus quelques meutes de prédateurs. —
    let mut sim = Sim::new(WorldSeed(seed), capacity);
    let seed_point =
        (cairn_core::km_to_tiles(1500.0) as i64, cairn_core::km_to_tiles(2100.0) as i64);
    let home = scenario::find_home(&mut sim, seed_point);
    let placed = scenario::populate(&mut sim, home, n_agents, 2);
    for i in 0..4i64 {
        let angle = i as f64 * 1.57;
        let (x, y) = (
            home.0 + (angle.cos() * cairn_core::km_to_tiles(4.0)) as i64,
            home.1 + (angle.sin() * cairn_core::km_to_tiles(4.0)) as i64,
        );
        if sim.world.tile(x, y).is_walkable() {
            sim.spawn_pack(x as f64, y as f64, fauna::PACK_START);
        }
    }
    let tile = sim.world.tile(home.0, home.1);
    eprintln!(
        "Foyer ({}, {}) — {:?}, {:.1} °C moy. — {placed} agents, seed {seed}, {years} ans, \
         échantillon /{sample_days} j → {out_path}",
        home.0, home.1, tile.biome, tile.temperature,
    );

    std::fs::create_dir_all("out").ok();
    let file = std::fs::File::create(&out_path).expect("création du CSV");
    let mut csv = std::io::BufWriter::new(file);
    writeln!(csv, "{}", HEADER.join(",")).expect("en-tête");

    // — Compteurs cumulés lus incrémentalement (queue des vecteurs), pour ne
    //   pas re-scanner tout l'historique à chaque échantillon. —
    let mut acc = Cumulative::default();
    let total_ticks = years * cairn_core::TICKS_PER_YEAR;
    let started = std::time::Instant::now();
    let mut window_start = started;
    let mut window_ticks = 0u64;

    for _ in 0..total_ticks {
        sim.step();
        window_ticks += 1;
        if !sim.time.tick.is_multiple_of(TICKS_PER_DAY) {
            continue;
        }
        let day = sim.time.tick / TICKS_PER_DAY;
        acc.ingest(&sim); // tenir les compteurs cumulés à jour chaque jour
        if !day.is_multiple_of(sample_days) {
            continue;
        }
        let tps = window_ticks as f64 / window_start.elapsed().as_secs_f64().max(1e-6);
        let row = sample_row(&mut sim, &acc, tps);
        writeln!(csv, "{}", row.join(",")).expect("ligne");
        csv.flush().ok();
        window_start = std::time::Instant::now();
        window_ticks = 0;
        // Progression annuelle sur stderr (le CSV, lui, reste pur).
        if day.is_multiple_of(360) {
            eprintln!(
                "an {:>2} — pop {:>4}, gibier {:>5.0}, préd {:>4.0}, clans {:>2}, struct {:>2}, \
                 {tps:>4.1} tps, {:.0}s écoulées",
                day / 360,
                sim.population(),
                sim.fauna_census().0,
                sim.fauna_census().1,
                sim.clans.len(),
                sim.structures.len(),
                started.elapsed().as_secs_f64(),
            );
        }
    }
    eprintln!("Terminé : {} lignes, {:.0}s.", HEADER.len(), started.elapsed().as_secs_f64());
}

/// Compteurs cumulés tenus à jour incrémentalement (on ne relit que la queue
/// neuve des vecteurs d'historique de la sim).
#[derive(Default)]
struct Cumulative {
    seen_deaths: usize,
    seen_births: usize,
    seen_events: usize,
    d_starv: u64,
    d_dehyd: u64,
    d_hypo: u64,
    d_old: u64,
    clans_formed: u64,
    clans_dissolved: u64,
}

impl Cumulative {
    fn ingest(&mut self, sim: &Sim) {
        for d in &sim.deaths[self.seen_deaths..] {
            match d.cause {
                DeathCause::Starvation => self.d_starv += 1,
                DeathCause::Dehydration => self.d_dehyd += 1,
                DeathCause::Hypothermia => self.d_hypo += 1,
                DeathCause::OldAge => self.d_old += 1,
                DeathCause::Predation => {} // non ventilée dans ce banc (à ajouter au besoin)
            }
        }
        self.seen_deaths = sim.deaths.len();
        self.seen_births = sim.births.len();
        for e in &sim.clan_events[self.seen_events..] {
            match e.kind {
                cairn_sim::ClanEventKind::Formed => self.clans_formed += 1,
                cairn_sim::ClanEventKind::Dissolved => self.clans_dissolved += 1,
            }
        }
        self.seen_events = sim.clan_events.len();
    }
}

/// L'ordre des colonnes — HEADER et `sample_row` doivent rester alignés.
const HEADER: &[&str] = &[
    "day", "year", "day_of_year", "felt_temp_c",
    // démographie
    "pop", "infants", "children", "adults", "females", "males", "pregnant",
    "births_cum", "deaths_cum", "deaths_starvation", "deaths_dehydration",
    "deaths_hypothermia", "deaths_oldage", "mean_age_y", "max_age_y",
    // physiologie (moyennes + pires cas)
    "mean_health", "min_health", "distress", "mean_hunger", "max_hunger",
    "mean_thirst", "max_thirst", "mean_fatigue", "mean_cold", "max_cold",
    // génétique (dérive des traits) + culture (compétences)
    "trait_strength", "trait_endurance", "trait_dexterity", "trait_curiosity",
    "trait_sociability", "trait_aggression", "skill_foraging", "skill_hunting",
    "skill_oratory", "known_springs", "known_cells",
    // faune
    "game_head", "herds", "predators", "packs", "hunted_cum",
    // clans
    "clans", "clan_members", "unaffiliated", "clan_size_min", "clan_size_max",
    "clan_size_mean", "clan_stock_total", "clan_stock_mean", "clans_formed_cum",
    "clans_dissolved_cum", "clans_desiring",
    // tensions
    "tension_max", "tension_mean", "tense_pairs",
    // structures
    "structures", "huts", "granaries", "palisades",
    // dispersion / migration
    "spread_w_km", "spread_h_km", "spread_max_km", "spread_mean_km",
    "centroid_x", "centroid_y",
    // monde / perf
    "chunks_generated", "chunks_loaded", "chunks_dirty", "tps",
    // diagnostic dispersion / chef (ancrage aux structures)
    "inter_clan_km", "chief_score_max", "chief_huts",
];

fn sample_row(sim: &mut Sim, acc: &Cumulative, tps: f64) -> Vec<String> {
    let tick = sim.time.tick;

    // — Une passe sur les agents : classes d'âge, sexes, physiologie, traits,
    //   compétences, savoirs, positions. —
    let (mut infants, mut children, mut adults, mut females, mut males, mut pregnant) =
        (0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
    let (mut sum_health, mut min_health, mut distress) = (0.0f64, 1.0f32, 0u32);
    let (mut sum_hunger, mut max_hunger) = (0.0f64, 0.0f32);
    let (mut sum_thirst, mut max_thirst) = (0.0f64, 0.0f32);
    let (mut sum_fatigue, mut sum_cold, mut max_cold) = (0.0f64, 0.0f64, 0.0f32);
    let (mut sum_age, mut max_age) = (0.0f64, 0.0f64);
    let (mut t_str, mut t_end, mut t_dex, mut t_cur, mut t_soc, mut t_agg) =
        (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let (mut s_for, mut s_hun, mut s_ora) = (0.0f64, 0.0f64, 0.0f64);
    let (mut sum_springs, mut sum_cells) = (0.0f64, 0.0f64);
    let mut affiliated = 0u32;
    let mut positions: Vec<(f64, f64)> = Vec::new();

    for (_, (pos, phys, demo, tr, sk, mem, cm)) in sim.agents.query::<(
        &Position,
        &Physiology,
        &Demographics,
        &Traits,
        &Skills,
        &Memory,
        &ClanMembership,
    )>().iter()
    {
        positions.push((pos.x, pos.y));
        if demo.is_infant(tick) {
            infants += 1;
        } else if demo.is_adult(tick) {
            adults += 1;
        } else {
            children += 1;
        }
        match demo.sex {
            cairn_sim::Sex::Female => females += 1,
            cairn_sim::Sex::Male => males += 1,
        }
        if demo.pregnancy.is_some() {
            pregnant += 1;
        }
        sum_health += f64::from(phys.health);
        min_health = min_health.min(phys.health);
        if phys.health < 0.3 {
            distress += 1;
        }
        sum_hunger += f64::from(phys.hunger);
        max_hunger = max_hunger.max(phys.hunger);
        sum_thirst += f64::from(phys.thirst);
        max_thirst = max_thirst.max(phys.thirst);
        sum_fatigue += f64::from(phys.fatigue);
        sum_cold += f64::from(phys.cold);
        max_cold = max_cold.max(phys.cold);
        let age = demo.age_years(tick);
        sum_age += age;
        max_age = max_age.max(age);
        t_str += f64::from(tr.strength);
        t_end += f64::from(tr.endurance);
        t_dex += f64::from(tr.dexterity);
        t_cur += f64::from(tr.curiosity);
        t_soc += f64::from(tr.sociability);
        t_agg += f64::from(tr.aggression);
        s_for += f64::from(sk.foraging);
        s_hun += f64::from(sk.hunting);
        s_ora += f64::from(sk.oratory);
        sum_springs += mem.springs.len() as f64;
        sum_cells += mem.known.len() as f64;
        if cm.0.is_some() {
            affiliated += 1;
        }
    }
    let pop = positions.len();
    let n = pop.max(1) as f64; // garde-fou division (population nulle)

    // — Centroïde, dispersion (boîte englobante + distances). —
    let (mut cx, mut cy, mut minx, mut miny, mut maxx, mut maxy) =
        (0.0, 0.0, f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in &positions {
        cx += x;
        cy += y;
        minx = minx.min(x);
        miny = miny.min(y);
        maxx = maxx.max(x);
        maxy = maxy.max(y);
    }
    cx /= n;
    cy /= n;
    let (mut max_d, mut sum_d) = (0.0f64, 0.0f64);
    for &(x, y) in &positions {
        let d = (x - cx).hypot(y - cy);
        max_d = max_d.max(d);
        sum_d += d;
    }
    let km = cairn_core::tiles_to_km;
    let (spread_w, spread_h, spread_max, spread_mean) = if pop == 0 {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        (km(maxx - minx), km(maxy - miny), km(max_d), km(sum_d / n))
    };

    // — Température ressentie au centroïde (cycle saisonnier vécu). —
    let felt = if pop == 0 {
        0.0
    } else {
        let (ix, iy) = (cx.floor() as i64, cy.floor() as i64);
        let t = sim.world.tile(ix, iy);
        sim.climate.instant(&t, iy, sim.time)
    };

    // — Faune. —
    let (game, predators, herds, packs) = sim.fauna_census();

    // — Clans : tailles, stock, désir. —
    let n_clans = sim.clans.len();
    let clan_members: usize = sim.clans.iter().map(|c| c.members.len()).sum();
    let (mut cmin, mut cmax, mut stock_total) = (usize::MAX, 0usize, 0.0f32);
    let mut desiring = 0u32;
    for c in &sim.clans {
        cmin = cmin.min(c.members.len());
        cmax = cmax.max(c.members.len());
        stock_total += c.stock;
        if c.desired.is_some() {
            desiring += 1;
        }
    }
    if n_clans == 0 {
        cmin = 0;
    }
    let clan_size_mean = if n_clans == 0 { 0.0 } else { clan_members as f64 / n_clans as f64 };
    let stock_mean = if n_clans == 0 { 0.0 } else { stock_total / n_clans as f32 };

    // — Tensions inter-clans. —
    let (mut tmax, mut tsum, mut tense_pairs) = (0.0f32, 0.0f64, 0u32);
    for &t in sim.clan_relations.tension.values() {
        tmax = tmax.max(t);
        tsum += f64::from(t);
        if t > 0.3 {
            tense_pairs += 1;
        }
    }
    let n_pairs = sim.clan_relations.tension.len().max(1) as f64;
    let tension_mean = tsum / n_pairs;

    // — Structures par type. —
    let (mut huts, mut granaries, mut palisades, mut chief_huts) = (0u32, 0u32, 0u32, 0u32);
    for s in &sim.structures {
        match s.kind {
            StructureKind::Hut => huts += 1,
            StructureKind::Granary => granaries += 1,
            StructureKind::Palisade => palisades += 1,
            StructureKind::ChiefHut => chief_huts += 1,
        }
    }

    // — Diagnostic dispersion : distance maximale entre deux foyers de clan
    //   (teste l'hypothèse « les clans s'écartent »). —
    let mut inter_clan = 0.0f64;
    for i in 0..sim.clans.len() {
        for j in (i + 1)..sim.clans.len() {
            let (a, b) = (&sim.clans[i], &sim.clans[j]);
            inter_clan = inter_clan.max((a.home.0 - b.home.0).hypot(a.home.1 - b.home.1));
        }
    }
    let inter_clan_km = km(inter_clan);

    // — Score du chef le plus établi (oratoire × prestige) : pour calibrer le
    //   seuil de la future hutte du chef. —
    let mut score: std::collections::BTreeMap<u64, f32> = std::collections::BTreeMap::new();
    for (_, (id, sk, pr)) in
        sim.agents.query::<(&cairn_sim::AgentId, &Skills, &cairn_sim::agent::Prestige)>().iter()
    {
        score.insert(id.0, sk.oratory * pr.0);
    }
    let chief_score_max = sim
        .clans
        .iter()
        .filter_map(|c| score.get(&c.chief.0).copied())
        .fold(0.0f32, f32::max);

    let m = |sum: f64| sum / n; // moyenne par tête (garde-fou pop nulle)
    vec![
        (sim.time.tick / TICKS_PER_DAY).to_string(),
        sim.time.year().to_string(),
        sim.time.day_of_year().to_string(),
        format!("{felt:.2}"),
        pop.to_string(),
        infants.to_string(),
        children.to_string(),
        adults.to_string(),
        females.to_string(),
        males.to_string(),
        pregnant.to_string(),
        acc.seen_births.to_string(),
        acc.seen_deaths.to_string(),
        acc.d_starv.to_string(),
        acc.d_dehyd.to_string(),
        acc.d_hypo.to_string(),
        acc.d_old.to_string(),
        format!("{:.2}", m(sum_age)),
        format!("{max_age:.2}"),
        format!("{:.3}", m(sum_health)),
        format!("{:.3}", if pop == 0 { 0.0 } else { min_health }),
        distress.to_string(),
        format!("{:.3}", m(sum_hunger)),
        format!("{max_hunger:.3}"),
        format!("{:.3}", m(sum_thirst)),
        format!("{max_thirst:.3}"),
        format!("{:.3}", m(sum_fatigue)),
        format!("{:.3}", m(sum_cold)),
        format!("{max_cold:.3}"),
        format!("{:.3}", m(t_str)),
        format!("{:.3}", m(t_end)),
        format!("{:.3}", m(t_dex)),
        format!("{:.3}", m(t_cur)),
        format!("{:.3}", m(t_soc)),
        format!("{:.3}", m(t_agg)),
        format!("{:.3}", m(s_for)),
        format!("{:.3}", m(s_hun)),
        format!("{:.3}", m(s_ora)),
        format!("{:.2}", m(sum_springs)),
        format!("{:.2}", m(sum_cells)),
        format!("{game:.0}"),
        herds.to_string(),
        format!("{predators:.0}"),
        packs.to_string(),
        format!("{:.0}", sim.hunted_head),
        n_clans.to_string(),
        clan_members.to_string(),
        (pop.saturating_sub(affiliated as usize)).to_string(),
        cmin.to_string(),
        cmax.to_string(),
        format!("{clan_size_mean:.2}"),
        format!("{stock_total:.1}"),
        format!("{stock_mean:.2}"),
        acc.clans_formed.to_string(),
        acc.clans_dissolved.to_string(),
        desiring.to_string(),
        format!("{tmax:.3}"),
        format!("{tension_mean:.3}"),
        tense_pairs.to_string(),
        sim.structures.len().to_string(),
        huts.to_string(),
        granaries.to_string(),
        palisades.to_string(),
        format!("{spread_w:.3}"),
        format!("{spread_h:.3}"),
        format!("{spread_max:.3}"),
        format!("{spread_mean:.3}"),
        format!("{cx:.0}"),
        format!("{cy:.0}"),
        sim.world.generated.to_string(),
        sim.world.loaded().to_string(),
        sim.world.dirty_count().to_string(),
        format!("{tps:.1}"),
        format!("{inter_clan_km:.2}"),
        format!("{chief_score_max:.3}"),
        chief_huts.to_string(),
    ]
}

