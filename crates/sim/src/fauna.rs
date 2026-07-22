//! La faune : troupeaux d'herbivores et meutes de prédateurs (BRIEF §2.4,
//! §3.2), en dynamique proie-prédateur **spatialisée**.
//!
//! **Choix de conception : l'entité est le troupeau, pas l'animal.** Le brief
//! demande une faune « simulée plus légèrement que les humains » (§3.2), et
//! le troupeau est de toute façon l'unité qui se déplace, se scinde et migre.
//! Un troupeau porte donc un **effectif continu** (`f32`) : on obtient la
//! dynamique de Lotka-Volterra sur cet effectif — croissance sur la pâture,
//! pertes par prédation et par chasse — sans payer 10 000 entités, et le
//! steering (§3.2 : machine à états + steering behaviors) s'applique au
//! groupe, ce qui est précisément l'échelle où « cohésion de troupeau » a un
//! sens.
//!
//! Rien n'est scripté ici. Les comportements observables tombent des règles :
//! - le troupeau broute la tuile la plus fournie à sa portée, donc il **laisse
//!   une traînée pâturée derrière lui** et avance vers l'herbe fraîche ;
//! - quand la pâture régionale s'effondre (surpâturage) ou que l'hiver arrête
//!   la croissance, sa satiété chute : il **migre vers le chaud** — la
//!   migration saisonnière n'est pas un calendrier, c'est une conséquence ;
//! - les prédateurs suivent les proies et s'effondrent quand elles manquent ;
//! - un troupeau trop nombreux **fissionne**, un troupeau décimé disparaît.

use cairn_core::{Pcg32, SimTime, WorldSeed, km_to_tiles, splitmix64};

use crate::agent::Position;
use crate::climate::Climate;
use crate::salt;
use crate::world::World;

/// Identifiant stable d'une entité de faune (comme `AgentId` pour les
/// humains) : c'est lui qui nourrit les flux RNG, jamais l'`Entity` de hecs,
/// qui recycle ses slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FaunaId(pub u64);

/// Pas de temps d'un tick, en jours — les taux vitaux s'écrivent par jour.
const DT_DAYS: f32 = 1.0 / 24.0;

// — Démographie des herbivores —

/// Effectif d'un troupeau à sa création.
pub const HERD_START: f32 = 25.0;
/// Sous cet effectif, le troupeau n'est plus viable : il disparaît (les
/// survivants sont réputés dispersés).
pub const HERD_MIN: f32 = 4.0;
/// Au-dessus, le troupeau devient ingérable et **fissionne** en deux.
pub const HERD_FISSION: f32 = 90.0;
/// Natalité maximale (pâture pleine), par jour.
const HERB_BIRTH_PER_DAY: f32 = 0.020;
/// Mortalité de fond, par jour. Le rapport mortalité/natalité fixe la
/// **satiété d'équilibre** : ici 0,5 — en dessous le troupeau fond, au-dessus
/// il croît. Aucune capacité de charge n'est écrite en dur : elle émerge de
/// ce que la pâture peut soutenir.
const HERB_DEATH_PER_DAY: f32 = 0.010;
/// Biomasse broutée par tête et par tick, étalée sur la tuile et ses voisines.
const GRAZE_PER_HEAD: f32 = 1.0;

// — Démographie des prédateurs —

pub const PACK_START: f32 = 5.0;
pub const PACK_MIN: f32 = 1.0;
/// Proies tuées par prédateur et par jour, quand le gibier est à portée.
const PRED_KILL_PER_DAY: f32 = 0.18;
/// Prédateurs entretenus par proie tuée (rendement trophique).
const PRED_CONV: f32 = 0.55;
/// Mortalité de fond des prédateurs, par jour.
const PRED_DEATH_PER_DAY: f32 = 0.020;

// — Déplacements (tuiles par tick, soit par heure) —

