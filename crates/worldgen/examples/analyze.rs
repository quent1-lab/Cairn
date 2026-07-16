//! Vérifications statistiques du worldgen — ce qu'aucun test unitaire ni
//! coup d'œil au PNG ne tranche.
//!
//! Deux mesures :
//!
//! A. **Connexité des terres** sur plusieurs seeds : distribution de la plus
//!    grande masse continentale connexe. Répond à « le monde est-il un
//!    archipel trop morcelé ? » indépendamment du cadrage d'un rendu.
//!
//! B. **Rain shadow** : pour chaque tuile de terre on mesure la barrière de
//!    relief rencontrée en remontant le vent, et on trace l'humidité moyenne
//!    par tranche de barrière. Si l'ombre pluviométrique fonctionne,
//!    l'humidité doit chuter quand la barrière au vent monte.
//!
//! Usage : cargo run --release -p cairn-worldgen --example analyze -- \
//!             [nb_seeds] [longueur_onde_continent] [sea_bias]

use cairn_core::WorldSeed;
use cairn_core::scale::{TILE_METERS, km_to_tiles, tiles_to_km};
use cairn_worldgen::{AltitudeConfig, AltitudeField, Deposit, HumidityConfig, WorldGen, WorldGenConfig};
use rayon::prelude::*;

fn main() {
    let mut args = std::env::args().skip(1);
    // Sans arguments, l'outil mesure exactement le monde par défaut : les
    // valeurs viennent d'AltitudeConfig::default(), pas de constantes en dur.
    let default_cfg = AltitudeConfig::default();
    let n_seeds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(50);
    let wavelength: f64 = args
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0 / default_cfg.continent_frequency);
    let sea_bias: f64 = args
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default_cfg.sea_bias);

    println!(
        "Échelle : 1 tuile = {TILE_METERS} m · continent ≈ {:.0} km ({:.0} k tuiles) · 300 km = {:.0} k tuiles",
        tiles_to_km(wavelength),
        wavelength / 1000.0,
        km_to_tiles(300.0) / 1000.0,
    );
    println!("Config : sea_bias = {sea_bias}\n");

    connectivity(n_seeds, wavelength, sea_bias);
    println!();
    rain_shadow(wavelength, sea_bias);
    println!();
    metal_districts(wavelength, sea_bias);
}

/// Construit une config d'altitude avec les surcharges de test.
fn altitude_config(wavelength: f64, sea_bias: f64) -> AltitudeConfig {
    AltitudeConfig {
        continent_frequency: 1.0 / wavelength,
        sea_bias,
        ..AltitudeConfig::default()
    }
}

// ─────────────────────────── A. Connexité ───────────────────────────

/// Grille d'échantillonnage : côté en cellules et pas en tuiles. La fenêtre
/// couvre GRID·SPACING tuiles ≈ 18 000 km, soit ~6 continents.
const GRID: usize = 384;
const SPACING: i64 = km_to_tiles(48.0) as i64;

/// Distance cible d'une route commerciale (le « 300 km » du brief pour la
/// séparation cuivre↔étain).
const TRADE_ROUTE_KM: f64 = 300.0;

fn connectivity(n_seeds: u64, wavelength: f64, sea_bias: f64) {
    println!("A. CONNEXITÉ DES TERRES — {n_seeds} seeds, fenêtre {} tuiles de côté", GRID as i64 * SPACING);

    // Chaque seed est indépendante : on parallélise sur les seeds.
    // Résultat par seed : (seed, % terres, % plus grande masse, étendue km).
    let mut results: Vec<(u64, f64, f64, f64)> = (0..n_seeds)
        .into_par_iter()
        .map(|seed| {
            let field = AltitudeField::with_config(WorldSeed(seed), altitude_config(wavelength, sea_bias));
            let land = sample_land(&field);
            let (land_frac, largest_frac, span_tiles) = largest_component(&land);
            (seed, land_frac, largest_frac, tiles_to_km(span_tiles))
        })
        .collect();

    let land_fracs: Vec<f64> = results.iter().map(|r| r.1).collect();
    let largest_fracs: Vec<f64> = results.iter().map(|r| r.2).collect();
    let spans_km: Vec<f64> = results.iter().map(|r| r.3).collect();

    println!(
        "  Terres émergées   : moy {:.1} %  [min {:.1} %, max {:.1} %]",
        100.0 * mean(&land_fracs),
        100.0 * min(&land_fracs),
        100.0 * max(&land_fracs),
    );
    println!(
        "  + grande masse    : médiane {:.1} % des terres  [min {:.1} %, max {:.1} %]",
        100.0 * median(&largest_fracs),
        100.0 * min(&largest_fracs),
        100.0 * max(&largest_fracs),
    );
    // Étendue = diagonale de la boîte englobante de la plus grande masse.
    // C'est le vrai juge du « une route de 300 km tient-elle sur terre ? ».
    println!(
        "  + grande masse    : étendue médiane {:.0} km  [min {:.0} km, max {:.0} km]",
        median(&spans_km),
        min(&spans_km),
        max(&spans_km),
    );
    let ok = spans_km.iter().filter(|&&s| s >= TRADE_ROUTE_KM).count();
    println!(
        "  Route de {TRADE_ROUTE_KM:.0} km possible sur la + grande masse : {ok}/{n_seeds} seeds ({:.0} %)",
        100.0 * ok as f64 / n_seeds as f64,
    );

    // Les 3 seeds les plus étroites : leur étendue en km est le vrai pire cas.
    results.sort_by(|a, b| a.3.total_cmp(&b.3));
    print!("  Seeds les + étroites : ");
    for (seed, _, _, span) in results.iter().take(3) {
        print!("{seed} ({span:.0} km)  ");
    }
    println!();
}

