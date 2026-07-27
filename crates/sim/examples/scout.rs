//! Reconnaissance géologique pour préparer un run **bronze** : trouve, pour une
//! seed, des gisements de **cuivre** et leur climat — de quoi choisir un foyer à
//! la fois **froid** (indispensable à la chaîne feu→poterie→four, voir le banc
//! `etincelle`) **et proche du cuivre** (pour la métallurgie), avec la distance
//! à l'**étain** (faisabilité de l'expédition qui, seule, mène au bronze).
//!
//! Purement worldgen (aucune simulation) → quelques secondes.
//!
//! Usage : cargo run --release -p cairn-sim --example scout -- [seed]

use std::collections::BTreeSet;

use cairn_core::{WorldSeed, km_to_tiles, tiles_to_km};
use cairn_sim::{Sim, commerce, scenario};
use cairn_worldgen::{Biome, Deposit, WorldGen};

fn dist(a: (i64, i64), b: (i64, i64)) -> f64 {
    (((a.0 - b.0).pow(2) + (a.1 - b.1).pow(2)) as f64).sqrt()
}

/// Silex le plus proche à résolution **fine** (maille `stride`, jusqu'à `max`).
/// `find_nearest_deposit` cherche au pas de 4 km, calibré pour les districts
/// métalliques compacts — mais le silex est **épars** (affleurements
/// sédimentaires de quelques tuiles), qu'un pas de 4 km enjamberait. Requête
/// géologique pure (aucun chunk matérialisé), anneaux carrés croissants.
fn nearest_flint(wg: &WorldGen, from: (i64, i64), max: i64, stride: i64) -> Option<(i64, i64)> {
    let is_flint = |x: i64, y: i64| {
        let e = wg.elevation(x, y);
        e > 0.0 && wg.deposit(x, y, e) == Deposit::Flint
    };
    if is_flint(from.0, from.1) {
        return Some(from);
    }
    let mut r = stride;
    while r <= max {
        let mut x = -r;
        while x <= r {
            for &y in &[-r, r] {
                if is_flint(from.0 + x, from.1 + y) {
                    return Some((from.0 + x, from.1 + y));
                }
            }
            x += stride;
        }
        let mut y = -r + stride;
        while y < r {
            for &x in &[-r, r] {
                if is_flint(from.0 + x, from.1 + y) {
                    return Some((from.0 + x, from.1 + y));
                }
            }
            y += stride;
        }
        r += stride;
    }
    None
}

