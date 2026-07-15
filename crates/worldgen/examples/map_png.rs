//! Rend une région du monde en PNG, une image par couche du pipeline :
//! altitude (hypsométrique), température (thermique), humidité (aridité),
//! biomes (avec rivières et lacs superposés).
//!
//! Usage : cargo run --release -p cairn-worldgen --example map_png -- [seed] [tuiles/pixel]
//! Le second argument zoome (1 tuile = 2 m) :
//!   ~8      → village (~16 km de côté)
//!   ~256    → région  (~500 km)
//!   ~4096   → monde   (~8000 km, pôle à pôle) — défaut
//! Sortie : out/map_<seed>_{alt,temp,hum,bio}.png

use std::collections::BTreeMap;
use std::time::Instant;

use cairn_core::WorldSeed;
use cairn_core::scale::km_to_tiles;
use cairn_worldgen::{
    Biome, Deposit, FREEZE_STILL_C, Hydrology, HydrologyConfig, Region, RockType, Water, WorldGen,
};
use rayon::prelude::*;

/// Glace de surface sur eau courante/lac gelé, et banquise (mer gelée).
const ICE: [u8; 3] = [206, 222, 236];
const SEA_ICE: [u8; 3] = [176, 198, 220];

/// Côté de l'image en pixels.
const SIZE: u32 = 1024;
/// Échelle par défaut : 1 pixel = N tuiles. À 4096 (× 1024 px × 2 m/tuile),
/// la carte couvre ~8000 km, soit un cycle de latitude complet — vue monde.
const DEFAULT_TILES_PER_PX: i64 = 4096;

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
    let tiles_per_px: i64 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .filter(|&t| t > 0)
        .unwrap_or(DEFAULT_TILES_PER_PX);
    // Centre de la vue en km (args 3 et 4) : par défaut l'origine. Permet
    // d'inspecter un continent précis ou un pôle sans être coincé sur (0, 0).
    let center_x = km_to_tiles(arg_f64(3).unwrap_or(0.0)) as i64;
    let center_y = km_to_tiles(arg_f64(4).unwrap_or(0.0)) as i64;

    let world = WorldGen::new(WorldSeed(seed));
    let half = (SIZE as i64 * tiles_per_px) / 2;
    let to_tile_x = |px: u32| center_x + i64::from(px) * tiles_per_px - half;
    let to_tile_y = |py: u32| center_y + i64::from(py) * tiles_per_px - half;
    let start = Instant::now();

    // Calcul parallèle par rangées ; `map` préserve l'ordre, le résultat
    // est donc identique au calcul séquentiel — déterminisme conservé.
    let rows: Vec<Vec<(f64, f64, f64)>> = (0..SIZE)
        .into_par_iter()
        .map(|py| {
            (0..SIZE)
                .map(|px| {
                    let (x, y) = (to_tile_x(px), to_tile_y(py));
                    let e = world.elevation(x, y);
                    let t = world.mean_temperature(x, y, e);
                    let h = world.humidity(x, y);
                    (e, t, h)
                })
                .collect()
        })
        .collect();

    let elapsed = start.elapsed();

    // Hydrologie sur la fenêtre rendue, une macro-cellule par pixel. Bornée
    // par construction : les rivières venues de hors-champ n'existent pas
    // (limite assumée du calcul en fenêtre, levée plus tard au chunking).
    let region = Region {
        origin_x: center_x - half,
        origin_y: center_y - half,
        width: SIZE as usize,
        height: SIZE as usize,
        tiles_per_cell: tiles_per_px,
    };
    let hydro_cfg = HydrologyConfig::default();
    let river_threshold = hydro_cfg.river_threshold;
    let hydro = Hydrology::compute(region, &hydro_cfg, |x, y| world.elevation(x, y));

    let mut img_alt = image::RgbImage::new(SIZE, SIZE);
    let mut img_temp = image::RgbImage::new(SIZE, SIZE);
    let mut img_hum = image::RgbImage::new(SIZE, SIZE);
    let mut img_bio = image::RgbImage::new(SIZE, SIZE);
    let mut img_geo = image::RgbImage::new(SIZE, SIZE);
    let mut land_px: u64 = 0;
    let mut biome_counts: BTreeMap<Biome, u64> = BTreeMap::new();
    let mut deposit_counts: BTreeMap<Deposit, u64> = BTreeMap::new();

    for (py, row) in rows.iter().enumerate() {
        for (px, &(e, t, h)) in row.iter().enumerate() {
            let (px, py) = (px as u32, py as u32);
            let (tx, ty) = (to_tile_x(px), to_tile_y(py));
            if e > 0.0 {
                land_px += 1;
            }
            let biome = Biome::classify(e, t, h);
            *biome_counts.entry(biome).or_insert(0) += 1;

            // L'hydrologie prime sur le biome climatique : une rivière ou un
            // lac recouvre la couleur de terrain. La surface gelée (dérivée de
            // la température) recouvre à son tour l'eau liquide.
            let water = hydro.water_at(tx, ty);
            let ocean = matches!(biome, Biome::Ocean | Biome::Coast);
            let bio_rgb = match water {
                Water::River if water.frozen(t) => ICE,
                Water::River => [48, 96, 176],
                Water::Lake if water.frozen(t) => ICE,
                Water::Lake => [40, 82, 150],
                // La mer/côte : gel via le seuil « eau stagnante ».
                _ if ocean && t < FREEZE_STILL_C => SEA_ICE,
                _ => biome_color(biome),
            };

            // Géologie : roche en fond, gisement en surimpression. En mer,
            // fond sombre.
            let geo_rgb = if e <= 0.0 {
                [20, 32, 52]
            } else {
                let deposit = world.deposit(tx, ty, e);
                *deposit_counts.entry(deposit).or_insert(0) += 1;
                match deposit {
                    Deposit::None => rock_color(world.rock_type(tx, ty)),
                    d => deposit_color(d),
                }
            };

            img_alt.put_pixel(px, py, gradient(HYPSO_STOPS, e));
            // Océans assombris sur les cartes dérivées : le trait de côte
            // reste lisible sans masquer le champ affiché.
            img_temp.put_pixel(px, py, darken_if(e <= 0.0, gradient(THERMAL_STOPS, t)));
            img_hum.put_pixel(px, py, darken_if(e <= 0.0, gradient(MOISTURE_STOPS, h)));
            img_bio.put_pixel(px, py, image::Rgb(bio_rgb));
            img_geo.put_pixel(px, py, image::Rgb(geo_rgb));
        }
    }

    // Élargissement des rivières selon le débit : le D8 concentre le flux en
    // une ligne d'une cellule. Un fleuve (forte accumulation) mérite d'être
    // plus large qu'un ruisseau — on dilate les cellules-rivières d'un rayon
    // croissant avec l'accumulation. L'état gelé suit la température locale.
    for py in 0..SIZE {
        for px in 0..SIZE {
            let (tx, ty) = (to_tile_x(px), to_tile_y(py));
            if hydro.water_at(tx, ty) != Water::River {
                continue;
            }
            let extra = river_extra_width(hydro.accumulation_at(tx, ty), river_threshold);
            if extra == 0 {
                continue;
            }
            for dy in -extra..=extra {
                for dx in -extra..=extra {
                    let (nx, ny) = (px as i64 + dx, py as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= SIZE as i64 || ny >= SIZE as i64 {
                        continue;
                    }
                    let (nx, ny) = (nx as u32, ny as u32);
                    let t = rows[ny as usize][nx as usize].1;
                    let color = if Water::River.frozen(t) { ICE } else { [48, 96, 176] };
                    img_bio.put_pixel(nx, ny, image::Rgb(color));
                }
            }
        }
    }

    std::fs::create_dir_all("out").expect("création du dossier out/");
    for (img, layer) in [
        (&img_alt, "alt"),
        (&img_temp, "temp"),
        (&img_hum, "hum"),
        (&img_bio, "bio"),
        (&img_geo, "geo"),
    ] {
        img.save(format!("out/map_{seed}_{layer}.png"))
            .expect("écriture du PNG");
    }

    // Montage : toutes les couches d'un même continent dans une image, pour
    // comparer les étages du pipeline d'un coup d'œil.
    save_montage(seed, &[&img_alt, &img_temp, &img_hum, &img_bio, &img_geo]);

    let land_pct = 100.0 * land_px as f64 / (u64::from(SIZE) * u64::from(SIZE)) as f64;
    println!(
        "out/map_{seed}_{{alt,temp,hum,bio,geo}}.png — terres émergées : {land_pct:.1} % — calcul : {elapsed:.2?}"
    );

    // Répartition des biomes terrestres, en % des terres émergées.
    let mut land_biomes: Vec<(Biome, u64)> = biome_counts
        .into_iter()
        .filter(|(b, _)| !matches!(b, Biome::Ocean | Biome::Coast))
        .collect();
    land_biomes.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    for (biome, n) in land_biomes {
        println!("  {:>5.1} %  {}", 100.0 * n as f64 / land_px as f64, biome.name());
    }

    // Gisements, en % des tuiles de terre.
    println!("  gisements (% des terres) :");
    let mut deposits: Vec<(Deposit, u64)> = deposit_counts
        .into_iter()
        .filter(|(d, _)| !matches!(d, Deposit::None))
        .collect();
    deposits.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    for (deposit, n) in deposits {
        println!("    {:>5.2} %  {}", 100.0 * n as f64 / land_px as f64, deposit.name());
    }
}