/// Dérive d'un troupeau qui broute : ~80 m/h, un pâturage qui avance.
const GRAZE_STEP_TILES: f64 = 40.0;
/// Portée d'échantillonnage de l'herbe autour du troupeau (~160 m).
const GRAZE_SAMPLE_TILES: i64 = 80;
/// Fuite : ~2 km avalés d'un trait.
const FLEE_STEP_TILES: f64 = km_to_tiles(2.0);
/// Distance à laquelle un troupeau détecte une menace (~600 m).
const FLEE_RADIUS_TILES: f64 = km_to_tiles(0.6);
/// **Souffle** : après un galop, le troupeau est à bout et ne peut plus
/// détaler pendant ces quelques heures — il reste sur ses gardes, sans
/// paître.
///
/// Cette limite n'est pas un détail d'équilibrage, c'est *le* mécanisme qui
/// rend la chasse humaine possible : une bête court plus vite qu'un homme
/// mais pas plus longtemps. Le chasseur qui marche sans relâche finit par la
/// rejoindre — c'est la **chasse à l'épuisement**, la plus ancienne qu'on
/// connaisse. Sans souffle, la proie détalait à chaque tick et aucune chasse
/// n'aboutissait jamais (mesuré : 0 prise, et le couple traqueur/troupeau
/// dérivait sur des milliers de km).
const FLEE_COOLDOWN: u16 = 3;
/// Migration : dérive **lente**, ~150 m/h soit ~3,5 km/jour. Le brout suffit
/// à chercher l'herbe fraîche voisine (`best_pasture`) ; la migration n'est
/// que le lent glissement saisonnier vers le chaud. La faire rapide était une
/// erreur mesurée : à 500 tuiles/tick, une bande de troupeaux entrant en
/// hiver ensemble se ruait vers l'horizon en générant des chunks vierges par
/// centaines à chaque tick (thrash du LRU).
const MIGRATE_STEP_TILES: f64 = km_to_tiles(0.15);
/// Distance de sondage du climat pour choisir le cap (~600 m).
const MIGRATE_PROBE_TILES: i64 = 300;
/// Poursuite d'une meute vers le gibier (~1,5 km/h).
const PACK_STEP_TILES: f64 = km_to_tiles(1.5);
/// Portée à laquelle une meute chasse un troupeau (~500 m).
pub const PACK_HUNT_RADIUS_TILES: f64 = km_to_tiles(0.5);

/// Ce que fait un troupeau — la machine à états du brief (§3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HerdState {
    Grazing,
    Fleeing,
    Migrating,
}

#[derive(Debug, Clone, Copy)]
pub struct Herd {
    pub population: f32,
    pub state: HerdState,
    /// Ticks de souffle restants : tant qu'ils courent, la bête ne peut plus
    /// galoper (voir [`FLEE_COOLDOWN`]).
    pub flee_ticks: u16,
    /// Satiété du dernier tick (0–1) : lue par la démo et les overlays.
    pub satiation: f32,
}

