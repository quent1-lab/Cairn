//! Rend une région du monde en PNG, une image par couche du pipeline :
//! altitude (hypsométrique), température (thermique), humidité (aridité).
//!
//! Usage : cargo run --release -p cairn-worldgen --example map_png -- [seed]
//! Sortie : out/map_<seed>_{alt,temp,hum}.png

use std::time::Instant;

use cairn_core::WorldSeed;
use cairn_worldgen::WorldGen;
use rayon::prelude::*;

/// Côté de l'image en pixels.
const SIZE: u32 = 1024;
/// 1 pixel = N tuiles : la carte couvre SIZE × N tuiles de côté.
const TILES_PER_PX: i64 = 16;

/// Palette hypsométrique, indexée par l'élévation normalisée. Rupture nette
/// au niveau de la mer pour lire le trait de côte.
const HYPSO_STOPS: &[(f64, [u8; 3])] = &[
    (-1.0, [8, 16, 64]),     // abysses
    (-0.4, [16, 48, 128]),   // océan
    (0.0, [70, 138, 200]),   // eaux côtières
    (0.001, [62, 126, 71]),  // plaines
    (0.25, [134, 158, 88]),  // collines
    (0.5, [168, 136, 88]),   // moyenne montagne
    (0.75, [136, 108, 96]),  // haute montagne
    (1.0, [245, 245, 245]),  // neiges
];

/// Échelle thermique, indexée en °C.
const THERMAL_STOPS: &[(f64, [u8; 3])] = &[
    (-30.0, [128, 60, 168]), // violet polaire
    (-10.0, [64, 88, 200]),  // bleu
    (0.0, [120, 180, 230]),  // bleu clair
    (10.0, [120, 190, 120]), // vert tempéré
    (20.0, [230, 210, 90]),  // jaune
    (30.0, [230, 120, 60]),  // orange
    (38.0, [180, 30, 40]),   // rouge écrasant
];

/// Échelle d'humidité, indexée dans [0, 1].
const MOISTURE_STOPS: &[(f64, [u8; 3])] = &[
    (0.0, [172, 132, 82]),  // aride
    (0.2, [196, 172, 104]), // steppe
    (0.4, [150, 168, 94]),  // herbeux
    (0.6, [86, 142, 86]),   // humide
    (0.8, [42, 112, 102]),  // très humide
    (1.0, [18, 76, 122]),   // saturé
];

fn main() {
    let seed: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);

    let world = WorldGen::new(WorldSeed(seed));
    let half = (SIZE as i64 * TILES_PER_PX) / 2;
    let start = Instant::now();

    // Calcul parallèle par rangées ; `map` préserve l'ordre, le résultat
    // est donc identique au calcul séquentiel — déterminisme conservé.
    let rows: Vec<Vec<(f64, f64, f64)>> = (0..SIZE)
        .into_par_iter()
        .map(|py| {
            (0..SIZE)
                .map(|px| {
                    let x = i64::from(px) * TILES_PER_PX - half;
                    let y = i64::from(py) * TILES_PER_PX - half;
                    let e = world.elevation(x, y);
                    let t = world.mean_temperature(x, y, e);
                    let h = world.humidity(x, y);
                    (e, t, h)
                })
                .collect()
        })
        .collect();

    let elapsed = start.elapsed();

    let mut img_alt = image::RgbImage::new(SIZE, SIZE);
    let mut img_temp = image::RgbImage::new(SIZE, SIZE);
    let mut img_hum = image::RgbImage::new(SIZE, SIZE);
    let mut land_px: u64 = 0;

    for (py, row) in rows.iter().enumerate() {
        for (px, &(e, t, h)) in row.iter().enumerate() {
            let (px, py) = (px as u32, py as u32);
            if e > 0.0 {
                land_px += 1;
            }
            img_alt.put_pixel(px, py, gradient(HYPSO_STOPS, e));
            // Océans assombris sur les cartes dérivées : le trait de côte
            // reste lisible sans masquer le champ affiché.
            img_temp.put_pixel(px, py, darken_if(e <= 0.0, gradient(THERMAL_STOPS, t)));
            img_hum.put_pixel(px, py, darken_if(e <= 0.0, gradient(MOISTURE_STOPS, h)));
        }
    }

    std::fs::create_dir_all("out").expect("création du dossier out/");
    for (img, layer) in [(&img_alt, "alt"), (&img_temp, "temp"), (&img_hum, "hum")] {
        img.save(format!("out/map_{seed}_{layer}.png"))
            .expect("écriture du PNG");
    }

    let land_pct = 100.0 * land_px as f64 / (u64::from(SIZE) * u64::from(SIZE)) as f64;
    println!(
        "out/map_{seed}_{{alt,temp,hum}}.png — terres émergées : {land_pct:.1} % — calcul : {elapsed:.2?}"
    );
}

fn darken_if(condition: bool, c: image::Rgb<u8>) -> image::Rgb<u8> {
    if condition {
        image::Rgb(c.0.map(|v| (f64::from(v) * 0.55) as u8))
    } else {
        c
    }
}

/// Interpolation linéaire par morceaux dans une rampe de couleurs.
fn gradient(stops: &[(f64, [u8; 3])], v: f64) -> image::Rgb<u8> {
    if v <= stops[0].0 {
        return image::Rgb(stops[0].1);
    }
    for pair in stops.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if v <= b.0 {
            let t = (v - a.0) / (b.0 - a.0);
            let lerp =
                |from: u8, to: u8| (f64::from(from) + (f64::from(to) - f64::from(from)) * t) as u8;
            return image::Rgb([
                lerp(a.1[0], b.1[0]),
                lerp(a.1[1], b.1[1]),
                lerp(a.1[2], b.1[2]),
            ]);
        }
    }
    image::Rgb(stops[stops.len() - 1].1)
}
