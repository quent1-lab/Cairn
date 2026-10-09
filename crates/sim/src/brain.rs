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
//! La perception reste **locale** (l'agent redécouvre son voisinage à chaque
//! délibération), mais depuis la Phase 3 elle est doublée d'une **mémoire**
//! individuelle (`crate::memory`) : à défaut de source visible, l'agent vise
//! celle qu'il a vue il y a dix jours. C'est elle qui rend l'exploration
//! praticable — sans elle, s'aventurer hors de la perception locale, c'est
//! s'aventurer hors de l'eau.

use std::collections::BTreeMap;

use cairn_core::{Pcg32, SimTime, km_to_tiles, splitmix64};
use cairn_worldgen::Biome;

use crate::agent::{AgentId, Physiology, Position, Task, TaskKind, WALK_TILES_PER_TICK};
use crate::curves::{Curve, softmax_pick, softmax_weights};
use crate::demography::{Demographics, HumanView, Kinship, Traits, find_human};
use crate::fauna::{HerdView, PackView};
use crate::memory::{Memory, cell_of};
use crate::salt;
use crate::social::{self, ClanId, ClanRelations, ClanView, RESIDENCE_RADIUS_TILES};
use crate::world::World;

/// Un agent re-délibère toutes les 4 h (et dès qu'il n'a plus de tâche).
/// Décalé par agent : les délibérations s'étalent sur les ticks.
pub const DELIBERATION_PERIOD: u64 = 4;
/// Température du softmax : bas = discipliné, haut = fantasque.
pub const SOFTMAX_TAU: f32 = 0.12;
/// En deçà de ce temps avant la mort (en ticks, donc en heures), un besoin
/// vital devient une urgence : 3 jours, l'ordre de grandeur de la survie sans
/// eau (la « règle des trois » : 3 jours sans eau, 3 semaines sans nourriture).
const VITAL_HORIZON_TICKS: f64 = 72.0;
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
/// Longueur de base d'une jambe d'errance (~500 m) — modulée par la
/// curiosité (×0,5 à ×1,5) et raccourcie pour les enfants. Borne aussi
/// l'étalement spatial de la population, donc le working set de chunks : à
/// 600 tuiles, 100 errants couvraient plus de chunks que le LRU n'en tient,
/// et chaque lecture régénérait un chunk (thrash mesuré : la simulation
/// s'effondrait).
const WANDER_LEG_TILES: f64 = 250.0;
/// En dessous de cette biomasse, une tuile ne vaut pas le déplacement.
const FORAGE_MIN_BIOMASS: u8 = 5;
/// Distance à laquelle un chasseur repère du gibier (~2 km). Large : c'est
/// une bête de plusieurs centaines de kilos dans un paysage ouvert.
const HERD_SIGHT_TILES: f64 = km_to_tiles(2.0);
/// Au-delà de cette distance à son parent, un enfant décroche tout pour le
/// rejoindre (~80 m).
const FOLLOW_TRIGGER_TILES: f64 = 40.0;
/// En deçà de cette distance au plus proche congénère, on ne se sent pas
/// isolé (~500 m).
const SOCIAL_TRIGGER_TILES: f64 = 250.0;
/// Longueur d'une jambe d'exploration (~1,2 km) : plus ample que l'errance —
/// on part *pour* voir, pas en passant.
const EXPLORE_LEG_TILES: f64 = 600.0;
/// Une cible d'exploration doit rester à ~2 km d'une source **connue** :
/// l'arrimage au réseau d'eau, qui borne la dérive (voir le candidat).
const EXPLORE_TETHER_TILES: i64 = 1000;
/// En dessous de ce niveau, le stock du clan n'est pas un but de trajet
/// (même idiome que `FORAGE_MIN_BIOMASS`).
const MIN_STOCK_WORTH_TRIP: f32 = 0.1;
/// En dessous de ce niveau, un surplus porté ne vaut pas le détour par le
/// foyer (même idiome que `MIN_STOCK_WORTH_TRIP`).
const MIN_CARRYING_WORTH_TRIP: f32 = 0.05;

/// Force du rappel quand on se trouve sur le territoire d'un autre peuple.
/// Assez pour peser dans le softmax sans écraser la soif ou la faim : on rentre
/// chez soi, on ne fuit pas — et un besoin vital passe toujours avant.
const INTRUSION_URGENCY: f32 = 0.30;
/// Charge à partir de laquelle le retour au foyer est pleinement urgent : sert
/// à normaliser l'urgence, pas une limite dure. Une bête entière (`meat_hunger`
/// dans `sim.rs`, ~14 points pour un cerf) la dépasse de loin : qui porte une
/// prise rentre.
const CARRYING_FULL_LOAD: f32 = 0.9;
/// Seuil de faim sous lequel un agriculteur peut consacrer du temps au champ :
/// on cultive **au calme** (surplus de temps), pas quand on meurt de faim — la
/// même philosophie que « bâtir repu ». La *découverte* de l'agriculture, elle,
/// demande la faim (voir `techs.ron`) : nécessité pour inventer, surplus pour
/// travailler.
const FED_TO_FARM: f32 = 0.55;
/// Poussée du drive agricole : modeste, il ne prime jamais sur la survie.
const FARM_DRIVE: f32 = 0.3;
/// Rayon autour du foyer en deçà duquel une meute est perçue comme une
/// **menace** sur le territoire (~1,5 km) : c'est ce qu'on défend.
const PREDATOR_THREAT_TILES: f64 = km_to_tiles(1.5);
/// Poussée du drive de défense (chasse des prédateurs) : modulée par
/// l'agressivité et la force — qui décident *qui* ose s'y risquer.
const PREDATOR_DEFENSE_DRIVE: f32 = 0.8;
/// Poussée du drive d'élevage (garder le cheptel) : modeste, comme
/// l'agriculture — le travail pastoral se fait au calme, pas dans l'urgence.
const HERD_DRIVE: f32 = 0.35;
/// Tension (mesurée en [0, 1], Phase 4) au-delà de laquelle un clan voisin est
/// un ennemi qu'on peut razzier.
const RAID_TENSION_THRESHOLD: f32 = 0.4;
/// Portée à laquelle un rival hostile devient une cible de raid (~1 km).
const RAID_RADIUS_TILES: f64 = km_to_tiles(1.0);
/// Poussée du drive de raid : forte, mais modulée par l'agressivité et la
/// tension — qui décident qui razzie qui, et quand.
const RAID_DRIVE: f32 = 0.9;
/// Plaie à partir de laquelle on ne razzie plus du tout.
const RAID_WOUND_LIMIT: f32 = 0.5;
/// Poussée du drive de pèlerinage : modeste, et de toute façon multipliée par
/// la ferveur — un tiède ne bouge pas, un fervent traverse la contrée.
const PILGRIMAGE_DRIVE: f32 = 0.5;

