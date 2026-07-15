//! Couche 5 du pipeline : l'hydrologie, par accumulation de flux D8.
//!
//! Contrairement aux couches précédentes, l'hydrologie n'est pas une
//! fonction point à point : savoir combien d'eau passe par une tuile exige
//! de connaître tout son bassin versant en amont. On la calcule donc sur une
//! **région bornée** à résolution grossière (une macro-grille), pas sur le
//! monde infini d'un coup — cohérent avec le BRIEF §2.2 (« macro-grille
//! grossière puis raffiné localement »).
//!
//! Trois étapes, chacune un algorithme classique :
//!
//! 1. **Comblement des cuvettes** (priority-flood + ε, Barnes 2014). Le bruit
//!    fractal crée d'innombrables minima locaux qui piégeraient l'eau. On
//!    remplit chaque dépression jusqu'à son point de débordement, en ajoutant
//!    un ε à chaque marche pour qu'il ne reste aucun plat : après ça, tout
//!    point a un chemin strictement descendant vers le bord du domaine. Les
//!    cuvettes ainsi noyées deviennent des **lacs**.
//! 2. **Directions d'écoulement D8** : chaque cellule s'écoule vers le plus
//!    bas de ses 8 voisins sur le relief comblé.
//! 3. **Accumulation** : en parcourant les cellules de la plus haute à la
//!    plus basse (l'ordre de dépilement du priority-flood, inversé), chacune
//!    verse son eau accumulée à sa voisine aval. Là où ça dépasse un seuil :
//!    une **rivière**.

use std::cmp::Ordering;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Incrément de comblement : garantit une pente non nulle partout après
/// remplissage, sans fausser le relief (le cumul sur un chemin de longueur L
/// vaut L·ε, négligeable devant le dénivelé réel pour ε ainsi choisi).
const FILL_EPSILON: f64 = 1e-6;

/// Fenêtre de calcul : un rectangle de `width × height` macro-cellules,
/// chaque cellule couvrant `tiles_per_cell` tuiles de côté.
#[derive(Debug, Clone, Copy)]
pub struct Region {
    pub origin_x: i64,
    pub origin_y: i64,
    pub width: usize,
    pub height: usize,
    pub tiles_per_cell: i64,
}

pub struct HydrologyConfig {
    /// Flux accumulé (en cellules drainées) au-delà duquel une cellule
    /// terrestre est une rivière.
    pub river_threshold: f32,
    /// Comblement minimal (en élévation normalisée) pour qu'une cuvette noyée
    /// compte comme lac — filtre les micro-pièges du bruit.
    pub lake_min_depth: f64,
}