/// Fond de roche (teintes sourdes) pour la couche géologie.
fn rock_color(rock: RockType) -> [u8; 3] {
    match rock {
        RockType::Sedimentary => [150, 140, 116],
        RockType::Metamorphic => [116, 116, 132],
        RockType::Igneous => [96, 84, 84],
    }
}

/// Couleur vive d'un gisement, en surimpression sur la roche.
fn deposit_color(deposit: Deposit) -> [u8; 3] {
    match deposit {
        Deposit::None => [0, 0, 0],
        Deposit::Flint => [60, 60, 66],
        Deposit::Clay => [176, 122, 88],
        Deposit::Obsidian => [24, 20, 32],
        Deposit::Copper => [214, 128, 64],
        Deposit::Tin => [180, 200, 210],
        Deposit::Gold => [240, 208, 72],
        Deposit::Iron => [150, 80, 60],
    }
}

/// Couleur d'aplat par biome — préfiguration de la palette du tileset.
fn biome_color(biome: Biome) -> [u8; 3] {
    match biome {
        Biome::Ocean => [24, 48, 96],
        Biome::Coast => [56, 108, 160],
        Biome::Glacier => [232, 238, 244],
        Biome::Tundra => [150, 158, 144],
        Biome::Taiga => [72, 106, 88],
        Biome::ColdDesert => [168, 152, 122],
        Biome::Steppe => [190, 174, 104],
        Biome::Grassland => [140, 172, 90],
        Biome::TemperateForest => [64, 122, 66],
        Biome::HotDesert => [228, 198, 132],
        Biome::Savanna => [204, 182, 92],
        Biome::TropicalForest => [30, 96, 48],
    }
}

