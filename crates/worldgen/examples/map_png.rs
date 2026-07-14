//! Rend une région du champ d'altitude en PNG (teintes hypsométriques).
//!
//! Usage : cargo run --release -p cairn-worldgen --example map_png -- [seed]
//! Sortie : out/map_<seed>.png

use cairn_core::WorldSeed;
use cairn_worldgen::AltitudeField;

/// Côté de l'image en pixels.
const SIZE: u32 = 1024;
/// 1 pixel = N tuiles : la carte couvre SIZE × N tuiles de côté.
const TILES_PER_PX: i64 = 8;

fn main() {
    let seed: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);

    let field = AltitudeField::new(WorldSeed(seed));
    let half = (SIZE as i64 * TILES_PER_PX) / 2;

    let mut img = image::RgbImage::new(SIZE, SIZE);
    for (px, py, pixel) in img.enumerate_pixels_mut() {
        let x = px as i64 * TILES_PER_PX - half;
        let y = py as i64 * TILES_PER_PX - half;
        *pixel = hypsometric(field.elevation(x, y));
    }

    std::fs::create_dir_all("out").expect("création du dossier out/");
    let path = format!("out/map_{seed}.png");
    img.save(&path).expect("écriture du PNG");
    println!("carte écrite dans {path}");
}

/// Palette hypsométrique : gradient par morceaux entre points de contrôle,
/// avec une rupture nette au niveau de la mer pour lire le trait de côte.
fn hypsometric(e: f64) -> image::Rgb<u8> {
    const STOPS: &[(f64, [u8; 3])] = &[
        (-1.0, [8, 16, 64]),     // abysses
        (-0.4, [16, 48, 128]),   // océan
        (0.0, [70, 138, 200]),   // eaux côtières
        (0.001, [62, 126, 71]),  // plaines
        (0.25, [134, 158, 88]),  // collines
        (0.5, [168, 136, 88]),   // moyenne montagne
        (0.75, [136, 108, 96]),  // haute montagne
        (1.0, [245, 245, 245]),  // neiges
    ];

    let mut prev = STOPS[0];
    for &stop in &STOPS[1..] {
        if e <= stop.0 {
            let t = (e - prev.0) / (stop.0 - prev.0);
            let lerp =
                |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round() as u8;
            return image::Rgb([
                lerp(prev.1[0], stop.1[0]),
                lerp(prev.1[1], stop.1[1]),
                lerp(prev.1[2], stop.1[2]),
            ]);
        }
        prev = stop;
    }
    image::Rgb(prev.1)
}
