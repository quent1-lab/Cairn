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
use cairn_worldgen::{AltitudeConfig, AltitudeField, HumidityConfig, WorldGen, WorldGenConfig};
use rayon::prelude::*;

fn main() {
    let mut args = std::env::args().skip(1);
    let n_seeds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(50);
    let wavelength: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(4096.0);
    let sea_bias: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0.1);

    println!(
        "Config : longueur d'onde continentale = {wavelength:.0} tuiles, sea_bias = {sea_bias}\n"
    );

    connectivity(n_seeds, wavelength, sea_bias);
    println!();
    rain_shadow(wavelength, sea_bias);
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
/// couvre GRID·SPACING tuiles, soit ~8 longueurs d'onde continentales.
const GRID: usize = 384;
const SPACING: i64 = 96;

fn connectivity(n_seeds: u64, wavelength: f64, sea_bias: f64) {
    println!("A. CONNEXITÉ DES TERRES — {n_seeds} seeds, fenêtre {} tuiles de côté", GRID as i64 * SPACING);

    // Chaque seed est indépendante : on parallélise sur les seeds.
    let mut results: Vec<(u64, f64, f64)> = (0..n_seeds)
        .into_par_iter()
        .map(|seed| {
            let field = AltitudeField::with_config(WorldSeed(seed), altitude_config(wavelength, sea_bias));
            let land = sample_land(&field);
            let (land_frac, largest_frac) = largest_component(&land);
            (seed, land_frac, largest_frac)
        })
        .collect();

    // % de terres émergées, agrégé.
    let land_fracs: Vec<f64> = results.iter().map(|r| r.1).collect();
    let largest_fracs: Vec<f64> = results.iter().map(|r| r.2).collect();

    println!(
        "  Terres émergées   : moy {:.1} %  [min {:.1} %, max {:.1} %]",
        100.0 * mean(&land_fracs),
        100.0 * min(&land_fracs),
        100.0 * max(&land_fracs),
    );
    println!(
        "  + grande masse    : moy {:.1} %  médiane {:.1} %  [min {:.1} %, max {:.1} %] des terres",
        100.0 * mean(&largest_fracs),
        100.0 * median(&largest_fracs),
        100.0 * min(&largest_fracs),
        100.0 * max(&largest_fracs),
    );
    // La plus grande masse en valeur absolue (tuiles²), pour juger « peut-on
    // y bâtir une civilisation ».
    let largest_tiles: Vec<f64> = results
        .iter()
        .map(|r| r.1 * r.2 * (GRID * GRID) as f64 * (SPACING * SPACING) as f64)
        .collect();
    println!(
        "  + grande masse    : médiane {:.0} M tuiles²",
        median(&largest_tiles) / 1e6,
    );

    // Les 3 seeds les plus morcelées : utiles à inspecter au PNG.
    results.sort_by(|a, b| a.2.total_cmp(&b.2));
    print!("  Seeds les + morcelées : ");
    for (seed, _, frac) in results.iter().take(3) {
        print!("{seed} ({:.0} %)  ", 100.0 * frac);
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
/// connexe parmi ces terres). Composantes en 4-connexité, par remplissage.
fn largest_component(land: &[bool]) -> (f64, f64) {
    let total = GRID * GRID;
    let land_count = land.iter().filter(|&&l| l).count();
    if land_count == 0 {
        return (0.0, 0.0);
    }

    let mut visited = vec![false; total];
    let mut stack = Vec::new();
    let mut largest = 0usize;

    for start in 0..total {
        if !land[start] || visited[start] {
            continue;
        }
        // Remplissage itératif (pile explicite, pas de récursion : la grille
        // fait 147 k cellules, la pile d'appels déborderait).
        let mut size = 0usize;
        stack.push(start);
        visited[start] = true;
        while let Some(i) = stack.pop() {
            size += 1;
            let (cx, cy) = (i % GRID, i / GRID);
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
        largest = largest.max(size);
    }

    (
        land_count as f64 / total as f64,
        largest as f64 / land_count as f64,
    )
}

// ─────────────────────────── B. Rain shadow ───────────────────────────

/// Nombre de pas de la remontée au vent pour mesurer la barrière.
const UPWIND_STEPS: usize = 40;
const UPWIND_STEP_TILES: f64 = 32.0;
const RS_GRID: usize = 220;
const RS_SPACING: i64 = 96;
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
