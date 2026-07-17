//! Rendu du monde vers un tampon RGBA — **sans aucune dépendance au DOM**.
//!
//! Isolé ici pour être testable nativement (l'exemple `preview` en tire un
//! PNG) : le canvas et les événements sont du plombage, le cœur est ce
//! parcours d'échantillons.

use cairn_worldgen::{Biome, WorldGen};

use crate::palette::{self, Layer, Sample};

/// Budget d'échantillons du worldgen par rendu : borne le coût au dézoom.
const MAX_SAMPLES: f64 = 120_000.0;

/// Espacement de la macro-grille d'humidité, en cellules de la grille de
/// rendu. L'humidité est un champ advecté lisse (pénétration ~256 km) et de
/// loin le calcul le plus cher — une remontée au vent, soit des dizaines
/// d'évaluations d'altitude par point. On ne l'évalue donc qu'un point sur
/// `HUMIDITY_LATTICE` dans chaque direction, puis on l'interpole
/// bilinéairement. Tout le reste (altitude, température, gisements) reste
/// calculé par cellule : les côtes et le relief gardent leur netteté, seul le
/// champ déjà lisse est sous-échantillonné.
const HUMIDITY_LATTICE: usize = 8;

/// Coordonnée de tuile (f64) sous un pixel, pour une caméra donnée.
pub fn tile_at(px: f64, py: f64, cx: f64, cy: f64, scale: f64, w: usize, h: usize) -> (f64, f64) {
    (
        cx + (px - w as f64 / 2.0) / scale,
        cy + (py - h as f64 / 2.0) / scale,
    )
}

fn sample(wg: &WorldGen, x: i64, y: i64, humidity: f64) -> Sample {
    let elevation = wg.elevation(x, y);
    let temperature = wg.mean_temperature(x, y, elevation);
    Sample {
        elevation,
        temperature,
        humidity,
        biome: Biome::classify(elevation, temperature, humidity),
        rock: wg.rock_type(x, y),
        deposit: wg.deposit(x, y, elevation),
    }
}