/// L'agent vu par la délibération : son identité et ses composants, groupés
/// pour ne pas trimballer sept paramètres.
pub struct AgentCtx<'a> {
    pub id: AgentId,
    pub pos: &'a Position,
    pub phys: &'a Physiology,
    pub traits: &'a Traits,
    pub demo: &'a Demographics,
    pub kin: &'a Kinship,
    pub clan: Option<ClanId>,
    /// Surplus de chasse actuellement porté (`crate::agent::Carrying`),
    /// déballé ici en `f32` brut — même traitement que `clan`.
    pub carrying: f32,
    /// Cet agent maîtrise-t-il l'agriculture ? Résolu une fois par le pas de
    /// simulation (`knowledge.has(agriculture)`) et déballé en `bool` ici, pour
    /// que `brain` reste découplé de l'arbre technologique.
    pub knows_agriculture: bool,
    /// La ferveur de cet agent (`crate::faith::Faith`), déballée en `f32` — même
    /// traitement que `clan` et `carrying`, pour que `brain` reste découplé du
    /// système de croyance.
    pub fervor: f32,
    /// La plaie de cet agent (`crate::agent::Wound`), dans [0, 1] : un blessé
    /// ne cherche pas la bagarre.
    pub wound: f32,
    /// La lumière du jour là où il se tient (`Climate::light`), 0 à 1.
    pub light: f32,
    /// Heures de jour qui restent (`Climate::daylight_left_hours`).
    pub daylight_left_h: f32,
}

/// Choisit la prochaine tâche de l'agent. Déterministe : le tirage dérive de
/// (seed, tick, id agent) — deux exécutions rejouent la même hésitation.
/// `mem` est lue (où boire, où explorer) **et** écrite (une source aperçue
/// s'apprend) : délibérer, c'est déjà mémoriser.
#[allow(clippy::too_many_arguments)]
pub fn decide(
    world: &mut World,
    time: SimTime,
    agent: AgentCtx<'_>,
    mem: &mut Memory,
    current: Option<TaskKind>,
    herds: &[HerdView],
    packs: &[PackView],
    humans: &[HumanView],
    clan_views: &BTreeMap<ClanId, ClanView>,
    relations: &ClanRelations,
    // Les lieux sacrés, si la divinité en a désigné (BRIEF §6.2).
    shrines: &[crate::cult::Shrine],
) -> Option<Task> {
    // La perception de l'eau est **mémorisée** — délibérer, c'est déjà
    // mémoriser. C'est le seul effet de bord de la délibération : il reste ici,
    // hors de `build_candidates`, pour que cette dernière reste pure et puisse
    // servir aussi l'inspection sans faire « apprendre » l'agent qu'on regarde.
    let here = agent.pos.tile();
    let spring = world.nearest_spring(here, SPRING_RADIUS_CHUNKS);
    if let Some(seen) = spring {
        mem.remember_spring(seen, (agent.pos.x, agent.pos.y));
    }
    // Le gibier aussi s'apprend en le voyant : on retient le dernier troupeau
    // aperçu. Arrivé sur une piste sans rien y voir, on l'oublie.
    match nearest_herd(agent.pos, herds, crate::climate::sight(agent.light)) {
        Some(herd) => {
            mem.game = Some(((herd.pos.0.floor() as i64, herd.pos.1.floor() as i64), time.tick));
        }
        None => {
            // Seulement une fois **sur** la piste : juste après avoir perdu un
            // troupeau de vue, on en est encore à moins de 2 km — l'oublier là
            // effaçait le souvenir avant qu'il serve (mesuré : 0 % de pistage).
            if let Some((spot, _)) = mem.game
                && agent.pos.distance_tiles(spot) <= GAME_ARRIVAL_TILES
            {
                mem.game = None;
            }
        }
    }
    let candidates =
        build_candidates(world, time, &agent, mem, spring, current, herds, packs, humans, clan_views, relations, shrines);

    let scores: Vec<f32> = candidates.iter().map(|c| c.2).collect();
    let mut rng =
        Pcg32::new(world.seed().derive(salt::DECISIONS) ^ splitmix64(time.tick), agent.id.0);
    let (kind, target, _) = candidates[softmax_pick(&scores, SOFTMAX_TAU, &mut rng)];
    Some(Task { kind, target })
}

