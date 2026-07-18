//! La délibération : l'étage « Tâche » de l'utility AI (BRIEF §4).
//!
//! Un agent qui délibère : (1) perçoit son contexte local — la source la plus
//! proche, la meilleure tuile à fourrager, un couvert forestier ; (2) score
//! chaque action **candidate** par des courbes de réponse sur ses besoins,
//! pondérées par le coût du trajet ; (3) tire au **softmax**. La tâche élue
//! persiste ensuite plusieurs ticks : on ne re-délibère que périodiquement
//! (bucketing temporel, §4) et un léger bonus d'engagement évite le
//! papillonnage entre deux tâches presque équivalentes.
//!
//! Phase 2 : perception **locale et sans mémoire** — l'agent redécouvre son
//! voisinage à chaque délibération. La carte mentale individuelle (savoir où
//! était l'eau il y a dix jours) est le cœur de la Phase 3.

use cairn_core::{Pcg32, SimTime, km_to_tiles, splitmix64};
use cairn_worldgen::Biome;

use crate::agent::{AgentId, Physiology, Position, Task, TaskKind, WALK_TILES_PER_TICK};
use crate::curves::{Curve, softmax_pick};
use crate::fauna::HerdView;
use crate::salt;
use crate::world::World;

/// Un agent re-délibère toutes les 4 h (et dès qu'il n'a plus de tâche).
/// Décalé par agent : les délibérations s'étalent sur les ticks.
pub const DELIBERATION_PERIOD: u64 = 4;
/// Température du softmax : bas = discipliné, haut = fantasque.
pub const SOFTMAX_TAU: f32 = 0.12;
/// Bonus accordé à la tâche en cours : l'inertie qui fait finir les choses.
const COMMITMENT_BONUS: f32 = 0.08;

/// Rayon (en chunks) de la recherche d'eau : 5 chunks ≈ 700 m. Large, mais
/// quasi gratuit grâce au cache de sources (aucune tuile matérialisée) — et
/// sans lui, trop d'agents mouraient de soif à 500 m d'une source.
const SPRING_RADIUS_CHUNKS: i64 = 5;
/// Rayon (en tuiles) et pas de la recherche de nourriture.
const FORAGE_RADIUS: i64 = 24;
const FORAGE_STRIDE: i64 = 3;
/// Rayon et pas de la recherche d'un couvert forestier.
const SHELTER_RADIUS: i64 = 32;
const SHELTER_STRIDE: i64 = 4;
/// Longueur d'une jambe d'errance (~500 m). Borne aussi l'étalement spatial
/// de la population, donc le working set de chunks : à 600 tuiles, 100
/// errants couvraient plus de chunks que le LRU n'en tient, et chaque
/// lecture régénérait un chunk (thrash mesuré : la simulation s'effondrait).
const WANDER_LEG_TILES: f64 = 250.0;
/// En dessous de cette biomasse, une tuile ne vaut pas le déplacement.
const FORAGE_MIN_BIOMASS: u8 = 5;
/// Distance à laquelle un chasseur repère du gibier (~2 km). Large : c'est
/// une bête de plusieurs centaines de kilos dans un paysage ouvert.
const HERD_SIGHT_TILES: f64 = km_to_tiles(2.0);