/// Produit le tampon RGBA (w·h·4 octets) de la vue.
///
/// On échantillonne le worldgen sur une grille (~1 point par tuile en zoom
/// rapproché, élargie au dézoom pour tenir le budget), puis on l'étire aux
/// pixels au plus proche voisin — rendu pixel-art, sans lissage.
pub fn render_to_buffer(
    wg: &WorldGen,
    cx: f64,
    cy: f64,
    scale: f64,
    layer: Layer,
    w: usize,
    h: usize,
) -> Vec<u8> {
    let mut stride = scale.max(1.0);
    let count = (w as f64 / stride).ceil() * (h as f64 / stride).ceil();
    if count > MAX_SAMPLES {
        stride *= (count / MAX_SAMPLES).sqrt();
    }

    let gw = (w as f64 / stride).ceil() as usize + 1;
    let gh = (h as f64 / stride).ceil() as usize + 1;

    // Macro-grille d'humidité **ancrée sur le monde** (et non sur l'écran) :
    // ses nœuds sont à des positions fixes en tuiles, indépendantes de la
    // caméra. Sinon, déplacer la vue ferait glisser le maillage sous le monde
    // et l'humidité interpolée en une tuile donnée changerait à chaque pan —
    // donc la classification des biomes « ondulerait ». L'espacement suit le
    // zoom (≈ `HUMIDITY_LATTICE` cellules de rendu), donc le coût est le même
    // que la version écran : ~un point d'humidité pour 64 cellules.
    let world_step = stride / scale; // tuiles entre deux cellules de rendu
    let node_tiles = (HUMIDITY_LATTICE as f64 * world_step).max(1.0);
    let (wx0, wy0) = tile_at(0.0, 0.0, cx, cy, scale, w, h);
    let (wx1, wy1) = tile_at(gw as f64 * stride, gh as f64 * stride, cx, cy, scale, w, h);
    // Indices de nœud (monde ÷ espacement) couvrant l'écran, +1 nœud de marge.
    let kx0 = (wx0 / node_tiles).floor() as i64;
    let ky0 = (wy0 / node_tiles).floor() as i64;
    let hw = ((wx1 / node_tiles).floor() as i64 - kx0 + 2) as usize;
    let hh = ((wy1 / node_tiles).floor() as i64 - ky0 + 2) as usize;
    let mut hum = vec![0f32; hw * hh];
    for j in 0..hh {
        for i in 0..hw {
            let wx = ((kx0 + i as i64) as f64 * node_tiles).floor() as i64;
            let wy = ((ky0 + j as i64) as f64 * node_tiles).floor() as i64;
            hum[j * hw + i] = wg.humidity(wx, wy) as f32;
        }
    }
    // Humidité interpolée bilinéairement à une position monde (continue).
    let humidity_at = |wx: f64, wy: f64| -> f64 {
        let fx = wx / node_tiles - kx0 as f64;
        let fy = wy / node_tiles - ky0 as f64;
        let i0 = (fx.max(0.0) as usize).min(hw - 2);
        let j0 = (fy.max(0.0) as usize).min(hh - 2);
        let (tx, ty) = (fx - i0 as f64, fy - j0 as f64);
        let at = |i: usize, j: usize| hum[j * hw + i] as f64;
        let top = at(i0, j0) + (at(i0 + 1, j0) - at(i0, j0)) * tx;
        let bot = at(i0, j0 + 1) + (at(i0 + 1, j0 + 1) - at(i0, j0 + 1)) * tx;
        top + (bot - top) * ty
    };

    let mut grid = vec![[0u8; 4]; gw * gh];
    for gy in 0..gh {
        for gx in 0..gw {
            let (wx, wy) = tile_at(gx as f64 * stride, gy as f64 * stride, cx, cy, scale, w, h);
            let s = sample(wg, wx.floor() as i64, wy.floor() as i64, humidity_at(wx, wy));
            grid[gy * gw + gx] = palette::color(layer, &s);
        }
    }

    let mut buf = vec![0u8; w * h * 4];
    for py in 0..h {
        let gy = (py as f64 / stride) as usize;
        for px in 0..w {
            let gx = (px as f64 / stride) as usize;
            let c = grid[gy * gw + gx];
            let i = (py * w + px) * 4;
            buf[i..i + 4].copy_from_slice(&c);
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_core::WorldSeed;

    /// À zoom fixe, un pan ne doit pas déformer la carte : le monde sous une
    /// tuile donnée rend la même couleur, où que tombe la caméra. C'est le
    /// non-régression du maillage d'humidité ancré au monde — avec l'ancien
    /// maillage ancré à l'écran, les biomes glissaient et ce test échouait.
    #[test]
    fn le_pan_ne_deforme_pas_les_biomes() {
        let wg = WorldGen::new(WorldSeed(42));
        let (w, h) = (64usize, 64usize);
        // scale = 1 px/tuile : une cellule de rendu = une tuile exacte, donc
        // un décalage entier de caméra est un simple glissement des pixels.
        let scale = 1.0;
        let shift = 10; // tuiles
        let a = render_to_buffer(&wg, 1000.0, 1000.0, scale, Layer::Biome, w, h);
        let b = render_to_buffer(&wg, 1000.0 + shift as f64, 1000.0, scale, Layer::Biome, w, h);
        // La tuile sous le pixel (px+shift) de A est celle sous le pixel px de B.
        for py in 0..h {
            for px in 0..(w - shift) {
                let ia = (py * w + px + shift) * 4;
                let ib = (py * w + px) * 4;
                assert_eq!(
                    a[ia..ia + 4],
                    b[ib..ib + 4],
                    "biome instable au pan en ({px}, {py})"
                );
            }
        }
    }
}
