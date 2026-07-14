//! Rend une région du monde en PNG, une image par couche du pipeline :
//! altitude (teintes hypsométriques) et température (échelle thermique).
//!
//! Usage : cargo run --release -p cairn-worldgen --example map_png -- [seed]
//! Sortie : out/map_<seed>_alt.png, out/map_<seed>_temp.png

use cairn_core::WorldSeed;
use cairn_worldgen::{AltitudeField, TemperatureField};

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

fn main() {
    let seed: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);

    let world_seed = WorldSeed(seed);
    let altitude = AltitudeField::new(world_seed);
    let temperature = TemperatureField::new(world_seed);
    let half = (SIZE as i64 * TILES_PER_PX) / 2;

    let mut img_alt = image::RgbImage::new(SIZE, SIZE);
    let mut img_temp = image::RgbImage::new(SIZE, SIZE);
    let mut land_px: u64 = 0;

    for py in 0..SIZE {
        for px in 0..SIZE {
            let x = i64::from(px) * TILES_PER_PX - half;
            let y = i64::from(py) * TILES_PER_PX - half;
            let e = altitude.elevation(x, y);
            let t = temperature.mean_temperature(x, y, e);

            if e > 0.0 {
                land_px += 1;
            }
            img_alt.put_pixel(px, py, gradient(HYPSO_STOPS, e));
            // Océans assombris sur la carte thermique : le trait de côte
            // reste lisible sans masquer le champ de température.
            let mut c = gradient(THERMAL_STOPS, t);
            if e <= 0.0 {
                c = image::Rgb(c.0.map(|v| (f64::from(v) * 0.55) as u8));
            }
            img_temp.put_pixel(px, py, c);
        }
    }

    std::fs::create_dir_all("out").expect("création du dossier out/");
    let path_alt = format!("out/map_{seed}_alt.png");
    let path_temp = format!("out/map_{seed}_temp.png");
    img_alt.save(&path_alt).expect("écriture du PNG altitude");
    img_temp.save(&path_temp).expect("écriture du PNG température");

    let land_pct = 100.0 * land_px as f64 / (u64::from(SIZE) * u64::from(SIZE)) as f64;
    println!("{path_alt}, {path_temp} — terres émergées : {land_pct:.1} %");
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