/// Construit et score toutes les tâches candidates de l'agent — le cœur de la
/// délibération, **sans tirage ni effet de bord** : elle lit `mem` mais ne
/// l'écrit pas (la source déjà perçue lui est passée en `spring`). Partagée par
/// [`decide`] (qui tire ensuite au softmax) et [`inspect`] (qui en montre les
/// scores). `world` reste `&mut` pour lire les tuiles (fourrage, couvert),
/// comme le fait déjà le rendu — aucune mutation de l'état simulé.
#[allow(clippy::too_many_arguments)]
fn build_candidates(
    world: &mut World,
    time: SimTime,
    agent: &AgentCtx<'_>,
    mem: &Memory,
    spring: Option<(i64, i64)>,
    current: Option<TaskKind>,
    herds: &[HerdView],
    packs: &[PackView],
    humans: &[HumanView],
    clan_views: &BTreeMap<ClanId, ClanView>,
    relations: &ClanRelations,
    shrines: &[crate::cult::Shrine],
) -> Vec<(TaskKind, (i64, i64), f32)> {
    let (id, pos, phys, traits, demo, kin, clan, carrying, knows_agriculture) = (
        agent.id, agent.pos, agent.phys, agent.traits, agent.demo, agent.kin, agent.clan,
        agent.carrying, agent.knows_agriculture,
    );
    let adult = demo.is_adult(time.tick);
    // Ce qu'on voit (MAR-2) : de nuit, ce qui se fait à vue — cueillir,
    // repérer et approcher le gibier, explorer, errer — ne rapporte presque
    // plus rien, et on le sait.
    let sight = crate::climate::sight(agent.light);
    let here = pos.tile();
    let mut candidates: Vec<(TaskKind, (i64, i64), f32)> = Vec::new();

    // — Boire : seuil flou et raide, la soif devient vite impérieuse. La
    //   perception locale d'abord (passée en `spring`, mémorisée par `decide`) ;
    //   sinon la **mémoire** prend le relais — c'est elle qui sauve l'agent
    //   parti trop loin de l'eau, et rend l'exploration moins suicidaire.
    // Une source où la marche vient de buter est mise de côté (D11) : on vise la
    // suivante plutôt que de retenter la même jusqu'à en mourir.
    let open = |s: &(i64, i64)| !mem.is_blocked(*s, time.tick);
    if let Some(target) =
        spring.filter(open).or_else(|| mem.nearest_open_spring((pos.x, pos.y), time.tick))
    {
        let urgency = Curve::Logistic { steepness: 9.0, midpoint: 0.45 }.eval(phys.thirst);
        let score = urgency * travel_discount(pos.distance_tiles(target));
        candidates.push((TaskKind::Drink, target, score));
    }

    // — Manger : pression progressive, pondérée par l'abondance trouvée. D10 :
    //   l'abondance est celle de la **maille** (ce que le pays offre de
    //   comestible, partagé entre ceux qui y vivent), plus la seule tuile.
    let (forage_target, forage_biomass) = best_forage(world, here);
    let urgency = Curve::Logistic { steepness: 6.0, midpoint: 0.4 }.eval(phys.hunger);
    let share_here = edible_share(world, (pos.x, pos.y), time.tick, humans);
    if forage_biomass >= FORAGE_MIN_BIOMASS {
        let abundance = (f32::from(forage_biomass) / 255.0).sqrt() * share_here;
        let score =
            urgency * abundance * sight * travel_discount(pos.distance_tiles(forage_target));
        candidates.push((TaskKind::Forage, forage_target, score));
    }
    // — Changer de pays quand le sien s'épuise : un cueilleur connaît les
    //   mailles voisines et va vers la mieux pourvue. Le trajet se juge à
    //   l'échelle d'une journée de marche, pas d'une heure : déplacer son
    //   camp est une décision de la journée.
    if share_here < 1.0 {
        let zone = crate::fauna::range_zone((pos.x, pos.y));
        let side = crate::fauna::RANGE_ZONE_TILES;
        // Piste F : à richesse égale, le pays **connu** l'emporte — on sait
        // où y sont l'eau et la nourriture. Sans cela, chaque changement de
        // maille faisait glisser le camp au hasard : deux bandes à 900 km
        // l'une de l'autre en 25 ans. Rien n'interdit l'inconnu.
        let mut best: Option<(f32, (i64, i64))> = None;
        for dz in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
            let c = ((zone.0 + dz.0) as f64 + 0.5) * side;
            let r = ((zone.1 + dz.1) as f64 + 0.5) * side;
            let share = edible_share(world, (c, r), time.tick, humans);
            if share <= share_here {
                continue;
            }
            let pull = (share - share_here) * familiarity_weight(mem, (c, r), side);
            if best.is_none_or(|(b, _)| pull > b) {
                best = Some((pull, (c as i64, r as i64)));
            }
        }
        if let Some((pull, target)) = best {
            let day = 1.0 / (1.0 + pos.distance_tiles(target) / (WALK_TILES_PER_TICK * 24.0));
            candidates.push((TaskKind::Wander, target, urgency * pull * day as f32));
        }
    }

    // — Chasser (adultes seulement) : même pression de faim que la
    //   cueillette, mais une prise nourrit bien davantage. Le gibier proche
    //   et nombreux attire ; quand les troupeaux ont été décimés ou ont fui,
    //   ce candidat s'efface tout seul et la faim repart sur la cueillette
    //   ou l'errance. C'est de là que sort le suivi des troupeaux — rien ne
    //   dit « suis le gibier ».
    //   D10 : l'envie de chasser suit le **besoin de viande**, pas la seule
    //   faim — une bête vaut un mois de nourriture. Le sien : la faim, moins
    //   ce qu'on porte déjà. Celui des siens : ce qui manque à la réserve du
    //   clan. On chasse pour le plus pressant des deux.
    let meat_need = {
        let own = (phys.hunger - carrying).max(0.0);
        let clan_gap = clan.and_then(|c| clan_views.get(&c)).map_or(0.0, |v| {
            let scale = crate::sim::STOCK_SCALE_PER_MEMBER * v.members.max(1) as f32;
            (1.0 - (v.stock + carrying) / scale).clamp(0.0, 1.0)
        });
        own.max(clan_gap)
    };
    let hunt_urgency = Curve::Logistic { steepness: 6.0, midpoint: 0.4 }.eval(meat_need);
    let nearest_herd = if adult { nearest_herd(pos, herds, sight) } else { None };
    if let Some(herd) = nearest_herd {
        let target = (herd.pos.0.floor() as i64, herd.pos.1.floor() as i64);
        let urgency = hunt_urgency;
        // Un gros troupeau est une proie plus sûre (plus de bêtes à approcher).
        let size = Curve::Power { k: 0.5 }.eval(herd.population / 60.0);
        let score = urgency * (0.55 + 0.45 * size) * sight * travel_discount(pos.distance_tiles(target));
        candidates.push((TaskKind::Hunt, target, score));
    }

    // — Pister (adultes, D10) : aucun troupeau à vue, mais on se souvient
    //   d'en avoir vu un. On part vers la piste, avec la même faim qui pousse
    //   à chasser, d'autant moins que le souvenir est vieux — le gibier bouge.
    //   Le trajet se juge à l'échelle d'une journée : on part pister, on ne
    //   fait pas un détour.
    //   Seulement s'il fait assez jour pour lire la piste (CHA-2b).
    if adult
        && nearest_herd.is_none()
        && sight >= crate::fauna::TRAIL_SIGHT
        && let Some((spot, seen)) = mem.game
    {
        let age_days = time.tick.saturating_sub(seen) as f64 / cairn_core::TICKS_PER_DAY as f64;
        let freshness = (-age_days / GAME_MEMORY_DAYS).exp() as f32;
        let urgency = hunt_urgency;
        let day = 1.0 / (1.0 + pos.distance_tiles(spot) / (WALK_TILES_PER_TICK * 24.0));
        candidates.push((TaskKind::Track, spot, urgency * 0.55 * freshness * day as f32 * sight));
    }

    // — Partir à la rencontre du gibier (D10, refait en MAR-4 sur les sorties
    //   hadza : O'Connell, Hawkes et Blurton Jones, 1985-86). Ni troupeau en
    //   vue, ni piste fraîche, et besoin de viande : on part de son camp — le
    //   foyer du clan, ou le gîte où l'on a dormi —, on balaie le pays du
    //   regard (2 km de part et d'autre), et on **rentre avant la nuit** : la
    //   cible n'est offerte que si l'aller et le retour tiennent dans le jour
    //   qui reste. Personne n'écrit d'horaire : c'est la lumière qui borne la
    //   sortie. Ouverte à tout adulte, clan ou pas (les femmes chassent dans la
    //   plupart des sociétés de fourrageurs, et un nourrisson porté ne gêne ni
    //   leur mobilité ni leur rendement — BaYaka, Agta).
    let fresh_track = mem.game.is_some_and(|(_, seen)| {
        time.tick.saturating_sub(seen) < cairn_core::TICKS_PER_DAY * GAME_MEMORY_DAYS as u64
    });
    let base: Option<(f64, f64)> = clan
        .and_then(|c| clan_views.get(&c))
        .map(|v| v.home)
        .or_else(|| mem.lodge.map(|(x, y)| (x as f64 + 0.5, y as f64 + 0.5)));
    if adult
        && nearest_herd.is_none()
        && !fresh_track
        && let Some(home) = base
    {
        let mut rng = Pcg32::new(world.seed().derive(salt::WANDER) ^ splitmix64(time.tick ^ 0x5eed), id.0);
        let angle = rng.next_f64() * std::f64::consts::TAU;
        let (dx, dy) = (angle.cos(), angle.sin());
        // D2 : la faim **personnelle** allonge la quête au-delà du territoire,
        // jusqu'à la portée d'une sortie à la journée (`FORAY_RADIUS_TILES`).
        // Pas le manque de la réserve du clan, qui pousse tout le monde en
        // permanence : c'est l'affamé qui va voir plus loin. Rien ne l'y
        // oblige (un candidat parmi d'autres), le rappel au clan reste offert
        // une fois loin (`ReturnToClan`), et qui ne rentre pas finit par
        // dormir ailleurs — il sort alors de la bande d'elle-même
        // (`social::RESIDENCE_NIGHTS`). La bande affamée se disperse ainsi
        // sans aucune règle de scission.
        let own_hunger = (phys.hunger - carrying).max(0.0);
        let reach_beyond = Curve::Logistic { steepness: 10.0, midpoint: 0.6 }.eval(own_hunger) as f64;
        let max_d = 0.9 * RESIDENCE_RADIUS_TILES
            + reach_beyond * (FORAY_RADIUS_TILES - 0.9 * RESIDENCE_RADIUS_TILES).max(0.0);
        // Le chemin qu'on peut faire avant la nuit : aller jusqu'au point, puis
        // revenir au camp.
        let walkable = f64::from(agent.daylight_left_h) * WALK_TILES_PER_TICK;
        let dist = |a: (f64, f64), b: (f64, f64)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
        let mut target = None;
        let mut d = 80.0;
        while d <= max_d {
            let pf = (home.0 + dx * d, home.1 + dy * d);
            let p = (pf.0.floor() as i64, pf.1.floor() as i64);
            if world.worldgen().elevation(p.0, p.1) <= 0.0 {
                break; // eau : on s'arrête à la dernière terre
            }
            if dist((pos.x, pos.y), pf) + dist(pf, home) > walkable {
                break; // la nuit tomberait avant le retour
            }
            target = Some(p);
            d += 80.0;
        }
        if let Some(target) = target {
            candidates.push((TaskKind::SeekGame, target, hunt_urgency * 0.55 * sight));
        }
    }

    // — Défendre le territoire des prédateurs (« éleveur », Phase 5) : une meute
    //   qui rôde près du foyer menace le gibier dont on vit (et, demain, le
    //   cheptel). Un adulte assez **agressif** et **fort** va l'affronter — au
    //   risque d'être blessé ou tué (`crate::combat`). Rien ne dit « attaque le
    //   loup » : c'est l'agressivité héritée changée en probabilité, et seulement
    //   quand la meute menace vraiment le foyer. La cible est la meute la plus
    //   proche du foyer ; l'exécution la (re)vise en chemin.
    if adult
        && let Some(clan_id) = clan
        && let Some(view) = clan_views.get(&clan_id)
    {
        let mut nearest: Option<(f64, (f64, f64))> = None;
        for p in packs {
            let d2 = (view.home.0 - p.pos.0).powi(2) + (view.home.1 - p.pos.1).powi(2);
            if nearest.is_none_or(|(bd, _)| d2 < bd) {
                nearest = Some((d2, p.pos));
            }
        }
        if let Some((d2, ppos)) = nearest
            && d2.sqrt() <= PREDATOR_THREAT_TILES
        {
            // Plus la meute est proche du foyer, plus ça presse ; l'agressivité
            // et la force décident qui ose s'y risquer.
            let urgency = 1.0 - (d2.sqrt() / PREDATOR_THREAT_TILES) as f32;
            let score = PREDATOR_DEFENSE_DRIVE
                * traits.aggression
                * (0.4 + 0.6 * traits.strength)
                * urgency
                * sight;
            let target = (ppos.0.floor() as i64, ppos.1.floor() as i64);
            candidates.push((TaskKind::HuntPredator, target, score));
        }
    }

    // — Razzier un clan rival (conflit inter-clans, Phase 5) : la tension
    //   mesurée depuis la Phase 4 qui trouve enfin sa conclusion. Un adulte
    //   **agressif**, quand un membre d'un clan avec lequel le sien est en forte
    //   **tension** passe à portée, va en découdre. Score ∝ agressivité ×
    //   tension — jamais « déclare la guerre », juste le trait et la tension
    //   changés en probabilité. La cible : le rival hostile le plus proche.
    if adult
        && let Some(my_clan) = clan
    {
        let mut best: Option<(f64, (f64, f64), f32)> = None; // (dist², position, tension)
        for h in humans {
            if h.id == id {
                continue;
            }
            let Some(their_clan) = h.clan else { continue };
            if their_clan == my_clan {
                continue;
            }
            let tension = relations.tension_between(my_clan, their_clan);
            if tension < RAID_TENSION_THRESHOLD {
                continue;
            }
            let d2 = (pos.x - h.pos.0).powi(2) + (pos.y - h.pos.1).powi(2);
            if d2 <= RAID_RADIUS_TILES.powi(2) && best.is_none_or(|(bd, _, _)| d2 < bd) {
                best = Some((d2, h.pos, tension));
            }
        }
        if let Some((_, hpos, tension)) = best {
            // On ne va pas au-devant des coups blessé : l'envie de razzier
            // décroît avec la plaie et s'éteint à mi-plaie (D13). Sans ce frein,
            // une mêlée durait jusqu'à la mort — chacun retournait se battre à
            // chaque délibération, quelle que soit sa blessure.
            let healthy = (1.0 - agent.wound / RAID_WOUND_LIMIT).clamp(0.0, 1.0);
            let score = RAID_DRIVE * traits.aggression * tension * healthy;
            let target = (hpos.0.floor() as i64, hpos.1.floor() as i64);
            candidates.push((TaskKind::Raid, target, score));
        }
    }

    // — Puiser dans le stock du clan : le pendant collectif de la cueillette
    //   et de la chasse (BRIEF §5.1). Même garde que le désespoir alimentaire
    //   de l'errance plus bas — rien de local ne répond — pour qu'un membre
    //   ne vide le stock commun que quand il en a vraiment besoin, pas parce
    //   qu'il existe. La cible est le foyer du clan : le stock se puise sur
    //   place, il ne se livre pas.
    if let Some(clan_id) = clan
        && let Some(view) = clan_views.get(&clan_id)
        && view.stock > MIN_STOCK_WORTH_TRIP
        && (forage_biomass < FORAGE_MIN_BIOMASS || share_here < LEAN_SHARE)
        && nearest_herd.is_none()
    {
        let urgency = Curve::Logistic { steepness: 6.0, midpoint: 0.4 }.eval(phys.hunger);
        let target = (view.home.0.floor() as i64, view.home.1.floor() as i64);
        let score = urgency * travel_discount(pos.distance_tiles(target));
        candidates.push((TaskKind::EatFromStock, target, score));
    }

    // — Rapporter le surplus de chasse au foyer : le pendant du dépôt de
    //   `EatFromStock` — la viande ne rejoint le stock qu'une fois portée
    //   jusqu'au clan (voir `Carrying`), elle ne s'y téléporte pas depuis le
    //   lieu de la mise à mort. Un candidat de plus dans le même softmax que
    //   `ReturnToClan` : rien ne force le retour, un besoin plus pressant
    //   (soif, froid) peut encore l'emporter — et si l'agent meurt ou change
    //   durablement de priorité en chemin, le surplus porté est simplement
    //   perdu, sans code dédié pour ce cas.
    if carrying > MIN_CARRYING_WORTH_TRIP
        && let Some(clan_id) = clan
        && let Some(view) = clan_views.get(&clan_id)
    {
        let target = (view.home.0.floor() as i64, view.home.1.floor() as i64);
        let urgency = (carrying / CARRYING_FULL_LOAD).clamp(0.3, 1.0);
        let score = urgency * travel_discount(pos.distance_tiles(target));
        candidates.push((TaskKind::BringSurplusHome, target, score));
    }

    // — Dormir : sur place, surtout la nuit ; la fatigue extrême s'impose.
    // La nuit suit le soleil (MAR-1) : longue l'hiver et vers les pôles.
    let night_factor = 0.55 + 0.6 * (1.0 - agent.light);
    let sleep_score = Curve::Power { k: 2.5 }.eval(phys.fatigue) * night_factor;
    candidates.push((TaskKind::Sleep, here, sleep_score));
    // — Ou rentrer dormir au campement (D10) : un membre de clan loin du foyer
    //   peut aller dormir près des siens. Personne ne l'y oblige — c'est la
    //   sociabilité qui y pousse, et la longueur du chemin qui en détourne
    //   (jugée sur quelques heures de marche). C'est le soir, autour du foyer,
    //   que les liens se nouent : les rencontres se font à moins de 120 m.
    // Le camp : celui du clan, ou, sans clan, le gîte de la nuit d'avant (MAR-4).
    if let Some(home) = base {
        let camp = (home.0.floor() as i64, home.1.floor() as i64);
        let d = pos.distance_tiles(camp);
        if d > CAMP_RADIUS_TILES {
            let walk = 1.0 / (1.0 + d / (WALK_TILES_PER_TICK * 6.0));
            let score = sleep_score * (0.5 + 0.5 * traits.sociability) * walk as f32;
            candidates.push((TaskKind::Sleep, camp, score));
        }
    }

    // — S'abriter : réponse linéaire au stress thermique, de préférence sous
    //   couvert forestier.
    if phys.cold > 0.05 {
        let target = nearest_forest(world, here).unwrap_or(here);
        let score = Curve::Linear { m: 1.2, b: 0.0 }.eval(phys.cold);
        candidates.push((TaskKind::Shelter, target, score));
    }

    // — Suivre son parent (enfants) : plus il est loin, plus ça presse. La
    //   cible est sa position d'instantané — re-visée à chaque délibération,
    //   ce qui suffit à le suivre pas à pas.
    if !adult {
        let parent = kin
            .mother
            .and_then(|m| find_human(humans, m))
            .or_else(|| kin.father.and_then(|f| find_human(humans, f)));
        if let Some(parent) = parent {
            let d = (pos.x - parent.pos.0).hypot(pos.y - parent.pos.1);
            if d > FOLLOW_TRIGGER_TILES {
                let score = ((d / 400.0) as f32).clamp(0.3, 1.0);
                let target = (parent.pos.0.floor() as i64, parent.pos.1.floor() as i64);
                candidates.push((TaskKind::Follow, target, score));
            }
        }
    }

    // — Rejoindre les autres (adultes isolés) : le drive « appartenir »,
    //   pondéré par la sociabilité. C'est lui qui maintient une densité de
    //   rencontres — donc des conceptions et des échanges de cartes mentales
    //   (`memory::exchange_knowledge`) — sans qu'aucune règle ne dise
    //   « restez groupés ».
    if adult {
        let mut nearest: Option<(f64, (f64, f64))> = None;
        for h in humans {
            if h.id == id {
                continue;
            }
            let d2 = (pos.x - h.pos.0).powi(2) + (pos.y - h.pos.1).powi(2);
            if nearest.is_none_or(|(bd, _)| d2 < bd) {
                nearest = Some((d2, h.pos));
            }
        }
        if let Some((d2, hpos)) = nearest {
            let d = d2.sqrt();
            if d > SOCIAL_TRIGGER_TILES {
                // Pas de décote de trajet : l'isolement **est** l'urgence, et
                // plus les autres sont loin, moins il faut tarder à rentrer.
                // (Avec la décote, mesuré : les explorateurs partaient sans
                // retour et la population se diluait sur 90 km.)
                let score = 0.4 * traits.sociability;
                let target = (hpos.0.floor() as i64, hpos.1.floor() as i64);
                candidates.push((TaskKind::Socialize, target, score));
            }
        }
    }

    // — Revenir au territoire du clan : le pendant de `Socialize` à l'échelle
    //   du groupe plutôt que du congénère le plus proche. Sans elle, rien ne
    //   ramène un membre vers son clan (voir `social` — la population
    //   diffusait sans borne) ; avec elle, le foyer d'hier (le centroïde
    //   calculé par `social::detect_clans`) exerce une attraction dès qu'on
    //   s'en éloigne de plus que le rayon de résidence — **le même rayon**
    //   qui sert à la détection, pour que l'attraction et le critère se
    //   répondent : rester en dessous du seuil, c'est rester détectable.
    if let Some(clan_id) = clan
        && let Some(view) = clan_views.get(&clan_id)
    {
        let d = (pos.x - view.home.0).hypot(pos.y - view.home.1);
        // Deux raisons de rentrer, et la seconde manquait.
        //
        // 1. On s'est trop éloigné des siens (la distance au foyer).
        // 2. **On est chez les autres.** Le territoire diffusé
        //    (`social::claim_from_views`) dit à qui appartient le sol qu'on
        //    foule ; s'il revient à un autre peuple, on n'a rien à y faire.
        //
        // Sans (2), rien n'éloignait jamais deux clans : mesuré sur 15 ans, ils
        // vivaient à 2,1 km les uns des autres pour un rayon de rappel de 4,5 km
        // — territoires confondus, membres mêlés en permanence, et un cycle sans
        // fin de 423 fissions pour 554 refusions. Ce n'est pas une répulsion
        // scriptée entre foyers : chacun préfère simplement être chez soi, et
        // les frontières se dessinent d'elles-mêmes.
        let intruding = social::claim_from_views((pos.x, pos.y), clan_views)
            .is_some_and(|owner| owner != clan_id);
        if d > RESIDENCE_RADIUS_TILES || intruding {
            let far = ((d / RESIDENCE_RADIUS_TILES) as f32 - 1.0).clamp(0.0, 1.0);
            let score = far.max(if intruding { INTRUSION_URGENCY } else { 0.0 }).max(0.3);
            let target = (view.home.0.floor() as i64, view.home.1.floor() as i64);
            candidates.push((TaskKind::ReturnToClan, target, score));
        }
    }

    // — Le pèlerinage : se rendre au lieu que le ciel a désigné. Un candidat
    //   de plus dans le softmax, jamais une contrainte — la faim et la soif
    //   passent devant, on ne meurt pas de piété. Seule la ferveur décide qui
    //   se dérange, et c'est **tout** ce que le Signe fait : il attire. Que des
    //   peuples convergent, se touchent et finissent par se disputer le lieu
    //   tombe ensuite de mécanismes qui existaient déjà (§6.2).
    if let Some(shrine) = nearest_shrine(shrines, (pos.x, pos.y), agent.fervor) {
        let d = (pos.x - shrine.0).hypot(pos.y - shrine.1);
        // Plus on croit, plus on marche ; et l'appel faiblit avec la distance.
        let score = agent.fervor * PILGRIMAGE_DRIVE * travel_discount(d);
        candidates.push((
            TaskKind::Pilgrimage,
            (shrine.0.floor() as i64, shrine.1.floor() as i64),
            score,
        ));
    }

    // — Bâtir (BRIEF §5.1) : le clan a mesuré qu'il désire une structure
    //   (`structures::plan`) et son stock peut la payer. On ne bâtit que
    //   repu (comme on explore repu) et le chantier est au foyer — le coût
    //   se prend dans le stock commun sur place, pas à distance. Un candidat
    //   de plus dans le softmax ; rien n'oblige à bâtir, un besoin plus
    //   pressant peut encore l'emporter.
    if adult
        && let Some(clan_id) = clan
        && let Some(view) = clan_views.get(&clan_id)
        && let Some(kind) = view.desired
    {
        let comfortable = phys.hunger < 0.6 && phys.thirst < 0.6 && phys.cold < 0.4;
        if comfortable && view.stock >= kind.cost() {
            let target = (view.home.0.floor() as i64, view.home.1.floor() as i64);
            let score = 0.3 * travel_discount(pos.distance_tiles(target));
            candidates.push((TaskKind::Build(kind), target, score));
        }
    }

    // — Cultiver (agriculture, Phase 5, chaîne §5.3) : un agriculteur, dans son
    //   territoire et assez repu pour dégager du temps, entretient la prairie où
    //   il se tient — il en élève la biomasse au-dessus de l'état sauvage. La
    //   *découverte* de l'agriculture demande la faim (nécessité, voir
    //   `techs.ron`) ; le *travail*, lui, se fait au calme (surplus). La récolte
    //   passe ensuite par la cueillette ordinaire d'une tuile devenue plus riche.
    if knows_agriculture
        && adult
        && phys.hunger < FED_TO_FARM
        && let Some(clan_id) = clan
        && let Some(view) = clan_views.get(&clan_id)
        && (pos.x - view.home.0).hypot(pos.y - view.home.1) <= RESIDENCE_RADIUS_TILES
    {
        let tile = world.tile(here.0, here.1);
        let cultivable = tile.is_walkable()
            && matches!(tile.biome, Biome::Grassland | Biome::Steppe | Biome::Savanna)
            && tile.soil_fertility > crate::sim::FIELD_MIN_FERTILITY
            && tile.biomass < crate::sim::CULTIVATED_CEILING;
        if cultivable {
            candidates.push((TaskKind::Cultivate, here, FARM_DRIVE));
        }
    }

    // — Garder le cheptel (domestication, Phase 5) : le pendant animal du champ.
    //   Un éleveur au calme, dans son territoire, va prélever durablement sur un
    //   troupeau **apprivoisé** proche (le cheptel a émergé de la protection —
    //   l'éleveur ayant chassé les prédateurs — et de la sédentarité). On garde
    //   repu, comme on cultive repu ; la récolte va au stock commun.
    if adult
        && phys.hunger < FED_TO_FARM
        && let Some(clan_id) = clan
        && let Some(view) = clan_views.get(&clan_id)
        && (pos.x - view.home.0).hypot(pos.y - view.home.1) <= RESIDENCE_RADIUS_TILES
    {
        let mut nearest: Option<(f64, (f64, f64))> = None;
        for h in herds {
            if h.tameness < crate::pastoral::DOMESTICATED_THRESHOLD {
                continue;
            }
            let d2 = (pos.x - h.pos.0).powi(2) + (pos.y - h.pos.1).powi(2);
            if nearest.is_none_or(|(bd, _)| d2 < bd) {
                nearest = Some((d2, h.pos));
            }
        }
        if let Some((_, hpos)) = nearest {
            let target = (hpos.0.floor() as i64, hpos.1.floor() as i64);
            candidates.push((TaskKind::Herd, target, HERD_DRIVE));
        }
    }

    // — Explorer (adultes au confort) : le drive « comprendre ». La
    //   curiosité pousse vers une cellule **jamais visitée** — huit azimuts
    //   sondés, on tire parmi ceux qui mènent à l'inconnu (et à la terre).
    //   Deux gardes-fous, tous deux mesurés sur la démo d'un an :
    //   - un besoin qui monte éteint l'envie (on n'explore que repu) ;
    //   - la cible doit rester **à portée d'une source connue** — on pousse
    //     la frontière le long du réseau d'eau, on ne s'enfonce pas dans
    //     l'inconnu sec. Sans cet arrimage, la population s'étalait sur
    //     90 km en un an et le churn de chunks écroulait la simulation
    //     (0,2 tick/s) tout en dissolvant la moindre vie sociale.
    let comfort = 1.0
        - phys
            .hunger
            .max(phys.thirst)
            .max(phys.fatigue)
            .max(phys.cold);
    if adult && comfort > 0.5 {
        let mut unknown: Vec<(i64, i64)> = Vec::new();
        for k in 0..8 {
            let angle = f64::from(k) * std::f64::consts::TAU / 8.0;
            let target = (
                (pos.x + angle.cos() * EXPLORE_LEG_TILES).floor() as i64,
                (pos.y + angle.sin() * EXPLORE_LEG_TILES).floor() as i64,
            );
            let watered = mem
                .nearest_known_spring((target.0 as f64, target.1 as f64))
                .is_some_and(|s| {
                    let d2 = (s.0 - target.0).pow(2) + (s.1 - target.1).pow(2);
                    d2 <= EXPLORE_TETHER_TILES * EXPLORE_TETHER_TILES
                });
            if watered
                && !mem.knows_cell(cell_of(target))
                && world.worldgen().elevation(target.0, target.1) > 0.0
            {
                unknown.push(target);
            }
        }
        if !unknown.is_empty() {
            let mut rng =
                Pcg32::new(world.seed().derive(salt::EXPLORE) ^ splitmix64(time.tick), id.0);
            let target = unknown[(rng.next_u32() as usize) % unknown.len()];
            let score = 0.15 * traits.curiosity * comfort * sight;
            candidates.push((TaskKind::Explore, target, score));
        }
    }

    // — Errer : bruit de fond exploratoire, qui enfle en désespoir quand un
    //   besoin monte sans solution — ni vue, ni **sue** : connaître une
    //   source, même lointaine, suffit à garder son calme. C'est l'errance
    //   qui disperse les groupes quand une zone s'épuise — personne ne le
    //   scripte.
    let mut desperation = 0.0;
    if spring.is_none() && mem.nearest_known_spring((pos.x, pos.y)).is_none() {
        desperation += 0.6 * phys.thirst;
    }
    // La faim ne pousse à partir que si **ni** la cueillette **ni** le gibier
    // ne répondent ici : c'est ce qui vide une zone surchassée et surpâturée.
    if (forage_biomass < 30 || share_here < LEAN_SHARE) && nearest_herd.is_none() {
        desperation += 0.5 * phys.hunger;
    }
    let wander_score = (0.06 + desperation).min(1.0) * sight;
    let mut wander_rng =
        Pcg32::new(world.seed().derive(salt::WANDER) ^ splitmix64(time.tick), id.0);
    let angle = wander_rng.next_f64() * std::f64::consts::TAU;
    // On vise la **terre** : on avance le long du rayon jusqu'à la dernière
    // tuile ferme avant l'eau. Sans ça, errer vers l'océan déclenchait un A*
    // (partiel, coûteux) pour un but inatteignable — cause n°1 de pathfinding
    // inutile près des côtes.
    //
    // La longueur de la jambe suit la **curiosité** (héritée) : un curieux
    // pousse plus loin du connu — c'est le critère mesurable « les curieux
    // explorent plus loin ». Les enfants font des jambes courtes : ils
    // gravitent autour de leurs parents.
    let leg = if adult {
        WANDER_LEG_TILES * (0.5 + f64::from(traits.curiosity))
    } else {
        WANDER_LEG_TILES * 0.25
    };
    let (dx, dy) = (angle.cos(), angle.sin());
    let mut wander_target = here;
    let mut d = 40.0;
    while d <= leg {
        let p = ((pos.x + dx * d).floor() as i64, (pos.y + dy * d).floor() as i64);
        if world.worldgen().elevation(p.0, p.1) <= 0.0 {
            break; // eau : on s'arrête à la dernière terre trouvée
        }
        wander_target = p;
        d += 40.0;
    }
    candidates.push((TaskKind::Wander, wander_target, wander_score));

    // — L'urgence vitale (chantier de l'eau) : un besoin qui tuera bientôt
    //   l'emporte. Le temps avant la mort se lit dans les rythmes mêmes du
    //   corps (`agent`) — la santé qui s'use — et non dans un réglage :
    //   à saturation, la soif tue en ~2 jours, la faim en ~2 semaines. Sous
    //   `VITAL_HORIZON_TICKS`, la réponse à ce besoin gagne un bonus qui monte
    //   vers 1 à mesure que la mort approche. Rien n'est forcé (le tirage
    //   reste un tirage) ; c'est la « règle des trois » de la survie : l'eau
    //   avant la nourriture.
    //   Seulement pour un besoin **saturé**, qui ronge déjà la santé : avant,
    //   les courbes ordinaires suffisent, et la soif (qui tue toujours en moins
    //   de trois jours) ferait sinon boire tout le monde plus tôt.
    let emergency = |need: f32, damage: f32| {
        if need < 1.0 {
            return 0.0;
        }
        let ticks_left = f64::from(phys.health.max(0.0) / damage);
        (1.0 - ticks_left / VITAL_HORIZON_TICKS).max(0.0) as f32
    };
    let thirst_bonus = emergency(phys.thirst, crate::agent::DAMAGE_DEHYDRATION);
    let hunger_bonus = emergency(phys.hunger, crate::agent::DAMAGE_STARVATION);
    for c in &mut candidates {
        match c.0 {
            TaskKind::Drink => c.2 += thirst_bonus,
            TaskKind::Forage
            | TaskKind::Hunt
            | TaskKind::Track
            | TaskKind::SeekGame
            | TaskKind::EatFromStock => c.2 += hunger_bonus,
            _ => {}
        }
    }

    // — Engagement : la tâche en cours part avec une longueur d'avance.
    if let Some(kind) = current {
        for c in &mut candidates {
            if c.0 == kind {
                c.2 += COMMITMENT_BONUS;
            }
        }
    }

    candidates
}