/// Échantillonne l'occupation terrestre (élévation > 0) sur la grille.
fn sample_land(field: &AltitudeField) -> Vec<bool> {
    let half = GRID as i64 * SPACING / 2;
    (0..GRID * GRID)
        .map(|i| {
            let (cx, cy) = (i % GRID, i / GRID);
            let x = cx as i64 * SPACING - half;
            let y = cy as i64 * SPACING - half;
            field.elevation(x, y) > 0.0
        })
        .collect()
}

/// Renvoie (fraction de terres, fraction que représente la plus grande masse
/// connexe, étendue de cette masse en tuiles). L'étendue est la diagonale de
/// la boîte englobante : le juge de « deux points distants de 300 km
/// peuvent-ils tenir sur cette terre ? ». Composantes en 4-connexité.
fn largest_component(land: &[bool]) -> (f64, f64, f64) {
    let total = GRID * GRID;
    let land_count = land.iter().filter(|&&l| l).count();
    if land_count == 0 {
        return (0.0, 0.0, 0.0);
    }

    let mut visited = vec![false; total];
    let mut stack = Vec::new();
    let mut largest = 0usize;
    let mut best_span_cells = 0.0f64;

    for start in 0..total {
        if !land[start] || visited[start] {
            continue;
        }
        // Remplissage itératif (pile explicite, pas de récursion : la grille
        // fait 147 k cellules, la pile d'appels déborderait). On suit la
        // boîte englobante de la composante au passage.
        let mut size = 0usize;
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (GRID, GRID, 0usize, 0usize);
        stack.push(start);
        visited[start] = true;
        while let Some(i) = stack.pop() {
            size += 1;
            let (cx, cy) = (i % GRID, i / GRID);
            min_x = min_x.min(cx);
            max_x = max_x.max(cx);
            min_y = min_y.min(cy);
            max_y = max_y.max(cy);
            let mut push = |nx: i64, ny: i64| {
                if (0..GRID as i64).contains(&nx) && (0..GRID as i64).contains(&ny) {
                    let j = ny as usize * GRID + nx as usize;
                    if land[j] && !visited[j] {
                        visited[j] = true;
                        stack.push(j);
                    }
                }
            };
            push(cx as i64 - 1, cy as i64);
            push(cx as i64 + 1, cy as i64);
            push(cx as i64, cy as i64 - 1);
            push(cx as i64, cy as i64 + 1);
        }
        if size > largest {
            largest = size;
            let (dx, dy) = ((max_x - min_x) as f64, (max_y - min_y) as f64);
            best_span_cells = (dx * dx + dy * dy).sqrt();
        }
    }

    (
        land_count as f64 / total as f64,
        largest as f64 / land_count as f64,
        best_span_cells * SPACING as f64,
    )
}

// ─────────────────────────── B. Rain shadow ───────────────────────────

/// Remontée au vent pour mesurer la barrière : ~150 km au pas de 3 km,
/// aligné sur la portée d'advection de l'humidité.
const UPWIND_STEPS: usize = 50;
const UPWIND_STEP_TILES: f64 = km_to_tiles(3.0);
/// Fenêtre rain shadow : ~4400 km, un continent aux reliefs variés.
const RS_GRID: usize = 220;
const RS_SPACING: i64 = km_to_tiles(20.0) as i64;
const RS_SEEDS: u64 = 4;

