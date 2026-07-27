//! Banc de validation de la **Phase 5 « L'ÉTINCELLE »** : lâche une population
//! dans un foyer **frais à vrais hivers** (c'est le froid qui pousse au feu —
//! une scène tempérée « repue » n'inventerait rien, ce qui est correct mais ne
//! montre rien) et observe l'Histoire advenir : découvertes, oublis, âges, et
//! l'apparition (ou non) du bronze au bout de la route de l'étain.
//!
//! Ce n'est pas un CSV d'analyse (voir `chronicle` pour ça) : c'est un rapport
//! **lisible** périodique + un verdict final face aux critères du BRIEF §9 —
//! « le feu découvert par ≥ 1 clan sur 3 », « une chaîne complète observée »,
//! « le bronze jamais sans route ». La validation à 500 ans se lance hors ligne
//! (des dizaines d'heures) ; ce banc sert à voir l'amorce marcher et à régler
//! `BASE_INSIGHT_PER_DAY`.
//!
//! Usage :
//!   cargo run --release -p cairn-sim --example etincelle -- \
//!       [seed] [années] [rapport_tous_les_n_jours] [capacité_chunks] [agents]
//!
//! Défauts : seed 42, 30 ans, rapport tous les 360 j (1 an), 16384 chunks,
//! 60 agents.

use std::collections::BTreeMap;
use std::io::Write;

use cairn_core::{TICKS_PER_DAY, TICKS_PER_YEAR, WorldSeed, km_to_tiles};
use cairn_sim::{Sim, TechEventKind, fauna, scenario};
use cairn_worldgen::Biome;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(42);
    let years: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(30);
    let report_days: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(360).max(1);
    let capacity: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(16384);
    let n_agents: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(60);
    // Foyer explicite optionnel (fx fy) : cibler un lieu précis — p. ex. un
    // district de cuivre froid repéré par l'example `scout` — au lieu de la
    // sélection par climat. C'est ce qui permet un run où le bronze est possible
    // (cuivre sur place + étain à portée d'expédition).
    let foyer: Option<(i64, i64)> = match (args.next(), args.next()) {
        (Some(x), Some(y)) => x.parse::<i64>().ok().zip(y.parse::<i64>().ok()),
        _ => None,
    };
    // Pré-amorçage (banc bronze) : 8ᵉ argument à 1 → les fondateurs arrivent
    // déjà néolithiques (feu + poterie + four), comme un peuple migrant vers
    // une terre de cuivre. La suite de la chaîne (métallurgie, bronze) reste
    // strictement émergente — rien n'est offert au-delà de ces trois savoirs.
    let seeded = args.next().is_some_and(|s| s == "1" || s == "true");

    // — Scène fraîche : forêt/taïga/prairie à 2–8 °C de moyenne (hivers sous 0),
    //   assez rude pour presser (froid → feu) mais pas la taïga glaciale qui
    //   éteint tout (voir le calibrage Phase 2). Repli tempéré si introuvable. —
    let mut sim = Sim::new(WorldSeed(seed), capacity);
    let seed_point = (km_to_tiles(1500.0) as i64, km_to_tiles(2100.0) as i64);
    let home = foyer.unwrap_or_else(|| {
        scenario::find_home_where(
            &mut sim,
            seed_point,
            1.0..=5.0,
            &[Biome::TemperateForest, Biome::Taiga, Biome::Grassland],
        )
        .unwrap_or_else(|| scenario::find_home(&mut sim, seed_point))
    });
    let placed = scenario::populate(&mut sim, home, n_agents, 2);
    if seeded {
        scenario::grant_techs(&mut sim, &["fire_mastery", "pottery", "kiln"]);
        eprintln!("Pré-amorçage : fondateurs néolithiques (feu, poterie, four) — banc bronze.");
    }
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
    eprintln!(
        "Foyer ({}, {}) — {:?}, {:.1} °C moy. — {placed} agents, seed {seed}, {years} ans",
        home.0, home.1, tile.biome, tile.temperature,
    );
    println!(
        "{:>4} {:>4} {:>4}  {:>8}  {:>3}  {:>5} {:>5}  {:>4} {:>4} {:>4} {:>4} {:>4} {:>4}  {:>3}  {}",
        "an", "pop", "clan", "P/N/B", "sav", "froid", "faim",
        "bois", "silx", "argl", "cuiv", "étn", "feu", "exp", "savoirs"
    );

    let total_ticks = years * TICKS_PER_YEAR;
    let started = std::time::Instant::now();
    for _ in 0..total_ticks {
        sim.step();
        if sim.time.tick.is_multiple_of(TICKS_PER_DAY) {
            let day = sim.time.tick / TICKS_PER_DAY;
            if day.is_multiple_of(report_days) {
                report(&sim, day);
            }
        }
    }

    verdict(&sim, started.elapsed());
}

