//! Rendu du monde vers un tampon RGBA — **sans aucune dépendance au DOM**.
//!
//! Isolé ici pour être testable nativement (l'exemple `preview` en tire un
//! PNG) : le canvas et les événements sont du plombage, le cœur est ce
//! parcours d'échantillons.

use cairn_worldgen::{Biome, WorldGen};

use crate::palette::{self, Layer, Sample};

/// Budget d'échantillons du worldgen par rendu : borne le coût au dézoom
/// (chaque échantillon calcule l'humidité).
const MAX_SAMPLES: f64 = 120_000.0;

/// Coordonnée de tuile (f64) sous un pixel, pour une caméra donnée.
pub fn tile_at(px: f64, py: f64, cx: f64, cy: f64, scale: f64, w: usize, h: usize) -> (f64, f64) {
    (
        cx + (px - w as f64 / 2.0) / scale,
        cy + (py - h as f64 / 2.0) / scale,
    )
}

fn sample(wg: &WorldGen, x: i64, y: i64) -> Sample {
    let elevation = wg.elevation(x, y);
    let temperature = wg.mean_temperature(x, y, elevation);
    let humidity = wg.humidity(x, y);
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
    let mut grid = vec![[0u8; 4]; gw * gh];
    for gy in 0..gh {
        for gx in 0..gw {
            let (wx, wy) = tile_at(gx as f64 * stride, gy as f64 * stride, cx, cy, scale, w, h);
            let s = sample(wg, wx.floor() as i64, wy.floor() as i64);
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