fn rain_shadow(wavelength: f64, sea_bias: f64) {
    println!("B. RAIN SHADOW — {RS_SEEDS} seeds, humidité moyenne par barrière au vent");
    println!("   (barrière = relief max rencontré en remontant le vent, moins l'altitude locale)");

    // 8 tranches de barrière, de 0 à 0.4 d'élévation normalisée.
    const BINS: usize = 8;
    const BIN_WIDTH: f64 = 0.05;
    let mut sum = [0.0f64; BINS];
    let mut count = [0u64; BINS];

    for seed in 0..RS_SEEDS {
        // Trajet unique (lateral_samples = 0) : on mesure la physique
        // d'advection brute, sans le lissage cosmétique qui l'atténuerait.
        let cfg = WorldGenConfig {
            altitude: altitude_config(wavelength, sea_bias),
            humidity: HumidityConfig { lateral_samples: 0, ..HumidityConfig::default() },
            ..WorldGenConfig::default()
        };
        let world = WorldGen::with_config(WorldSeed(seed), cfg);
        let half = RS_GRID as i64 * RS_SPACING / 2;

        // Calcul parallèle par rangée, agrégé ensuite (déterministe).
        let rows: Vec<Vec<(usize, f64)>> = (0..RS_GRID)
            .into_par_iter()
            .map(|gy| {
                let mut out = Vec::new();
                for gx in 0..RS_GRID {
                    let x = gx as i64 * RS_SPACING - half;
                    let y = gy as i64 * RS_SPACING - half;
                    let e = world.elevation(x, y);
                    if e <= 0.0 {
                        continue; // barrière définie sur les terres seulement
                    }
                    // world.wind : le même champ que celui de l'humidité,
                    // pas un doublon susceptible de diverger.
                    let barrier = upwind_barrier(&world, x, y, e);
                    let bin = ((barrier / BIN_WIDTH) as usize).min(BINS - 1);
                    let h = world.humidity(x, y);
                    out.push((bin, h));
                }
                out
            })
            .collect();

        for (bin, h) in rows.into_iter().flatten() {
            sum[bin] += h;
            count[bin] += 1;
        }
    }

    println!("   barrière (Δélév)   humidité moy   n");
    for b in 0..BINS {
        if count[b] == 0 {
            continue;
        }
        let lo = b as f64 * BIN_WIDTH;
        let hi = lo + BIN_WIDTH;
        let bar = "█".repeat((sum[b] / count[b] as f64 * 40.0) as usize);
        println!(
            "   {lo:.2}–{hi:.2}          {:.3}   {:>7}  {bar}",
            sum[b] / count[b] as f64,
            count[b],
        );
    }
}

/// Barrière de relief au vent : altitude maximale rencontrée en remontant le
/// vent sur UPWIND_STEPS pas, moins l'altitude de la tuile. Positive quand
/// la tuile est sous le vent d'un relief plus haut (situation d'ombre).
fn upwind_barrier(world: &WorldGen, x: i64, y: i64, e_here: f64) -> f64 {
    let (mut px, mut py) = (x as f64, y as f64);
    let mut max_elev = e_here;
    for _ in 0..UPWIND_STEPS {
        let (vx, vy) = world.wind.wind(px.round() as i64, py.round() as i64);
        px -= vx * UPWIND_STEP_TILES;
        py -= vy * UPWIND_STEP_TILES;
        let e = world.elevation(px.round() as i64, py.round() as i64);
        max_elev = max_elev.max(e);
    }
    (max_elev - e_here).max(0.0)
}

// ─────────────────────────── C. Districts métalliques ───────────────────────────

const MD_SEEDS: u64 = 8;
const MD_GRID: usize = 256;
/// Pas ~14 km : la fenêtre (~3580 km) couvre un continent, assez fin pour
/// capter des amas de ~50 km.
const MD_SPACING: i64 = km_to_tiles(14.0) as i64;