impl Herd {
    pub fn new(population: f32) -> Self {
        Self {
            population,
            state: HerdState::Grazing,
            flee_ticks: 0,
            satiation: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Pack {
    pub population: f32,
    /// Proies tuées au dernier tick : le signal qui nourrit sa croissance.
    pub last_kills: f32,
}

impl Pack {
    pub fn new(population: f32) -> Self {
        Self { population, last_kills: 0.0 }
    }
}

/// Instantané d'un troupeau, pris avant les systèmes : c'est ce que lisent
/// les chasseurs humains, les meutes et les autres troupeaux. Travailler sur
/// un instantané (plutôt qu'en requêtant le monde pendant qu'on le mute)
/// garde l'ordre des lectures indépendant de l'ordre des écritures — donc le
/// déterminisme.
#[derive(Debug, Clone, Copy)]
pub struct HerdView {
    pub entity: hecs::Entity,
    pub pos: (f64, f64),
    pub population: f32,
}

/// Effectif retiré à un troupeau (prédation ou chasse humaine), à appliquer
/// après coup.
#[derive(Debug, Clone, Copy)]
pub struct Kill {
    pub herd: hecs::Entity,
    pub head: f32,
}

// — Équations pures (testables sans monde) —

/// Un tick de démographie herbivore : la natalité suit la satiété, la
/// mortalité est de fond. Renvoie le nouvel effectif.
pub fn herd_population_step(population: f32, satiation: f32) -> f32 {
    let rate = HERB_BIRTH_PER_DAY * satiation.clamp(0.0, 1.0) - HERB_DEATH_PER_DAY;
    (population + population * rate * DT_DAYS).max(0.0)
}

/// Un tick de démographie de meute : elle croît de ses prises, décline sans.
/// C'est l'équation prédateur de Lotka-Volterra — un prédateur sans proie
/// s'éteint, ce qui borne la pression sur le gibier sans aucun plafond écrit.
pub fn pack_population_step(population: f32, kills: f32) -> f32 {
    let growth = PRED_CONV * kills;
    let decay = PRED_DEATH_PER_DAY * population * DT_DAYS;
    (population + growth - decay).max(0.0)
}

/// Proies qu'une meute de `population` prédateurs prélève en un tick, bornée
/// par le gibier réellement présent.
pub fn pack_kills(population: f32, prey_available: f32) -> f32 {
    (PRED_KILL_PER_DAY * population * DT_DAYS).min(prey_available.max(0.0))
}

/// Vecteur unitaire fuyant `threat`. Si la menace est pile dessus, on part
/// vers +x plutôt que de diviser par zéro.
fn away_from(pos: (f64, f64), threat: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (pos.0 - threat.0, pos.1 - threat.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 { (1.0, 0.0) } else { (dx / len, dy / len) }
}

/// La menace la plus proche dans `threats`, si elle est dans le rayon de
/// détection. Départage déterministe par distance puis par ordre de la liste.
fn nearest_threat(pos: (f64, f64), threats: &[(f64, f64)]) -> Option<(f64, f64)> {
    let mut best: Option<(f64, (f64, f64))> = None;
    for &t in threats {
        let d2 = (pos.0 - t.0).powi(2) + (pos.1 - t.1).powi(2);
        if d2 <= FLEE_RADIUS_TILES * FLEE_RADIUS_TILES
            && best.is_none_or(|(bd, _)| d2 < bd)
        {
            best = Some((d2, t));
        }
    }
    best.map(|(_, t)| t)
}

/// Déplace `pos` de `step` tuiles dans la direction `dir`, en refusant
/// d'entrer dans l'eau : la faune terrestre ne traverse pas l'océan. Sondé
/// dans le **baseline** (élévation), comme la marche humaine — matérialiser
/// des chunks pour ça écroulerait le LRU.
fn try_move(world: &World, pos: &mut Position, dir: (f64, f64), step: f64) {
    let next = (pos.x + dir.0 * step, pos.y + dir.1 * step);
    if world.worldgen().elevation(next.0.floor() as i64, next.1.floor() as i64) > 0.0 {
        pos.x = next.0;
        pos.y = next.1;
    }
}

/// La tuile la plus fournie en biomasse à portée de brout, et sa biomasse.
/// Échantillonnage en croix (8 directions + sur place) : 9 lectures par
/// troupeau et par tick, pas un balayage.
///
/// **Les ex æquo sont départagés au hasard** (tirage dérivé de la seed, donc
/// déterministe), et c'est essentiel : sur une prairie uniforme les huit
/// voisins portent la même herbe, et un départage par l'ordre de la liste
/// ferait gagner toujours la même direction. Mesuré : tous les troupeaux du
/// monde marchaient **plein est en ligne droite à 1,9 km/jour** — une dérive
/// balistique au lieu d'une errance. Le hasard rend la marche diffusive,
/// c'est-à-dire un vrai domaine vital.
fn best_pasture(world: &mut World, rng: &mut Pcg32, pos: (f64, f64)) -> ((i64, i64), u8) {
    let here = (pos.0.floor() as i64, pos.1.floor() as i64);
    let r = GRAZE_SAMPLE_TILES;
    let mut candidates = [(here, 0u8); 9];
    let mut n = 0;
    let mut best_biomass = 0u8;
    for (dx, dy) in [
        (0, 0),
        (r, 0), (-r, 0), (0, r), (0, -r),
        (r, r), (-r, r), (r, -r), (-r, -r),
    ] {
        let p = (here.0 + dx, here.1 + dy);
        let tile = world.tile(p.0, p.1);
        if !tile.is_walkable() {
            continue;
        }
        match tile.biomass.cmp(&best_biomass) {
            std::cmp::Ordering::Greater => {
                best_biomass = tile.biomass;
                candidates[0] = (p, tile.biomass);
                n = 1;
            }
            std::cmp::Ordering::Equal => {
                candidates[n] = (p, tile.biomass);
                n += 1;
            }
            std::cmp::Ordering::Less => {}
        }
    }
    if n == 0 {
        return (here, 0); // cerné par l'eau : on ne bouge pas
    }
    candidates[(rng.next_u32() as usize) % n]
}

/// Cap de migration : vers le plus chaud, sondé au nord et au sud. C'est ce
/// qui produit la migration saisonnière — l'hiver rend la bande d'origine
/// stérile, le troupeau descend, et le printemps le fait remonter. Aucun
/// calendrier n'est consulté.
fn migration_heading(world: &mut World, climate: &Climate, pos: (f64, f64), time: SimTime) -> (f64, f64) {
    let here = (pos.0.floor() as i64, pos.1.floor() as i64);
    let (north, south) = (here.1 - MIGRATE_PROBE_TILES, here.1 + MIGRATE_PROBE_TILES);
    let t_north = {
        let tile = world.tile(here.0, north);
        climate.daily_mean(&tile, north, time)
    };
    let t_south = {
        let tile = world.tile(here.0, south);
        climate.daily_mean(&tile, south, time)
    };
    if t_north > t_south { (0.0, -1.0) } else { (0.0, 1.0) }
}

/// Le système des troupeaux : fuite, pâture, migration, démographie.
/// `threats` = positions des humains et des meutes (tout ce qui fait fuir).
/// Renvoie les entités à retirer et les scissions à créer — les mutations
/// structurelles se font hors itération.
pub fn update_herds(
    fauna: &mut hecs::World,
    world: &mut World,
    climate: &Climate,
    time: SimTime,
    seed: WorldSeed,
    threats: &[(f64, f64)],
) -> (Vec<hecs::Entity>, Vec<(f64, f64, f32)>) {
    let tick_seed = seed.derive(salt::FAUNA) ^ splitmix64(time.tick);
    let mut doomed = Vec::new();
    let mut fissions = Vec::new();

    for (entity, (id, herd, pos)) in fauna.query_mut::<(&FaunaId, &mut Herd, &mut Position)>() {
        let mut rng = Pcg32::new(tick_seed, id.0);

        // Le souffle revient, qu'il y ait une menace ou non.
        if herd.flee_ticks > 0 {
            herd.flee_ticks -= 1;
        }

        // — Fuite : elle prime sur tout le reste, y compris la faim — mais
        //   seulement si la bête a encore du souffle.
        let threat = nearest_threat((pos.x, pos.y), threats);
        let bolting = match threat {
            Some(t) if herd.flee_ticks == 0 => {
                herd.state = HerdState::Fleeing;
                herd.flee_ticks = FLEE_COOLDOWN;
                try_move(world, pos, away_from((pos.x, pos.y), t), FLEE_STEP_TILES);
                true
            }
            // Menacé mais à bout de souffle : il ne détale pas, et il ne
            // broute pas non plus — c'est là qu'il se fait prendre.
            Some(_) => {
                herd.state = HerdState::Fleeing;
                true
            }
            None => false,
        };

        if !bolting {
            // — Pâture : viser l'herbe la plus fournie à portée. C'est *elle*
            //   qui répond au manque local — un creux de pâture ne déclenche
            //   pas la migration, il pousse simplement le troupeau vers le
            //   voisin le plus vert. Une zone entièrement broutée fait donc
            //   fondre puis dériver le troupeau, sans stampede.
            let (target, biomass) = best_pasture(world, &mut rng, (pos.x, pos.y));
            herd.satiation = f32::from(biomass) / 255.0;

            let winter = {
                let tile = world.tile(pos.x.floor() as i64, pos.y.floor() as i64);
                !climate.grows(&tile, pos.y.floor() as i64, time)
            };
            if winter {
                // Seul l'hiver — la pâture gelée sur toute la bande — met le
                // troupeau en route vers le chaud. Lentement.
                herd.state = HerdState::Migrating;
                let dir = migration_heading(world, climate, (pos.x, pos.y), time);
                // Un peu de dispersion latérale : les troupeaux ne migrent
                // pas en file indienne sur le même méridien.
                let jitter = (rng.next_f64() - 0.5) * 0.6;
                let len = (1.0f64 + jitter * jitter).sqrt();
                try_move(world, pos, (jitter / len, dir.1 / len), MIGRATE_STEP_TILES);
            } else {
                herd.state = HerdState::Grazing;
                let dx = target.0 as f64 + 0.5 - pos.x;
                let dy = target.1 as f64 + 0.5 - pos.y;
                let len = (dx * dx + dy * dy).sqrt();
                if len > 1e-6 {
                    try_move(world, pos, (dx / len, dy / len), GRAZE_STEP_TILES.min(len));
                }
                graze(world, (pos.x, pos.y), herd.population);
            }
        }

        // — Démographie : la satiété commande.
        herd.population = herd_population_step(herd.population, herd.satiation);
        if herd.population < HERD_MIN {
            doomed.push(entity);
        } else if herd.population > HERD_FISSION {
            // Trop nombreux : la moitié part fonder un troupeau ailleurs.
            herd.population *= 0.5;
            let angle = rng.next_f64() * std::f64::consts::TAU;
            let d = km_to_tiles(1.0);
            fissions.push((pos.x + angle.cos() * d, pos.y + angle.sin() * d, herd.population));
        }
    }
    (doomed, fissions)
}

/// Le troupeau broute : la tuile sous lui et ses quatre voisines. C'est ce
/// qui laisse une traînée pâturée visible, et ce qui fait qu'un troupeau trop
/// gros épuise sa propre pâture.
fn graze(world: &mut World, pos: (f64, f64), population: f32) {
    let here = (pos.0.floor() as i64, pos.1.floor() as i64);
    let per_tile = (GRAZE_PER_HEAD * population / 5.0) as u16;
    for (dx, dy) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
        let tile = world.tile_mut(here.0 + dx, here.1 + dy);
        tile.biomass = tile.biomass.saturating_sub(per_tile.min(255) as u8);
    }
}

/// Le système des meutes : poursuite du gibier, prises, démographie.
/// Renvoie les prises à appliquer aux troupeaux et les meutes à retirer.
pub fn update_packs(
    fauna: &mut hecs::World,
    world: &World,
    herds: &[HerdView],
) -> (Vec<Kill>, Vec<hecs::Entity>) {
    let mut kills = Vec::new();
    let mut doomed = Vec::new();

    for (entity, (pack, pos)) in fauna.query_mut::<(&mut Pack, &mut Position)>() {
        // Gibier le plus proche : départage par distance puis ordre de la
        // liste (elle-même construite dans l'ordre d'itération) → déterministe.
        let mut nearest: Option<(f64, &HerdView)> = None;
        for h in herds {
            let d2 = (pos.x - h.pos.0).powi(2) + (pos.y - h.pos.1).powi(2);
            if nearest.is_none_or(|(bd, _)| d2 < bd) {
                nearest = Some((d2, h));
            }
        }

        pack.last_kills = 0.0;
        if let Some((d2, herd)) = nearest {
            let dist = d2.sqrt();
            if dist <= PACK_HUNT_RADIUS_TILES {
                let taken = pack_kills(pack.population, herd.population);
                pack.last_kills = taken;
                kills.push(Kill { herd: herd.entity, head: taken });
            } else {
                // Poursuite : la meute suit les troupeaux, donc elle suit
                // aussi leurs migrations — sans qu'on l'ait écrit.
                let dir = ((herd.pos.0 - pos.x) / dist, (herd.pos.1 - pos.y) / dist);
                try_move(world, pos, dir, PACK_STEP_TILES.min(dist));
            }
        }

        pack.population = pack_population_step(pack.population, pack.last_kills);
        if pack.population < PACK_MIN {
            doomed.push(entity);
        }
    }
    (kills, doomed)
}

/// Applique les prises (prédation + chasse humaine) aux troupeaux. L'ordre du
/// vecteur est déterministe, et un troupeau ramené sous le seuil sera retiré
/// au tick suivant par [`update_herds`].
pub fn apply_kills(fauna: &mut hecs::World, kills: &[Kill]) {
    for kill in kills {
        if let Ok(mut herd) = fauna.get::<&mut Herd>(kill.herd) {
            herd.population = (herd.population - kill.head).max(0.0);
        }
    }
}

// — Immigration —
//
// Un troupeau qui tombe sous `HERD_MIN` disparaît, et rien d'autre ne le
// remplace : contrairement à la végétation, qui repart toujours d'une
// banque de graines (`ecology::logistic_step`), un troupeau à 0 individu
// n'a pas de descendance possible. Une zone surchassée au point de perdre
// tous ses troupeaux restait donc vide *pour toujours* — observé et déjà
// noté comme limite en Phase 2 (« la coexistence durable suppose... un
// afflux de gibier »). Ce module ajoute cet afflux : chaque jour, une petite
// chance qu'un troupeau apparaisse près de la population, sur une tuile
// giboyeuse loin de tout troupeau existant. **`Sim::step` (pas ce module)
// garde l'appel derrière `hunted_head > 0`** : l'immigration ne repeuple
// qu'une zone déjà chassée, elle ne fait jamais apparaître de gibier depuis
// le néant — sans cette garde, un scénario délibérément sans faune (tests
// de cohésion sociale, `herd_grid=0`) en recevrait quand même.

/// Chance qu'un site candidat soit tenté par jour (indépendante du succès :
/// la plupart des tentatives échouent simplement le test de distance dans
/// une zone déjà giboyeuse — voir plus bas pourquoi c'est voulu).
const IMMIGRATION_CHANCE_PER_DAY: f32 = 0.15;
/// Le site candidat est tiré autour d'un humain vivant pris au hasard, entre
/// ces deux rayons : assez loin pour ne pas apparaître sous les pieds de
/// quelqu'un, assez proche pour rester dans la zone chargée (chunks déjà
/// résidents pour la plupart — pas de génération forcée au loin).
const IMMIGRATION_MIN_RADIUS_TILES: f64 = km_to_tiles(1.0);
const IMMIGRATION_MAX_RADIUS_TILES: f64 = km_to_tiles(4.0);
/// Aucun troupeau ne doit déjà se trouver à moins de cette distance du site
/// candidat. **C'est ce qui rend le mécanisme auto-limitant** : une zone
/// déjà giboyeuse n'a simplement jamais de site candidat valide — pas besoin
/// d'écrire « seulement si le gibier local est rare », l'effet en découle.
const IMMIGRATION_MIN_HERD_DISTANCE_TILES: f64 = km_to_tiles(3.0);
/// Biomasse minimale du site pour valoir la peine (même seuil que le
/// placement initial des troupeaux, `scenario::populate`).
const IMMIGRATION_MIN_BIOMASS: u8 = 40;

/// Le site `candidate` convainc-il ? Fonction pure (pas de RNG) : testable
/// sans tirage, séparée de [`daily_immigration`] qui, elle, choisit le site.
fn is_valid_immigration_site(
    candidate: (f64, f64),
    tile: &crate::Tile,
    herds: &[(f64, f64)],
) -> bool {
    if !tile.is_walkable() || tile.biomass < IMMIGRATION_MIN_BIOMASS {
        return false;
    }
    !herds.iter().any(|&(hx, hy)| {
        let d2 = (hx - candidate.0).powi(2) + (hy - candidate.1).powi(2);
        d2 < IMMIGRATION_MIN_HERD_DISTANCE_TILES * IMMIGRATION_MIN_HERD_DISTANCE_TILES
    })
}

/// Passe quotidienne : tire un site candidat près d'un humain au hasard, le
/// valide, et renvoie sa position si un troupeau doit y apparaître (au
/// crate appelant de le faire naître — ce module ne connaît pas
/// `Sim::spawn_herd`, comme le reste de `fauna` ne connaît pas `Sim`).
pub fn daily_immigration(
    humans: &[(f64, f64)],
    herds: &[(f64, f64)],
    world: &mut World,
    seed: WorldSeed,
    time: SimTime,
) -> Option<(f64, f64)> {
    let mut rng = Pcg32::new(seed.derive(salt::IMMIGRATION) ^ splitmix64(time.tick), 0);
    if humans.is_empty() || rng.next_f32() >= IMMIGRATION_CHANCE_PER_DAY {
        return None;
    }
    let anchor = humans[(rng.next_u32() as usize) % humans.len()];
    let angle = rng.next_f64() * std::f64::consts::TAU;
    let r = IMMIGRATION_MIN_RADIUS_TILES
        + rng.next_f64() * (IMMIGRATION_MAX_RADIUS_TILES - IMMIGRATION_MIN_RADIUS_TILES);
    let candidate = (anchor.0 + angle.cos() * r, anchor.1 + angle.sin() * r);
    let tile = world.tile(candidate.0.floor() as i64, candidate.1.floor() as i64);
    is_valid_immigration_site(candidate, &tile, herds).then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_troupeau_bien_nourri_croit_et_un_troupeau_affame_fond() {
        // Satiété au-dessus du point d'équilibre (0,5) : croissance.
        let mut repu = 30.0;
        let mut affame = 30.0;
        for _ in 0..24 * 30 {
            repu = herd_population_step(repu, 1.0);
            affame = herd_population_step(affame, 0.0);
        }
        assert!(repu > 33.0, "un mois de pâture pleine doit faire croître : {repu:.1}");
        assert!(affame < 25.0, "un mois sans herbe doit faire fondre : {affame:.1}");
    }

    #[test]
    fn la_satiete_d_equilibre_stabilise_l_effectif() {
        // À satiété = mortalité/natalité = 0,5, le troupeau ne bouge pas.
        let mut pop = 40.0;
        for _ in 0..24 * 90 {
            pop = herd_population_step(pop, 0.5);
        }
        assert!((pop - 40.0).abs() < 0.5, "doit rester ~40 têtes : {pop:.2}");
    }

    #[test]
    fn une_meute_sans_proie_s_eteint() {
        let mut pop = 8.0;
        let mut jours = 0;
        while pop >= PACK_MIN && jours < 24 * 365 {
            pop = pack_population_step(pop, 0.0);
            jours += 1;
        }
        assert!(pop < PACK_MIN, "une meute sans gibier doit disparaître");
    }

    #[test]
    fn une_meute_bien_nourrie_croit() {
        let mut pop = 5.0;
        for _ in 0..24 * 60 {
            let kills = pack_kills(pop, 1000.0); // gibier abondant
            pop = pack_population_step(pop, kills);
        }
        assert!(pop > 5.0, "avec du gibier à volonté, la meute croît : {pop:.1}");
    }

    #[test]
    fn les_prises_sont_bornees_par_le_gibier_present() {
        // On ne tue pas plus de bêtes qu'il n'y en a : sinon l'effectif
        // passerait négatif et la meute se nourrirait de fantômes.
        assert_eq!(pack_kills(100.0, 0.3), 0.3);
        assert!(pack_kills(2.0, 1000.0) < 1.0);
    }

    #[test]
    fn on_fuit_a_l_oppose_de_la_menace() {
        let dir = away_from((100.0, 100.0), (90.0, 100.0));
        assert!((dir.0 - 1.0).abs() < 1e-9 && dir.1.abs() < 1e-9);
        // Menace pile dessus : pas de division par zéro.
        let dir = away_from((0.0, 0.0), (0.0, 0.0));
        assert!(dir.0.is_finite() && dir.1.is_finite());
    }

    #[test]
    fn la_menace_hors_de_portee_n_alarme_pas() {
        let loin = FLEE_RADIUS_TILES + 10.0;
        assert!(nearest_threat((0.0, 0.0), &[(loin, 0.0)]).is_none());
        assert!(nearest_threat((0.0, 0.0), &[(10.0, 0.0)]).is_some());
        // La plus proche gagne.
        let t = nearest_threat((0.0, 0.0), &[(200.0, 0.0), (10.0, 0.0)]).unwrap();
        assert_eq!(t, (10.0, 0.0));
    }

    /// Cherche une tuile praticable et giboyeuse près de `from`, pour les
    /// tests — même idiome que `sim::tests::find_land` (sondage par pas de
    /// 16 km dans 8 directions, jusqu'à 6 400 km — les océans entre
    /// continents font des milliers de km), avec la biomasse en plus de la
    /// terre : `find_land` seul retomberait parfois sur une plage ou un
    /// désert, pas assez giboyeux pour ces tests.
    fn find_lush(world: &mut World, from: (i64, i64)) -> (i64, i64) {
        let step = cairn_core::km_to_tiles(16.0) as i64;
        for r in 0..400i64 {
            let d = r * step;
            for &(x, y) in &[
                (d, 0), (-d, 0), (0, d), (0, -d),
                (d, d), (-d, d), (d, -d), (-d, -d),
            ] {
                let p = (from.0 + x, from.1 + y);
                let tile = world.tile(p.0, p.1);
                if tile.is_walkable() && tile.biomass >= IMMIGRATION_MIN_BIOMASS {
                    return p;
                }
            }
        }
        panic!("aucune tuile giboyeuse trouvée près de {from:?}");
    }

    #[test]
    fn un_site_giboyeux_sans_troupeau_voisin_est_valide() {
        let mut world = World::new(WorldSeed(42), 64);
        let lush = find_lush(&mut world, (0, 0));
        let tile = world.tile(lush.0, lush.1);
        let candidate = (lush.0 as f64 + 0.5, lush.1 as f64 + 0.5);
        assert!(
            is_valid_immigration_site(candidate, &tile, &[]),
            "une tuile giboyeuse sans troupeau à proximité doit être un site valide"
        );
    }

    #[test]
    fn un_troupeau_proche_invalide_le_site_mais_pas_un_troupeau_lointain() {
        let mut world = World::new(WorldSeed(42), 64);
        let lush = find_lush(&mut world, (0, 0));
        let tile = world.tile(lush.0, lush.1);
        let candidate = (lush.0 as f64 + 0.5, lush.1 as f64 + 0.5);

        let tout_pres = [(candidate.0 + 10.0, candidate.1)];
        assert!(
            !is_valid_immigration_site(candidate, &tile, &tout_pres),
            "un troupeau à 10 tuiles doit invalider le site (bien sous IMMIGRATION_MIN_HERD_DISTANCE_TILES)"
        );

        let tres_loin = [(candidate.0 + km_to_tiles(50.0), candidate.1)];
        assert!(
            is_valid_immigration_site(candidate, &tile, &tres_loin),
            "un troupeau à 50 km ne doit pas gêner un site par ailleurs valide"
        );
    }

    /// LE critère de ce mécanisme : une zone vidée de tout troupeau finit par
    /// être réensemencée. Beaucoup de jours simulés (le tirage n'a que 15 %
    /// de chances par jour) pour une confiance statistique, comme les autres
    /// tests stochastiques de ce projet (émergence de clan, exploration…).
    #[test]
    fn une_zone_giboyeuse_sans_troupeau_finit_par_etre_reensemencee() {
        let mut world = World::new(WorldSeed(42), 64);
        let lush = find_lush(&mut world, (0, 0));
        let anchor = (lush.0 as f64 + 0.5, lush.1 as f64 + 0.5);
        let humans = [anchor];

        let mut found = false;
        for day in 0..2000u64 {
            let time = SimTime { tick: day * 24 };
            if daily_immigration(&humans, &[], &mut world, WorldSeed(42), time).is_some() {
                found = true;
                break;
            }
        }
        assert!(found, "aucune immigration en ~5,5 ans simulés sur une zone giboyeuse vide");
    }

    /// Le pendant du critère précédent : une zone déjà dense en troupeaux
    /// (tous les candidats retombent à moins d'`IMMIGRATION_MIN_HERD_DISTANCE_TILES`)
    /// ne doit **jamais** recevoir d'immigrant — le mécanisme est
    /// auto-limitant sans qu'aucune règle ne dise « pas si déjà peuplé ».
    #[test]
    fn une_zone_deja_dense_en_troupeaux_ne_recoit_jamais_d_immigrant() {
        let mut world = World::new(WorldSeed(42), 64);
        let lush = find_lush(&mut world, (0, 0));
        let anchor = (lush.0 as f64 + 0.5, lush.1 as f64 + 0.5);
        let humans = [anchor];
        // Un quadrillage serré de troupeaux couvrant largement le rayon
        // d'apparition possible (jusqu'à IMMIGRATION_MAX_RADIUS_TILES).
        let step = km_to_tiles(2.0);
        let mut herds = Vec::new();
        for gy in -3..=3 {
            for gx in -3..=3 {
                herds.push((anchor.0 + gx as f64 * step, anchor.1 + gy as f64 * step));
            }
        }
        for day in 0..2000u64 {
            let time = SimTime { tick: day * 24 };
            assert!(
                daily_immigration(&humans, &herds, &mut world, WorldSeed(42), time).is_none(),
                "jour {day} : une zone déjà dense en troupeaux ne doit jamais recevoir d'immigrant"
            );
        }
    }

    #[test]
    fn sans_humain_aucune_immigration() {
        let mut world = World::new(WorldSeed(42), 16);
        for day in 0..500u64 {
            let time = SimTime { tick: day * 24 };
            assert!(daily_immigration(&[], &[], &mut world, WorldSeed(42), time).is_none());
        }
    }
}
