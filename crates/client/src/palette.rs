//! Couches d'affichage et palettes. Les mêmes teintes que l'outil `map_png`,
//! pour que l'observation en direct corresponde aux rendus de debug.

use cairn_worldgen::{Biome, Deposit, RockType};

/// Couche de données affichée. Le joueur bascule de l'une à l'autre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Biome,
    Elevation,
    Temperature,
    Humidity,
    Geology,
}

impl Layer {
    pub const ALL: [Layer; 5] = [
        Layer::Biome,
        Layer::Elevation,
        Layer::Temperature,
        Layer::Humidity,
        Layer::Geology,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Layer::Biome => "biome",
            Layer::Elevation => "elevation",
            Layer::Temperature => "temperature",
            Layer::Humidity => "humidity",
            Layer::Geology => "geology",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Layer::Biome => "Biomes",
            Layer::Elevation => "Altitude",
            Layer::Temperature => "Température",
            Layer::Humidity => "Humidité",
            Layer::Geology => "Géologie",
        }
    }

    pub fn from_id(id: &str) -> Option<Layer> {
        Layer::ALL.into_iter().find(|l| l.id() == id)
    }
}

/// Un échantillon du monde en un point : ce dont toutes les couches ont besoin.
pub struct Sample {
    pub elevation: f64,
    pub temperature: f64,
    pub humidity: f64,
    pub biome: Biome,
    pub rock: RockType,
    pub deposit: Deposit,
}

/// Couleur RGBA (opaque) de l'échantillon selon la couche active.
pub fn color(layer: Layer, s: &Sample) -> [u8; 4] {
    let rgb = match layer {
        Layer::Biome => biome_color(s.biome),
        Layer::Elevation => ramp(&HYPSO, s.elevation),
        Layer::Temperature => darken_sea(s, ramp(&THERMAL, s.temperature)),
        Layer::Humidity => darken_sea(s, ramp(&MOISTURE, s.humidity)),
        Layer::Geology => geology_color(s),
    };
    [rgb[0], rgb[1], rgb[2], 255]
}

/// Assombrit l'océan sur les couches climatiques, pour garder le trait de côte.
fn darken_sea(s: &Sample, c: [u8; 3]) -> [u8; 3] {
    if s.elevation <= 0.0 {
        c.map(|v| (v as f64 * 0.55) as u8)
    } else {
        c
    }
}

fn geology_color(s: &Sample) -> [u8; 3] {
    if s.elevation <= 0.0 {
        return [20, 32, 52];
    }
    match s.deposit {
        Deposit::None => match s.rock {
            RockType::Sedimentary => [150, 140, 116],
            RockType::Metamorphic => [116, 116, 132],
            RockType::Igneous => [96, 84, 84],
        },
        Deposit::Flint => [60, 60, 66],
        Deposit::Clay => [176, 122, 88],
        Deposit::Obsidian => [24, 20, 32],
        Deposit::Copper => [214, 128, 64],
        Deposit::Tin => [180, 200, 210],
        Deposit::Gold => [240, 208, 72],
        Deposit::Iron => [150, 80, 60],
    }
}

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

/// Point de contrôle d'une rampe : (valeur, couleur).
type Stop = (f64, [u8; 3]);

const HYPSO: [Stop; 8] = [
    (-1.0, [8, 16, 64]),
    (-0.4, [16, 48, 128]),
    (0.0, [70, 138, 200]),
    (0.001, [62, 126, 71]),
    (0.25, [134, 158, 88]),
    (0.5, [168, 136, 88]),
    (0.75, [136, 108, 96]),
    (1.0, [245, 245, 245]),
];

const THERMAL: [Stop; 7] = [
    (-30.0, [128, 60, 168]),
    (-10.0, [64, 88, 200]),
    (0.0, [120, 180, 230]),
    (10.0, [120, 190, 120]),
    (20.0, [230, 210, 90]),
    (30.0, [230, 120, 60]),
    (38.0, [180, 30, 40]),
];

const MOISTURE: [Stop; 6] = [
    (0.0, [172, 132, 82]),
    (0.2, [196, 172, 104]),
    (0.4, [150, 168, 94]),
    (0.6, [86, 142, 86]),
    (0.8, [42, 112, 102]),
    (1.0, [18, 76, 122]),
];

/// Interpolation linéaire par morceaux dans une rampe de couleurs.
fn ramp(stops: &[Stop], v: f64) -> [u8; 3] {
    if v <= stops[0].0 {
        return stops[0].1;
    }
    for pair in stops.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if v <= b.0 {
            let t = (v - a.0) / (b.0 - a.0);
            let lerp = |i: usize| (a.1[i] as f64 + (b.1[i] as f64 - a.1[i] as f64) * t) as u8;
            return [lerp(0), lerp(1), lerp(2)];
        }
    }
    stops[stops.len() - 1].1
}
