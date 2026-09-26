//! Banc de l'anomalie « la capacité du store change la trajectoire ».
//!
//! La simulation est déterministe bit à bit : si l'éviction d'un chunk était
//! transparente, deux capacités donneraient des mondes identiques. Toute
//! différence prouve le contraire ; ce banc en mesure la **taille** et le
//! **signe**, à deux niveaux.
//!
//! - **Niveau 1 — le store seul.** Vingt brouteurs scriptés, dont le chemin ne
//!   dépend pas de l'état du monde (aucun chaos possible), lisent et écrivent
//!   comme un troupeau ; l'écologie quotidienne tourne sans météo. On compare
//!   tuile à tuile, à la fin, toutes les tuiles écrites.
//! - **Niveau 2 — la simulation sans faune.** Des humains seuls, pas de
//!   troupeau ni de meute, immigration coupée : la cueillette lit la
//!   biomasse, et c'est tout. Premier jour de divergence des positions.
//!
//! ```text
//! cargo run --release -p cairn-sim --example eviction -- [seed] [jours] [petite] [grande] [jours_niveau2]
//! ```

use std::collections::BTreeSet;

use cairn_core::{Pcg32, SimTime, TICKS_PER_DAY, WorldSeed, km_to_tiles, splitmix64};
use cairn_sim::{AgentId, Herd, Pack, Position, Sim, ecology, scenario};

/// Même foyer que `derive` et `chronicle`.
fn home(sim: &mut Sim) -> (i64, i64) {
    let seed_point = (km_to_tiles(1500.0) as i64, km_to_tiles(2100.0) as i64);
    scenario::find_home(sim, seed_point)
}

const GRAZERS: u64 = 20;
/// Pas horaire d'un brouteur : celui d'un troupeau qui paît (~80 m).
const STEP: f64 = 40.0;
/// Demi-côté de la boîte où errent les brouteurs (3 km ⇒ boîte de 6 km,
/// ~2 200 chunks : la grande capacité les tient tous, la petite non).
const BOX: f64 = 1500.0;
const BITE: u8 = 30;

/// Biomasse finale de chaque tuile écrite, dans l'ordre des coordonnées.
type Releve = Vec<((i64, i64), u8)>;

/// Niveau 1. Renvoie la biomasse des tuiles écrites, plus (chunks générés,
/// chunks résidents) avant la comparaison.
fn niveau1(seed: u64, days: u64, cap: usize) -> (Releve, u64, usize) {
    let mut sim = Sim::new(WorldSeed(seed), cap);
    let h = home(&mut sim);
    let mut pos: Vec<(f64, f64)> = (0..GRAZERS)
        .map(|i| {
            let a = i as f64 * 0.7;
            (h.0 as f64 + a.cos() * 800.0, h.1 as f64 + a.sin() * 800.0)
        })
        .collect();
    let mut written: BTreeSet<(i64, i64)> = BTreeSet::new();
    for tick in 0..days * TICKS_PER_DAY {
        for (i, p) in pos.iter_mut().enumerate() {
            // Le chemin ne dépend que de (seed, tick, brouteur) — jamais du
            // monde : les deux capacités voient exactement les mêmes gestes.
            let mut rng = Pcg32::new(seed ^ splitmix64(tick), i as u64);
            let angle = rng.next_f64() * std::f64::consts::TAU;
            p.0 = (p.0 + angle.cos() * STEP).clamp(h.0 as f64 - BOX, h.0 as f64 + BOX);
            p.1 = (p.1 + angle.sin() * STEP).clamp(h.1 as f64 - BOX, h.1 as f64 + BOX);
            let (x, y) = (p.0.floor() as i64, p.1.floor() as i64);
            // Neuf sondes, comme `best_pasture` : des lectures qui chargent.
            for dy in [-20, 0, 20] {
                for dx in [-20, 0, 20] {
                    let _ = sim.world.tile(x + dx, y + dy);
                }
            }
            for (dx, dy) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                let t = sim.world.tile_mut(x + dx, y + dy);
                t.biomass = t.biomass.saturating_sub(BITE);
                written.insert((x + dx, y + dy));
            }
        }
        if tick.is_multiple_of(TICKS_PER_DAY) {
            ecology::daily_regrowth(&mut sim.world, &sim.climate, SimTime { tick }, &[], &[], 1);
        }
    }
    let generated = sim.world.generated;
    let loaded = sim.world.loaded();
    let values = written.iter().map(|&(x, y)| ((x, y), sim.world.tile(x, y).biomass)).collect();
    (values, generated, loaded)
}