fn main() {
    let seed: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(42);
    let mut sim = Sim::new(WorldSeed(seed), 512);
    // Le **vrai** foyer du banc `etincelle` — recalculé par la même sélection
    // par climat (froid mais sans cuivre proche : c'est le plafond néolithique
    // constaté sur le run de 63 ans). Le coder en dur donnerait un point faux et
    // fausserait toutes les distances qui en dépendent.
    let seed_point = (km_to_tiles(1500.0) as i64, km_to_tiles(2100.0) as i64);
    let taiga = scenario::find_home_where(
        &mut sim,
        seed_point,
        1.0..=5.0,
        &[Biome::TemperateForest, Biome::Taiga, Biome::Grassland],
    )
    .unwrap_or_else(|| scenario::find_home(&mut sim, seed_point));

    println!("── Seed {seed} : reconnaissance du cuivre (foyer bronze) ──");
    let ht = sim.world.tile(taiga.0, taiga.1);
    println!(
        "Foyer du banc : ({}, {}) — {:?}, {:.1} °C.",
        taiga.0, taiga.1, ht.biome, ht.temperature
    );

    // 1. Le cuivre le plus proche du foyer taïga — confirme (ou infirme) que
    //    l'absence de cuivre est bien la cause du plafond néolithique.
    match commerce::find_nearest_deposit(&sim.world, taiga, Deposit::Copper) {
        Some(cu) => println!(
            "Cuivre le plus proche du foyer taïga : ({}, {}) à {:.0} km.",
            cu.0,
            cu.1,
            tiles_to_km(dist(taiga, cu)),
        ),
        None => println!("Aucun cuivre à moins de ~1500 km du foyer taïga."),
    }

    // 2. Survey régional : on cherche du cuivre depuis une grille de centres,
    //    on déduplique, et on rapporte le climat de chaque district — pour
    //    repérer un cuivre froid et habitable.
    let stride = km_to_tiles(300.0) as i64;
    let mut coppers: BTreeSet<(i64, i64)> = BTreeSet::new();
    for gy in -6..=6 {
        for gx in -6..=6 {
            let c = (taiga.0 + gx * stride, taiga.1 + gy * stride);
            if let Some(cu) = commerce::find_nearest_deposit(&sim.world, c, Deposit::Copper) {
                coppers.insert(cu);
            }
        }
    }

    // Climat de chaque district (température moyenne, biome, altitude), plus la
    // distance à l'étain et au foyer taïga.
    struct Row {
        temp: f32,
        elev: f32,
        biome: String,
        d_taiga_km: f64,
        d_tin_km: f64,
        pos: (i64, i64),
    }
    let mut rows: Vec<Row> = coppers
        .iter()
        .map(|&(x, y)| {
            let tile = sim.world.tile(x, y);
            let d_tin_km = commerce::find_nearest_deposit(&sim.world, (x, y), Deposit::Tin)
                .map(|t| tiles_to_km(dist((x, y), t)))
                .unwrap_or(f64::INFINITY);
            Row {
                temp: tile.temperature,
                elev: tile.elevation,
                biome: format!("{:?}", tile.biome),
                d_taiga_km: tiles_to_km(dist(taiga, (x, y))),
                d_tin_km,
                pos: (x, y),
            }
        })
        .collect();
    rows.sort_by(|a, b| a.temp.total_cmp(&b.temp)); // du plus froid au plus chaud

    println!("\n{} districts de cuivre distincts. Du plus froid au plus chaud :", rows.len());
    println!(
        "{:>7} {:>6} {:>16} {:>10} {:>10}  position",
        "temp°C", "elev", "biome", "d_taïga", "d_étain"
    );
    for r in &rows {
        println!(
            "{:>7.1} {:>6.2} {:>16} {:>8.0}km {:>8.0}km  ({}, {})",
            r.temp, r.elev, r.biome, r.d_taiga_km, r.d_tin_km, r.pos.0, r.pos.1
        );
    }

    // 3. Suggestion : le cuivre le plus froid mais **habitable** (temp dans la
    //    bande [0, 9] °C — assez froid pour presser, pas la glace qui éteint —
    //    sur un biome terrestre non gelé) et dont l'étain reste atteignable.
    if let Some(r) = rows.iter().find(|r| {
        r.temp >= 0.0
            && r.temp <= 9.0
            && !r.biome.contains("Ocean")
            && !r.biome.contains("Coast")
            && !r.biome.contains("Glacier")
            && r.d_tin_km.is_finite()
    }) {
        println!(
            "\n➜ Candidat foyer bronze : ({}, {}) — {:.1} °C, {}, étain à {:.0} km.\n  \
             (Lancer le banc en le ciblant, avec du cuivre sur place et de l'étain à portée.)",
            r.pos.0, r.pos.1, r.temp, r.biome, r.d_tin_km,
        );
    } else {
        println!("\nAucun cuivre froid-habitable dans cette région — essayer une autre seed.");
    }

    // 4. Silex à portée : la voie « friction » du feu (fire_mastery) exige du
    //    **silex**. Or le silex (roche **sédimentaire** affleurante) et le
    //    cuivre (roche **ignée**, pics de province) ne coexistent pas
    //    localement — un district de cuivre est en terrain igné, où le silex
    //    ne se forme pas. On mesure donc, à résolution fine, à quelle distance
    //    le silex réapparaît autour des districts de cuivre froids, et — en
    //    **référence** — autour du foyer taïga d'origine (qui, lui, exposait
    //    100 % de ses habitants au silex). Si le silex reste à plusieurs km,
    //    la voie friction est hors de portée d'agents qui, au froid, fourragent
    //    près du foyer : le feu devra venir d'ailleurs (incendie, diffusion).
    let wg = sim.world.worldgen();
    let fine_stride = km_to_tiles(0.2) as i64; // 200 m : l'échelle du fourrage
    let fine_max = km_to_tiles(20.0) as i64;
    let probe = |label: String, p: (i64, i64)| match nearest_flint(wg, p, fine_max, fine_stride) {
        Some(f) => println!("  {label:<30} silex à {:>5.1} km", tiles_to_km(dist(p, f))),
        None => println!("  {label:<30} aucun silex à moins de 20 km"),
    };
    println!("\nSilex (voie friction du feu) — distance au plus proche (maille 200 m) :");
    probe("foyer taïga d'origine (réf.)".to_string(), taiga);
    for r in rows.iter().filter(|r| r.temp >= 0.0 && r.temp <= 9.0) {
        probe(format!("cuivre {:>4.1}°C ({}, {})", r.temp, r.pos.0, r.pos.1), r.pos);
    }
}
