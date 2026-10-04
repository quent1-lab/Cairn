//! A* **budgété** sur une grille de navigation grossière (BRIEF §8.2).
//!
//! Le monde est immense (tuile = 2 m) : un A* pleine résolution sur des
//! centaines de tuiles serait ruineux. On raisonne donc sur une grille de
//! **cellules de [`NAV_STRIDE`] tuiles** — assez fine pour contourner baies,
//! caps et lacs, assez grossière pour qu'un budget de quelques milliers de
//! nœuds porte à ~1 km. Deux garde-fous :
//! - **budget de nœuds** par requête : au-delà, on rend le **meilleur chemin
//!   partiel** (vers la cellule de la frontière au plus petit coût estimé) —
//!   l'agent progresse et recalcule plus tard, plutôt que de figer le tick ;
//! - **budget de requêtes** par tick (côté appelant) : on ne lance pas 5 000
//!   A* d'un coup.
//!
//! L'A* n'est appelé que quand la marche en ligne droite bute sur l'eau : sur
//! terrain ouvert (le cas courant), on ne paie rien.
//!
//! Déterminisme : coûts **entiers** (pas de flottant dans le tas), tas ordonné
//! par `(f, cellule)` — ordre total, ex æquo départagés par les coordonnées —,
//! `BTreeMap`/`BTreeSet` partout.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

/// Côté d'une cellule de navigation, en tuiles (16 → 32 m).
pub const NAV_STRIDE: i64 = 16;

/// Coûts entiers de déplacement d'une cellule à l'autre (×1000).
const ORTHO: i64 = 1000;
const DIAG: i64 = 1414; // ≈ √2 · 1000

/// Nœuds expansés au maximum par requête. ~2500 cellules ≈ rayon 28 cellules
/// ≈ 450 tuiles ≈ 900 m de portée de détour ; au-delà, le meilleur partiel et
/// un recalcul plus tard. Volontairement borné : chaque nœud coûte des
/// évaluations de bruit (élévation), donc c'est ce qui plafonne le coût CPU.
pub const NODE_BUDGET: usize = 2500;

/// Les 8 voisins d'une cellule.
const NEIGHBORS: [(i64, i64); 8] = [
    (1, 0), (-1, 0), (0, 1), (0, -1),
    (1, 1), (1, -1), (-1, 1), (-1, -1),
];

/// La ligne droite de `a` à `b` reste-t-elle sur la terre ? (échantillon
/// toutes les 2 tuiles.)
fn segment_on_land(a: (i64, i64), b: (i64, i64), is_land: &impl Fn(i64, i64) -> bool) -> bool {
    let (dx, dy) = ((b.0 - a.0) as f64, (b.1 - a.1) as f64);
    let n = (dx.hypot(dy) / 2.0).ceil() as i64;
    (0..=n).all(|k| {
        let t = k as f64 / n.max(1) as f64;
        is_land((a.0 as f64 + 0.5 + dx * t).floor() as i64, (a.1 as f64 + 0.5 + dy * t).floor() as i64)
    })
}

/// Distance octile (heuristique admissible et **consistante** pour une grille
/// 8-connexe) entre deux cellules, en coût ×1000.
fn heuristic(a: (i64, i64), b: (i64, i64)) -> i64 {
    let dx = (a.0 - b.0).abs();
    let dy = (a.1 - b.1).abs();
    ORTHO * dx.max(dy) + (DIAG - ORTHO) * dx.min(dy)
}

fn to_cell(tile: (i64, i64)) -> (i64, i64) {
    (tile.0.div_euclid(NAV_STRIDE), tile.1.div_euclid(NAV_STRIDE))
}

/// Tuile au centre d'une cellule — le point où l'agent vise réellement.
fn cell_center(cell: (i64, i64)) -> (i64, i64) {
    (cell.0 * NAV_STRIDE + NAV_STRIDE / 2, cell.1 * NAV_STRIDE + NAV_STRIDE / 2)
}