/// Choisit la prochaine tâche de l'agent. Déterministe : le tirage dérive de
/// (seed, tick, id agent) — deux exécutions rejouent la même hésitation.
pub fn decide(
    world: &mut World,
    time: SimTime,
    id: AgentId,
    pos: &Position,
    phys: &Physiology,
    current: Option<TaskKind>,
    herds: &[HerdView],
) -> Option<Task> {
    let here = pos.tile();
    let mut candidates: Vec<(TaskKind, (i64, i64), f32)> = Vec::new();

    // — Boire : seuil flou et raide, la soif devient vite impérieuse.
    let spring = world.nearest_spring(here, SPRING_RADIUS_CHUNKS);
    if let Some(target) = spring {
        let urgency = Curve::Logistic { steepness: 9.0, midpoint: 0.45 }.eval(phys.thirst);
        let score = urgency * travel_discount(pos.distance_tiles(target));
        candidates.push((TaskKind::Drink, target, score));
    }

    // — Manger : pression progressive, pondérée par l'abondance trouvée.
    let (forage_target, forage_biomass) = best_forage(world, here);
    if forage_biomass >= FORAGE_MIN_BIOMASS {
        let urgency = Curve::Logistic { steepness: 6.0, midpoint: 0.4 }.eval(phys.hunger);
        let abundance = (f32::from(forage_biomass) / 255.0).sqrt();
        let score =
            urgency * abundance * travel_discount(pos.distance_tiles(forage_target));
        candidates.push((TaskKind::Forage, forage_target, score));
    }

    // — Chasser : même pression de faim que la cueillette, mais une prise
    //   nourrit bien davantage. Le gibier proche et nombreux attire ; quand
    //   les troupeaux ont été décimés ou ont fui, ce candidat s'efface tout
    //   seul et la faim repart sur la cueillette ou l'errance. C'est de là
    //   que sort le suivi des troupeaux — rien ne dit « suis le gibier ».
    let nearest_herd = nearest_herd(pos, herds);
    if let Some(herd) = nearest_herd {
        let target = (herd.pos.0.floor() as i64, herd.pos.1.floor() as i64);
        let urgency = Curve::Logistic { steepness: 6.0, midpoint: 0.4 }.eval(phys.hunger);
        // Un gros troupeau est une proie plus sûre (plus de bêtes à approcher).
        let size = Curve::Power { k: 0.5 }.eval(herd.population / 60.0);
        let score = urgency * (0.55 + 0.45 * size) * travel_discount(pos.distance_tiles(target));
        candidates.push((TaskKind::Hunt, target, score));
    }

    // — Dormir : sur place, surtout la nuit ; la fatigue extrême s'impose.
    let night_factor = if time.is_night() { 1.15 } else { 0.55 };
    let sleep_score = Curve::Power { k: 2.5 }.eval(phys.fatigue) * night_factor;
    candidates.push((TaskKind::Sleep, here, sleep_score));

    // — S'abriter : réponse linéaire au stress thermique, de préférence sous
    //   couvert forestier.
    if phys.cold > 0.05 {
        let target = nearest_forest(world, here).unwrap_or(here);
        let score = Curve::Linear { m: 1.2, b: 0.0 }.eval(phys.cold);
        candidates.push((TaskKind::Shelter, target, score));
    }

    // — Errer : bruit de fond exploratoire, qui enfle en désespoir quand un
    //   besoin monte sans solution locale. C'est lui qui disperse les groupes
    //   quand une zone s'épuise — personne ne le scripte.
    let mut desperation = 0.0;
    if spring.is_none() {
        desperation += 0.6 * phys.thirst;
    }
    // La faim ne pousse à partir que si **ni** la cueillette **ni** le gibier
    // ne répondent ici : c'est ce qui vide une zone surchassée et surpâturée.
    if forage_biomass < 30 && nearest_herd.is_none() {
        desperation += 0.5 * phys.hunger;
    }
    let wander_score = (0.06 + desperation).min(1.0);
    let mut wander_rng =
        Pcg32::new(world.seed().derive(salt::WANDER) ^ splitmix64(time.tick), id.0);
    let angle = wander_rng.next_f64() * std::f64::consts::TAU;
    // On vise la **terre** : on avance le long du rayon jusqu'à la dernière
    // tuile ferme avant l'eau. Sans ça, errer vers l'océan déclenchait un A*
    // (partiel, coûteux) pour un but inatteignable — cause n°1 de pathfinding
    // inutile près des côtes.
    let (dx, dy) = (angle.cos(), angle.sin());
    let mut wander_target = here;
    let mut d = 40.0;
    while d <= WANDER_LEG_TILES {
        let p = ((pos.x + dx * d).floor() as i64, (pos.y + dy * d).floor() as i64);
        if world.worldgen().elevation(p.0, p.1) <= 0.0 {
            break; // eau : on s'arrête à la dernière terre trouvée
        }
        wander_target = p;
        d += 40.0;
    }
    candidates.push((TaskKind::Wander, wander_target, wander_score));

    // — Engagement : la tâche en cours part avec une longueur d'avance.
    if let Some(kind) = current {
        for c in &mut candidates {
            if c.0 == kind {
                c.2 += COMMITMENT_BONUS;
            }
        }
    }

    let scores: Vec<f32> = candidates.iter().map(|c| c.2).collect();
    let mut rng =
        Pcg32::new(world.seed().derive(salt::DECISIONS) ^ splitmix64(time.tick), id.0);
    let (kind, target, _) = candidates[softmax_pick(&scores, SOFTMAX_TAU, &mut rng)];
    Some(Task { kind, target })
}