/// Une motivation candidate, exposée pour l'inspection : la tâche, sa cible,
/// son score brut et sa probabilité softmax. La « pile de motivations avec
/// scores » du BRIEF §7.2 — *voir* pourquoi l'agent choisit ce qu'il choisit.
#[derive(Debug, Clone, Copy)]
pub struct Motivation {
    pub kind: TaskKind,
    pub target: (i64, i64),
    pub score: f32,
    pub probability: f32,
}

/// Rejoue la délibération d'un agent **sans effet de bord** (en particulier
/// sans mémoriser la source aperçue — un agent inspecté ne doit pas apprendre
/// plus vite qu'un autre, ce qui briserait le déterminisme) et renvoie ses
/// motivations, de la plus probable à la moins probable. Purement pour
/// l'affichage : `decide` reste seul juge des tâches réelles.
#[allow(clippy::too_many_arguments)]
pub fn inspect(
    world: &mut World,
    time: SimTime,
    agent: &AgentCtx<'_>,
    mem: &Memory,
    current: Option<TaskKind>,
    herds: &[HerdView],
    packs: &[PackView],
    humans: &[HumanView],
    clan_views: &BTreeMap<ClanId, ClanView>,
    relations: &ClanRelations,
    shrines: &[crate::cult::Shrine],
) -> Vec<Motivation> {
    let here = agent.pos.tile();
    let spring = world.nearest_spring(here, SPRING_RADIUS_CHUNKS); // lue, jamais mémorisée
    let candidates =
        build_candidates(world, time, agent, mem, spring, current, herds, packs, humans, clan_views, relations, shrines);
    let scores: Vec<f32> = candidates.iter().map(|c| c.2).collect();
    let probs = softmax_weights(&scores, SOFTMAX_TAU);
    let mut out: Vec<Motivation> = candidates
        .iter()
        .zip(probs)
        .map(|(&(kind, target, score), probability)| Motivation { kind, target, score, probability })
        .collect();
    // La pile, du plus fort au plus faible (total_cmp : ordre total sur f32,
    // pas de surprise NaN).
    out.sort_by(|a, b| b.probability.total_cmp(&a.probability));
    out
}