/// Cherche un chemin de `start_tile` à `goal_tile` en contournant l'eau.
/// `is_land(x, y)` dit si une tuile est franchissable (terre).
///
/// Renvoie une suite de **tuiles-étapes** (centres de cellules) à suivre en
/// ligne droite, la dernière étant le but exact si le but est atteint ; ou le
/// meilleur chemin partiel si le budget est épuisé ; ou `None` si l'agent est
/// totalement cerné (aucune cellule voisine n'est terre).
pub fn astar(
    start_tile: (i64, i64),
    goal_tile: (i64, i64),
    is_land: impl Fn(i64, i64) -> bool,
    node_budget: usize,
) -> Option<Vec<(i64, i64)>> {
    let start = to_cell(start_tile);
    let goal = to_cell(goal_tile);
    if start == goal {
        return Some(vec![goal_tile]);
    }

    // Franchissabilité mise en **cache par cellule** : sans lui, chaque cellule
    // serait re-testée jusqu'à 8 fois (une par voisine qui la considère), et le
    // test = une évaluation de bruit fBm coûteuse. Le cache la ramène à une
    // fois par cellule — le gros du gain CPU de l'A*.
    let mut land: BTreeMap<(i64, i64), bool> = BTreeMap::new();
    let mut land_cell = |c: (i64, i64)| -> bool {
        *land.entry(c).or_insert_with(|| {
            let (x, y) = cell_center(c);
            is_land(x, y)
        })
    };

    let mut open: BinaryHeap<Reverse<(i64, (i64, i64))>> = BinaryHeap::new();
    let mut g: BTreeMap<(i64, i64), i64> = BTreeMap::new();
    let mut came: BTreeMap<(i64, i64), (i64, i64)> = BTreeMap::new();
    let mut closed: BTreeSet<(i64, i64)> = BTreeSet::new();

    g.insert(start, 0);
    open.push(Reverse((heuristic(start, goal), start)));
    // Meilleure cellule vue (plus proche du but), pour le repli partiel.
    let mut best = (heuristic(start, goal), start);
    let mut expanded = 0usize;

    while let Some(Reverse((_, cell))) = open.pop() {
        if cell == goal {
            return Some(reconstruct(&came, cell, Some(goal_tile)));
        }
        // Tas paresseux : une cellule peut y figurer plusieurs fois ; on ne
        // l'expanse qu'une fois (heuristique consistante ⇒ 1er pop optimal).
        if !closed.insert(cell) {
            continue;
        }
        if expanded >= node_budget {
            // Budget épuisé : on suit la cellule de la frontière au plus petit
            // coût total estimé (celle que l'A* aurait explorée ensuite), et
            // non la plus proche du but à vol d'oiseau — celle-là est souvent
            // la rive d'en face, un cul-de-sac d'où l'on ne repart jamais
            // (chantier de l'eau, piste E : on mourait de soif à 331 m d'une
            // source de l'autre côté d'un lac).
            let frontier = std::iter::once(Reverse((g[&cell] + heuristic(cell, goal), cell)))
                .chain(open.into_iter())
                .filter(|Reverse((_, c))| *c != start && (*c == cell || !closed.contains(c)))
                .min_by_key(|Reverse(entry)| *entry);
            return match frontier {
                Some(Reverse((_, c))) => Some(reconstruct(&came, c, None)),
                None => None,
            };
        }
        expanded += 1;

        let cg = g[&cell];
        for (dx, dy) in NEIGHBORS {
            let nb = (cell.0 + dx, cell.1 + dy);
            if closed.contains(&nb) || !land_cell(nb) {
                continue;
            }
            // Depuis la cellule de départ, la première étape doit se rejoindre
            // à pied depuis la tuile **exacte** de l'agent (piste E3 : sur une
            // langue de terre, le segment vers le centre d'une cellule voisine
            // traversait l'eau ; la marche butait au premier pas, et l'assoiffé
            // restait 40 heures à 330 m d'une source).
            if cell == start && !segment_on_land(start_tile, cell_center(nb), &is_land) {
                continue;
            }
            let step = if dx != 0 && dy != 0 { DIAG } else { ORTHO };
            let ng = cg + step;
            if g.get(&nb).is_none_or(|&old| ng < old) {
                g.insert(nb, ng);
                came.insert(nb, cell);
                let h = heuristic(nb, goal);
                open.push(Reverse((ng + h, nb)));
                if h < best.0 {
                    best = (h, nb);
                }
            }
        }
    }

    // Pas de chemin complet : on rend le meilleur partiel, si on a bougé.
    if best.1 != start {
        Some(reconstruct(&came, best.1, None))
    } else {
        None
    }
}

