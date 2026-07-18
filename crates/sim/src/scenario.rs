//! Amorçage de scènes : trouver un foyer habitable et y lâcher une population
//! avec son gibier. Mutualisé entre la démo native et le client web pour que
//! les deux montrent le même genre de départ.

use cairn_core::km_to_tiles;
use cairn_worldgen::Biome;

use crate::fauna;
use crate::sim::Sim;

/// Cherche un foyer tempéré (prairie ou forêt tempérée, climat doux, une
/// source à portée) en spirale depuis `around`. Balayage déterministe en
/// anneaux de 20 km ; sonde le worldgen sans matérialiser de chunks.
pub fn find_home(sim: &mut Sim, around: (i64, i64)) -> (i64, i64) {
    let step = km_to_tiles(20.0) as i64;
    for ring in 0..200i64 {
        let r = ring * step;
        let mut candidates = Vec::new();
        if ring == 0 {
            candidates.push(around);
        } else {
            for i in (-ring..=ring).map(|i| i * step) {
                candidates.push((around.0 + i, around.1 - r));
                candidates.push((around.0 + i, around.1 + r));
                candidates.push((around.0 - r, around.1 + i));
                candidates.push((around.0 + r, around.1 + i));
            }
        }
        for (x, y) in candidates {
            let wg = sim.world.worldgen();
            let e = wg.elevation(x, y);
            if e <= 0.0 {
                continue;
            }
            let t = wg.mean_temperature(x, y, e);
            if !(6.0..=18.0).contains(&t) {
                continue;
            }
            if !matches!(wg.biome(x, y), Biome::Grassland | Biome::TemperateForest) {
                continue;
            }
            if sim.world.nearest_spring((x, y), 4).is_some() {
                return (x, y);
            }
        }
    }
    around
}

/// Lâche `agents` humains autour de `home` (grille de pas 12 tuiles, terres
/// seulement) et `herd_rings` couronnes de gibier. Renvoie le nombre d'agents
/// effectivement placés.
pub fn populate(sim: &mut Sim, home: (i64, i64), agents: usize, herd_grid: i64) -> usize {
    let mut placed = 0;
    let mut ring = 0i64;
    while placed < agents && ring < 200 {
        ring += 1;
        let r = ring * 12;
        for dy in (-r..=r).step_by(12) {
            for dx in (-r..=r).step_by(12) {
                if dx.abs() < r && dy.abs() < r {
                    continue; // seulement la couronne courante
                }
                let (x, y) = (home.0 + dx, home.1 + dy);
                if sim.world.tile(x, y).is_walkable() {
                    sim.spawn_agent(x as f64 + 0.5, y as f64 + 0.5);
                    placed += 1;
                    if placed >= agents {
                        break;
                    }
                }
            }
        }
    }

    // Gibier sur une grille à ~1,5 km de pas, sur les terres fournies en herbe.
    let pas = km_to_tiles(1.5) as i64;
    for gy in -herd_grid..=herd_grid {
        for gx in -herd_grid..=herd_grid {
            let (x, y) = (home.0 + gx * pas, home.1 + gy * pas);
            let tile = sim.world.tile(x, y);
            if tile.is_walkable() && tile.biomass > 40 {
                sim.spawn_herd(x as f64, y as f64, fauna::HERD_START);
            }
        }
    }
    // Quelques meutes en périphérie ; elles trouveront le gibier seules.
    for i in 0..4i64 {
        let angle = i as f64 * 1.57;
        let (x, y) = (
            home.0 + (angle.cos() * km_to_tiles(4.0)) as i64,
            home.1 + (angle.sin() * km_to_tiles(4.0)) as i64,
        );
        if sim.world.tile(x, y).is_walkable() {
            sim.spawn_pack(x as f64, y as f64, fauna::PACK_START);
        }
    }
    placed
}

/// Lâche une **bande** de `humans` humains autour du point `at` (spirale de
/// tuiles franchissables), plus du gibier auto-généré alentour (`herds`
/// troupeaux et une meute). Sert au placement manuel depuis le client : le
/// joueur désigne le lieu et le nombre d'humains, les animaux suivent.
/// Renvoie le nombre d'humains réellement placés (l'eau en écarte).
pub fn drop_band(sim: &mut Sim, at: (i64, i64), humans: usize, herds: usize) -> usize {
    let mut placed = 0;
    // Spirale carrée croissante : dépose serré, en débordant sur l'eau sans y
    // poser personne.
    let mut radius = 0i64;
    while placed < humans && radius < 64 {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                // Uniquement l'anneau courant (bord du carré).
                if dx.abs() != radius && dy.abs() != radius {
                    continue;
                }
                let (x, y) = (at.0 + dx * 3, at.1 + dy * 3);
                if sim.world.tile(x, y).is_walkable() {
                    sim.spawn_agent(x as f64 + 0.5, y as f64 + 0.5);
                    placed += 1;
                    if placed >= humans {
                        break;
                    }
                }
            }
            if placed >= humans {
                break;
            }
        }
        radius += 1;
    }

    // Gibier auto-généré en couronne autour de la bande (~1 km), sur l'herbe.
    let ring = km_to_tiles(1.0);
    for i in 0..herds {
        let angle = i as f64 * 2.399_963; // ~golden angle : réparti, non aligné
        let (x, y) = (
            at.0 + (angle.cos() * ring) as i64,
            at.1 + (angle.sin() * ring) as i64,
        );
        let tile = sim.world.tile(x, y);
        if tile.is_walkable() && tile.biomass > 30 {
            sim.spawn_herd(x as f64, y as f64, fauna::HERD_START);
        }
    }
    // Une meute un peu plus loin.
    let (px, py) = (at.0 + km_to_tiles(3.0) as i64, at.1);
    if sim.world.tile(px, py).is_walkable() {
        sim.spawn_pack(px as f64, py as f64, fauna::PACK_START);
    }
    placed
}
