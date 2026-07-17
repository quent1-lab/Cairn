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
/// On échantillonne le worldgen sur un **maillage ancré au monde** dont le pas
/// (en tuiles) est quantifié à une puissance de deux — un « palier de LOD ». Le
/// pas est la plus petite puissance de deux qui tient le budget `MAX_SAMPLES`
/// (jamais moins de 1 : au zoom fort, une tuile par nœud). Les nœuds tombent
/// donc à des positions **fixes dans le monde**, indépendantes de la caméra :
/// à l'intérieur d'un palier, ni le pan ni le zoom ne changent *quelles* tuiles
/// sont échantillonnées. C'est ce qui supprime le scintillement — le
/// sel-et-poivre des gisements qui « bougeait », les biomes qui « ondulaient ».
/// Le pas ne double qu'aux frontières de palier (une octave de zoom), où la
/// moitié des nœuds sont réutilisés : la transition est discrète et douce. On
/// étire ensuite au plus proche voisin — rendu pixel-art, sans lissage.
pub fn render_to_buffer(
    wg: &WorldGen,
    cx: f64,
    cy: f64,
    scale: f64,
    layer: Layer,
    w: usize,
    h: usize,
) -> Vec<u8> {
    // Pas d'échantillonnage en tuiles = plus petite puissance de deux tenant le
    // budget. `world_w·world_h / pas²` est le nombre de nœuds à l'écran ; on
    // veut ≤ MAX_SAMPLES, d'où `pas ≥ √(world_w·world_h / MAX_SAMPLES)`.
    let world_w = w as f64 / scale;
    let world_h = h as f64 / scale;
    let raw = ((world_w * world_h) / MAX_SAMPLES).sqrt().max(1.0);
    let step = 1i64 << raw.log2().ceil().max(0.0) as u32;
    let stepf = step as f64;

    // Coin monde du pixel (0, 0), puis indice de nœud (⌊tuile / pas⌋) de chaque
    // colonne et ligne — précalculés pour éviter un floor par pixel. Comme ces
    // indices ne dépendent que de la position monde, un même point du monde
    // retombe toujours sur le même nœud, quelle que soit la caméra.
    let wx_min = cx - w as f64 / (2.0 * scale);
    let wy_min = cy - h as f64 / (2.0 * scale);
    let node_x: Vec<i64> = (0..w).map(|px| ((wx_min + px as f64 / scale) / stepf).floor() as i64).collect();
    let node_y: Vec<i64> = (0..h).map(|py| ((wy_min + py as f64 / scale) / stepf).floor() as i64).collect();
    let kx_min = node_x[0];
    let ky_min = node_y[0];
    let nx = (node_x[w - 1] - kx_min + 1) as usize;
    let ny = (node_y[h - 1] - ky_min + 1) as usize;

    // Humidité sur un maillage encore ×HUMIDITY_LATTICE plus grossier (elle est
    // lisse et coûteuse) : ses nœuds sont les nœuds principaux d'indice
    // multiple de HUMIDITY_LATTICE — donc eux aussi figés dans le monde. On
    // ancre sa base sur un tel multiple pour couvrir tout l'écran.
    let hl = HUMIDITY_LATTICE as i64;
    let hbx = kx_min.div_euclid(hl) * hl;
    let hby = ky_min.div_euclid(hl) * hl;
    let hnx = ((node_x[w - 1] - hbx) / hl + 2) as usize;
    let hny = ((node_y[h - 1] - hby) / hl + 2) as usize;
    let mut hum = vec![0f32; hnx * hny];
    for j in 0..hny {
        for i in 0..hnx {
            let wx = (hbx + i as i64 * hl) * step;
            let wy = (hby + j as i64 * hl) * step;
            hum[j * hnx + i] = wg.humidity(wx, wy) as f32;
        }
    }
    // Humidité bilinéaire au nœud principal (kx, ky).
    let humidity_at = |kx: i64, ky: i64| -> f64 {
        let fx = (kx - hbx) as f64 / HUMIDITY_LATTICE as f64;
        let fy = (ky - hby) as f64 / HUMIDITY_LATTICE as f64;
        let i0 = (fx as usize).min(hnx - 2);
        let j0 = (fy as usize).min(hny - 2);
        let (tx, ty) = (fx - i0 as f64, fy - j0 as f64);
        let at = |i: usize, j: usize| hum[j * hnx + i] as f64;
        let top = at(i0, j0) + (at(i0 + 1, j0) - at(i0, j0)) * tx;
        let bot = at(i0, j0 + 1) + (at(i0 + 1, j0 + 1) - at(i0, j0 + 1)) * tx;
        top + (bot - top) * ty
    };

    // Cache : une couleur par nœud monde. C'est ici que vit le budget.
    let mut cache = vec![[0u8; 4]; nx * ny];
    for j in 0..ny {
        for i in 0..nx {
            let (kx, ky) = (kx_min + i as i64, ky_min + j as i64);
            let s = sample(wg, kx * step, ky * step, humidity_at(kx, ky));
            cache[j * nx + i] = palette::color(layer, &s);
        }
    }

    // Étirement : chaque pixel prend la couleur du nœud monde qui le contient.
    let mut buf = vec![0u8; w * h * 4];
    for (py, &ky) in node_y.iter().enumerate() {
        let j = (ky - ky_min) as usize;
        for (px, &kx) in node_x.iter().enumerate() {
            let i = (kx - kx_min) as usize;
            let o = (py * w + px) * 4;
            buf[o..o + 4].copy_from_slice(&cache[j * nx + i]);
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_core::WorldSeed;

    /// À zoom fixe, un pan ne doit que **glisser** la carte : le monde sous une
    /// tuile donnée rend la même couleur, où que tombe la caméra. C'est le
    /// non-régression du maillage ancré au monde. On le vérifie sur deux
    /// couches : les biomes (dérivés de l'humidité) et la géologie (le
    /// sel-et-poivre des gisements, le cas le plus sensible au scintillement).
    fn pan_ne_deforme_pas(layer: Layer) {
        let wg = WorldGen::new(WorldSeed(42));
        let (w, h) = (64usize, 64usize);
        // scale = 1 px/tuile : une cellule = une tuile exacte, un décalage
        // entier de caméra est un simple glissement des pixels.
        let scale = 1.0;
        let shift = 10; // tuiles
        let a = render_to_buffer(&wg, 1000.0, 1000.0, scale, layer, w, h);
        let b = render_to_buffer(&wg, 1000.0 + shift as f64, 1000.0, scale, layer, w, h);
        // La tuile sous le pixel (px+shift) de A est celle sous le pixel px de B.
        for py in 0..h {
            for px in 0..(w - shift) {
                let ia = (py * w + px + shift) * 4;
                let ib = (py * w + px) * 4;
                assert_eq!(a[ia..ia + 4], b[ib..ib + 4], "instable au pan en ({px}, {py})");
            }
        }
    }

    #[test]
    fn le_pan_ne_deforme_pas_les_biomes() {
        pan_ne_deforme_pas(Layer::Biome);
    }

    #[test]
    fn le_pan_ne_fait_pas_bouger_les_minerais() {
        pan_ne_deforme_pas(Layer::Geology);
    }

    /// Un zoom qui ne franchit pas de frontière de palier (même `step`) ne doit
    /// pas rééchantillonner : au pixel central, le monde vaut exactement le
    /// centre caméra, donc le même nœud est vu et la couleur est identique.
    #[test]
    fn le_zoom_dans_un_palier_est_stable() {
        let wg = WorldGen::new(WorldSeed(42));
        let (w, h) = (200usize, 200usize);
        let (cx, cy) = (1000.0, 1000.0);
        let center = ((h / 2) * w + w / 2) * 4;
        // 0,30 et 0,40 px/tuile tombent dans le même palier (step = 2 ici).
        let a = render_to_buffer(&wg, cx, cy, 0.30, Layer::Geology, w, h);
        let b = render_to_buffer(&wg, cx, cy, 0.40, Layer::Geology, w, h);
        assert_eq!(
            a[center..center + 4],
            b[center..center + 4],
            "zoom instable dans un même palier de LOD"
        );
    }
}