/// Remonte `came_from` de `end` jusqu'au départ et renvoie les centres de
/// cellules dans l'ordre du trajet (départ exclu). Si `exact_goal` est fourni
/// (but atteint), la dernière étape est ce point exact plutôt que le centre
/// de la cellule but.
fn reconstruct(
    came: &BTreeMap<(i64, i64), (i64, i64)>,
    end: (i64, i64),
    exact_goal: Option<(i64, i64)>,
) -> Vec<(i64, i64)> {
    let mut cells = Vec::new();
    let mut cur = end;
    while let Some(&prev) = came.get(&cur) {
        cells.push(cur);
        cur = prev;
    }
    cells.reverse();
    let mut path: Vec<(i64, i64)> = cells.iter().map(|&c| cell_center(c)).collect();
    if let (Some(goal), Some(last)) = (exact_goal, path.last_mut()) {
        *last = goal;
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Terre partout : le chemin est direct (une étape = le but).
    #[test]
    fn terrain_ouvert_chemin_direct() {
        let path = astar((0, 0), (200, 0), |_, _| true, NODE_BUDGET).unwrap();
        assert!(!path.is_empty());
        assert_eq!(*path.last().unwrap(), (200, 0), "doit finir au but exact");
    }

    /// Un mur d'eau vertical avec une **brèche** : l'agent doit la trouver.
    /// Mur en x ∈ [96, 112) (une bande de cellules), sauf une ouverture en
    /// y ∈ [0, 16) (la cellule (6, 0)).
    #[test]
    fn contourne_un_mur_par_la_breche() {
        let is_land = |x: i64, y: i64| {
            let wall = (96..112).contains(&x);
            let gap = (0..16).contains(&y);
            !(wall && !gap)
        };
        // Départ à gauche du mur, but à droite, décalés en y : sans la brèche
        // aucun passage direct.
        let path = astar((32, 320), (200, 320), is_land, NODE_BUDGET)
            .expect("un chemin par la brèche existe");
        // Toutes les étapes tombent sur de la terre.
        for &(x, y) in &path {
            assert!(is_land(x, y), "étape ({x}, {y}) dans l'eau");
        }
        // Le chemin passe par la brèche (une étape avec y < 16).
        assert!(
            path.iter().any(|&(_, y)| y < 16),
            "le chemin devrait emprunter la brèche"
        );
        assert_eq!(*path.last().unwrap(), (200, 320));
    }

    /// But inatteignable (mur d'eau **complet**) : on rend un partiel non vide
    /// qui reste sur la terre, du bon côté du mur.
    #[test]
    fn mur_complet_rend_un_partiel() {
        let is_land = |x: i64, _y: i64| !(96..112).contains(&x);
        let path = astar((32, 0), (400, 0), is_land, NODE_BUDGET).unwrap();
        assert!(!path.is_empty());
        for &(x, y) in &path {
            assert!(is_land(x, y));
            assert!(x < 96, "le partiel ne doit pas franchir le mur");
        }
    }

    /// Complètement cerné par l'eau : aucun chemin.
    #[test]
    fn cerne_par_l_eau_aucun_chemin() {
        let ile = |x: i64, y: i64| (0..16).contains(&x) && (0..16).contains(&y);
        assert!(astar((8, 8), (500, 500), ile, NODE_BUDGET).is_none());
    }

    /// Déterminisme : deux appels identiques donnent le même chemin.
    #[test]
    fn deterministe() {
        let is_land = |x: i64, y: i64| !((96..112).contains(&x) && !(0..16).contains(&y));
        let a = astar((32, 500), (300, 500), is_land, NODE_BUDGET);
        let b = astar((32, 500), (300, 500), is_land, NODE_BUDGET);
        assert_eq!(a, b);
    }

    /// Le budget de nœuds borne le coût : un très petit budget rend un partiel
    /// (ou rien), jamais une boucle infinie.
    #[test]
    fn budget_serre_termine() {
        let path = astar((0, 0), (100_000, 0), |_, _| true, 10);
        // Avec 10 nœuds on ne rejoint pas un but à 100 km ; on rend un partiel.
        assert!(path.is_some());
        assert!(path.unwrap().len() <= 12);
    }

    /// Piste E du chantier de l'eau : un lac dont le tour dépasse la portée
    /// d'une recherche (~900 m). Le partiel visait la cellule la plus proche
    /// du but à vol d'oiseau — la rive d'en face —, puis, relancé de là, ne
    /// trouvait rien de plus proche : « cerné », la source était déclarée
    /// inaccessible, et l'assoiffé mourait à 331 m de l'eau (trace réelle).
    /// En suivant, recalcul après recalcul, ce que rend l'A*, on doit finir
    /// par contourner le lac.
    #[test]
    fn un_long_detour_finit_par_contourner_le_lac() {
        // Un mur d'eau de 4 km de large (y entre 0 et 47), le but juste derrière.
        let is_land = |x: i64, y: i64| !((-1000..1000).contains(&x) && (0..48).contains(&y));
        let goal = (0, 100);
        let mut at = (0, -40);
        for _ in 0..40 {
            let Some(path) = astar(at, goal, is_land, NODE_BUDGET) else {
                panic!("déclaré cerné en {at:?} alors que le lac se contourne");
            };
            let last = *path.last().unwrap();
            if last == goal {
                return;
            }
            assert_ne!(last, at, "le partiel doit faire progresser");
            at = last;
        }
        panic!("40 recalculs sans contourner le lac (arrêté en {at:?})");
    }

    /// Piste E3 du chantier de l'eau (trace réelle : l'A* trouvait un chemin
    /// complet de 52 étapes, et l'assoiffé ne bougeait pas pendant 40 heures).
    /// L'A* raisonne de centre de cellule en centre de cellule ; le premier
    /// tronçon, de la tuile exacte de l'agent à la première étape, n'était
    /// jamais vérifié — s'il traverse l'eau, la marche bute au premier pas.
    #[test]
    fn le_premier_troncon_se_marche_depuis_la_tuile_exacte() {
        // Un mur d'eau (x de 10 à 13, y de 0 à 19) entre l'agent et la cellule
        // voisine de l'est ; on le contourne par le nord.
        let is_land = |x: i64, y: i64| !((10..14).contains(&x) && (0..20).contains(&y));
        let start = (2, 8);
        let path = astar(start, (100, 8), is_land, NODE_BUDGET).expect("un chemin existe");
        let first = path[0];
        let (dx, dy) = ((first.0 - start.0) as f64, (first.1 - start.1) as f64);
        let n = dx.hypot(dy).ceil() as i64;
        let clear = (0..=n).all(|k| {
            let t = k as f64 / n.max(1) as f64;
            is_land((start.0 as f64 + dx * t).floor() as i64, (start.1 as f64 + dy * t).floor() as i64)
        });
        assert!(clear, "le premier tronçon {start:?} → {first:?} traverse l'eau");
    }
}