/// Décote de trajet : 1 à distance nulle, ½ à une heure de marche.
fn travel_discount(dist_tiles: f64) -> f32 {
    (1.0 / (1.0 + dist_tiles / WALK_TILES_PER_TICK)) as f32
}

/// Le troupeau visible le plus proche, à 2 km de jour et quelques dizaines de
/// mètres de nuit (`sight`, MAR-2). Départage déterministe : distance, puis
/// ordre de l'instantané.
fn nearest_herd(pos: &Position, herds: &[HerdView], sight: f32) -> Option<HerdView> {
    let mut best: Option<(f64, HerdView)> = None;
    let range = HERD_SIGHT_TILES * f64::from(sight);
    for h in herds {
        let d2 = (pos.x - h.pos.0).powi(2) + (pos.y - h.pos.1).powi(2);
        if d2 <= range * range && best.is_none_or(|(bd, _)| d2 < bd) {
            best = Some((d2, *h));
        }
    }
    best.map(|(_, h)| h)
}

/// Durée de vie d'un souvenir de gibier, en jours : un troupeau se déplace
/// de quelques kilomètres par jour, ses traces s'effacent en quelques jours.
const GAME_MEMORY_DAYS: f64 = 3.0;
/// Au-delà de cette distance du foyer de son clan (~500 m), on peut choisir
/// de rentrer dormir au campement plutôt que sur place.
const CAMP_RADIUS_TILES: f64 = 250.0;
/// Ce que vaut une maille inconnue face à une maille connue de même richesse
/// (piste F : l'attachement d'une bande à ses terres). Choix ; ordre de
/// grandeur : l'inconnu n'est pas interdit, il pèse moitié moins.
const FAMILIAR_FLOOR: f32 = 0.5;
/// Portée d'une sortie à la journée depuis le camp (~10 km : Kelly 1995,
/// *The Foraging Spectrum* — au-delà, on ne fait plus l'aller-retour, on
/// déménage). C'est jusqu'où un affamé pousse sa quête de gibier (D2).
const FORAY_RADIUS_TILES: f64 = km_to_tiles(10.0);
/// Arrivé à moins de 100 m de la piste sans rien voir, on l'oublie.
const GAME_ARRIVAL_TILES: f64 = 50.0;