fn metal_districts(wavelength: f64, sea_bias: f64) {
    println!("C. DISTRICTS MÉTALLIQUES — {MD_SEEDS} seeds, fenêtre {:.0} km", tiles_to_km(MD_GRID as f64 * MD_SPACING as f64));

    // Bins de distance cuivre↔étain (au plus proche voisin), en km.
    const BINS: usize = 8;
    const BIN_KM: f64 = 100.0;
    let mut nn_hist = [0u64; BINS];
    let mut nn_all: Vec<f64> = Vec::new();
    let (mut viable, mut both_present) = (0u32, 0u32);
    let mut cu_frac_sum = 0.0;

    for seed in 0..MD_SEEDS {
        let cfg = WorldGenConfig {
            altitude: altitude_config(wavelength, sea_bias),
            ..WorldGenConfig::default()
        };
        let world = WorldGen::with_config(WorldSeed(seed), cfg);
        let half = MD_GRID as i64 * MD_SPACING / 2;

        let mut land = vec![false; MD_GRID * MD_GRID];
        let mut coppers = Vec::new();
        let mut tins = Vec::new();
        let mut land_count = 0u64;
        let mut cu_count = 0u64;
        for gy in 0..MD_GRID {
            for gx in 0..MD_GRID {
                let x = gx as i64 * MD_SPACING - half;
                let y = gy as i64 * MD_SPACING - half;
                let e = world.elevation(x, y);
                if e <= 0.0 {
                    continue;
                }
                land[gy * MD_GRID + gx] = true;
                land_count += 1;
                match world.deposit(x, y, e) {
                    Deposit::Copper => {
                        coppers.push((gx, gy));
                        cu_count += 1;
                    }
                    Deposit::Tin => tins.push((gx, gy)),
                    _ => {}
                }
            }
        }
        cu_frac_sum += cu_count as f64 / land_count.max(1) as f64;

        if coppers.is_empty() || tins.is_empty() {
            continue;
        }
        both_present += 1;

        // Distance au plus proche voisin cuivre → étain (en km).
        let cell_km = tiles_to_km(MD_SPACING as f64);
        for &(cx, cy) in &coppers {
            let min_cells = tins
                .iter()
                .map(|&(tx, ty)| {
                    let (dx, dy) = (cx as f64 - tx as f64, cy as f64 - ty as f64);
                    (dx * dx + dy * dy).sqrt()
                })
                .fold(f64::INFINITY, f64::min);
            let km = min_cells * cell_km;
            nn_all.push(km);
            nn_hist[((km / BIN_KM) as usize).min(BINS - 1)] += 1;
        }

        // Bronze viable ? Cuivre et étain sur la MÊME masse terrestre connexe.
        let comp = label_components(&land);
        let cu_comps: std::collections::BTreeSet<i32> =
            coppers.iter().map(|&(x, y)| comp[y * MD_GRID + x]).collect();
        let tin_comps: std::collections::BTreeSet<i32> =
            tins.iter().map(|&(x, y)| comp[y * MD_GRID + x]).collect();
        if cu_comps.intersection(&tin_comps).next().is_some() {
            viable += 1;
        }
    }

    println!("  cuivre : {:.2} % des terres (en amas, non dispersé)", 100.0 * cu_frac_sum / MD_SEEDS as f64);
    println!("  distance cuivre→étain au plus proche voisin :");
    for (b, &count) in nn_hist.iter().enumerate() {
        if count == 0 {
            continue;
        }
        let bar = "█".repeat(count as usize * 40 / nn_all.len().max(1));
        println!("    {:>4}–{:<4} km  {count:>6}  {bar}", b as f64 * BIN_KM, (b + 1) as f64 * BIN_KM);
    }
    if !nn_all.is_empty() {
        println!("    médiane {:.0} km", median(&nn_all));
    }
    println!(
        "  bronze viable (cuivre+étain sur la même masse) : {viable}/{both_present} seeds où les deux existent",
    );
}

/// Étiquette les composantes terrestres connexes (4-connexité). Renvoie l'id de
/// composante par cellule (-1 pour l'eau).
fn label_components(land: &[bool]) -> Vec<i32> {
    let n = MD_GRID * MD_GRID;
    let mut comp = vec![-1i32; n];
    let mut next = 0i32;
    let mut stack = Vec::new();
    for start in 0..n {
        if !land[start] || comp[start] >= 0 {
            continue;
        }
        stack.push(start);
        comp[start] = next;
        while let Some(i) = stack.pop() {
            let (cx, cy) = (i % MD_GRID, i / MD_GRID);
            let push = |nx: i64, ny: i64, stack: &mut Vec<usize>, comp: &mut Vec<i32>| {
                if (0..MD_GRID as i64).contains(&nx) && (0..MD_GRID as i64).contains(&ny) {
                    let j = ny as usize * MD_GRID + nx as usize;
                    if land[j] && comp[j] < 0 {
                        comp[j] = next;
                        stack.push(j);
                    }
                }
            };
            push(cx as i64 - 1, cy as i64, &mut stack, &mut comp);
            push(cx as i64 + 1, cy as i64, &mut stack, &mut comp);
            push(cx as i64, cy as i64 - 1, &mut stack, &mut comp);
            push(cx as i64, cy as i64 + 1, &mut stack, &mut comp);
        }
        next += 1;
    }
    comp
}

// ─────────────────────────── stats ───────────────────────────

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}
fn min(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::INFINITY, f64::min)
}
fn max(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}
fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    s[s.len() / 2]
}