impl Default for HydrologyConfig {
    fn default() -> Self {
        Self {
            river_threshold: 150.0,
            lake_min_depth: 0.03,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Water {
    None,
    Ocean,
    Lake,
    River,
}

/// Température moyenne (°C) sous laquelle la surface d'une **eau stagnante**
/// (lac, mer) gèle.
pub const FREEZE_STILL_C: f64 = 0.0;
/// Idem pour l'**eau courante** : une rivière continue de couler sous la
/// glace et ne prend en surface que par grand froid — d'où un seuil plus bas.
pub const FREEZE_FLOWING_C: f64 = -6.0;

impl Water {
    /// La surface de cette eau est-elle gelée à cette température moyenne ?
    ///
    /// Choix de conception : le **débit** est une donnée géographique
    /// permanente (l'hydrologie ne le recalcule pas). Le gel est seulement un
    /// **état de surface** dérivé de la température. Résultat émergent : aux
    /// hautes latitudes tempérées, les lacs et la mer gèlent tandis que les
    /// rivières restent ouvertes ; aux pôles, tout gèle.
    pub fn frozen(self, temp_c: f64) -> bool {
        match self {
            Water::River => temp_c < FREEZE_FLOWING_C,
            Water::Ocean | Water::Lake => temp_c < FREEZE_STILL_C,
            Water::None => false,
        }
    }
}

pub struct Hydrology {
    region: Region,
    accumulation: Vec<f32>,
    water: Vec<Water>,
}

impl Hydrology {
    /// Calcule l'hydrologie sur `region`. `elevation(x, y)` fournit
    /// l'élévation normalisée d'une tuile (≤ 0 : eau) — générique pour être
    /// testable sur un relief synthétique autant que sur le monde réel.
    pub fn compute<F>(region: Region, cfg: &HydrologyConfig, elevation: F) -> Self
    where
        F: Fn(i64, i64) -> f64,
    {
        let (w, h) = (region.width, region.height);
        let n = w * h;

        // Élévation au centre de chaque macro-cellule.
        let half = region.tiles_per_cell / 2;
        let elev: Vec<f64> = (0..n)
            .map(|i| {
                let (cx, cy) = (i % w, i / w);
                let x = region.origin_x + cx as i64 * region.tiles_per_cell + half;
                let y = region.origin_y + cy as i64 * region.tiles_per_cell + half;
                elevation(x, y)
            })
            .collect();

        let (filled, order) = fill_depressions(&elev, w, h);
        let downstream = flow_directions(&filled, w, h);
        let accumulation = accumulate(&elev, &downstream, &order);

        let water = (0..n)
            .map(|i| classify(elev[i], filled[i], accumulation[i], cfg))
            .collect();

        Self {
            region,
            accumulation,
            water,
        }
    }

    fn cell_index(&self, x: i64, y: i64) -> usize {
        let clamp = |v: i64, max: usize| v.clamp(0, max as i64 - 1) as usize;
        let cx = clamp((x - self.region.origin_x).div_euclid(self.region.tiles_per_cell), self.region.width);
        let cy = clamp((y - self.region.origin_y).div_euclid(self.region.tiles_per_cell), self.region.height);
        cy * self.region.width + cx
    }

    /// Classification hydrologique de la tuile (x, y).
    pub fn water_at(&self, x: i64, y: i64) -> Water {
        self.water[self.cell_index(x, y)]
    }

    /// Flux accumulé à la tuile (x, y), en nombre de cellules drainées.
    pub fn accumulation_at(&self, x: i64, y: i64) -> f32 {
        self.accumulation[self.cell_index(x, y)]
    }
}

/// Voisinage à 8 connexions, bordé par le domaine.
fn neighbors(cx: usize, cy: usize, w: usize, h: usize) -> impl Iterator<Item = usize> {
    const OFFSETS: [(i32, i32); 8] = [
        (-1, -1), (0, -1), (1, -1),
        (-1, 0), (1, 0),
        (-1, 1), (0, 1), (1, 1),
    ];
    OFFSETS.into_iter().filter_map(move |(dx, dy)| {
        let nx = cx as i32 + dx;
        let ny = cy as i32 + dy;
        if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
            Some(ny as usize * w + nx as usize)
        } else {
            None
        }
    })
}

/// Priority-flood + ε. Renvoie le relief comblé et l'ordre de dépilement
/// (élévation comblée croissante) — cet ordre est réutilisé pour
/// l'accumulation, sans re-trier.
fn fill_depressions(elev: &[f64], w: usize, h: usize) -> (Vec<f64>, Vec<usize>) {
    let n = w * h;
    let mut filled = vec![0.0; n];
    let mut closed = vec![false; n];
    let mut order = Vec::with_capacity(n);
    // Clé du tas : (altitude comblée, index de cellule). L'index n'est pas
    // qu'une charge utile — il départage les altitudes égales par un ordre
    // total strict, sinon le dépilement de deux cellules ex æquo dépendrait
    // de l'ordre d'insertion et pourrait faire basculer des directions D8
    // d'un run à l'autre. Invisible sur le PNG, fatal au déterminisme.
    let mut heap: BinaryHeap<Reverse<(OrdF64, u32)>> = BinaryHeap::new();

    // Amorçage : tout le bord du domaine est un exutoire, à son altitude.
    for cy in 0..h {
        for cx in 0..w {
            if cx == 0 || cy == 0 || cx == w - 1 || cy == h - 1 {
                let i = cy * w + cx;
                filled[i] = elev[i];
                closed[i] = true;
                heap.push(Reverse((OrdF64(filled[i]), i as u32)));
            }
        }
    }

    // On étend depuis les points les plus bas : chaque voisin est comblé au
    // niveau du point d'où on l'atteint, jamais en dessous de son propre sol.
    while let Some(Reverse((OrdF64(e), i))) = heap.pop() {
        let i = i as usize;
        order.push(i);
        let (cx, cy) = (i % w, i / w);
        for j in neighbors(cx, cy, w, h) {
            if !closed[j] {
                closed[j] = true;
                filled[j] = elev[j].max(e + FILL_EPSILON);
                heap.push(Reverse((OrdF64(filled[j]), j as u32)));
            }
        }
    }

    (filled, order)
}

/// Pour chaque cellule intérieure, l'indice de sa voisine la plus basse sur
/// le relief comblé. Les cellules de bord sont des exutoires (`None`).
fn flow_directions(filled: &[f64], w: usize, h: usize) -> Vec<Option<u32>> {
    (0..w * h)
        .map(|i| {
            let (cx, cy) = (i % w, i / w);
            if cx == 0 || cy == 0 || cx == w - 1 || cy == h - 1 {
                return None;
            }
            neighbors(cx, cy, w, h)
                .min_by(|&a, &b| filled[a].total_cmp(&filled[b]))
                .filter(|&j| filled[j] < filled[i])
                .map(|j| j as u32)
        })
        .collect()
}

/// Accumulation de flux : chaque cellule terrestre apporte une unité de
/// pluie, propagée vers l'aval. L'ordre inversé du priority-flood traite
/// toujours l'amont avant l'aval — pas besoin de tri topologique explicite.
fn accumulate(elev: &[f64], downstream: &[Option<u32>], order: &[usize]) -> Vec<f32> {
    let mut accum: Vec<f32> = elev.iter().map(|&e| if e > 0.0 { 1.0 } else { 0.0 }).collect();
    for &i in order.iter().rev() {
        if let Some(d) = downstream[i] {
            accum[d as usize] += accum[i];
        }
    }
    accum
}

fn classify(elev: f64, filled: f64, accum: f32, cfg: &HydrologyConfig) -> Water {
    if elev <= 0.0 {
        Water::Ocean
    } else if filled - elev > cfg.lake_min_depth {
        Water::Lake
    } else if accum >= cfg.river_threshold {
        Water::River
    } else {
        Water::None
    }
}

/// `f64` totalement ordonné, pour l'usage dans un tas binaire.
///
/// `f64` n'implémente que `PartialOrd` (à cause de `NaN`, non comparable),
/// donc pas utilisable tel quel comme clé de `BinaryHeap`, qui exige `Ord`.
/// On délègue à `total_cmp`, qui impose un ordre total déterministe sur tous
/// les flottants — le déterminisme est ici une exigence, pas un confort.
#[derive(PartialEq)]
struct OrdF64(f64);
impl Eq for OrdF64 {}
impl PartialOrd for OrdF64 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for OrdF64 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(w: usize, h: usize) -> Region {
        Region { origin_x: 0, origin_y: 0, width: w, height: h, tiles_per_cell: 1 }
    }

    #[test]
    fn l_eau_s_accumule_vers_l_aval() {
        // Plan incliné vers l'est : toute la région s'écoule vers +x.
        let cfg = HydrologyConfig::default();
        let hydro = Hydrology::compute(region(20, 20), &cfg, |x, _| 1.0 - 0.01 * x as f64);
        // Sur une même rangée, l'aval (est) draine plus que l'amont (ouest).
        assert!(hydro.accumulation_at(15, 10) > hydro.accumulation_at(5, 10));
    }

    #[test]
    fn une_cuvette_fermee_devient_un_lac() {
        // Bol : bord haut (1.0), centre bas (0.5). Sans exutoire interne,
        // l'eau remonte jusqu'au débordement → le fond est un lac.
        let cfg = HydrologyConfig::default();
        let bowl = |x: i64, y: i64| {
            let d = (x - 10).abs().max((y - 10).abs()) as f64;
            0.5 + 0.5 * d / 10.0
        };
        let hydro = Hydrology::compute(region(21, 21), &cfg, bowl);
        assert_eq!(hydro.water_at(10, 10), Water::Lake);
    }

    #[test]
    fn ocean_classe_comme_ocean() {
        let cfg = HydrologyConfig::default();
        let hydro = Hydrology::compute(region(10, 10), &cfg, |_, _| -0.3);
        assert_eq!(hydro.water_at(5, 5), Water::Ocean);
    }

    #[test]
    fn deterministe() {
        let cfg = HydrologyConfig::default();
        let f = |x: i64, y: i64| ((x * 7 + y * 13) % 100) as f64 / 100.0;
        let a = Hydrology::compute(region(32, 32), &cfg, f);
        let b = Hydrology::compute(region(32, 32), &cfg, f);
        for i in 0..32 * 32 {
            let (x, y) = ((i % 32) as i64, (i / 32) as i64);
            assert_eq!(a.accumulation_at(x, y).to_bits(), b.accumulation_at(x, y).to_bits());
        }
    }
}