/// Une ligne de rapport : an, population, clans, âges, savoirs vivants, plus des
/// **diagnostics** pour comprendre pourquoi (ou non) l'insight se déclenche :
/// pression de froid maximale des clans, et couverture d'exposition des adultes
/// aux ingrédients du feu (bois, silex, feu vu).
fn report(sim: &Sim, day: u64) {
    use cairn_sim::{Age, Demographics, Exposure, Exposures};
    let (mut p, mut n, mut b) = (0u32, 0u32, 0u32);
    for clan in &sim.clans {
        match sim.clan_age(clan.id) {
            Age::Paleolithic => p += 1,
            Age::Neolithic => n += 1,
            Age::BronzeAge => b += 1,
        }
    }
    let (mut adults, mut wood, mut flint, mut clay, mut copper, mut tin, mut fire_e) =
        (0u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
    for (_, (demo, exp)) in sim.agents.query::<(&Demographics, &Exposures)>().iter() {
        if demo.is_adult(sim.time.tick) {
            adults += 1;
            if exp.has(Exposure::Wood) {
                wood += 1;
            }
            if exp.has(Exposure::Flint) {
                flint += 1;
            }
            if exp.has(Exposure::Clay) {
                clay += 1;
            }
            if exp.has(Exposure::Copper) {
                copper += 1;
            }
            if exp.has(Exposure::Tin) {
                tin += 1;
            }
            if exp.has(Exposure::Fire) {
                fire_e += 1;
            }
        }
    }
    let pct = |x: u32| if adults > 0 { 100 * x / adults } else { 0 };
    let cold_max = sim.clan_pressure.values().map(|p| p.cold).fold(0.0_f32, f32::max);
    let famine_max = sim.clan_pressure.values().map(|p| p.famine).fold(0.0_f32, f32::max);

    let known: Vec<&str> =
        sim.known_techs.iter().map(|&t| sim.tech_tree.get(t).label.as_str()).collect();
    println!(
        "{:>4} {:>4} {:>4}  {:>8}  {:>3}  {:>5.2} {:>5.2}  {:>3}% {:>3}% {:>3}% {:>3}% {:>3}% {:>3}%  {:>3}  {}",
        day / 360,
        sim.population(),
        sim.clans.len(),
        format!("{p}/{n}/{b}"),
        sim.known_techs.len(),
        cold_max,
        famine_max,
        pct(wood),
        pct(flint),
        pct(clay),
        pct(copper),
        pct(tin),
        pct(fire_e),
        sim.expeditions.len(),
        if known.is_empty() { "—".to_string() } else { known.join(", ") },
    );
    // Flush explicite : stdout est bufferisé par blocs quand il est redirigé
    // (un run en tâche de fond) — sans ça, on ne verrait rien avant la fin.
    let _ = std::io::stdout().flush();
}

/// Le verdict final : ce qui a été découvert (et combien de fois), ce qui a été
/// oublié, l'âge le plus avancé atteint, et l'état de la chaîne du bronze.
fn verdict(sim: &Sim, elapsed: std::time::Duration) {
    use cairn_sim::Age;

    // Comptage des événements par tech.
    let mut discovered: BTreeMap<u16, u32> = BTreeMap::new();
    let mut forgotten: BTreeMap<u16, u32> = BTreeMap::new();
    for e in &sim.tech_events {
        match e.kind {
            TechEventKind::Discovered => *discovered.entry(e.tech.0).or_default() += 1,
            TechEventKind::Forgotten => *forgotten.entry(e.tech.0).or_default() += 1,
        }
    }

    println!("\n═══ VERDICT après {:.0} min de calcul ═══", elapsed.as_secs_f64() / 60.0);
    println!("Population finale : {} en {} clans.", sim.population(), sim.clans.len());

    println!("\nHistoire technologique (par ordre de l'arbre) :");
    for tech in sim.tech_tree.iter() {
        let d = discovered.get(&tech.id.0).copied().unwrap_or(0);
        let f = forgotten.get(&tech.id.0).copied().unwrap_or(0);
        let alive = sim.known_techs.contains(&tech.id);
        if d == 0 && f == 0 {
            println!("  · {:<24} jamais découvert", tech.label);
        } else {
            println!(
                "  {} {:<24} découvert ×{d}{}{}",
                if alive { "✓" } else { "✗" },
                tech.label,
                if f > 0 { format!(", oublié ×{f}") } else { String::new() },
                if alive { "" } else { " (perdu)" },
            );
        }
    }

    let max_age = sim
        .clans
        .iter()
        .map(|c| sim.clan_age(c.id))
        .max()
        .unwrap_or(Age::Paleolithic);
    println!("\nÂge le plus avancé encore vivant : {}", max_age.label());

    // Critères du BRIEF (indicatifs sur un run court ; la vraie mesure est à
    // 500 ans, hors ligne).
    let fire = sim.tech_tree.id_of("fire_mastery").map(|id| sim.known_techs.contains(&id)).unwrap_or(false);
    let bronze_ever = sim.tech_tree.id_of("bronze").map(|id| discovered.contains_key(&id.0)).unwrap_or(false);
    println!("\nRepères (indicatifs) :");
    println!("  feu maîtrisé quelque part : {}", if fire { "oui" } else { "pas encore" });
    println!("  expéditions en cours      : {}", sim.expeditions.len());
    println!("  bronze déjà apparu        : {}", if bronze_ever { "OUI" } else { "non" });
}