/// Décote de trajet : 1 à distance nulle, ½ à une heure de marche.
fn travel_discount(dist_tiles: f64) -> f32 {
    (1.0 / (1.0 + dist_tiles / WALK_TILES_PER_TICK)) as f32
}

/// Le troupeau visible le plus proche. Départage déterministe : distance,
/// puis ordre de l'instantané.
fn nearest_herd(pos: &Position, herds: &[HerdView]) -> Option<HerdView> {
    let mut best: Option<(f64, HerdView)> = None;
    for h in herds {
        let d2 = (pos.x - h.pos.0).powi(2) + (pos.y - h.pos.1).powi(2);
        if d2 <= HERD_SIGHT_TILES * HERD_SIGHT_TILES && best.is_none_or(|(bd, _)| d2 < bd) {
            best = Some((d2, *h));
        }
    }
    best.map(|(_, h)| h)
}

/// La tuile la plus fournie en biomasse autour de `from` (échantillonnage en
/// grille, ordre de parcours fixe → déterministe). Renvoie (tuile, biomasse).
fn best_forage(world: &mut World, from: (i64, i64)) -> ((i64, i64), u8) {
    let mut best = (from, 0u8);
    let mut best_d2 = i64::MAX;
    let mut dy = -FORAGE_RADIUS;
    while dy <= FORAGE_RADIUS {
        let mut dx = -FORAGE_RADIUS;
        while dx <= FORAGE_RADIUS {
            let p = (from.0 + dx, from.1 + dy);
            let tile = world.tile(p.0, p.1);
            let d2 = dx * dx + dy * dy;
            // Mieux fourni, ou aussi fourni mais plus proche.
            if tile.is_walkable()
                && (tile.biomass > best.1 || (tile.biomass == best.1 && d2 < best_d2))
            {
                best = (p, tile.biomass);
                best_d2 = d2;
            }
            dx += FORAGE_STRIDE;
        }
        dy += FORAGE_STRIDE;
    }
    best
}

/// La tuile forestière la plus proche (couvert = abri passif contre le froid).
fn nearest_forest(world: &mut World, from: (i64, i64)) -> Option<(i64, i64)> {
    let mut best: Option<(i64, (i64, i64))> = None;
    let mut dy = -SHELTER_RADIUS;
    while dy <= SHELTER_RADIUS {
        let mut dx = -SHELTER_RADIUS;
        while dx <= SHELTER_RADIUS {
            let p = (from.0 + dx, from.1 + dy);
            let tile = world.tile(p.0, p.1);
            if matches!(
                tile.biome,
                Biome::TemperateForest | Biome::TropicalForest | Biome::Taiga
            ) {
                let candidate = (dx * dx + dy * dy, p);
                if best.is_none_or(|b| candidate < b) {
                    best = Some(candidate);
                }
            }
            dx += SHELTER_STRIDE;
        }
        dy += SHELTER_STRIDE;
    }
    best.map(|(_, p)| p)
}