/// Niveau 2 : empreinte quotidienne des positions humaines (triées par id),
/// et population, jour par jour.
fn niveau2(seed: u64, days: u64, cap: usize) -> Vec<(u64, usize)> {
    let mut sim = Sim::new(WorldSeed(seed), cap);
    let h = home(&mut sim);
    scenario::populate(&mut sim, h, 40, 0);
    let fauna: Vec<_> = sim
        .fauna
        .query::<&Herd>()
        .iter()
        .map(|(e, _)| e)
        .chain(sim.fauna.query::<&Pack>().iter().map(|(e, _)| e))
        .collect();
    for e in fauna {
        let _ = sim.fauna.despawn(e);
    }
    sim.allow_fauna_immigration = false;
    let mut out = Vec::new();
    for _ in 0..days {
        for _ in 0..TICKS_PER_DAY {
            sim.step();
        }
        let mut v: Vec<(u64, u64, u64)> = sim
            .agents
            .query::<(&AgentId, &Position)>()
            .iter()
            .map(|(_, (id, p))| (id.0, p.x.to_bits(), p.y.to_bits()))
            .collect();
        v.sort_unstable();
        let hash = v.iter().fold(0u64, |acc, &(i, x, y)| {
            splitmix64(acc ^ splitmix64(i ^ splitmix64(x ^ splitmix64(y))))
        });
        out.push((hash, sim.population()));
    }
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(42);
    let days: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(120);
    let small: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(256);
    let large: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(4096);
    let days2: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(60);

    println!("══ NIVEAU 1 — store seul · seed {seed} · {days} j · capacités {small} / {large}");
    let (a, gen_a, loaded_a) = niveau1(seed, days, large);
    println!(
        "  grande capacité : {gen_a} chunks générés, {loaded_a} résidents ⇒ {}",
        if gen_a as usize == loaded_a { "aucune éviction (témoin valide)" } else { "ÉVICTIONS — témoin invalide" }
    );
    let (b, gen_b, loaded_b) = niveau1(seed, days, small);
    println!("  petite capacité : {gen_b} chunks générés, {loaded_b} résidents");
    let n = a.len();
    let (mut differ, mut neg, mut sum_abs, mut max_abs, mut sum_signed) = (0usize, 0usize, 0i64, 0i64, 0i64);
    let mut hist = [0usize; 6]; // |Δ| : 1, 2-6, 7-20, 21-50, 51-100, >100
    for ((_, va), (_, vb)) in a.iter().zip(&b) {
        let d = i64::from(*vb) - i64::from(*va);
        if d != 0 {
            differ += 1;
            if d < 0 {
                neg += 1;
            }
            let ad = d.abs();
            sum_abs += ad;
            sum_signed += d;
            max_abs = max_abs.max(ad);
            let k = match ad {
                1 => 0,
                2..=6 => 1,
                7..=20 => 2,
                21..=50 => 3,
                51..=100 => 4,
                _ => 5,
            };
            hist[k] += 1;
        }
    }
    println!("  tuiles écrites comparées : {n}");
    println!(
        "  tuiles qui diffèrent     : {differ} ({:.2} %)",
        differ as f64 / n.max(1) as f64 * 100.0
    );
    if differ > 0 {
        println!(
            "  dont petite < grande     : {neg} ({:.1} %)  ← repousse manquée si ≫ 50 %",
            neg as f64 / differ as f64 * 100.0
        );
        println!(
            "  |Δ| moyen {:.2} · Δ moyen signé {:+.2} · |Δ| max {max_abs}  (tolérance d'arrondi : 6)",
            sum_abs as f64 / differ as f64,
            sum_signed as f64 / differ as f64
        );
        println!(
            "  |Δ| : 1 → {} · 2-6 → {} · 7-20 → {} · 21-50 → {} · 51-100 → {} · >100 → {}",
            hist[0], hist[1], hist[2], hist[3], hist[4], hist[5]
        );
    }

    if days2 > 0 {
        println!("\n══ NIVEAU 2 — simulation sans faune · 40 humains · {days2} j · capacités 512 / 16384");
        let s = niveau2(seed, days2, 512);
        let l = niveau2(seed, days2, 16384);
        let first = s.iter().zip(&l).position(|(x, y)| x.0 != y.0);
        match first {
            None => println!("  trajectoires IDENTIQUES sur {days2} jours"),
            Some(d) => println!("  première divergence des positions : jour {}", d + 1),
        }
        for &d in &[1u64, 10, 30, 60, 120] {
            if d as usize <= s.len() {
                let i = d as usize - 1;
                println!(
                    "  jour {d:>3} : population {} / {} · positions {}",
                    s[i].1,
                    l[i].1,
                    if s[i].0 == l[i].0 { "identiques" } else { "différentes" }
                );
            }
        }
    }
}