/// En deçà de cette part (deux jours de nourriture par tête dans la maille),
/// le pays « ne répond plus » : c'est le moment de puiser dans le stock.
const LEAN_SHARE: f32 = 0.3;

/// Ce que la maille de `pos` offre de comestible à chacun de ceux qui y
/// vivent, rapporté à une semaine de nourriture : 1 = de quoi tenir, 0 = rien.
fn edible_share(world: &mut World, pos: (f64, f64), tick: u64, humans: &[HumanView]) -> f32 {
    const WEEK_KCAL: f64 = 7.0 * 2_500.0;
    let zone = crate::fauna::range_zone(pos);
    let here = humans.iter().filter(|h| crate::fauna::range_zone(h.pos) == zone).count().max(1);
    let kcal = world.edible_kcal(pos, tick);
    (kcal / (here as f64 * WEEK_KCAL)).min(1.0) as f32
}

/// Le poids d'une maille selon qu'on la connaît : `FAMILIAR_FLOOR` pour une
/// maille où l'on n'a jamais mis les pieds, 1 pour une maille parcourue en
/// entier (part des cellules de mémoire foulées).
fn familiarity_weight(mem: &Memory, center: (f64, f64), side: f64) -> f32 {
    let step = crate::memory::MEMORY_CELL_TILES as f64;
    let (mut seen, mut all) = (0u32, 0u32);
    let mut y = center.1 - side / 2.0;
    while y < center.1 + side / 2.0 {
        let mut x = center.0 - side / 2.0;
        while x < center.0 + side / 2.0 {
            all += 1;
            seen += u32::from(mem.known.contains(&crate::memory::cell_of((x as i64, y as i64))));
            x += step;
        }
        y += step;
    }
    let known = if all == 0 { 0.0 } else { seen as f32 / all as f32 };
    FAMILIAR_FLOOR + (1.0 - FAMILIAR_FLOOR) * known
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

/// Le lieu sacré le plus proche à portée d'appel, pour qui a la ferveur d'y
/// aller. Duplique volontairement la géométrie de `Sim::shrine_call` : la
/// délibération ne voit pas `Sim`, seulement des instantanés — même situation
/// que `social::claim_from_views`, et un test garde les deux d'accord.
fn nearest_shrine(
    shrines: &[crate::cult::Shrine],
    from: (f64, f64),
    fervor: f32,
) -> Option<(f64, f64)> {
    if fervor < crate::cult::PILGRIM_FERVOR {
        return None;
    }
    shrines
        .iter()
        .map(|s| (s.pos, (from.0 - s.pos.0).hypot(from.1 - s.pos.1)))
        .filter(|&(_, d)| d <= crate::cult::SHRINE_PULL_TILES)
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(pos, _)| pos)
}

/// Expose [`nearest_shrine`] au test qui garde les deux lectures du lieu sacré
/// cohérentes (voir `cult`) — la fonction elle-même reste privée.
#[cfg(test)]
pub(crate) fn nearest_shrine_for_test(
    shrines: &[crate::cult::Shrine],
    from: (f64, f64),
    fervor: f32,
) -> Option<(f64, f64)> {
    nearest_shrine(shrines, from, fervor)
}