/// Argument de ligne de commande à la position `n`, parsé en f64.
fn arg_f64(n: usize) -> Option<f64> {
    std::env::args().nth(n).and_then(|s| s.parse().ok())
}

/// Rayon d'élargissement d'une rivière (en pixels) selon son débit accumulé,
/// exprimé en multiples du seuil de rivière. Ruisseau = 0 (1 px), grand
/// fleuve = jusqu'à 3 px de plus de chaque côté.
fn river_extra_width(accum: f32, threshold: f32) -> i64 {
    match accum / threshold {
        r if r >= 16.0 => 3,
        r if r >= 6.0 => 2,
        r if r >= 2.5 => 1,
        _ => 0,
    }
}

/// Assemble les couches en une seule image (grille 3×2), chacune réduite de
/// moitié, pour visualiser tout le pipeline d'un continent d'un coup.
fn save_montage(seed: u64, layers: &[&image::RgbImage]) {
    use image::imageops::{FilterType, overlay, resize};
    let (cw, ch) = (SIZE / 2, SIZE / 2);
    let (cols, rows) = (3u32, 2u32);
    let mut montage = image::RgbImage::from_pixel(cw * cols, ch * rows, image::Rgb([16, 16, 20]));
    for (i, layer) in layers.iter().enumerate() {
        let small = resize(*layer, cw, ch, FilterType::Nearest);
        let (col, row) = (i as u32 % cols, i as u32 / cols);
        overlay(&mut montage, &small, i64::from(col * cw), i64::from(row * ch));
    }
    montage
        .save(format!("out/map_{seed}_montage.png"))
        .expect("écriture du montage");
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
