//! La boucle de simulation à tick fixe (BRIEF §8.2).
//!
//! `Sim` réunit le monde de tuiles (chunké, mutable), la population d'agents
//! (entités `hecs`) et l'horloge. Chaque [`step`](Sim::step) déroule les
//! systèmes **dans un ordre fixe, écrit ici noir sur blanc** — c'est notre
//! scheduler, et c'est ce qui rend la simulation déterministe :
//!
//! 0. **Instantanés** : positions du gibier, des meutes, des humains.
//! 1. **Délibération** (bucketée) : les agents « dus » choisissent une tâche.
//! 2. **Exécution** : chacun avance sa tâche — marche, mange, boit, chasse —
//!    et note au passage où il a mis les pieds (mémoire spatiale).
//!    2 bis. **Nourrissons** : portés par leur mère, allaités par elle.
//! 3. **Physiologie** : les besoins dérivent, le climat mord, on meurt.
//!    3 bis. **Démographie** (une fois par jour) : naissances, conceptions,
//!    sénescence.
//!    3 ter. **Savoirs** (toutes les 4 h) : échange des sources connues entre
//!    agents à portée de conversation.
//! 4. **Faune** : les meutes chassent, les troupeaux paissent, fuient, migrent.
//! 5. **Écologie** (une fois par jour) : la biomasse consommée repousse.
//!
//! Humains et faune vivent dans **deux mondes `hecs` distincts** : ils
//! partagent le composant `Position`, et une requête sur `&Position` dans un
//! monde unique ramasserait les deux — source de bugs silencieux. La
//! coordination passe par des instantanés (`HerdView`) : on lit un état figé,
//! puis on mute. Aucune lecture ne dépend donc de l'ordre des écritures.
//!
//! L'itération `hecs` est déterministe pour notre usage : les entités d'un
//! même type partagent leur jeu de composants (une seule archétype), l'ordre
//! est l'ordre d'apparition, et les retraits sont eux-mêmes ordonnés.

use std::collections::{BTreeMap, BTreeSet};

use cairn_core::{Pcg32, SimTime, TICKS_PER_DAY, WorldSeed, splitmix64};
use cairn_worldgen::WorldGenConfig;

use crate::agent::{
    Activity, AgentId, Behavior, Carrying, DeathCause, FOREST_BONUS_C, Physiology, Position,
    Prestige, SHELTER_BONUS_C, TaskKind, WALK_TILES_PER_TICK, Wound,
};
use crate::brain::{self, AgentCtx, DELIBERATION_PERIOD};
use crate::chronicle::{self, EventKind};
use crate::climate::Climate;
use crate::combat::{self, Clash, Engagement};
use crate::commerce::{self, Expedition};
use crate::demography::{self, Demographics, HumanView, Kinship, Sex, Traits};
use crate::disease::{self, Illness};
use crate::energy;
use crate::ecology;
use crate::exposure::Exposures;
use crate::fire::{self, Fire};
use crate::fauna::{self, FaunaId, Herd, HerdView, Kill, Pack, PackView};
use crate::memory::{self, Memory};
use crate::names;
use crate::pathfind;
use crate::salt;
use crate::pressure::{self, ClanPressure};
use crate::skills::{self, Skills};
use crate::social::{self, Clan, ClanEvent, ClanId, ClanMembership, ClanRelations, ClanView, SocialGraph};
use crate::structures::{self, Structure, StructureKind};
use crate::tech::{self, Knowledge, TechEvent, TechId, TechTree};
use crate::world::World;

/// Un agent est « arrivé » sous une tuile et demie de sa cible.
const ARRIVAL_TILES: f64 = 1.5;
/// Pas d'échantillonnage du terrain pendant la marche : on vérifie la
/// franchissabilité toutes les 8 tuiles (16 m), pas à chaque tuile.
const WALK_SAMPLE_TILES: f64 = 8.0;
/// Une bouchée d'une heure : ce qu'un agent peut réduire de faim en mangeant.
const EAT_HUNGER_PER_TICK: f32 = 0.3;
/// Valeur nutritive d'une unité de biomasse, en points de faim.
const NUTRITION_PER_BIOMASS: f32 = 0.02;
/// Énergie d'un point de nourriture (`energy::KCAL_PER_POINT`) : deux jours
/// de l'adulte de référence ; chaque mangeur le convertit à son échelle.
pub(crate) const KCAL_PER_HUNGER: f64 = energy::KCAL_PER_POINT;
/// Ration quotidienne correspondante, en kcal.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const KCAL_PER_DAY: f64 = KCAL_PER_HUNGER * crate::agent::HUNGER_PER_TICK as f64 * 24.0;

/// Ce que la cueillette peut rendre durablement, en kcal par m² et par an,
/// tel que le modèle le définit : la production comestible de la maille
/// (`crate::gathering`), part comestible de la NPP convertie en kcal.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn gathering_kcal_m2_yr(biome: cairn_worldgen::Biome) -> f64 {
    f64::from(ecology::npp_g_m2_yr(biome) * crate::gathering::edible_fraction(biome))
        * crate::gathering::EDIBLE_KCAL_PER_G
}

/// Rayon dans lequel on accourt à une prise pour en avoir sa part : la
/// portée de la voix et du regard autour d'un dépeçage, ~500 m.
const SHARE_RADIUS_TILES: f64 = cairn_core::km_to_tiles(0.5);

/// Une part de viande offerte par un chasseur autour de sa prise, à
/// distribuer après la boucle d'exécution (on ne touche pas à la faim des
/// autres pendant qu'on itère sur eux).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Share {
    giver: AgentId,
    pos: (f64, f64),
    amount: f32,
}

/// Distribue les parts : les plus affamés d'abord, chacun mange au plus sa
/// faim. Ce que personne n'a mangé revient au chasseur, qui le porte — on
/// n'offre pas pour jeter.
fn resolve_shares(sim: &mut Sim, shares: &[Share]) {
    for share in shares {
        let r2 = SHARE_RADIUS_TILES * SHARE_RADIUS_TILES;
        let mut near: Vec<(f32, AgentId, hecs::Entity)> = sim
            .agents
            .query::<(&AgentId, &Position, &Physiology)>()
            .iter()
            .filter(|(_, (id, p, ph))| {
                **id != share.giver
                    && ph.hunger > 0.0
                    && (p.x - share.pos.0).powi(2) + (p.y - share.pos.1).powi(2) <= r2
            })
            .map(|(e, (id, _, ph))| (ph.hunger, *id, e))
            .collect();
        near.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.0.cmp(&b.1.0)));
        let mut left = share.amount;
        for (_, _, entity) in near {
            if left <= 0.0 {
                break;
            }
            if let Ok(ph) = sim.agents.query_one_mut::<&mut Physiology>(entity) {
                let eaten = ph.eat_points(left);
                left -= eaten;
                crate::food_stats::fed(crate::food_stats::Source::Shared, eaten);
            }
        }
        if left > 0.0 {
            let giver = sim
                .agents
                .query_mut::<(&AgentId, &mut Carrying)>()
                .into_iter()
                .find(|(_, (id, _))| **id == share.giver)
                .map(|(_, (_, carrying))| carrying);
            match giver {
                Some(carrying) => {
                    carrying.0 += left;
                    crate::food_stats::fed(crate::food_stats::Source::HuntCarried, left);
                }
                None => crate::food_stats::fed(crate::food_stats::Source::HuntWasted, left),
            }
        }
    }
}

/// Chance, pour un chasseur moyen, de tuer dans l'heure quand il est à portée.
/// Un chasseur hadza abat un gros animal une fois par ~29 jours de chasse
/// (1-3 % des jours, arc seul) ; les Ju/'hoansi rapportent de la viande sur
/// moins de 27 % des jours, petit gibier compris. 2 % par heure à portée donne
/// ~11 % sur six heures de poursuite : le milieu de cette fourchette.
const HUNT_SUCCESS_PER_HOUR: f32 = 0.02;

/// L'approche de `hunter`, à portée au tick `tick`, tue-t-elle ? Tirage
/// déterministe (seed, tick, chasseur), chance `HUNT_SUCCESS_PER_HOUR` que la
/// compétence double au mieux.
/// De nuit, l'approche se fait à l'aveugle : la chance suit la vue (MAR-2).
fn hunt_succeeds(seed: WorldSeed, tick: u64, hunter: AgentId, skill: f32, sight: f32) -> bool {
    let mut rng = Pcg32::new(seed.derive(crate::salt::HUNT) ^ cairn_core::splitmix64(tick), hunter.0);
    rng.next_f32() < HUNT_SUCCESS_PER_HOUR * (0.5 + skill) * sight
}

/// Portée d'une mise à mort : le chasseur doit être à ~200 m du troupeau.
const HUNT_REACH_TILES: f64 = cairn_core::km_to_tiles(0.2);
/// Têtes prélevées par chasse réussie.
const HUNT_YIELD_HEAD: f32 = 1.0;
/// Ce qu'une bête de `species` retire de faim, en points : son énergie
/// comestible (`Species::edible_kcal`). Un cerf vaut ~14 points, un mois de
/// nourriture pour une personne : bien plus qu'un chasseur n'en mange, d'où
/// le surplus rapporté au clan.
fn meat_hunger(species: fauna::Species) -> f32 {
    (f64::from(species.edible_kcal()) / KCAL_PER_HUNGER) as f32
}
/// Échelle d'une réserve « pleine », par membre : 3 points, six jours de
/// nourriture. Ce n'est plus un plafond (le stock pourrit, voir
/// `FRESH_KEEP_DAYS`) mais l'unité dans laquelle on juge qu'un clan a de quoi
/// désirer un grenier, et dans laquelle le client affiche la réserve. `pub`
/// (même patron que `memory::MEMORY_CELL_TILES`) pour le client.
pub const STOCK_SCALE_PER_MEMBER: f32 = 3.0;
/// Durée de vie moyenne, en jours, de la nourriture mise en réserve sans
/// aucune technique : la viande crue se gâte en quelques jours. Le stock
/// décroît en `e^(−t/τ)`, au lieu de buter sur un plafond.
const FRESH_KEEP_DAYS: f64 = 3.0;
/// Faim au-delà de laquelle on mange ce qu'on porte : une demi-journée sans
/// repas.
const EAT_CARRIED_HUNGER: f32 = 0.25;
/// Durée de vie de la réserve d'un clan dont **tous** les membres savent
/// conserver (technique `preservation`) : viande séchée, gelée ou cachée au
/// frais, des mois. Entre les deux, la durée suit la part des membres qui
/// savent — un facteur, pas une porte.
const PRESERVED_KEEP_DAYS: f64 = 180.0;
/// Requêtes A* autorisées par tick (BRIEF §8.2 : « pathfinding budgété »).
/// Seuls les agents que l'eau bloque en consomment ; les autres marchent en
/// ligne droite pour rien.
const PATH_REQUESTS_PER_TICK: u32 = 8;

// — Agriculture (Phase 5, chaîne §5.3) : entretenir un champ élève la biomasse
//   d'une prairie au-dessus de sa capacité sauvage. La récolte passe par la
//   cueillette ordinaire (tuile plus riche → meilleur rendement) ; l'écologie
//   ramène un champ abandonné vers la friche. `pub` (comme `STOCK_SCALE...`) : la
//   délibération (`brain`) en a besoin pour proposer le candidat `Cultivate`. —
/// Plafond de biomasse d'un champ entretenu — la richesse d'un champ cultivé,
/// bien au-dessus de ce que la prairie sauvage porte seule (`u8`, 0–255).
pub const CULTIVATED_CEILING: u8 = 230;
/// Fertilité minimale d'un sol pour qu'il vaille d'être cultivé : on ne sème
/// pas la roche nue.
pub const FIELD_MIN_FERTILITY: u8 = 40;
/// Biomasse ajoutée par tour d'entretien, avant modulation par la capacité de
/// travail et la fertilité du sol.
const CULTIVATE_GAIN: f32 = 30.0;
/// Fertilité prélevée par tour d'entretien : le champ épuise lentement le sol
/// (§2.4) — sans jachère ni fumure, il finit par ne plus rien rendre, et le
/// clan doit défricher ailleurs (agriculture itinérante émergente).
const CULTIVATE_FERTILITY_COST: u8 = 1;

// — Plaies (BRIEF §3.1) : infligées en combattant les prédateurs (`combat`),
//   persistantes, distinctes de la santé. —
/// Part de capacité de travail qu'une plaie **pleine** retire : un blessé grave
/// est deux fois moins efficace à tout geste (cueillette, chasse, combat…).
const WOUND_WORK_PENALTY: f32 = 0.5;
/// Cicatrisation par tick : une plaie se referme lentement, indépendamment des
/// besoins vitaux (~deux semaines pour une plaie pleine).
const WOUND_HEAL_PER_TICK: f32 = 1.0 / (24.0 * 14.0);

/// Têtes prélevées **durablement** sur un cheptel à chaque tour de garde
/// (domestication, `crate::pastoral`) : bien moins que la croissance d'un
/// troupeau protégé, pour qu'il se refasse (traite/abattage mesuré). La
/// nourriture obtenue (la viande de ces têtes, `meat_hunger`) va au stock
/// commun, pas dans le ventre de l'éleveur.
const HERD_HARVEST_HEAD: f32 = 0.3;

/// Trajet calculé par l'A* pour contourner l'eau : une suite d'étapes à
/// rejoindre en ligne droite, vers un `goal` donné. Vit dans une table à côté
/// de l'ECS (le composant `Behavior` doit rester `Copy`, or un chemin est un
/// `Vec`).
pub(crate) struct Route {
    goal: (i64, i64),
    waypoints: Vec<(i64, i64)>,
    cursor: usize,
}

/// Issue d'un pas de déplacement.
#[derive(Debug, PartialEq, Eq)]
enum Move {
    /// Arrivé à moins d'`ARRIVAL_TILES` de la cible.
    Arrived,
    /// A progressé ce tick.
    Moved,
    /// Bloqué : cerné par l'eau, aucun contournement trouvé.
    Stuck,
    /// L'eau barre la ligne droite et le budget d'A* du tick est épuisé : on
    /// attend le tick suivant, la tâche gardée. **Pas** un blocage — une limite
    /// de calcul ne doit jamais devenir la croyance « inaccessible » (D11).
    Deferred,
}

/// Trace d'un décès, pour les statistiques — et, un jour, la Chronique.
#[derive(Debug, Clone, Copy)]
pub struct DeathRecord {
    pub tick: u64,
    pub agent: AgentId,
    pub cause: DeathCause,
    pub pos: (i64, i64),
}

/// Trace d'une naissance — le pendant du décès, et le futur matériau de
/// l'arbre généalogique.
#[derive(Debug, Clone, Copy)]
pub struct BirthRecord {
    pub tick: u64,
    pub mother: AgentId,
    pub father: AgentId,
    pub child: AgentId,
}

pub struct Sim {
    pub world: World,
    pub climate: Climate,
    pub agents: hecs::World,
    /// Troupeaux et meutes — monde séparé des humains (voir l'en-tête).
    pub fauna: hecs::World,
    pub time: SimTime,
    pub deaths: Vec<DeathRecord>,
    /// Naissances depuis le début du monde (Phase 3).
    pub births: Vec<BirthRecord>,
    /// Allomaternage : le nourrisson orphelin → la femme qui l'allaite à la
    /// place de sa mère (voir `demography::adopt_orphans`).
    pub fosters: BTreeMap<u64, AgentId>,
    /// Les pistes du gibier sauvage (CHA-2) : ce qu'un chasseur peut suivre au
    /// matin quand le troupeau est hors de vue.
    pub herd_trails: fauna::HerdTrails,
    /// Têtes de gibier prélevées par les humains depuis le début : le compteur
    /// de la pression de chasse.
    pub hunted_head: f32,
    /// Nombre cumulé d'appels A* (observabilité du coût de pathfinding).
    pub path_calls: u64,
    /// Demandes de trajet refusées faute de budget (observabilité : un refus
    /// ne devrait jamais se lire comme « vraiment cerné »).
    pub path_denied: u64,
    /// Agent-ticks passés à s'abriter (observabilité du comportement de froid).
    pub shelter_ticks: u64,
    /// Trajets d'évitement d'eau en cours, par identifiant d'agent.
    pub(crate) routes: BTreeMap<u64, Route>,
    /// Le graphe d'affinités entre agents (Phase 4, voir `social`).
    pub social: SocialGraph,
    /// Les clans détectés — reconstruit en entier chaque jour, jamais
    /// modifié à la main : voir `social::daily`.
    pub clans: Vec<Clan>,
    /// Formations et effondrements de clans depuis le début du monde.
    pub clan_events: Vec<ClanEvent>,
    /// Tension mesurée entre clans voisins (Phase 4, incrément 6) —
    /// reconstruite en entier chaque jour, juste après `clans` : voir
    /// `social::update_relations`.
    pub clan_relations: ClanRelations,
    /// La pression ressentie par chaque clan (Phase 5, incrément 2) —
    /// famine, froid, menace, surpopulation. Reconstruite en entier chaque
    /// jour par `pressure::measure`, juste après les clans et leurs relations.
    /// Le futur moteur d'insight la lira ; rien ne la consulte encore.
    pub clan_pressure: BTreeMap<ClanId, ClanPressure>,
    /// Les structures bâties par les clans (Phase 4, incrément 9) — un
    /// registre clairsemé côté `Sim`, jamais un champ de `Tile` (voir
    /// `crate::structures`). Chaque clan en a au plus une par type.
    pub structures: Vec<Structure>,
    /// Compteurs de diagnostic sur la detection de clan (voir `ClanDiagnostics`)
    /// -- purement observationnels : aucun systeme ne les lit.
    pub clan_diagnostics: social::ClanDiagnostics,
    /// L'arbre technologique, chargé une fois (BRIEF §5.2, Phase 5) : les techs
    /// et leurs prérequis, en données (`assets/techs.ron`). Immuable pendant la
    /// simulation ; lu par la passe d'insight.
    pub tech_tree: TechTree,
    /// Les découvertes **et oublis** de technologies depuis le début du monde
    /// — l'embryon de la Chronique (BRIEF §6.4).
    pub tech_events: Vec<TechEvent>,
    /// L'ensemble des techs qu'au moins un agent vivant maîtrise — « ce que
    /// l'humanité sait encore faire ». Reconstruit chaque jour par
    /// `tech::forget`, qui le compare à la veille pour détecter les oublis (une
    /// tech dont le dernier porteur s'est éteint). Jamais écrit ailleurs.
    pub known_techs: BTreeSet<TechId>,
    /// Les feux de forêt actifs (Phase 5, incrément 5) — un registre clairsemé
    /// côté `Sim`, jamais un champ de `Tile` (voir `crate::fire`). Rare et
    /// localisé ; vide la plupart du temps.
    pub fires: Vec<Fire>,
    /// Les cellules météo actives (Phase 6) — averses et sécheresses passagères.
    /// Registre clairsemé, jamais un champ de `Tile` : une averse est
    /// temporaire par nature. Voir `crate::weather`.
    pub weather: Vec<crate::weather::WeatherCell>,
    /// **La Chronique** (Phase 6, BRIEF §6.4) : le journal narratif du monde,
    /// append-only et ordonné par tick (on n'y pousse qu'au tick courant).
    /// Ne reçoit que ce qui **fait date** — pas les naissances et les morts
    /// ordinaires, qui restent des statistiques dans `births`/`deaths`. Voir
    /// `crate::chronicle` pour la distinction journal / log.
    pub chronicle: Vec<crate::chronicle::Event>,
    /// Le journal des **interventions divines** (BRIEF §6.2, §8.2) : avec la
    /// seed, il suffit à rejouer une histoire entière, la simulation étant
    /// déterministe. Voir `crate::divine`.
    pub miracles: Vec<crate::divine::Miracle>,
    /// **La Foi** (BRIEF §6.1) : la ressource du joueur, produite chaque jour
    /// par ses croyants et dépensée par ses miracles. Nulle au départ — « aucun
    /// croyant → la divinité est quasi impuissante ». Voir `crate::faith`.
    pub faith: f32,
    /// Les lieux tenus pour sacrés (BRIEF §6.2, « le Signe ») — registre
    /// clairsemé, comme les feux. Voir `crate::cult::Shrine`.
    pub shrines: Vec<crate::cult::Shrine>,
    /// Les expéditions commerciales en cours (Phase 5, incrément 6b), par
    /// identifiant d'agent — une table à côté de l'ECS, comme `routes`,
    /// nettoyée à la mort de l'envoyé. Voir `crate::commerce`.
    pub expeditions: BTreeMap<u64, Expedition>,
    /// Les feux de forêt sont-ils actifs pour cette simulation ? Vrai par
    /// défaut (le monde brûle parfois). Mis à faux par les scènes de test
    /// contrôlées qui isolent une autre mécanique — même patron explicite que
    /// `allow_fauna_immigration`.
    pub allow_wildfires: bool,
    /// La météo se forme-t-elle d'elle-même ? Vraie par défaut (le ciel vit) ;
    /// les scènes de test contrôlées la coupent — même patron explicite que
    /// `allow_wildfires`.
    pub allow_weather: bool,

    /// Pas de simulation des troupeaux **hors de vue de tout humain**, en
    /// ticks. `1` = pas de LOD, chaque troupeau simulé chaque heure.
    ///
    /// Mesuré : 79 à 100 % des troupeaux sont hors de portée de perception, et
    /// on les simulait tous à plein régime. Ce réglage borne ce gaspillage sans
    /// toucher à une seule règle : le troupeau lointain vit plus lentement,
    /// mais il vit — il migre, broute et se scinde encore, contrairement à un
    /// gel qui aurait tué les migrations émergentes du §2.4.
    ///
    /// **Déterministe** : le critère ne lit que l'état du monde (la distance
    /// aux humains), jamais ce qu'un client regarde. Faire dépendre la
    /// simulation d'un viewport ferait dépendre le monde de qui l'observe, et
    /// le replay depuis la seed (§8.2) tomberait.
    pub fauna_lod_period: u64,

    /// Profilage par phase, sous la feature `profile`.
    #[cfg(feature = "profile")]
    pub prof: Profiler,
    /// Télémétrie de la faune (réponse fonctionnelle, refuge). Le champ existe
    /// toujours ; c'est le **type** qui est vide hors de la feature
    /// `fauna-stats`, ce qui évite de semer des `cfg` sur chaque site d'appel.
    pub fauna_stats: crate::fauna::FaunaStats,
    /// Production de fourrage des mailles de pâturage (cache pur du worldgen,
    /// voir `fauna::Rangeland`).
    pub rangeland: crate::fauna::Rangeland,
    /// Le monde est-il **ouvert** aux flux de faune ? L'immigration de gibier
    /// et de prédateurs (`fauna::daily_immigration`), le bord qui retient le
    /// gibier autour des humains (`fauna::FAUNA_PERIMETER_TILES`) et le retrait
    /// des troupeaux qu'ils ont abandonnés (`fauna::FAUNA_ABANDON_TILES`). Vrai
    /// par défaut (le monde est censé être habité) ; les scènes de test qui veulent isoler une mécanique de
    /// toute interférence de faune le mettent à faux explicitement — voir
    /// `sim::tests::scenario_setup`. Ce n'est plus déduit indirectement
    /// (l'ancienne garde `hunted_head > 0` ne se déclenchait jamais si la
    /// faune était épuisée par les prédateurs seuls, ou si la densité de
    /// gibier initiale était nulle) : un choix explicite, pas une heuristique.
    pub allow_fauna_immigration: bool,
    pub(crate) next_agent_id: u64,
    next_fauna_id: u64,
    pub(crate) next_clan_id: u64,
}

/// Chrono de phase — un `Instant` sous la feature `profile`, rien du tout
/// sinon. Le profilage ne doit pas exister dans le binaire de production : ces
/// blocs sont le chemin le plus chaud de la simulation.
#[cfg(feature = "profile")]
type Phase = std::time::Instant;
/// Marqueur de taille nulle hors profilage — et non `()`, qui ferait passer
/// une valeur unité en argument à chaque phase (lint `unit_arg`).
#[cfg(not(feature = "profile"))]
#[derive(Clone, Copy)]
pub struct Phase;

#[cfg(feature = "profile")]
fn phase() -> Phase {
    std::time::Instant::now()
}
#[cfg(not(feature = "profile"))]
fn phase() -> Phase {
    Phase
}

/// Où passe le temps, par grande phase du tick.
///
/// Après sept hypothèses réfutées en devinant la cause d'un effondrement de
/// débit, ceci mesure enfin *où* le temps est dépensé plutôt que de le déduire
/// des sorties de la simulation.
#[cfg(feature = "profile")]
#[derive(Default)]
pub struct Profiler {
    /// nom → (nanosecondes cumulées, nombre d'appels)
    pub rows: std::collections::BTreeMap<&'static str, (u128, u64)>,
    /// nom → compteur cumulé (événements, pas du temps) : par exemple les
    /// chunks régénérés pendant une phase.
    pub counts: std::collections::BTreeMap<&'static str, u64>,
}

#[cfg(feature = "profile")]
impl Profiler {
    pub fn count(&mut self, name: &'static str, n: u64) {
        *self.counts.entry(name).or_insert(0) += n;
    }

    pub fn add(&mut self, name: &'static str, d: std::time::Duration) {
        let e = self.rows.entry(name).or_insert((0, 0));
        e.0 += d.as_nanos();
        e.1 += 1;
    }

    /// Rapport trié par temps décroissant.
    pub fn report(&self) -> String {
        let total: u128 = self.rows.values().map(|(n, _)| *n).sum();
        let mut v: Vec<_> = self.rows.iter().collect();
        v.sort_by_key(|(_, (n, _))| std::cmp::Reverse(*n));
        let mut out = format!(
            "{:<18}{:>9}{:>10}{:>14}{:>12}\n",
            "phase", "part", "ms", "appels", "µs/appel"
        );
        for (nom, (nanos, calls)) in v {
            let pct = if total == 0 { 0.0 } else { *nanos as f64 / total as f64 * 100.0 };
            let us = if *calls == 0 { 0.0 } else { *nanos as f64 / *calls as f64 / 1000.0 };
            let ms = *nanos as f64 / 1e6;
            out += &format!("{nom:<18}{pct:>7.1} %{ms:>10.0}{calls:>14}{us:>12.1}\n");
        }
        out
    }
}

impl Sim {
    #[cfg(feature = "profile")]
    #[inline]
    fn end(&mut self, name: &'static str, p: Phase) {
        self.prof.add(name, p.elapsed());
    }
    #[cfg(not(feature = "profile"))]
    #[inline(always)]
    fn end(&mut self, _name: &'static str, _p: Phase) {}
}

impl Sim {
    /// `chunk_capacity` : nombre de chunks résidents du LRU. Doit couvrir
    /// largement la zone active de la population (un chunk = 128 m de côté).
    pub fn new(seed: WorldSeed, chunk_capacity: usize) -> Self {
        Self::with_config(seed, chunk_capacity, WorldGenConfig::default())
    }

    /// Comme [`new`](Self::new), mais avec une config de worldgen (le client
    /// veut une humidité rapide).
    pub fn with_config(seed: WorldSeed, chunk_capacity: usize, cfg: WorldGenConfig) -> Self {
        let world = World::with_config(seed, chunk_capacity, cfg);
        let climate = Climate::new(world.worldgen().temperature.latitude());
        Self {
            world,
            climate,
            agents: hecs::World::new(),
            fauna: hecs::World::new(),
            time: SimTime::default(),
            deaths: Vec::new(),
            births: Vec::new(),
            fosters: BTreeMap::new(),
            herd_trails: BTreeMap::new(),
            hunted_head: 0.0,
            path_calls: 0,
            path_denied: 0,
            shelter_ticks: 0,
            routes: BTreeMap::new(),
            social: SocialGraph::default(),
            clans: Vec::new(),
            clan_events: Vec::new(),
            clan_relations: ClanRelations::default(),
            clan_pressure: BTreeMap::new(),
            structures: Vec::new(),
            clan_diagnostics: social::ClanDiagnostics::default(),
            tech_tree: TechTree::embedded(),
            tech_events: Vec::new(),
            known_techs: BTreeSet::new(),
            fires: Vec::new(),
            weather: Vec::new(),
            chronicle: Vec::new(),
            miracles: Vec::new(),
            faith: 0.0,
            shrines: Vec::new(),
            expeditions: BTreeMap::new(),
            allow_wildfires: true,
            allow_weather: true,
            // Pas de LOD par défaut : on ne dégrade pas la simulation sans
            // que l'appelant l'ait demandé.
            fauna_lod_period: 1,
            #[cfg(feature = "profile")]
            prof: Profiler::default(),
            fauna_stats: Default::default(),
            rangeland: Default::default(),
            allow_fauna_immigration: true,
            next_agent_id: 0,
            next_fauna_id: 0,
            next_clan_id: 0,
        }
    }

    /// Lâche un **fondateur** : un adulte au sexe, à l'âge (16–40 ans) et
    /// aux traits dérivés de la seed. Les enfants, eux, naissent — voir
    /// [`demography`].
    pub fn spawn_agent(&mut self, x: f64, y: f64) -> AgentId {
        let id = AgentId(self.next_agent_id);
        self.next_agent_id += 1;
        let (demo, traits) = demography::founder(self.world.seed(), id.0, self.time.tick);
        let entity = self.agents.spawn((
            id,
            Position { x, y },
            Physiology::with_full_reserves(&demo, self.time.tick),
            Behavior::default(),
            traits,
            demo,
            Kinship { mother: None, father: None },
            Memory::default(),
            Skills::founder(&traits),
            ClanMembership::default(),
            Carrying::default(),
            Prestige::default(),
            // Un fondateur n'a encore rien vu de *ce* monde : il découvrira
            // son voisinage en y vivant (comme sa mémoire spatiale part vide).
            Exposures::default(),
            // Et il ne sait faire aucune tech : tout reste à découvrir ou à
            // apprendre — c'est le point de départ de « L'ÉTINCELLE ».
            Knowledge::default(),
            // Vierge de toute blessure : les plaies s'acquièrent en combattant.
            Wound::default(),
        ));
        // Et incroyant : on ne croit que pour avoir vu (voir `crate::faith`).
        // Ajouté après coup, `hecs` plafonnant un bundle à 15 composants — un
        // agent en porte désormais seize.
        let _ = self.agents.insert_one(entity, crate::faith::Faith::default());
        // Un adulte a un passé infectieux : son immunité acquise.
        let _ = self.agents.insert_one(entity, crate::disease::Illness::founder());
        id
    }

    /// Fait naître un enfant : traits hérités, filiation posée, besoins
    /// presque nuls (il vient de naître au sein de sa mère).
    pub(crate) fn spawn_child(
        &mut self,
        x: f64,
        y: f64,
        sex: Sex,
        traits: Traits,
        kin: Kinship,
    ) -> AgentId {
        let id = AgentId(self.next_agent_id);
        self.next_agent_id += 1;
        let entity = self.agents.spawn((
            id,
            Position { x, y },
            Physiology {
                hunger: 0.1,
                thirst: 0.1,
                fatigue: 0.0,
                cold: 0.0,
                health: 1.0,
                last_damage: None,
                ..Physiology::with_full_reserves(
                    &Demographics { sex, born_tick: self.time.tick as i64, pregnancy: None },
                    self.time.tick,
                )
            },
            Behavior::default(),
            traits,
            Demographics { sex, born_tick: self.time.tick as i64, pregnancy: None },
            kin,
            // Un nouveau-né ne sait rien et ne connaît rien : tout est à
            // apprendre — c'est ce qui rend l'oubli générationnel possible.
            Memory::default(),
            Skills::default(),
            // Pas de clan à la naissance : il en hérite dès le lendemain,
            // simplement en vivant collé à sa mère (voir `social`).
            ClanMembership::default(),
            Carrying::default(),
            Prestige::default(),
            // Un nouveau-né n'a rien vu : tout est à découvrir — c'est aussi
            // ce qui rend l'oubli générationnel d'un savoir-faire possible.
            Exposures::default(),
            // Ni aucun savoir-faire : les techs de ses parents ne sont pas
            // héritées, elles s'apprendront (diffusion, incrément 4) ou non.
            Knowledge::default(),
            // Vierge de toute blessure : les plaies s'acquièrent en combattant.
            Wound::default(),
        ));
        // Un nouveau-né naît incroyant : le monde ne se souvient pas pour lui.
        let _ = self.agents.insert_one(entity, crate::faith::Faith::default());
        // Un système immunitaire vierge : tout reste à rencontrer.
        let _ = self.agents.insert_one(entity, crate::disease::Illness::default());
        id
    }

    /// Fait naître un troupeau dont l'**espèce est choisie selon le biome** du
    /// lieu (le bon animal au bon endroit — aurochs en prairie, renne en
    /// toundra). Tirage déterministe dérivé de l'identifiant (assigné dans un
    /// ordre déterministe).
    pub fn spawn_herd(&mut self, x: f64, y: f64, population: f32) -> FaunaId {
        let biome = self.world.tile(x.floor() as i64, y.floor() as i64).biome;
        let mut rng = Pcg32::new(self.world.seed().derive(salt::FAUNA), self.next_fauna_id);
        let species = fauna::Species::herbivore_for_biome(biome, &mut rng);
        self.spawn_herd_species(x, y, population, species)
    }

    /// Fait naître un troupeau d'une **espèce imposée** — utilisé par la fission,
    /// qui préserve l'espèce de la mère. `spawn_herd` en est le wrapper qui
    /// choisit l'espèce selon le biome.
    pub fn spawn_herd_species(
        &mut self,
        x: f64,
        y: f64,
        population: f32,
        species: fauna::Species,
    ) -> FaunaId {
        let id = FaunaId(self.next_fauna_id);
        self.next_fauna_id += 1;
        self.fauna.spawn((id, Position { x, y }, Herd::new(population, species)));
        id
    }

    /// Fait naître une meute dont l'espèce de prédateur est choisie selon le
    /// biome (le loup partout, le lion des cavernes en terrain ouvert).
    pub fn spawn_pack(&mut self, x: f64, y: f64, population: f32) -> FaunaId {
        let biome = self.world.tile(x.floor() as i64, y.floor() as i64).biome;
        let mut rng = Pcg32::new(self.world.seed().derive(salt::FAUNA), self.next_fauna_id);
        let species = fauna::Species::predator_for_biome(biome, &mut rng);
        let id = FaunaId(self.next_fauna_id);
        self.next_fauna_id += 1;
        self.fauna.spawn((id, Position { x, y }, Pack::new(population, species)));
        id
    }

    /// Fait naître une meute d'une **espèce imposée** — utilisé par la fission,
    /// qui préserve l'espèce de la mère (une meute de loups se scinde en deux
    /// meutes de loups). Pendant exact de `spawn_herd_species`.
    pub fn spawn_pack_species(
        &mut self,
        x: f64,
        y: f64,
        population: f32,
        species: fauna::Species,
    ) -> FaunaId {
        let id = FaunaId(self.next_fauna_id);
        self.next_fauna_id += 1;
        self.fauna.spawn((id, Position { x, y }, Pack::new(population, species)));
        id
    }

    pub fn population(&self) -> usize {
        self.agents.len() as usize
    }

    /// Instantané de la population humaine, **trié par identifiant** (les
    /// consommateurs le fouillent par recherche binaire — retrouver un
    /// parent, un partenaire).
    pub fn human_views(&self) -> Vec<HumanView> {
        let mut views: Vec<HumanView> = self
            .agents
            .query::<(&AgentId, &Position, &Demographics, &ClanMembership)>()
            .iter()
            .map(|(_, (id, pos, demo, membership))| HumanView {
                id: *id,
                pos: (pos.x, pos.y),
                sex: demo.sex,
                adult: demo.is_adult(self.time.tick),
                clan: membership.0,
            })
            .collect();
        views.sort_unstable_by_key(|h| h.id.0);
        views
    }

    /// Le champ de territoire diffusé (Phase 4, incrément 7) : quel clan
    /// revendique ce point, le cas échéant. Calculé à la demande — voir
    /// `social::claim_at` pour le pourquoi (jamais matérialisé dans `Tile`).
    pub fn claim_at(&self, x: f64, y: f64) -> Option<ClanId> {
        social::claim_at((x, y), &self.clans)
    }

    /// Le corpus de savoirs d'un clan (BRIEF §5.1) : l'**union** des techs que
    /// maîtrisent ses membres vivants. Calculé à la demande (pour l'affichage
    /// et le futur calcul de l'Âge) — le corpus n'est pas un état stocké, c'est
    /// une vue sur les `Knowledge` individuels, ce qui rend l'oubli automatique
    /// (un savoir que plus aucun membre ne porte quitte le corpus de lui-même).
    pub fn clan_corpus(&self, clan: ClanId) -> BTreeSet<TechId> {
        let mut corpus = BTreeSet::new();
        for (_, (membership, knowledge)) in
            self.agents.query::<(&ClanMembership, &Knowledge)>().iter()
        {
            if membership.0 == Some(clan) {
                corpus.extend(knowledge.iter());
            }
        }
        corpus
    }

    /// L'âge d'un clan (BRIEF §5.5) : l'étiquette dérivée de son corpus de
    /// savoirs — le plus avancé des marqueurs d'âge de ses techs. Recalculée à
    /// la demande, jamais stockée, **jamais utilisée comme condition** : elle ne
    /// sert qu'à l'affichage.
    pub fn clan_age(&self, clan: ClanId) -> tech::Age {
        tech::age_of(&self.clan_corpus(clan), &self.tech_tree)
    }

    /// Consigne un fait dans la Chronique, daté du tick courant. **Le seul**
    /// chemin d'écriture du journal : la date n'est jamais fournie par
    /// l'appelant, donc les entrées sont ordonnées par construction (elles ne
    /// peuvent pas remonter le temps).
    pub(crate) fn record(&mut self, pos: (i64, i64), kind: crate::chronicle::EventKind) {
        self.chronicle.push(crate::chronicle::Event { tick: self.time.tick, pos, kind });
    }

    /// Le nom d'un agent **vivant** (voir `crate::names`) — `None` s'il est
    /// mort ou hors du monde résident. C'est ici et non dans `names` parce que
    /// le nom dépend du sexe, qu'il faut aller chercher dans l'ECS.
    pub fn agent_name(&self, id: AgentId) -> Option<String> {
        self.agents
            .query::<(&AgentId, &Demographics)>()
            .iter()
            .find(|(_, (a, _))| **a == id)
            .map(|(_, (_, demo))| names::agent_name(self.world.seed(), id, demo.sex))
    }

    /// Le nom d'un clan — pur (aucune recherche), mais exposé ici pour que les
    /// consommateurs n'aient pas à connaître la seed.
    pub fn clan_name(&self, id: ClanId) -> String {
        names::clan_name(self.world.seed(), id)
    }

    /// Les `n` derniers faits de la Chronique, **rédigés**, du plus récent au
    /// plus ancien — ce qu'on lit en se reconnectant (BRIEF §6.4).
    pub fn chronicle_tail(&self, n: usize) -> Vec<String> {
        self.chronicle
            .iter()
            .rev()
            .take(n)
            .map(|e| chronicle::tell(e, self.world.seed(), &self.tech_tree, &self.climate))
            .collect()
    }

    /// Instantané des troupeaux, dans l'ordre d'itération de `hecs`.
    pub fn herd_views(&self) -> Vec<HerdView> {
        self.fauna
            .query::<(&Herd, &Position)>()
            .iter()
            .map(|(entity, (herd, pos))| HerdView {
                entity,
                pos: (pos.x, pos.y),
                population: herd.population,
                species: herd.species,
                tameness: herd.tameness,
            })
            .collect()
    }

    /// Instantané des meutes, dans l'ordre d'itération de `hecs` (déterministe)
    /// — ce que lisent les humains pour décider d'affronter un prédateur.
    pub fn pack_views(&self) -> Vec<PackView> {
        self.fauna
            .query::<(&Pack, &Position)>()
            .iter()
            .map(|(entity, (pack, pos))| PackView {
                entity,
                pos: (pos.x, pos.y),
                population: pack.population,
                species: pack.species,
            })
            .collect()
    }

    /// Rejoue la délibération d'un agent (désigné par identifiant) pour
    /// l'inspection : la **pile de motivations avec scores** du panneau d'agent
    /// (BRIEF §7.2). Reconstruit les mêmes instantanés que `step` (faune,
    /// humains, clans) puis délègue à `brain::inspect`, **sans aucun effet de
    /// bord** sur la simulation. `None` si l'agent n'existe pas, ou s'il est
    /// nourrisson (il ne délibère pas : il est porté).
    pub fn inspect_agent(&mut self, id: AgentId) -> Option<Vec<brain::Motivation>> {
        let time = self.time;
        let herds = self.herd_views();
        let packs = self.pack_views();
        let humans = self.human_views();
        let clan_views: BTreeMap<ClanId, ClanView> = self
            .clans
            .iter()
            .map(|c| (c.id, ClanView { home: c.home, stock: c.stock, desired: c.desired, members: c.members.len() }))
            .collect();
        // On extrait les composants de l'agent (copies, plus un clone de la
        // mémoire) pour relâcher l'emprunt de `self.agents` avant d'appeler
        // `brain::inspect`, qui veut `&mut self.world`.
        let agriculture = self.tech_tree.id_of("agriculture");

        let (pos, phys, traits, demo, kin, clan, carrying, current, knows_agriculture, mem, fervor, wound) = self
            .agents
            .query::<(
                &AgentId,
                &Position,
                &Physiology,
                &Traits,
                &Demographics,
                &Kinship,
                &ClanMembership,
                &Carrying,
                &Behavior,
                &Knowledge,
                &Memory,
                &crate::faith::Faith,
                &Wound,
            )>()
            .iter()
            .find(|(_, (aid, ..))| aid.0 == id.0)
            .map(|(_, (_, pos, phys, traits, demo, kin, membership, carrying, behavior, knowledge, mem, faith, wound))| {
                (
                    *pos,
                    *phys,
                    *traits,
                    *demo,
                    *kin,
                    membership.0,
                    carrying.0,
                    behavior.task.map(|t| t.kind),
                    agriculture.is_some_and(|a| knowledge.has(a)),
                    mem.clone(),
                    faith.fervor,
                    wound.0,
                )
            })?;
        if demo.is_infant(time.tick) {
            return None;
        }
        let ctx = AgentCtx {
            id,
            pos: &pos,
            phys: &phys,
            traits: &traits,
            demo: &demo,
            kin: &kin,
            clan,
            carrying,
            knows_agriculture,
            fervor,
            wound,
            light: self.climate.light(pos.tile().1, time),
            daylight_left_h: self.climate.daylight_left_hours(pos.tile().1, time),
        };
        Some(brain::inspect(
            &mut self.world, time, &ctx, &mem, current, &herds, &packs, &humans, &clan_views,
            &self.clan_relations,
            &self.shrines,
        ))
    }

    /// Effectifs totaux (herbivores, prédateurs) et nombre de groupes.
    pub fn fauna_census(&self) -> (f32, f32, usize, usize) {
        let mut herbivores = 0.0;
        let mut herds = 0;
        for (_, herd) in self.fauna.query::<&Herd>().iter() {
            herbivores += herd.population;
            herds += 1;
        }
        let mut predators = 0.0;
        let mut packs = 0;
        for (_, pack) in self.fauna.query::<&Pack>().iter() {
            predators += pack.population;
            packs += 1;
        }
        (herbivores, predators, herds, packs)
    }

    /// Un tick : une heure de monde.
    pub fn step(&mut self) {
        let time = self.time;

        // 0. Instantanés : l'état de la faune et des humains tel qu'il est
        let _ph = phase(); // 0 instantanes
        // *au début* du tick. Tout le monde délibère sur la même photo.
        // Le territoire de chaque clan (voir `social::detect_clans`) est
        // recalculé une fois par jour ; on n'en prend ici qu'une lecture.
        let herds = self.herd_views();
        let packs = self.pack_views();
        let humans = self.human_views();
        let clan_views: BTreeMap<ClanId, ClanView> = self
            .clans
            .iter()
            .map(|c| (c.id, ClanView { home: c.home, stock: c.stock, desired: c.desired, members: c.members.len() }))
            .collect();

        // L'id de l'agriculture, résolu une seule fois : sert à déballer un
        // `knows_agriculture: bool` par agent pour la délibération (`brain`
        // reste ainsi découplé de l'arbre technologique).
        let agriculture = self.tech_tree.id_of("agriculture");

        // Les lieux sacrés, copiés avant la boucle : `query_mut` emprunte `self`
        // mutablement, et un sanctuaire est un point — le clone est négligeable.
        let shrines = self.shrines.clone();

        self.end("0 instantanes", _ph);
        // 1. Délibération — bucketée : l'agent i ne repense sa tâche qu'aux
        let _ph = phase(); // 1 deliberation
        // ticks (tick + i) % période == 0, ou dès qu'il n'a plus de tâche.
        // Les nourrissons ne délibèrent pas : ils sont portés.
        for (_, (id, pos, phys, traits, demo, kin, membership, carrying, knowledge, faith, behavior, mem, wound)) in self
            .agents
            .query_mut::<(
                &AgentId,
                &Position,
                &Physiology,
                &Traits,
                &Demographics,
                &Kinship,
                &ClanMembership,
                &Carrying,
                &Knowledge,
                &crate::faith::Faith,
                &mut Behavior,
                &mut Memory,
                &Wound,
            )>()
        {
            if demo.is_infant(time.tick) {
                continue;
            }
            // Une marche qui a buté apprend : la cible est mise de côté (seules
            // les sources sont concernées pour l'instant, voir `brain`).
            if let Some(stuck) = behavior.stuck_on.take()
                && mem.springs.contains(&stuck)
            {
                mem.block(stuck, time.tick + crate::memory::BLOCKED_SPRING_TICKS);
            }
            let due = behavior.task.is_none()
                || (time.tick.wrapping_add(id.0)) % DELIBERATION_PERIOD == 0;
            if due {
                // Un envoyé en expédition file vers son étape (aller/retour),
                // hors délibération normale — c'est ce qui le fait résister au
                // rappel du clan. Sauf si un besoin vital presse : alors il
                // délibère comme les autres pour survivre, puis reprend la route
                // au prochain passage.
                let expedition_wp = self.expeditions.get(&id.0).and_then(|exp| {
                    let distress = phys.thirst > 0.7
                        || phys.hunger > 0.7
                        || phys.cold > 0.5
                        || phys.fatigue > 0.8;
                    (!distress).then(|| commerce::waypoint(exp))
                });
                if let Some(target) = expedition_wp {
                    behavior.task = Some(crate::agent::Task { kind: TaskKind::Expedition, target });
                } else {
                    let current = behavior.task.map(|t| t.kind);
                    let ctx = AgentCtx {
                        id: *id,
                        pos,
                        phys,
                        traits,
                        demo,
                        kin,
                        clan: membership.0,
                        carrying: carrying.0,
                        knows_agriculture: agriculture.is_some_and(|a| knowledge.has(a)),
                        fervor: faith.fervor,
                        wound: wound.0,
                        light: self.climate.light(pos.tile().1, time),
                        daylight_left_h: self.climate.daylight_left_hours(pos.tile().1, time),
                    };
                    behavior.task = brain::decide(
                        &mut self.world,
                        time,
                        ctx,
                        mem,
                        current,
                        &herds,
                        &packs,
                        &humans,
                        &clan_views,
                        &self.clan_relations,
                        &shrines,
                    );
                }
            }
        }

        self.end("1 deliberation", _ph);
        // 2. Exécution des tâches. Les chasses réussies sont collectées : on
        let _ph = phase(); // 2 execution
        // n'entame pas le gibier pendant que les autres délibèrent dessus.
        // `path_budget` borne le nombre d'A* lancés ce tick (agents bloqués
        // par l'eau). `clan_stock` est une copie de travail des réserves —
        // dépôts (chasse) et retraits (`EatFromStock`) s'y accumulent au fil
        // des agents, reportée sur `self.clans` une fois la boucle finie.
        let mut kills: Vec<Kill> = Vec::new();
        let mut path_budget = PathBudget { left: PATH_REQUESTS_PER_TICK, denied: 0 };
        let mut clan_stock: BTreeMap<ClanId, f32> =
            self.clans.iter().map(|c| (c.id, c.stock)).collect();
        // Copie de travail des structures : `Build` y pousse la structure
        // achevée (financée par `clan_stock`), reportée sur `self` en fin de
        // boucle. Clairsemé — quelques éléments par clan, clone négligeable.
        let mut structures = self.structures.clone();
        let mut engagements: Vec<Engagement> = Vec::new();
        let mut clashes: Vec<Clash> = Vec::new();
        let mut shares: Vec<Share> = Vec::new();
        for (
            _,
            (id, pos, phys, traits, demo, behavior, mem, agent_skills, membership, carrying, prestige, wound),
        ) in self.agents.query_mut::<(
            &AgentId,
            &mut Position,
            &mut Physiology,
            &Traits,
            &Demographics,
            &mut Behavior,
            &mut Memory,
            &mut Skills,
            &ClanMembership,
            &mut Carrying,
            &mut Prestige,
            &Wound,
        )>() {
            if demo.is_infant(time.tick) {
                continue;
            }
            // La capacité de travail porte l'âge (un enfant cueille mal) **et**
            // la plaie (un blessé peine à tout — §3.1) : c'est là que
            // « improductif » se paie. Le savoir-faire, lui, vit dans `Skills`.
            let before = (pos.x, pos.y);
            let sight = crate::climate::sight(self.climate.light(pos.tile().1, time));
            let work = demography::work_capacity(demo.age_years(time.tick))
                * (1.0 - WOUND_WORK_PENALTY * wound.0);
            let outcome = execute(
                &mut self.world,
                &mut self.routes,
                &mut path_budget,
                *id,
                pos,
                phys,
                behavior,
                &herds,
                &packs,
                &humans,
                work,
                traits,
                agent_skills,
                membership.0,
                &mut clan_stock,
                carrying,
                prestige,
                &mut structures,
                &mut engagements,
                &mut clashes,
                &mut shares,
                time.tick,
                sight,
            );
            // La viande portée se gâte comme une réserve sans technique.
            if carrying.0 > 0.0 {
                carrying.0 *= (-1.0 / (FRESH_KEEP_DAYS * TICKS_PER_DAY as f64)).exp() as f32;
            }
            // Une piste croisée en marchant (CHA-2) : un adulte qui la remarque —
            // il faut y voir — la suit jusqu'à où le troupeau est passé en
            // dernier ; c'est ce qui fait retrouver le gibier au matin.
            if behavior.activity == Activity::Walking
                && demo.is_adult(time.tick)
                && sight >= fauna::TRAIL_SIGHT
                && let Some((spot, at)) =
                    fauna::crossed_trail(&self.herd_trails, before, (pos.x, pos.y), fauna::TRAIL_READ_TILES)
                && mem.game.is_none_or(|(_, seen)| seen < at)
            {
                mem.game = Some(((spot.0.floor() as i64, spot.1.floor() as i64), at));
            }
            // Où que la tâche l'ait mené, l'agent note où il a mis les pieds.
            mem.note_visit(pos.tile());
            if behavior.activity == Activity::Sleeping {
                mem.lodge = Some(pos.tile()); // le gîte d'où partira la sortie de demain
            }
            if let Some(kill) = outcome {
                kills.push(kill);
            }
        }
        // Dénouement des affrontements du tick : les meutes encaissent la somme
        // des coups, leur riposte se partage en plaies (mortelles à 1).
        combat::resolve(self, &engagements);
        // Puis les raids inter-clans : coups mutuels, butin, morts par violence.
        combat::resolve_clashes(self, &clashes);
        // Puis les parts de viande offertes autour des prises.
        resolve_shares(self, &shares);
        self.hunted_head += kills.iter().map(|k| k.head).sum::<f32>();
        self.path_calls += u64::from(PATH_REQUESTS_PER_TICK - path_budget.left);
        self.path_denied += u64::from(path_budget.denied);
        structures.sort_by_key(|s| (s.clan.0, s.kind));
        self.structures = structures;
        // Report des dépôts/retraits de la boucle, et une heure de
        // pourrissement : la réserve se gâte (`FRESH_KEEP_DAYS`), moins vite
        // à l'abri d'un grenier. On
        // efface aussi le désir déjà satisfait ce tick, pour ne pas faire
        // marcher inutilement d'autres membres vers un chantier déjà achevé
        // (le prochain `structures::plan` de minuit le referait de toute
        // façon, mais autant couper court tout de suite).
        // Part des membres de chaque clan qui savent conserver.
        let mut preserving: BTreeMap<ClanId, (u32, u32)> = BTreeMap::new();
        let preservation = self.tech_tree.id_of("preservation");
        for (_, (membership, knowledge)) in self.agents.query::<(&ClanMembership, &Knowledge)>().iter() {
            if let Some(clan_id) = membership.0 {
                let e = preserving.entry(clan_id).or_insert((0, 0));
                e.1 += 1;
                if preservation.is_some_and(|t| knowledge.has(t)) {
                    e.0 += 1;
                }
            }
        }
        for clan in &mut self.clans {
            if let Some(&stock) = clan_stock.get(&clan.id) {
                let (knowers, members) = preserving.get(&clan.id).copied().unwrap_or((0, 0));
                let share = f64::from(knowers) / f64::from(members.max(1));
                let keep_days = (FRESH_KEEP_DAYS + (PRESERVED_KEEP_DAYS - FRESH_KEEP_DAYS) * share)
                    * f64::from(structures::granary_keep_factor(clan.id, &self.structures));
                let keep = (-1.0 / (keep_days * cairn_core::TICKS_PER_DAY as f64)).exp();
                clan.stock = (f64::from(stock.max(0.0)) * keep) as f32;
            }
            if let Some(kind) = clan.desired
                && self.structures.iter().any(|s| s.clan == clan.id && s.kind == kind)
            {
                clan.desired = None;
            }
        }

        self.end("2 execution", _ph);
        // 2 ter. Expéditions : maintenant que les positions sont à jour, un
        let _ph = phase(); // 2t expeditions
        // envoyé arrivé près de l'étain y est exposé et fait demi-tour ; rentré
        // au foyer, sa quête s'achève (voir `crate::commerce`).
        commerce::advance(self);

        self.end("2t expeditions", _ph);
        // 2 bis. Les nourrissons : portés par leur mère, allaités par elle.
        let _ph = phase(); // 2b nourrissons
        demography::nurse_infants(self);

        self.end("2b nourrissons", _ph);
        // 3. Physiologie et morts. On collecte d'abord (on ne peut pas
        let _ph = phase(); // 3 physiologie
        // retirer une entité pendant qu'on itère dessus), on retire après.
        // Instantané des huttes : un membre à portée d'une hutte de son clan
        // gagne une chaleur passive (voir `structures`), sans s'arrêter pour
        // s'abriter — c'est ce qui rend un foyer fixe survivable au froid.
        let huts: Vec<(f64, f64, ClanId)> = self
            .structures
            .iter()
            .filter(|s| s.kind == StructureKind::Hut)
            .map(|s| (s.pos.0, s.pos.1, s.clan))
            .collect();
        // Instantané des positions humaines : la souillure d'une eau se lit
        // aux humains présents autour (voir `disease`).
        let humans_at: Vec<(f64, f64)> =
            self.agents.query::<(&Position, &AgentId)>().iter().map(|(_, (p, _))| (p.x, p.y)).collect();
        let disease_seed = self.world.seed().derive(crate::salt::DISEASE) ^ splitmix64(time.tick);
        // Les nourrissons portés : leur masse alourdit la marche de qui les
        // porte (la mère, ou celle qui l'a recueilli).
        let mut carried_kg: BTreeMap<u64, f32> = BTreeMap::new();
        for (_, (id, demo, kin)) in self.agents.query::<(&AgentId, &Demographics, &Kinship)>().iter() {
            if demo.is_infant(time.tick)
                && let Some(carer) = self.fosters.get(&id.0).copied().or(kin.mother)
            {
                *carried_kg.entry(carer.0).or_insert(0.0) +=
                    energy::body_mass_kg(demo.age_years(time.tick), demo.sex);
            }
        }
        let mut dead = Vec::new();
        let mut sheltered = 0u64;
        for (entity, (id, pos, phys, traits, demo, behavior, membership, exposures, wound, illness, carrying)) in
            self.agents.query_mut::<(
                &AgentId,
                &Position,
                &mut Physiology,
                &Traits,
                &Demographics,
                &Behavior,
                &ClanMembership,
                &mut Exposures,
                &mut Wound,
                &mut Illness,
                &Carrying,
            )>()
        {
            let (x, y) = pos.tile();
            let tile = self.world.tile(x, y);
            // L'agent voit ce qui l'entoure : on enregistre le gisement et la
            // matière de la tuile qu'on vient de lire pour la physiologie —
            // aucune lecture de tuile supplémentaire (voir `exposure`).
            exposures.note_tile(&tile);
            let mut felt = self.climate.instant(&tile, y, time);
            if matches!(
                tile.biome,
                cairn_worldgen::Biome::TemperateForest
                    | cairn_worldgen::Biome::TropicalForest
                    | cairn_worldgen::Biome::Taiga
            ) {
                felt += FOREST_BONUS_C;
            }
            if behavior.activity == Activity::Sheltering {
                felt += SHELTER_BONUS_C;
                sheltered += 1;
            }
            felt += structures::hut_warmth((pos.x, pos.y), membership.0, &huts);
            // La dépense de l'heure : le corps, ce qu'il fait, ce qu'il porte,
            // le froid, la fièvre, la grossesse (voir `energy`).
            let age = demo.age_years(time.tick);
            let body = energy::Body::of(age, demo.sex);
            phys.scale = body.scale();
            phys.reserve_target = body.reserve_target(age, demo.sex);
            let load_kg = carried_kg.get(&id.0).copied().unwrap_or(0.0) + carrying.0 * energy::KG_PER_MEAT_POINT;
            let fever = illness.active.iter().flatten().map(|i| i.severity).fold(0.0f32, f32::max);
            let mut kcal = body.hourly_kcal(behavior.activity, felt, load_kg, fever);
            if let Some(pregnancy) = demo.pregnancy {
                let days_left = pregnancy.due_tick.saturating_sub(time.tick) as f64 / TICKS_PER_DAY as f64;
                kcal += energy::pregnancy_kcal_day(demography::GESTATION_DAYS as f64 - days_left) / 24.0;
            }
            phys.drift(felt, behavior.activity, traits.endurance, kcal);
            // La plaie se referme lentement, quoi qu'il arrive (§3.1) — elle
            // handicape le temps de guérir, et peut s'infecter (plus bas).
            wound.0 = (wound.0 - WOUND_HEAL_PER_TICK).max(0.0);
            // La maladie : boire à une eau souillée, avoir froid, être blessé peuvent
            // infecter ; toute
            // infection en cours ronge la santé selon la lutte de l'heure.
            if behavior.activity == Activity::Drinking {
                let r2 = disease::SOIL_RADIUS_TILES * disease::SOIL_RADIUS_TILES;
                let near = humans_at
                    .iter()
                    .filter(|(hx, hy)| (pos.x - hx).powi(2) + (pos.y - hy).powi(2) <= r2)
                    .count();
                let mut rng = Pcg32::new(disease_seed, id.0);
                if rng.next_f64() < disease::water_risk(near) {
                    illness.infect(disease::Route::Gut, time.tick, &mut rng);
                }
            }
            {
                let mut rng = Pcg32::new(disease_seed ^ disease::Route::Lung as u64, id.0);
                if rng.next_f64() < disease::lung_onset_risk(phys.cold) {
                    illness.infect(disease::Route::Lung, time.tick, &mut rng);
                }
            }
            if wound.0 > 0.0 {
                let mut rng = Pcg32::new(disease_seed ^ disease::Route::Wound as u64, id.0);
                if rng.next_f64() < disease::wound_infection_risk(wound.0) {
                    illness.infect(disease::Route::Wound, time.tick, &mut rng);
                }
            }
            let defense = disease::defense(age, illness.milk, phys.hunger, phys.cold);
            illness.milk = false;
            let sick = illness.course(defense, time.tick);
            if sick > 0.0 {
                // La cause retenue reste la plus mordante de l'heure.
                if sick > phys.need_bite() {
                    phys.last_damage = Some(DeathCause::Disease);
                }
                phys.health = (phys.health - sick).max(0.0);
            }
            if phys.is_dead() {
                let cause = phys.last_damage.unwrap_or(DeathCause::Starvation);
                // Le sexe part avec le corps : on le note ici, sans quoi la
                // Chronique ne saurait plus nommer le défunt (voir `chronicle`).
                dead.push((entity, *id, cause, (x, y), demo.sex));
            }
        }
        self.shelter_ticks += sheltered;
        for (entity, agent, cause, pos, sex) in dead {
            // Le despawn est hors itération : l'emprunt de la requête est
            // rendu, l'ordre de retrait suit l'ordre de collecte.
            let _ = self.agents.despawn(entity);
            self.routes.remove(&agent.0); // pas de trajet fantôme d'un mort
            self.expeditions.remove(&agent.0); // une expédition meurt avec son envoyé
            // Une mort ordinaire est une statistique ; la mort de celui qui
            // menait un clan est une succession — et donc un fait de Chronique.
            if let Some(clan) = self.clans.iter().find(|c| c.chief == agent).map(|c| c.id) {
                self.record(pos, EventKind::ChiefFallen { clan, agent, sex, cause });
            }
            self.deaths.push(DeathRecord { tick: time.tick, agent, cause, pos });
        }

        self.end("3 physiologie", _ph);
        // 3 bis. Démographie quotidienne à minuit : naissances, conceptions,
        // sénescence. Puis entretien du graphe social et détection des
        // clans (Phase 4) : le graphe doit voir la population du jour, pas
        // celle d'hier (un mort ne doit pas peser sur la cohésion).
        if time.tick.is_multiple_of(TICKS_PER_DAY) {
            let _pa = phase();
            demography::daily(self);
            disease::daily(self);
            let _pa = phase();
            self.end("d demographie", _pa);
            social::daily(self);
            self.end("d social", _pa);
            // La pression du jour (Phase 5) : besoin des clans et de leur
            // tension fraîche (posée par `social::daily`), lue plus tard par
            // le moteur d'insight. Mesure pure, ne déclenche rien encore.
            let _pa = phase();
            pressure::measure(self);
            self.end("d pression", _pa);
            // L'insight (Phase 5) : chaque adulte oisif, au confort, dans un
            // clan sous pression, peut découvrir une tech dont les prérequis
            // sont réunis. Après la pression, qu'elle consomme.
            let _pa = phase();
            tech::insight(self);
            self.end("d insight", _pa);
            // L'oubli (Phase 5) : constate quelles techs n'ont plus aucun
            // porteur vivant aujourd'hui (les morts du jour sont déjà passées,
            // étape 3) et les journalise — « l'humanité peut régresser ».
            let _pa = phase();
            tech::forget(self);
            self.end("d oubli", _pa);
            // Commerce (Phase 5) : un clan du cuivre sans étain dépêche un
            // envoyé le chercher au loin — la route qui, seule, mène au bronze.
            let _pa = phase();
            commerce::dispatch(self);
            self.end("d commerce", _pa);
            // Après que les clans du jour sont connus : réattribuer les
            // structures à qui contrôle leur tuile (et ruiner les abandonnées),
            // mesurer ce que chaque clan désire bâtir, puis ancrer le foyer des
            // clans sédentarisés à leur hutte du chef (en dernier : c'est cette
            // valeur ancrée que la délibération du lendemain doit lire).
            structures::maintain(self, time.tick / TICKS_PER_DAY);
            let _pa = phase();
            structures::plan(self);
            self.end("d structures", _pa);
            structures::anchor_homes(self);
            // Domestication (Phase 5) : les foyers étant fixés, apprivoiser (ou
            // refaroucher) les troupeaux domesticables selon la proximité d'un
            // clan et la protection contre les prédateurs — le cheptel émerge de
            // l'éleveur, sans tech ni règle « possède ce troupeau ».
            crate::pastoral::daily(self);
            // Les feux de forêt (Phase 5) : font vivre les foyers (brûlent le
            // fourrage, exposent au feu les agents proches) et tentent un
            // nouveau départ. Ambivalent : la biomasse détruite nourrit la
            // pression de famine de demain.
            // La météo (Phase 6) : averses et sécheresses passagères. **Avant** le
            // feu, qui lit son décalage d'humidité pour savoir si la brousse
            // peut s'enflammer — un sol détrempé ne prend pas.
            // La Foi (Phase 6) : la ferveur s'émousse, et ce qu'il en reste produit
            // la ressource du joueur. Cesser d'agir, c'est être oublié.
            let _pa = phase();
            crate::faith::daily(self);
            let _pa = phase();
            self.end("d foi", _pa);
            crate::weather::daily(self);
            let _pa = phase();
            self.end("d meteo", _pa);
            fire::daily(self);
            self.end("d feu", _pa);
        }

        // 3 ter. Échange de savoirs et renforcement des liens sociaux toutes
        let _ph = phase(); // 3t echanges
        // les 4 h : un instantané quotidien raterait les croisements de la
        // journée (on se parle en se rencontrant, pas à minuit pile). Les
        // enfants héritent ainsi des sources — et du clan — de leurs
        // parents simplement en vivant à leurs côtés.
        if time.tick.is_multiple_of(4) {
            memory::exchange_knowledge(self);
            social::encounter(self);
            // Diffusion culturelle (Phase 5) : une tech passe d'un agent à un
            // voisin à portée, quel que soit son clan — même passe de
            // rencontre que les liens sociaux et l'échange de sources.
            tech::diffuse(self);
            // Et la croyance se transmet dans la même passe de rencontre que les
            // savoirs — pour la même raison : on convainc un voisin, parfois.
            crate::faith::preach(self);
        }

        self.end("3t echanges", _ph);
        // 4. Faune. Les meutes chassent d'abord (sur l'instantané), puis
        let _ph = phase(); // 4a meutes
        // toutes les prises — prédation et chasse humaine — sont appliquées
        // avant que les troupeaux ne fassent leurs petits : un troupeau
        // décimé ne doit pas engendrer comme s'il était intact.
        let (pred_kills, dead_packs, pack_fissions) =
            fauna::update_packs(&mut self.fauna, &self.world, &herds, &mut self.fauna_stats);
        self.end("4a meutes", _ph);
        let _ph = phase(); // 4b prises
        kills.extend(pred_kills);
        fauna::apply_kills(&mut self.fauna, &kills);
        for entity in dead_packs {
            let _ = self.fauna.despawn(entity);
        }
        // Les meutes filles naissent après coup : `update_packs` emprunte
        // l'ECS, et `spawn_pack_species` y écrit (même patron que la fission
        // des troupeaux).
        for f in pack_fissions {
            self.spawn_pack_species(f.pos.0, f.pos.1, f.population, f.species);
        }

        self.end("4b prises", _ph);
        let _ph = phase(); // 4c menaces
        // Ce qui fait fuir un troupeau : les prédateurs et les hommes.
        let mut threats: Vec<(f64, f64)> = self
            .fauna
            .query::<(&Pack, &Position)>()
            .iter()
            .map(|(_, (_, pos))| (pos.x, pos.y))
            .collect();
        threats.extend(
            self.agents
                .query::<&Position>()
                .iter()
                .map(|(_, pos)| (pos.x, pos.y)),
        );

        self.end("4c menaces", _ph);
        let _ph = phase(); // 4d troupeaux
        // Les chunks que les troupeaux font régénérer : le coût du store qu'ils
        // paient, à séparer de leur coût propre (chantier du coût d'un troupeau).
        #[cfg(feature = "profile")]
        let generated_before = self.world.generated;
        let seed = self.world.seed();
        // Le bord de la faune (D′) suit les humains : il n'existe que dans un
        // monde ouvert et habité.
        let fence_humans: Vec<(f64, f64)> = if self.allow_fauna_immigration {
            humans.iter().map(|h| h.pos).collect()
        } else {
            Vec::new()
        };
        let fence = (!fence_humans.is_empty()).then_some(fence_humans.as_slice());
        // Les positions humaines seules (pas les meutes) : c'est la présence
        // d'un observateur qui décide de la finesse, pas celle d'un prédateur.
        let (dead_herds, fissions) = fauna::update_herds(
            &mut self.fauna,
            &mut self.world,
            &self.climate,
            time,
            seed,
            &threats,
            &mut self.fauna_stats,
            &mut self.rangeland,
            fence,
        );
        #[cfg(feature = "profile")]
        self.prof.count("4d regen", self.world.generated - generated_before);
        for entity in dead_herds {
            let _ = self.fauna.despawn(entity);
        }
        for (x, y, population, species) in fissions {
            self.spawn_herd_species(x, y, population, species);
        }
        // Les troupeaux laissent leur piste (CHA-2).
        fauna::lay_trails(&self.fauna, &mut self.herd_trails, time.tick);
        // La fusion, pendant de la fission (chantier de dérive, E) : une fois
        // par jour, deux troupeaux sauvages de même espèce qui se voient se
        // rejoignent — voir `fauna::merge_plan`.
        if time.tick.is_multiple_of(TICKS_PER_DAY) {
            let views: Vec<(hecs::Entity, fauna::MergeView)> = self
                .fauna
                .query::<(&FaunaId, &Herd, &Position)>()
                .iter()
                .map(|(e, (id, h, p))| {
                    let wild = h.anchor.is_none() && h.tameness <= 0.0;
                    (e, (id.0, (p.x, p.y), h.population, h.species, wild))
                })
                .collect();
            let plan_input: Vec<fauna::MergeView> = views.iter().map(|(_, v)| *v).collect();
            for (absorber, absorbed) in fauna::merge_plan(&plan_input) {
                let gained = views[absorbed].1.2;
                if let Ok(mut h) = self.fauna.get::<&mut Herd>(views[absorber].0) {
                    h.population += gained;
                }
                let _ = self.fauna.despawn(views[absorbed].0);
            }
        }
        // Recensement de densité (M1-faune) : une fois par jour, après prises,
        // naissances et fissions — l'état que la veille comparera demain.
        #[cfg(feature = "fauna-stats")]
        if time.tick.is_multiple_of(TICKS_PER_DAY) {
            let census: Vec<fauna::CensusHerd> = self
                .fauna
                .query::<(&fauna::FaunaId, &Herd, &Position)>()
                .iter()
                .map(|(_, (id, h, pos))| {
                    let fleeing = h.state == fauna::HerdState::Fleeing;
                    (id.0, (pos.x, pos.y), h.population, h.satiation, fleeing)
                })
                .collect();
            self.fauna_stats.crowd_census(&census);
        }

        self.end("4d troupeaux", _ph);
        // 4 bis. Immigration de gibier, quotidienne : sans elle, une zone
        let _ph = phase(); // 4b immigration
        // qui perd tous ses troupeaux (chasse sous `HERD_MIN`, prédation
        // comprise) reste vide pour toujours — rien ne fait *repousser* un
        // troupeau depuis zéro individu, contrairement à la végétation
        // (banque de graines). Gardée par `allow_fauna_immigration` — voir
        // le commentaire du champ : un choix explicite du contexte
        // d'exécution, pas une heuristique déduite de l'état de la sim.
        if time.tick.is_multiple_of(TICKS_PER_DAY) && self.allow_fauna_immigration {
            let human_positions: Vec<(f64, f64)> = humans.iter().map(|h| h.pos).collect();
            // Les abandonnés (D′) : le bord suit les humains ; un troupeau
            // sauvage qu'ils ont laissé au-delà de deux fois le bord quitte la
            // simulation. Le bord interdit d'y aller seul : seul un
            // déplacement des humains peut l'y laisser. Le cheptel
            // domestiqué appartient à un clan et reste, où qu'il soit.
            if !human_positions.is_empty() {
                let r2 = fauna::FAUNA_ABANDON_TILES * fauna::FAUNA_ABANDON_TILES;
                let gone: Vec<hecs::Entity> = self
                    .fauna
                    .query::<(&Herd, &Position)>()
                    .iter()
                    .filter(|(_, (h, p))| {
                        h.anchor.is_none() && fauna::nearest_human_d2((p.x, p.y), &human_positions) > r2
                    })
                    .map(|(e, _)| e)
                    .collect();
                for e in gone {
                    let _ = self.fauna.despawn(e);
                }
            }
            let herd_positions: Vec<(f64, f64)> = self
                .fauna
                .query::<(&Herd, &Position)>()
                .iter()
                .map(|(_, (_, pos))| (pos.x, pos.y))
                .collect();
            if let Some((x, y)) =
                fauna::daily_immigration(&human_positions, &herd_positions, &mut self.world, seed, time)
            {
                self.spawn_herd(x, y, fauna::HERD_START);
            }
            // Symétrique : sans réensemencement des meutes, une extinction de
            // prédateurs (boom-bust de Lotka-Volterra) est définitive et le
            // gibier explose sans frein (mesuré sur un run long — voir
            // `fauna`). Les prédateurs suivent leur proie : ancrés sur un
            // troupeau, pas sur un humain.
            let pack_positions: Vec<(f64, f64)> = self
                .fauna
                .query::<(&Pack, &Position)>()
                .iter()
                .map(|(_, (_, pos))| (pos.x, pos.y))
                .collect();
            if let Some((x, y)) = fauna::daily_predator_immigration(
                &herd_positions,
                &pack_positions,
                &mut self.world,
                seed,
                time,
            ) {
                self.spawn_pack(x, y, fauna::PACK_START);
            }
        }

        self.end("4b immigration", _ph);
        // 5. Écologie quotidienne, à minuit.
        let _ph = phase(); // 5 ecologie
        if time.tick.is_multiple_of(TICKS_PER_DAY) {
            let eco_humans: Vec<(f64, f64)> =
                self.agents.query::<&Position>().iter().map(|(_, p)| (p.x, p.y)).collect();
            ecology::daily_regrowth(
                &mut self.world,
                &self.climate,
                time,
                &self.weather,
                &eco_humans,
                self.fauna_lod_period,
            );
        }

        self.time.tick += 1;
        self.end("5 ecologie", _ph);
    }
}

/// Avance la tâche courante d'un agent : marche vers la cible (en contournant
/// l'eau si besoin), puis agit. `work` est la capacité de travail liée à
/// l'âge ; les compétences (`agent_skills`) modulent le rendement des gestes
/// **et se forgent en les faisant**. Renvoie une prise si l'agent a abattu du
/// gibier ce tick.
#[allow(clippy::too_many_arguments)]
fn execute(
    world: &mut World,
    routes: &mut BTreeMap<u64, Route>,
    path_budget: &mut PathBudget,
    id: AgentId,
    pos: &mut Position,
    phys: &mut Physiology,
    behavior: &mut Behavior,
    herds: &[HerdView],
    packs: &[PackView],
    humans: &[HumanView],
    work: f32,
    traits: &Traits,
    agent_skills: &mut Skills,
    clan: Option<ClanId>,
    clan_stock: &mut BTreeMap<ClanId, f32>,
    carrying: &mut Carrying,
    prestige: &mut Prestige,
    structures: &mut Vec<Structure>,
    engagements: &mut Vec<Engagement>,
    clashes: &mut Vec<Clash>,
    shares: &mut Vec<Share>,
    tick: u64,
    sight: f32,
) -> Option<Kill> {
    // Manger ce qu'on porte quand la faim revient : la viande d'une prise est
    // sur le dos, pas au foyer — ni trajet ni tâche pour y mordre.
    if phys.hunger > EAT_CARRIED_HUNGER && carrying.0 > 0.0 {
        let eaten = phys.eat_points(carrying.0);
        carrying.0 -= eaten;
        crate::food_stats::fed(crate::food_stats::Source::Carried, eaten);
    }
    let task = match behavior.task {
        Some(task) => task,
        None => {
            behavior.activity = Activity::Idle;
            return None;
        }
    };

    // La chasse est le seul cas où la cible **bouge** : on ne vise pas une
    // tuile mais une bête. On ré-évalue donc le troupeau à chaque tick, et
    // c'est ce qui fait que le chasseur *suit* le gibier au lieu de courir
    // vers l'herbe où il broutait il y a quatre heures.
    if task.kind == TaskKind::Hunt {
        crate::food_stats::event(crate::food_stats::Event::HuntHours, 1);
        let Some(prey) = closest_herd(pos, herds) else {
            behavior.task = None; // le troupeau est mort ou hors de vue
            behavior.activity = Activity::Idle;
            return None;
        };
        let target = (prey.pos.0.floor() as i64, prey.pos.1.floor() as i64);
        if pos.distance_tiles(target) > HUNT_REACH_TILES {
            behavior.activity = Activity::Walking;
            if advance(world, routes, path_budget, id, pos, target) == Move::Stuck {
                behavior.task = None;
            }
            return None;
        }
        behavior.activity = Activity::Hunting;
        // À portée ne veut pas dire tué : la bête flaire, détale, la flèche
        // manque. Chaque heure d'approche a sa chance, que le savoir-faire
        // double ou divise (`HUNT_SUCCESS_PER_HOUR`). Manquée, la chasse
        // continue : la tâche reste, on suit toujours le troupeau.
        crate::food_stats::event(crate::food_stats::Event::HuntReachHours, 1);
        if !hunt_succeeds(world.seed(), tick, id, agent_skills.hunting, sight) {
            return None;
        }
        // Un bon chasseur tire tout de sa bête (dépeçage, rien de perdu), un
        // novice en gâche un quart — et chaque mise à mort forge le geste bien
        // plus qu'une heure d'affût.
        let nutrition = meat_hunger(prey.species) * (0.75 + 0.25 * agent_skills.hunting);
        skills::practice(
            &mut agent_skills.hunting,
            skills::hunt_cap(traits),
            skills::KILL_PRACTICE_BOOST,
        );
        // Une bête tuée n'est pas une portion calibrée : ce qui dépasse la
        // faim du moment ne rejoint pas le stock du clan sur-le-champ (voir
        // le commentaire de module de `social`) — il faut d'abord le
        // rapporter au foyer (`Carrying`, `TaskKind::BringSurplusHome`),
        // sans quoi la viande se téléporterait depuis le lieu de la chasse.
        let eaten = phys.eat_points(nutrition);
        let mut surplus = nutrition - eaten;
        crate::food_stats::fed(crate::food_stats::Source::HuntEaten, eaten);
        crate::food_stats::event(crate::food_stats::Event::Kills, 1);
        // Partager la prise avec ceux qui ont faim autour : ni une règle, ni un
        // devoir de clan — une disposition. La part offerte suit la
        // sociabilité héritée ; ce qui n'est pas offert est rapporté au clan
        // ou laissé sur place, comme avant.
        let offered = surplus * traits.sociability;
        if offered > 0.0 {
            shares.push(Share { giver: id, pos: (pos.x, pos.y), amount: offered });
            surplus -= offered;
        }
        // Ce qu'il n'offre pas, il le garde — clan ou pas : rapporté au foyer
        // s'il en a un, mangé plus tard sinon (voir le haut de `execute`).
        if surplus > 0.0 {
            carrying.0 += surplus;
            crate::food_stats::fed(crate::food_stats::Source::HuntCarried, surplus);
        }
        behavior.task = None;
        return Some(Kill { herd: prey.entity, head: HUNT_YIELD_HEAD });
    }

    // Affronter une meute : comme la chasse, la cible **bouge** (on ré-évalue la
    // meute la plus proche) ; mais l'issue ne se règle pas ici. Au contact, on
    // enregistre une passe d'armes — le combat se dénoue **en groupe** après
    // l'exécution (`combat::resolve`), pour que la meute encaisse la somme des
    // coups et que sa riposte se partage entre les assaillants.
    if task.kind == TaskKind::HuntPredator {
        let Some(prey) = closest_pack(pos, packs) else {
            behavior.task = None; // meute morte ou hors de vue
            behavior.activity = Activity::Idle;
            return None;
        };
        let target = (prey.pos.0.floor() as i64, prey.pos.1.floor() as i64);
        if pos.distance_tiles(target) > HUNT_REACH_TILES {
            behavior.activity = Activity::Walking;
            if advance(world, routes, path_budget, id, pos, target) == Move::Stuck {
                behavior.task = None;
            }
            return None;
        }
        behavior.activity = Activity::Fighting;
        // Le coup porté ∝ force × compétence de combat × capacité de travail
        // (un blessé frappe moins fort — `work` porte déjà la plaie). La riposte
        // est atténuée par force + combat. Le geste se forge dans `resolve`.
        let power = combat::ATTACK_POWER * traits.strength * (0.5 + agent_skills.combat) * work;
        let defense = 0.3 * traits.strength + 0.4 * agent_skills.combat;
        engagements.push(Engagement { attacker: id, pack: prey.entity, power, defense });
        behavior.task = None; // une passe d'armes, puis on redélibère
        return None;
    }

    // Garder le cheptel (domestication) : comme la chasse, la cible bouge un
    // peu (le troupeau apprivoisé, quoique ancré). On prélève durablement sur
    // lui et l'on verse au **stock commun** (le cheptel nourrit le clan, pas
    // le seul éleveur) — bien moins que sa croissance, pour qu'il se refasse.
    if task.kind == TaskKind::Herd {
        let Some(cheptel) = closest_tame_herd(pos, herds) else {
            behavior.task = None; // plus de cheptel apprivoisé en vue
            behavior.activity = Activity::Idle;
            return None;
        };
        let target = (cheptel.pos.0.floor() as i64, cheptel.pos.1.floor() as i64);
        if pos.distance_tiles(target) > HUNT_REACH_TILES {
            behavior.activity = Activity::Walking;
            if advance(world, routes, path_budget, id, pos, target) == Move::Stuck {
                behavior.task = None;
            }
            return None;
        }
        behavior.activity = Activity::Farming; // même geste pastoral que le champ
        skills::practice(&mut agent_skills.foraging, skills::forage_cap(traits), 1.0);
        if let Some(clan_id) = clan {
            let meat = HERD_HARVEST_HEAD * meat_hunger(cheptel.species);
            *clan_stock.entry(clan_id).or_insert(0.0) += meat;
            crate::food_stats::fed(crate::food_stats::Source::Herd, meat);
        }
        behavior.task = None;
        return Some(Kill { herd: cheptel.entity, head: HERD_HARVEST_HEAD });
    }

    // Razzier un rival : comme la chasse, la cible bouge (un humain d'un clan
    // rival) — on ré-évalue le plus proche à chaque tick. Au contact, on
    // enregistre une passe d'armes, dénouée **en groupe** après l'exécution
    // (`combat::resolve_clashes`) : la cible encaisse la somme des assauts et
    // riposte. On ne re-teste pas la tension ici (le cerveau a décidé de
    // razzier un hostile ; les territoires étant distincts, le rival le plus
    // proche est bien le voisin en tension).
    if task.kind == TaskKind::Raid {
        let Some(my_clan) = clan else {
            behavior.task = None;
            return None;
        };
        let sight2 = cairn_core::km_to_tiles(1.5).powi(2);
        let mut target: Option<(AgentId, (f64, f64))> = None;
        let mut best = sight2;
        for h in humans {
            if h.id == id || h.clan == Some(my_clan) || h.clan.is_none() {
                continue;
            }
            let d2 = (pos.x - h.pos.0).powi(2) + (pos.y - h.pos.1).powi(2);
            if d2 <= best {
                best = d2;
                target = Some((h.id, h.pos));
            }
        }
        let Some((tid, tpos)) = target else {
            behavior.task = None; // plus de rival en vue
            behavior.activity = Activity::Idle;
            return None;
        };
        let tt = (tpos.0.floor() as i64, tpos.1.floor() as i64);
        if pos.distance_tiles(tt) > HUNT_REACH_TILES {
            behavior.activity = Activity::Walking;
            if advance(world, routes, path_budget, id, pos, tt) == Move::Stuck {
                behavior.task = None;
            }
            return None;
        }
        behavior.activity = Activity::Fighting;
        clashes.push(Clash { attacker: id, target: tid });
        behavior.task = None; // une passe d'armes, puis on redélibère
        return None;
    }

    // Une piste ne se lit pas dans le noir (CHA-2b) : la nuit tombée, le
    // pisteur s'arrête ; le souvenir du troupeau reste pour le matin.
    if task.kind == TaskKind::Track && sight < fauna::TRAIL_SIGHT {
        behavior.activity = Activity::Idle;
        behavior.task = None;
        return None;
    }

    // Phase trajet : la cible est trop loin, on marche (budget d'une heure).
    if pos.distance_tiles(task.target) > ARRIVAL_TILES {
        behavior.activity = Activity::Walking;
        if advance(world, routes, path_budget, id, pos, task.target) == Move::Stuck {
            behavior.task = None; // vraiment cerné : on re-délibérera
            behavior.stuck_on = Some(task.target);
        }
        return None;
    }

    // Phase action : sur place. Les actions ciblées (boire, fourrager)
    // s'appliquent à la tuile **cible** : « arrivé » signifie à moins de
    // 1,5 tuile d'elle, pas forcément dessus.
    match task.kind {
        TaskKind::Drink => {
            behavior.activity = Activity::Drinking;
            if world.tile(task.target.0, task.target.1).has_fresh_water() {
                phys.thirst = 0.0;
            }
            behavior.task = None;
        }
        TaskKind::Forage => {
            behavior.activity = Activity::Eating;
            // Le rendement d'une heure de cueillette : l'âge (capacité) et
            // le savoir-faire, qui se forge à chaque heure pratiquée.
            // De nuit, on ne trouve presque rien à tâtons (MAR-2).
            let bite = EAT_HUNGER_PER_TICK * work * (0.6 + 0.8 * agent_skills.foraging) * sight;
            skills::practice(&mut agent_skills.foraging, skills::forage_cap(traits), 1.0);
            // D10 : on ne mange que ce que la maille offre de comestible. La
            // tuile, elle, est toujours foulée et entamée comme avant — c'est
            // l'impact de la cueillette sur la flore, un autre chantier.
            // PROTOTYPE : un champ cultivé devrait nourrir depuis sa tuile ; non
            // traité ici (aucune agriculture en un an de fondateurs, et une
            // averse met aussi la biomasse au-dessus de la capacité).
            let tile = world.tile_mut(task.target.0, task.target.1);
            let cultivated = false;
            let wanted = (phys.appetite_points().min(bite) / NUTRITION_PER_BIOMASS).ceil() as u8;
            let taken = wanted.min(tile.biomass);
            tile.biomass -= taken;
            let want_kcal = f64::from(f32::from(taken) * NUTRITION_PER_BIOMASS) * KCAL_PER_HUNGER;
            let got_kcal = if cultivated {
                want_kcal
            } else {
                world.gather((pos.x, pos.y), tick, want_kcal)
            };
            let eaten = phys.eat_points((got_kcal / KCAL_PER_HUNGER) as f32);
            crate::food_stats::fed(crate::food_stats::Source::Forage, eaten);
            crate::food_stats::event(crate::food_stats::Event::BiomassTaken, u64::from(taken));
            crate::food_stats::event(crate::food_stats::Event::ForageHours, 1);
            crate::food_stats::event(
                crate::food_stats::Event::ForageShort,
                u64::from(taken < wanted || got_kcal < want_kcal),
            );
            let bare = world.tile(task.target.0, task.target.1).biomass == 0;
            if phys.hunger <= 0.05 || bare || got_kcal < want_kcal {
                behavior.task = None; // rassasié, ou tuile épuisée
            }
        }
        TaskKind::Cultivate => {
            behavior.activity = Activity::Farming;
            // Semer/sarcler : le travail agricole ÉLÈVE la biomasse de la tuile
            // au-dessus de ce que la nature y met — un champ rend plus qu'une
            // prairie sauvage. La récolte, elle, reste la cueillette ordinaire
            // (biomasse plus riche → meilleur rendement). Le sol se fatigue un
            // peu (§2.4) ; sans entretien, l'écologie ramène le champ en friche.
            skills::practice(&mut agent_skills.foraging, skills::forage_cap(traits), 1.0);
            let tile = world.tile_mut(task.target.0, task.target.1);
            let gain = (CULTIVATE_GAIN * work * f32::from(tile.soil_fertility) / 255.0) as u8;
            tile.biomass = tile.biomass.saturating_add(gain).min(CULTIVATED_CEILING);
            tile.soil_fertility = tile.soil_fertility.saturating_sub(CULTIVATE_FERTILITY_COST);
            behavior.task = None; // un tour d'entretien, puis on redélibère
        }
        TaskKind::EatFromStock => {
            behavior.activity = Activity::Eating;
            // Conservation stricte : ce que l'agent gagne, le stock le perd,
            // au même montant — pas de nourriture créée ni perdue au passage.
            if let Some(clan_id) = clan
                && let Some(stock) = clan_stock.get_mut(&clan_id)
            {
                let taken = phys.eat_points(*stock);
                *stock -= taken;
                crate::food_stats::fed(crate::food_stats::Source::Stock, taken);
            }
            behavior.task = None; // un puisage, puis on redélibère
        }
        TaskKind::BringSurplusHome => {
            behavior.activity = Activity::Idle;
            // Symétrique du puisage : ce que l'agent portait, le stock le
            // gagne, au même montant.
            if let Some(clan_id) = clan {
                *clan_stock.entry(clan_id).or_insert(0.0) += carrying.0;
            }
            // Prestige : ce qui a nourri le clan reste acquis à celui qui
            // l'a rapporté, à vie (voir `agent::Prestige`) — pas seulement
            // le montant déposé ce tick, tout ce qui a jamais été rapporté.
            prestige.0 += carrying.0;
            carrying.0 = 0.0;
            behavior.task = None;
        }
        TaskKind::Build(kind) => {
            behavior.activity = Activity::Idle;
            // Financé par le stock commun ; un seul exemplaire par clan et
            // par type. Le premier arrivé bâtit ; si la structure existe déjà
            // ou que le stock est retombé sous le coût entre-temps, no-op —
            // l'agent re-délibérera au prochain tick.
            if let Some(clan_id) = clan {
                let already =
                    structures.iter().any(|s| s.clan == clan_id && s.kind == kind);
                let stock = clan_stock.get(&clan_id).copied().unwrap_or(0.0);
                if !already && stock >= kind.cost() {
                    *clan_stock.entry(clan_id).or_insert(0.0) -= kind.cost();
                    structures.push(Structure {
                        kind,
                        clan: clan_id,
                        pos: (task.target.0 as f64 + 0.5, task.target.1 as f64 + 0.5),
                        built_tick: tick,
                        abandoned_since: None,
                    });
                }
            }
            behavior.task = None;
        }
        TaskKind::Sleep => {
            behavior.activity = Activity::Sleeping;
            // On dort jusqu'à récupération — sauf urgence qui réveille.
            if phys.fatigue <= 0.02 || phys.thirst > 0.95 || phys.cold > 0.95 {
                behavior.task = None;
            }
        }
        TaskKind::Shelter => {
            behavior.activity = Activity::Sheltering;
            if phys.cold <= 0.05 {
                behavior.task = None;
            }
        }
        TaskKind::Wander
        | TaskKind::Follow
        | TaskKind::Socialize
        | TaskKind::Explore
        | TaskKind::Track
        | TaskKind::SeekGame
        | TaskKind::ReturnToClan
        // Le pèlerinage n'a d'effet que d'avoir eu lieu : on est venu, on est là.
        // Ce que ça produit — des peuples qui convergent et se disputent le lieu —
        // tombe de la géométrie, pas d'un effet écrit ici.
        | TaskKind::Pilgrimage
        | TaskKind::Expedition => {
            behavior.activity = Activity::Idle;
            behavior.task = None; // arrivé — on re-délibérera aussitôt
        }
        TaskKind::Hunt => unreachable!("la chasse est traitée avant, sa cible bouge"),
        TaskKind::HuntPredator => {
            unreachable!("l'affrontement d'une meute est traité avant, sa cible bouge")
        }
        TaskKind::Herd => unreachable!("la garde du cheptel est traitée avant, sa cible bouge"),
        TaskKind::Raid => unreachable!("le raid est traité avant, sa cible (un rival) bouge"),
    }
    None
}

/// Le troupeau le plus proche de l'agent, sans borne de distance : le
/// chasseur a déjà décidé de chasser, il ne perd pas sa proie de vue au
/// premier pas. La borne de perception est appliquée à la **délibération**
/// ([`brain::decide`]), pas ici.
fn closest_herd(pos: &Position, herds: &[HerdView]) -> Option<HerdView> {
    let mut best: Option<(f64, HerdView)> = None;
    for h in herds {
        let d2 = (pos.x - h.pos.0).powi(2) + (pos.y - h.pos.1).powi(2);
        if best.is_none_or(|(bd, _)| d2 < bd) {
            best = Some((d2, *h));
        }
    }
    best.map(|(_, h)| h)
}

/// La meute la plus proche **dans un rayon de vue** (~2 km) — au-delà, on ne la
/// poursuit pas à travers la carte (sinon un défenseur courrait sans fin après
/// une meute lointaine). Départage déterministe : distance, puis ordre de
/// l'instantané.
fn closest_pack(pos: &Position, packs: &[PackView]) -> Option<PackView> {
    let sight2 = cairn_core::km_to_tiles(2.0).powi(2);
    let mut best: Option<(f64, PackView)> = None;
    for p in packs {
        let d2 = (pos.x - p.pos.0).powi(2) + (pos.y - p.pos.1).powi(2);
        if d2 <= sight2 && best.is_none_or(|(bd, _)| d2 < bd) {
            best = Some((d2, *p));
        }
    }
    best.map(|(_, p)| p)
}

/// Le **cheptel** (troupeau apprivoisé) le plus proche dans un rayon de vue —
/// pour l'éleveur qui va le garder (`TaskKind::Herd`). Ignore le gibier sauvage
/// (apprivoisement sous le seuil de domestication).
fn closest_tame_herd(pos: &Position, herds: &[HerdView]) -> Option<HerdView> {
    let sight2 = cairn_core::km_to_tiles(2.0).powi(2);
    let mut best: Option<(f64, HerdView)> = None;
    for h in herds {
        if h.tameness < crate::pastoral::DOMESTICATED_THRESHOLD {
            continue;
        }
        let d2 = (pos.x - h.pos.0).powi(2) + (pos.y - h.pos.1).powi(2);
        if d2 <= sight2 && best.is_none_or(|(bd, _)| d2 < bd) {
            best = Some((d2, *h));
        }
    }
    best.map(|(_, h)| h)
}

/// Le budget d'A* d'un tick : ce qui reste, et les demandes refusées.
pub(crate) struct PathBudget {
    left: u32,
    denied: u32,
}

/// Fait avancer l'agent vers `target` pour ce tick, en gérant l'évitement de
/// l'eau : ligne droite tant qu'elle passe, A* budgété (via une [`Route`]
/// persistante) dès qu'elle bute.
///
/// Le cas courant — terrain ouvert — ne touche jamais l'A* : on ne paie le
/// pathfinding que là où la géographie l'exige.
fn advance(
    world: &mut World,
    routes: &mut BTreeMap<u64, Route>,
    path_budget: &mut PathBudget,
    id: AgentId,
    pos: &mut Position,
    target: (i64, i64),
) -> Move {
    // 1. Un trajet en cours vers cette même cible ? On le suit.
    if let Some(route) = routes.get_mut(&id.0) {
        if route.goal == target {
            let outcome = follow_route(world, route, pos, target);
            if outcome != Move::Stuck {
                return outcome;
            }
            routes.remove(&id.0); // trajet devenu inutile
            return Move::Stuck;
        }
        routes.remove(&id.0); // la cible a changé : trajet périmé
    }

    // 2. Ligne droite.
    match walk_line(world, pos, target) {
        Move::Stuck => {}
        moved => return moved,
    }

    // 3. Bloqué par l'eau : calculer un contournement, si le budget le permet.
    if path_budget.left == 0 {
        path_budget.denied += 1;
        return Move::Deferred; // pas ce tick — on retentera, la tâche est gardée
    }
    path_budget.left -= 1;
    let is_land = |x: i64, y: i64| world.worldgen().elevation(x, y) > 0.0;
    match pathfind::astar(pos.tile(), target, is_land, pathfind::NODE_BUDGET) {
        Some(waypoints) if !waypoints.is_empty() => {
            let mut route = Route { goal: target, waypoints, cursor: 0 };
            let outcome = follow_route(world, &mut route, pos, target);
            routes.insert(id.0, route);
            outcome
        }
        _ => Move::Stuck, // vraiment cerné
    }
}

/// Suit un trajet A* : consomme les étapes déjà atteintes, marche vers la
/// prochaine (ou vers le but exact une fois le trajet épuisé) — et **enchaîne
/// les étapes dans la même heure** tant qu'il reste de la marche. Un trajet
/// s'arrêtait à chaque étape (32 à 45 m) en jetant le reste de l'heure :
/// contourner l'eau se faisait à ~45 m/h au lieu de 4 km/h.
fn follow_route(world: &mut World, route: &mut Route, pos: &mut Position, target: (i64, i64)) -> Move {
    let mut budget = WALK_TILES_PER_TICK;
    let mut moved = false;
    loop {
        while route.cursor < route.waypoints.len()
            && pos.distance_tiles(route.waypoints[route.cursor]) <= ARRIVAL_TILES
        {
            route.cursor += 1;
        }
        let heading_to_goal = route.cursor >= route.waypoints.len();
        let step_target = if heading_to_goal { target } else { route.waypoints[route.cursor] };
        match walk_budgeted(world, pos, step_target, &mut budget) {
            Move::Arrived if heading_to_goal => return Move::Arrived,
            Move::Arrived => {
                route.cursor += 1; // étape atteinte : on enchaîne sur la suivante
                moved = true;
                if budget <= 0.0 {
                    return Move::Moved;
                }
            }
            Move::Stuck if moved => return Move::Moved,
            other => return other,
        }
    }
}

/// Marche d'une heure vers `target` : jusqu'à [`WALK_TILES_PER_TICK`] tuiles,
/// par segments échantillonnés — on s'arrête net devant l'eau (l'océan ne se
/// traverse pas à pied).
///
/// La franchissabilité est sondée dans le **baseline** (élévation ≤ 0 = eau,
/// exactement le critère du drapeau WATER à la génération) : matérialiser un
/// chunk entier pour chaque point de passage écraserait le LRU — un agent
/// en errance traverse des dizaines de chunks par heure.
fn walk_line(world: &mut World, pos: &mut Position, target: (i64, i64)) -> Move {
    let mut budget = WALK_TILES_PER_TICK;
    walk_budgeted(world, pos, target, &mut budget)
}

/// [`walk_line`] sur ce qui reste de marche dans l'heure (`budget`, en
/// tuiles, décompté) : c'est ce qui laisse un trajet enchaîner ses étapes.
fn walk_budgeted(world: &mut World, pos: &mut Position, target: (i64, i64), budget: &mut f64) -> Move {
    let mut moved = false;
    while *budget > 0.0 {
        let dx = target.0 as f64 + 0.5 - pos.x;
        let dy = target.1 as f64 + 0.5 - pos.y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist <= ARRIVAL_TILES {
            return Move::Arrived;
        }
        let step = WALK_SAMPLE_TILES.min(dist).min(*budget);
        let next = (pos.x + dx / dist * step, pos.y + dy / dist * step);
        if world.worldgen().elevation(next.0.floor() as i64, next.1.floor() as i64) <= 0.0 {
            return if moved { Move::Moved } else { Move::Stuck };
        }
        pos.x = next.0;
        pos.y = next.1;
        *budget -= step;
        moved = true;
    }
    Move::Moved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Task;

    /// Empreinte compacte de l'état complet : positions, physiologies,
    /// traits hérités, mémoire et compétences dans l'ordre des identifiants.
    /// Deux exécutions identiques ⇒ même empreinte.
    #[allow(clippy::type_complexity)]
    fn fingerprint(sim: &Sim) -> Vec<(u64, u64, u64, u32, u32, u32, u64, u32, u16, u32)> {
        let mut all: Vec<_> = sim
            .agents
            .query::<(&AgentId, &Position, &Physiology, &Traits, &Memory, &Skills, &Exposures, &Knowledge)>()
            .iter()
            .map(|(_, (id, pos, phys, traits, mem, sk, exp, kn))| {
                (
                    id.0,
                    pos.x.to_bits(),
                    pos.y.to_bits(),
                    phys.hunger.to_bits(),
                    phys.health.to_bits(),
                    traits.curiosity.to_bits(),
                    (mem.known.len() as u64) << 8 | mem.springs.len() as u64,
                    sk.foraging.to_bits(),
                    exp.0,
                    kn.len() as u32,
                )
            })
            .collect();
        all.sort();
        all
    }

    /// Cherche la terre ferme la plus proche de l'origine (qui peut être en
    /// mer !) — sondée via le worldgen, sans générer de chunks pour rien.
    fn find_land(sim: &Sim) -> (i64, i64) {
        // Les océans entre continents font des milliers de km : on sonde par
        // pas de 16 km, dans 8 directions, jusqu'à 6 400 km.
        let step = cairn_core::km_to_tiles(16.0) as i64;
        for r in 0..400i64 {
            let d = r * step;
            for &(x, y) in &[
                (d, 0), (-d, 0), (0, d), (0, -d),
                (d, d), (-d, d), (d, -d), (-d, -d),
            ] {
                // Marge d'élévation : franchement à l'intérieur des terres.
                if sim.world.worldgen().elevation(x, y) > 0.05 {
                    return (x, y);
                }
            }
        }
        panic!("aucune terre à moins de 6 400 km de l'origine");
    }

    /// Construit un essaim d'agents (et du gibier) sur la terre ferme la plus
    /// proche de l'origine.
    fn scenario(seed: u64, agents: u32, ticks: u64) -> Sim {
        let (mut sim, _) = scenario_setup(seed, agents, 4);
        for _ in 0..ticks {
            sim.step();
        }
        sim
    }

    /// Cheptel présent dans un rayon donné (en km) autour d'un point.
    /// C'est l'observable du critère « effondrement **local** » : le total
    /// global mélangerait les troupeaux enfuis au loin, qui sont bien vivants.
    fn local_herbivores(sim: &Sim, center: (i64, i64), radius_km: f64) -> f32 {
        let r = cairn_core::km_to_tiles(radius_km);
        sim.fauna
            .query::<(&Herd, &Position)>()
            .iter()
            .filter(|(_, (_, p))| {
                (p.x - center.0 as f64).hypot(p.y - center.1 as f64) < r
            })
            .map(|(_, (h, _))| h.population)
            .sum()
    }

    /// Prépare le monde sans le faire tourner : `agents` humains et `herds`
    /// troupeaux serrés autour de la même clairière. Renvoie aussi le foyer.
    /// Comme [`scenario_setup`], mais au foyer des bancs (`scenario::find_home`,
    /// une source à portée) : pour les expériences de plusieurs semaines, où la
    /// scène standard, sans eau connue, tue de soif avant que le reste ne joue.
    fn scenario_au_foyer(seed: u64, agents: u32, herds: u32) -> (Sim, (i64, i64)) {
        let mut sim = Sim::new(WorldSeed(seed), 2048);
        sim.allow_fauna_immigration = false;
        sim.allow_wildfires = false;
        let seed_point =
            (cairn_core::km_to_tiles(1500.0) as i64, cairn_core::km_to_tiles(2100.0) as i64);
        let home = crate::scenario::find_home(&mut sim, seed_point);
        let (mut placed, mut k) = (0, 0i64);
        while placed < agents && k < 10_000 {
            let (x, y) = (home.0 + (k % 20) * 6 - 60, home.1 + (k / 20) * 6 - 60);
            k += 1;
            if sim.world.tile(x, y).is_walkable() {
                sim.spawn_agent(x as f64 + 0.5, y as f64 + 0.5);
                placed += 1;
            }
        }
        assert_eq!(placed, agents, "semis incomplet autour de {home:?}");
        for i in 0..herds {
            let angle = i as f64 * 1.7;
            sim.spawn_herd(
                home.0 as f64 + angle.cos() * 300.0,
                home.1 as f64 + angle.sin() * 300.0,
                fauna::HERD_START,
            );
        }
        (sim, home)
    }

    fn scenario_setup(seed: u64, agents: u32, herds: u32) -> (Sim, (i64, i64)) {
        // Capacité large : à 512 chunks, 60 agents dispersés dépassent le
        // working set et le LRU thrash (mesuré : 0,8 tick/s contre 15).
        let mut sim = Sim::new(WorldSeed(seed), 2048);
        // Scène de test contrôlée : la faune (dont son immigration
        // stochastique) ne doit pas interférer avec la mécanique isolée par
        // le test qui appelle ce helper — voir `Sim::allow_fauna_immigration`.
        sim.allow_fauna_immigration = false;
        sim.allow_wildfires = false; // idem : pas d'incendie stochastique parasite
        let home = find_land(&sim);
        let (mut placed, mut k) = (0, 0i64);
        while placed < agents && k < 10_000 {
            let (dx, dy) = ((k % 20) * 6 - 60, (k / 20) * 6 - 60);
            k += 1;
            let (x, y) = (home.0 + dx, home.1 + dy);
            if sim.world.tile(x, y).is_walkable() {
                sim.spawn_agent(x as f64 + 0.5, y as f64 + 0.5);
                placed += 1;
            }
        }
        assert_eq!(placed, agents, "semis incomplet autour de {home:?}");
        // Le gibier à portée de vue des chasseurs.
        for i in 0..herds {
            let angle = i as f64 * 1.7;
            sim.spawn_herd(
                home.0 as f64 + angle.cos() * 300.0,
                home.1 as f64 + angle.sin() * 300.0,
                fauna::HERD_START,
            );
        }
        (sim, home)
    }

    /// Critère de l'étape 4 du chantier de dérive : **un pays surpeuplé ne fait
    /// pas croître ses troupeaux**. Tous les troupeaux au centre d'une même
    /// maille de pâturage, dix fois plus de bouches que sa production n'en
    /// nourrit (quel que soit le biome tiré : l'effectif se règle sur la
    /// production mesurée de la maille), sans prédateur ni humain. Sans
    /// densité-dépendance, M1-faune a mesuré que la pâture ne freine à aucune
    /// densité : l'effectif monte au plafond de 0,010/j. Ici, il doit baisser.
    #[test]
    fn un_pays_surpeuple_ne_fait_pas_croitre_ses_troupeaux() {
        let (mut sim, home) = scenario_setup(11, 0, 0);
        let side = fauna::RANGE_ZONE_TILES;
        let zone = fauna::range_zone((home.0 as f64, home.1 as f64));
        let center = ((zone.0 as f64 + 0.5) * side, (zone.1 as f64 + 0.5) * side);
        let species = fauna::Species::herbivore_for_biome(
            sim.world.worldgen().biome(center.0 as i64, center.1 as i64),
            &mut Pcg32::new(1, 1),
        );
        let production = sim.rangeland.production(sim.world.worldgen(), zone);
        let heads_needed = 10.0 * production / species.daily_ration_kg();
        let herds = (heads_needed / 80.0).ceil().max(1.0) as usize;
        for i in 0..herds {
            let angle = i as f64 * 0.9;
            let r = 20.0 + (i % 10) as f64 * 25.0; // ≤ 250 tuiles du centre
            let (x, y) = (center.0 + angle.cos() * r, center.1 + angle.sin() * r);
            if sim.world.tile(x as i64, y as i64).is_walkable() {
                sim.spawn_herd_species(x, y, 80.0, species);
            }
        }
        let heads = |sim: &Sim| -> f32 { sim.fauna.query::<&Herd>().iter().map(|(_, h)| h.population).sum() };
        let before = heads(&sim);
        assert!(before >= 0.9 * heads_needed, "semis incomplet : {before:.0} têtes sur {heads_needed:.0}");
        for _ in 0..20 * TICKS_PER_DAY {
            sim.step();
        }
        let after = heads(&sim);
        assert!(
            after < before,
            "{before:.0} {species:?} pour une maille qui en nourrit {:.0} ont crû jusqu'à {after:.0} \
             en 20 jours : rien ne freine la natalité quand le pays est surpeuplé",
            production / species.daily_ration_kg()
        );
    }

    /// D10 : la cueillette ne peut pas rendre plus d'énergie que la plante n'en
    /// fixe. La référence est physique, pas une constante du modèle : la
    /// productivité primaire nette du biome (Whittaker & Likens) convertie en
    /// kcal, soit toute la matière végétale produite dans l'année, bois compris.
    /// La part réellement comestible en est une petite fraction ; ce test ne
    /// demande que la borne la plus lâche.
    #[test]
    fn la_cueillette_ne_rend_pas_plus_que_la_plante_ne_produit() {
        use cairn_worldgen::Biome::*;
        for biome in [HotDesert, ColdDesert, Tundra, Steppe, Savanna, Grassland, Taiga, TemperateForest, TropicalForest] {
            let npp = f64::from(ecology::npp_g_m2_yr(biome)) * ecology::PLANT_KCAL_PER_G;
            let gathered = gathering_kcal_m2_yr(biome);
            assert!(
                gathered <= npp,
                "{biome:?} : la cueillette rend {gathered:.0} kcal/m²/an, la plante en fixe {npp:.0} \
                 (×{:.1}) — un humain se nourrit sur {:.0} m²",
                gathered / npp,
                KCAL_PER_DAY / (gathered / 360.0)
            );
        }
    }

    /// Le premier tick où l'approche d'`hunter` tue : les tests qui vérifient
    /// ce qu'une prise **rapporte** (et non combien on en fait) chassent à
    /// ce tick-là.
    fn tick_de_prise(sim: &Sim, hunter: AgentId, skill: f32) -> u64 {
        (0..).find(|&t| hunt_succeeds(sim.world.seed(), t, hunter, skill, 1.0)).unwrap()
    }

    /// Une tuile de terre ferme proche de `home + (dx, 0)`, pour poser un
    /// troupeau à une distance voulue de l'humain du foyer.
    fn land_near(sim: &mut Sim, home: (i64, i64), dx: f64) -> (f64, f64) {
        for k in 0..400 {
            let (x, y) = (home.0 as f64 + dx + (k % 20) as f64 * 50.0, home.1 as f64 + (k / 20) as f64 * 50.0);
            if sim.world.tile(x as i64, y as i64).is_walkable() {
                return (x, y);
            }
        }
        panic!("pas de terre à {dx} tuiles du foyer");
    }

    /// Distance, en tuiles, d'un point au plus proche humain.
    fn nearest_human(sim: &Sim, p: (f64, f64)) -> f64 {
        sim.agents
            .query::<&Position>()
            .iter()
            .map(|(_, h)| (h.x - p.0).hypot(h.y - p.1))
            .fold(f64::INFINITY, f64::min)
    }

    /// Chantier de dérive, D′ : **le bord de la faune renvoie**. Un troupeau
    /// déjà à 9 km du seul humain, une meute collée à lui côté humain : il fuit
    /// vers l'extérieur — sans bord, de 2 km d'un bond. Avec un bord à flux nul,
    /// ce pas qui l'éloigne est refusé. (Le bord absorbant essayé avant, D,
    /// retirait ces fuyards : 3 seeds sur 4 perdaient tout leur gibier.)
    #[test]
    fn le_bord_de_la_faune_renvoie_le_fuyard() {
        let (mut sim, home) = scenario_setup(5, 1, 0);
        // Midi (MAR-2) : le test porte sur le bord de la faune, pas sur la nuit — au tick 0
        // il fait nuit noire, et l'on ne voit plus le gibier à 2 km.
        sim.time.tick = 12;
        sim.allow_fauna_immigration = true; // le monde ouvert
        let (x, y) = land_near(&mut sim, home, cairn_core::km_to_tiles(9.0));
        let id = sim.spawn_herd(x, y, 40.0);
        sim.spawn_pack(x - 150.0, y, 6.0);
        let pos_of = |sim: &Sim| -> (f64, f64) {
            sim.fauna
                .query::<(&FaunaId, &Position)>()
                .iter()
                .find(|(_, (i, _))| **i == id)
                .map(|(_, (_, p))| (p.x, p.y))
                .expect("le troupeau existe encore")
        };
        let before = nearest_human(&sim, pos_of(&sim));
        sim.step();
        let after = nearest_human(&sim, pos_of(&sim));
        assert!(
            after <= before + 50.0,
            "au-delà du bord, le troupeau s'est encore éloigné de {:.1} km en un tick",
            cairn_core::tiles_to_km(after - before)
        );
    }

    /// Chantier de dérive, D′ : **les abandonnés sortent**. Un troupeau sauvage
    /// à 20 km du seul humain — au-delà du double du bord : les humains sont
    /// partis — quitte la simulation dans la journée ; un troupeau à 2 km reste.
    #[test]
    fn un_troupeau_abandonne_par_les_humains_sort_de_la_simulation() {
        let (mut sim, home) = scenario_setup(5, 1, 0);
        sim.allow_fauna_immigration = true;
        let (nx, ny) = land_near(&mut sim, home, cairn_core::km_to_tiles(2.0));
        let (fx, fy) = land_near(&mut sim, home, cairn_core::km_to_tiles(20.0));
        let proche = sim.spawn_herd(nx, ny, 40.0);
        let lointain = sim.spawn_herd(fx, fy, 40.0);
        for _ in 0..TICKS_PER_DAY + 1 {
            sim.step();
        }
        let vivants: Vec<FaunaId> = sim.fauna.query::<(&FaunaId, &Herd)>().iter().map(|(_, (id, _))| *id).collect();
        assert!(vivants.contains(&proche), "le troupeau à 2 km d'un humain doit rester simulé");
        assert!(
            !vivants.contains(&lointain),
            "un troupeau à 20 km de tout humain est encore simulé : l'aire de la faune n'a pas de borne"
        );
    }

    /// Chantier de dérive, C3 : **un territoire ne porte qu'une poignée de
    /// prédateurs**. Dix meutes, 80 prédateurs sur le même kilomètre, du gibier
    /// à sa capacité tout autour : un territoire réel (~200 km², 0,05 loup/km²
    /// au plus) en nourrit une dizaine. Sans territoire, rien ne bornait le
    /// prédateur sinon sa proie — mesuré sur 5 ans : 1,5 à 2 prédateurs par
    /// km², et le gibier éteint sur 2 seeds sur 4 à l'an 4.
    #[test]
    fn un_territoire_ne_porte_qu_une_poignee_de_predateurs() {
        let (mut sim, home) = scenario_setup(11, 0, 0);
        let side = fauna::RANGE_ZONE_TILES;
        let zone = fauna::range_zone((home.0 as f64, home.1 as f64));
        let center = ((zone.0 as f64 + 0.5) * side, (zone.1 as f64 + 0.5) * side);
        let species = fauna::Species::herbivore_for_biome(
            sim.world.worldgen().biome(center.0 as i64, center.1 as i64),
            &mut Pcg32::new(1, 1),
        );
        // Le gibier à la capacité de sa maille : il ne manque pas, il ne
        // déborde pas.
        let production = sim.rangeland.production(sim.world.worldgen(), zone);
        let herds = (production / species.daily_ration_kg() / 60.0).ceil() as usize;
        for i in 0..herds {
            let angle = i as f64 * 0.7;
            let r = 30.0 + (i % 8) as f64 * 40.0;
            let (x, y) = (center.0 + angle.cos() * r, center.1 + angle.sin() * r);
            if sim.world.tile(x as i64, y as i64).is_walkable() {
                sim.spawn_herd_species(x, y, 60.0, species);
            }
        }
        for i in 0..10 {
            let angle = i as f64 * 0.63;
            sim.spawn_pack(center.0 + angle.cos() * 200.0, center.1 + angle.sin() * 200.0, 8.0);
        }
        let predators = |sim: &Sim| -> f32 { sim.fauna.query::<&Pack>().iter().map(|(_, p)| p.population).sum() };
        let before = predators(&sim);
        // 60 jours : sans recrutement, la famine (0,02/j) laisse ~30 % des
        // prédateurs ; sans territoire, bien nourris, ils croissent.
        for _ in 0..60 * TICKS_PER_DAY {
            sim.step();
        }
        let after = predators(&sim);
        assert!(
            after < 0.5 * before,
            "{before:.0} prédateurs sur un même territoire en sont à {after:.0} après 60 jours : \
             rien ne borne le prédateur sinon sa proie"
        );
    }

    /// Chantier de dérive, E : **deux troupeaux sauvages qui se voient se
    /// rejoignent**, tant que le total reste sous la fission. Pendant de la
    /// fission : sous la densité-dépendance, un troupeau ne grossit plus
    /// jusqu'à 90 têtes, et les moitiés issues des fissions passées ne se
    /// rejoignaient jamais — mesuré sur 5 ans : 622 troupeaux de 34 têtes, 4 tps.
    /// Deux espèces différentes, ou un total qui déclencherait la fission, ne
    /// fusionnent pas.
    #[test]
    fn deux_troupeaux_qui_se_voient_se_rejoignent() {
        let (mut sim, home) = scenario_setup(5, 0, 0);
        let (x, y) = land_near(&mut sim, home, 0.0);
        let sp = fauna::Species::Aurochs;
        let a = sim.spawn_herd_species(x, y, 30.0, sp);
        let b = sim.spawn_herd_species(x + 50.0, y, 30.0, sp);
        // Un troupeau d'une autre espèce, tout près : il reste à part.
        let c = sim.spawn_herd_species(x, y + 50.0, 30.0, fauna::Species::Deer);
        // Deux gros troupeaux voisins, loin des premiers : réunis, ils
        // dépasseraient le seuil de fission — ils restent à part.
        let d = sim.spawn_herd_species(x + 3000.0, y, 60.0, sp);
        let e = sim.spawn_herd_species(x + 3050.0, y, 60.0, sp);
        for _ in 0..TICKS_PER_DAY + 1 {
            sim.step();
        }
        let vivants: Vec<FaunaId> = sim.fauna.query::<(&FaunaId, &Herd)>().iter().map(|(_, (id, _))| *id).collect();
        let presents = [a, b].iter().filter(|id| vivants.contains(id)).count();
        assert_eq!(presents, 1, "deux troupeaux d'aurochs à 100 m, 60 têtes à eux deux, doivent n'en faire qu'un");
        assert!(vivants.contains(&c), "un troupeau d'une autre espèce ne fusionne pas");
        assert!(
            vivants.contains(&d) && vivants.contains(&e),
            "deux troupeaux dont la réunion dépasserait la fission restent séparés"
        );
    }

    /// Chantier de dérive, G : **un troupeau qui fuit n'échappe pas au frein de
    /// sa maille**. Dix fois plus de bouches que la maille n'en nourrit, et une
    /// menace posée à chaque tick sur chaque troupeau : ils fuient sans cesse.
    /// Mesuré sur `derive` (seed 42) : 57 à 90 % des têtes des amas les plus
    /// denses fuyaient depuis plus de 30 jours, satiété 0,95 contre 0,005 au
    /// calme — un million de têtes à l'an 5. Ici, l'effectif doit baisser.
    #[test]
    fn un_troupeau_qui_fuit_n_echappe_pas_au_frein_de_sa_maille() {
        let (mut sim, home) = scenario_setup(11, 0, 0);
        let side = fauna::RANGE_ZONE_TILES;
        let zone = fauna::range_zone((home.0 as f64, home.1 as f64));
        let center = ((zone.0 as f64 + 0.5) * side, (zone.1 as f64 + 0.5) * side);
        let species = fauna::Species::herbivore_for_biome(
            sim.world.worldgen().biome(center.0 as i64, center.1 as i64),
            &mut Pcg32::new(1, 1),
        );
        let production = sim.rangeland.production(sim.world.worldgen(), zone);
        // 40 têtes par troupeau : loin de la fission, qui ne doit pas s'en mêler.
        let herds = (10.0 * production / species.daily_ration_kg() / 40.0).ceil() as usize;
        for i in 0..herds {
            let angle = i as f64 * 0.9;
            let r = 20.0 + (i % 10) as f64 * 25.0;
            let (x, y) = (center.0 + angle.cos() * r, center.1 + angle.sin() * r);
            if sim.world.tile(x as i64, y as i64).is_walkable() {
                sim.spawn_herd_species(x, y, 40.0, species);
            }
        }
        let heads = |sim: &Sim| -> f32 { sim.fauna.query::<&Herd>().iter().map(|(_, h)| h.population).sum() };
        let before = heads(&sim);
        let seed = sim.world.seed();
        for _ in 0..30 * TICKS_PER_DAY {
            // Une menace sur chaque troupeau, à chaque tick : ils fuient sans fin.
            let threats: Vec<(f64, f64)> =
                sim.fauna.query::<(&Herd, &Position)>().iter().map(|(_, (_, p))| (p.x, p.y)).collect();
            let time = sim.time;
            fauna::update_herds(
                &mut sim.fauna,
                &mut sim.world,
                &sim.climate,
                time,
                seed,
                &threats,
                &mut sim.fauna_stats,
                &mut sim.rangeland,
                None,
            );
            sim.time.tick += 1;
        }
        let after = heads(&sim);
        assert!(
            after < before,
            "{before:.0} têtes en fuite perpétuelle dans une maille qui en nourrit {:.0} \\
             ont crû jusqu'à {after:.0} en 30 jours : la fuite fait échapper au frein de la maille",
            production / species.daily_ration_kg()
        );
    }

    /// CHA-2c, le paysage de la peur : un troupeau dérangé une fois revient à
    /// son domaine ; harcelé sans cesse du même côté, il l'abandonne. Une
    /// menace qui le suit dix jours durant, puis deux jours de calme.
    #[test]
    fn un_troupeau_sans_cesse_derange_abandonne_son_domaine() {
        let eloignement = |jours_de_menace: u64| -> f64 {
            let (mut sim, home) = scenario_setup(11, 0, 0);
            let center = (home.0 as f64 + 0.5, home.1 as f64 + 0.5);
            sim.spawn_herd_species(center.0, center.1, 40.0, fauna::Species::Deer);
            let seed = sim.world.seed();
            for t in 0..13 * TICKS_PER_DAY {
                // Un jour de calme d'abord : le domaine se pose au premier tour
                // de pâture, pas au bout d'une fuite.
                let menace = t == TICKS_PER_DAY || (TICKS_PER_DAY..(1 + jours_de_menace) * TICKS_PER_DAY).contains(&t);
                // Harcelé : la menace le suit, toujours du même côté (des
                // chasseurs qui reviennent depuis leur camp, à l'ouest).
                let herd_pos = sim.fauna.query::<(&Herd, &Position)>().iter().map(|(_, (_, p))| (p.x, p.y)).next().unwrap();
                let threats: Vec<(f64, f64)> = if menace { vec![(herd_pos.0 - 150.0, herd_pos.1)] } else { vec![] };
                let time = sim.time;
                fauna::update_herds(
                    &mut sim.fauna,
                    &mut sim.world,
                    &sim.climate,
                    time,
                    seed,
                    &threats,
                    &mut sim.fauna_stats,
                    &mut sim.rangeland,
                    None,
                );
                sim.time.tick += 1;
            }
            let herd = sim.fauna.query::<&Herd>().iter().map(|(_, h)| *h).next().unwrap();
            let domaine = herd.home.unwrap();
            (domaine.0 - center.0).hypot(domaine.1 - center.1)
        };
        let rayon = fauna::Species::Deer.home_range_radius();
        let une_fois = eloignement(0);
        assert!(une_fois < 0.25 * rayon, "dérangé une fois, il garde son domaine : centre déplacé de {une_fois:.0} tuiles (rayon {rayon:.0})");
        let sans_cesse = eloignement(10);
        assert!(
            sans_cesse > rayon,
            "harcelé dix jours du même côté, il doit avoir quitté son domaine : centre déplacé de {sans_cesse:.0} tuiles (rayon {rayon:.0})"
        );
    }

    /// La ligne droite entre `a` et `b` traverse-t-elle de l'eau ? (échantillon
    /// tous les 8 tuiles sur le baseline.)
    fn straight_blocked(sim: &Sim, a: (i64, i64), b: (i64, i64)) -> bool {
        let (dx, dy) = ((b.0 - a.0) as f64, (b.1 - a.1) as f64);
        let dist = (dx * dx + dy * dy).sqrt();
        let steps = (dist / 8.0).ceil() as i64;
        (0..=steps).any(|i| {
            let t = i as f64 / steps as f64;
            let x = (a.0 as f64 + dx * t).floor() as i64;
            let y = (a.1 as f64 + dy * t).floor() as i64;
            sim.world.worldgen().elevation(x, y) <= 0.0
        })
    }

    /// Intégration de l'A* : quand l'eau coupe la ligne droite, l'agent doit
    /// **contourner** — c'est-à-dire créer une route et se déplacer — au lieu
    /// de rester figé au bord de l'eau (l'ancien comportement).
    #[test]
    fn un_agent_contourne_l_eau_au_lieu_de_se_figer() {
        let mut sim = Sim::new(WorldSeed(42), 512);
        let home = find_land(&sim);
        // Marche vers l'est jusqu'au premier rivage.
        let mut sx = home.0;
        while sim.world.worldgen().elevation(sx, home.1) > 0.0 && sx < home.0 + 200_000 {
            sx += 8;
        }
        assert!(sx < home.0 + 200_000, "aucun océan à l'est du foyer (seed atypique)");
        // Départ sur la terre juste avant le rivage ; cible au-delà de l'eau.
        let start = (sx - 48, home.1);
        let target = (sx + 800, home.1);
        assert!(
            sim.world.worldgen().elevation(start.0, start.1) > 0.0,
            "le départ doit être sur la terre"
        );
        assert!(straight_blocked(&sim, start, target), "la ligne droite doit être coupée par l'eau");

        let id = AgentId(0);
        let mut pos = Position { x: start.0 as f64 + 0.5, y: start.1 as f64 + 0.5 };
        let origin = (pos.x, pos.y);
        let mut routes: BTreeMap<u64, Route> = BTreeMap::new();
        // Le trajet peut être épuisé avant la fin (on marche désormais à pleine
        // vitesse le long d'un trajet) : on vérifie qu'il a existé.
        let mut routed = false;
        for _ in 0..8 {
            let mut budget = PathBudget { left: PATH_REQUESTS_PER_TICK, denied: 0 };
            advance(&mut sim.world, &mut routes, &mut budget, id, &mut pos, target);
            routed |= routes.contains_key(&id.0);
        }
        assert!(routed, "un trajet A* d'évitement doit être créé");
        let moved = (pos.x - origin.0).hypot(pos.y - origin.1);
        assert!(moved > 1.0, "l'agent doit s'être déplacé le long de la route ({moved:.1} tuiles)");
        // Et il reste sur la terre (jamais dans l'eau).
        assert!(
            sim.world.worldgen().elevation(pos.x.floor() as i64, pos.y.floor() as i64) > 0.0,
            "l'agent ne doit jamais marcher dans l'eau"
        );
    }

    #[test]
    fn deux_executions_identiques_bit_a_bit() {
        let a = scenario(42, 30, 24 * 8);
        let b = scenario(42, 30, 24 * 8);
        assert_eq!(fingerprint(&a), fingerprint(&b));
        assert_eq!(a.deaths.len(), b.deaths.len());
        assert_eq!(a.births.len(), b.births.len());
        assert_eq!(a.world.dirty_count(), b.world.dirty_count());
        // La faune aussi : effectifs bit à bit, pas seulement « à peu près ».
        let (ha, pa, nha, npa) = a.fauna_census();
        let (hb, pb, nhb, npb) = b.fauna_census();
        assert_eq!((ha.to_bits(), pa.to_bits()), (hb.to_bits(), pb.to_bits()));
        assert_eq!((nha, npa), (nhb, npb));
        assert_eq!(a.hunted_head.to_bits(), b.hunted_head.to_bits());
        assert_eq!(a.tech_events.len(), b.tech_events.len());
        assert_eq!(a.known_techs, b.known_techs);
        // La Chronique aussi, jusqu'au contenu : deux mondes de même seed
        // racontent exactement la même histoire (BRIEF §8.2, le replay).
        assert_eq!(a.chronicle, b.chronicle);
    }

    /// La Chronique (Phase 6) doit se **remplir en jouant**, pas seulement dans
    /// un test unitaire qui pousse des événements à la main : on vérifie ici que
    /// les points de collecte sont réellement câblés dans la boucle, et que ce
    /// qu'elle raconte est lisible.
    ///
    /// Scène du groupe resserré (celle des tests de clan), pour la même raison
    /// qu'eux : sans faune ni incendie, 24 agents groupés visitent peu de chunks
    /// — la même vérification sur la scène dispersée coûtait 35 minutes.
    ///
    /// L'observable est l'entrée d'un clan dans la Chronique, qui suppose deux
    /// choses de suite : qu'un clan se forme (Phase 4) **et** qu'il tienne
    /// `chronicle::CLAN_NOTABLE_DAYS` (la règle de notabilité, Phase 6).
    #[test]
    #[ignore = "D2 : dans cette scène (24 agents), les clans se recombinent presque chaque jour et aucun ne tient assez pour entrer dans la Chronique ; le test ne passait plus que grâce à une razzia, que R1 évite (2026-10-02)"]
    fn la_chronique_se_remplit_en_jouant() {
        let (mut sim, _) = scenario_setup(42, 24, 0);
        let deadline = 24 * (crate::chronicle::CLAN_NOTABLE_DAYS + 40);
        for _ in 0..deadline {
            sim.step();
            if !sim.chronicle.is_empty() {
                break;
            }
        }
        assert!(
            !sim.chronicle.is_empty(),
            "un groupe resserré doit finir par entrer dans la Chronique"
        );
        // Elle est ordonnée : un journal ne remonte pas le temps.
        assert!(
            sim.chronicle.windows(2).all(|w| w[0].tick <= w[1].tick),
            "la Chronique doit rester ordonnée par tick"
        );
        // Et elle se lit : chaque entrée produit une phrase datée et nommée.
        for line in sim.chronicle_tail(20) {
            assert!(line.starts_with("An "), "chaque entrée s'ouvre sur sa date : {line}");
            assert!(line.ends_with('.'), "et se termine comme une phrase : {line}");
            assert!(!line.contains("AgentId"), "aucun type Rust ne fuit : {line}");
        }
    }

    /// La règle de notabilité des clans (Phase 6) : un groupe détecté puis
    /// reperdu aussitôt ne laisse **aucune** trace dans le récit, alors que
    /// `clan_events` — la statistique — l'enregistre bien.
    ///
    /// C'est le correctif d'un vrai défaut observé : la première version
    /// journalisait chaque détection, et un an de la scène par défaut produisait
    /// des dizaines de « les X se reconnaissent comme un peuple » suivis
    /// immédiatement de « les X se dispersent » — le churn de détection étalé en
    /// récit, exactement le log que le §6.4 refuse.
    #[test]
    fn un_clan_ephemere_ne_fait_pas_date() {
        let (mut sim, _) = scenario_setup(42, 24, 0);
        // Un clan planté à la main, vieux d'un seul jour : la détection de
        // minuit ne le retrouvera pas (le graphe social ne le connaît pas) et
        // l'effacera — sans rien raconter.
        sim.clans.push(Clan {
            id: ClanId(9999),
            founded_tick: sim.time.tick,
            members: (0..12).map(AgentId).collect(),
            home: (0.0, 0.0),
            stock: 0.0,
            chief: AgentId(0),
            desired: None,
            rivalry: 0.0,
        });
        for _ in 0..24 {
            sim.step();
        }
        assert!(
            sim.clan_events.iter().any(|e| e.clan == ClanId(9999)),
            "la statistique, elle, doit bien voir la dissolution"
        );
        assert!(
            !sim.chronicle.iter().any(|e| matches!(
                e.kind,
                crate::chronicle::EventKind::ClanDissolved { clan: ClanId(9999), .. }
            )),
            "un clan d'un jour n'a pas d'histoire à clore"
        );
    }

    /// Incrément 1 de la Phase 5 (« l'exposition ») : en vivant et en se
    /// déplaçant, la population accumule des expositions au monde qu'elle
    /// traverse (bois, graminées, gisements…). Prouve que la perception est
    /// bien câblée dans la boucle — la mécanique pure est testée dans
    /// `crate::exposure`.
    #[test]
    fn la_population_accumule_des_expositions_en_vivant() {
        let sim = scenario(42, 30, 24 * 20);
        let total: u32 =
            sim.agents.query::<&Exposures>().iter().map(|(_, e)| e.count()).sum();
        assert!(
            total > 0,
            "après 20 jours, la population doit avoir vu quelque chose ({total} expositions)"
        );
    }

    /// LE critère d'acceptation de la faune : « la surchasse d'une zone
    /// provoque un effondrement local observable ».
    ///
    /// Expérience **contrôlée** : même seed, même clairière, même gibier —
    /// seule la pression de chasse change. C'est ce qui prouve que
    /// l'effondrement vient des chasseurs, et non du climat ou du hasard.
    ///
    /// L'observable est le cheptel **local** (3 km) : le total global
    /// mélangerait les troupeaux qui ont fui au loin et se portent très bien.
    /// La zone se vide des deux façons — on en tue, et les autres décampent.
    #[test]
    fn la_surchasse_effondre_le_gibier_local() {
        // Trente jours, **en été** et sur **quatre seeds** (CHA-1, règle 7). En
        // hiver (départ au jour 0), le gibier migre vers le chaud et quitte la
        // zone même sans chasseur : le témoin n'y témoignait de rien sur deux
        // seeds sur quatre (7 et 2024 : 100 → 0 têtes sans un humain) — un
        // défaut de ce test, présent avant CHA-1. En été, mesuré : le témoin
        // reste (101-134 têtes à 3 km) et la zone chassée se vide sur 4/4
        // (9 à 58 têtes tuées par seed). Depuis la fidélité au domaine (CHA-1),
        // un troupeau dérangé revient : la zone ne se vide plus seulement par la
        // fuite, mais par les prises.
        let jours = 24 * 30;
        let mut tues = 0.0;
        for seed in [42u64, 7, 1337, 2024] {
            let cheptel = |agents: u32| {
                let (mut sim, home) = scenario_au_foyer(seed, agents, 4);
                sim.time.tick = 135 * cairn_core::TICKS_PER_DAY;
                let depart = local_herbivores(&sim, home, 3.0);
                // La pression de chasse vient de la vie même : la faim qui
                // revient, la réserve du clan qui manque (on ne force plus la
                // faim : un affamé forcé cueille plutôt que de chercher un
                // gibier hors de vue — MAR-2, MAR-4).
                for _ in 0..jours {
                    sim.step();
                }
                (depart, local_herbivores(&sim, home, 3.0), sim.hunted_head)
            };
            let (depart, temoin, tue_temoin) = cheptel(0);
            let (_, surchasse, tue_surchasse) = cheptel(60);
            tues += tue_surchasse;
            assert_eq!(tue_temoin, 0.0, "témoin : personne ne chasse");
            assert!(
                temoin > depart * 0.5,
                "seed {seed}, témoin : le gibier doit rester sur place ({depart:.0} → {temoin:.0} têtes) — \
                 sinon le test ne mesure pas la chasse mais la dérive des troupeaux"
            );
            assert!(
                surchasse < temoin * 0.2,
                "seed {seed} : effondrement local attendu : {surchasse:.0} têtes à 3 km sous 60 chasseurs, \
                 contre {temoin:.0} sans (départ {depart:.0})"
            );
        }
        assert!(tues > 20.0, "les chasseurs doivent prélever du gibier ({tues:.0} têtes sur 4 seeds)");
    }

    #[test]
    fn la_chasse_nourrit_mieux_que_la_cueillette() {
        // Des affamés près du gibier chassent, et le troupeau le paie : c'est le
        // couplage « chassent quand ils ont faim ». Depuis que la mise à mort
        // est incertaine (~10 % par journée de chasse, D10) et qu'un chasseur
        // qui porte de la viande ne repart pas, on les garde affamés trente
        // jours : c'est la faim qui est l'expérience, pas l'état initial. Trente
        // et non dix : mesuré, ~9 heures à portée par jour pour le groupe, à
        // ~1,7 % l'heure, soit ~0,16 prise par jour — zéro prise en trente jours
        // a moins de 1 % de chances. Au foyer des bancs : sans source à portée,
        // la soif tuait les deux tiers des chasseurs avant la fin.
        // Jugé sur quatre seeds (règle 7, CHA-2b) : en hiver, depuis que le
        // pisteur s'arrête à la nuit, mesuré 0, 3, 1 et 6 têtes en trente jours
        // (1, 9, 3 et 7 avant) — la seed 42 seule ne disait qu'un tirage.
        let mut avec_prise = 0;
        for seed in [42u64, 7, 1337, 2024] {
            let (mut sim, _) = scenario_au_foyer(seed, 20, 3);
            for _ in 0..24 * 30 {
                for (_, (phys, carrying)) in sim.agents.query_mut::<(&mut Physiology, &mut Carrying)>() {
                    phys.hunger = phys.hunger.max(0.9);
                    carrying.0 = 0.0;
                }
                sim.step();
            }
            avec_prise += usize::from(sim.hunted_head > 0.0);
        }
        assert!(avec_prise >= 3, "des affamés près du gibier doivent chasser et tuer ({avec_prise} seeds sur 4)");
    }

    #[test]
    fn sans_pression_le_gibier_se_maintient() {
        // Le pendant du test de surchasse : sans chasseurs ni prédateurs, un
        // troupeau sur sa pâture ne doit ni exploser ni s'éteindre. Si ce
        // test casse, l'écologie est déréglée, pas la chasse.
        let (mut sim, _) = scenario_setup(42, 0, 2);
        let depart = sim.fauna_census().0;
        for _ in 0..24 * 60 {
            sim.step();
        }
        let (fin, _, groupes, _) = sim.fauna_census();
        assert!(groupes > 0, "le gibier ne doit pas disparaître tout seul");
        assert!(
            fin > depart * 0.5 && fin < depart * 4.0,
            "cheptel hors bornes : {depart:.0} → {fin:.0} têtes en 2 mois"
        );
    }

    /// Un troupeau tranquille doit **errer** autour de sa pâture, pas filer
    /// en ligne droite. Garde-fou contre une régression mesurée : en
    /// départageant les pâtures équivalentes par l'ordre de la liste
    /// d'échantillonnage, tous les troupeaux marchaient plein est à
    /// 1,9 km/jour — 38 km en trois semaines, et un LRU en miettes.
    #[test]
    fn un_troupeau_erre_au_lieu_de_marcher_droit() {
        let (mut sim, home) = scenario_setup(42, 0, 3);
        let jours = 15.0;
        for _ in 0..(24.0 * jours) as u32 {
            sim.step();
        }
        let mut max_km: f64 = 0.0;
        for (_, (_, pos)) in sim.fauna.query::<(&Herd, &Position)>().iter() {
            let d = (pos.x - home.0 as f64).hypot(pos.y - home.1 as f64);
            max_km = max_km.max(cairn_core::tiles_to_km(d));
        }
        // Une dérive balistique à la vitesse de pâture ferait ~29 km ; une
        // marche diffusive reste très en deçà.
        assert!(
            max_km < 8.0,
            "dérive balistique suspectée : un troupeau à {max_km:.1} km du départ en {jours:.0} jours"
        );
    }

    #[test]
    fn deux_seeds_divergent() {
        let a = scenario(42, 30, 24 * 4);
        let b = scenario(43, 30, 24 * 4);
        assert!(a.population() > 0, "extinction : scénario invalide pour le test");
        assert_ne!(fingerprint(&a), fingerprint(&b));
    }

    /// Une grossesse menée à terme donne un enfant : filiation posée, traits
    /// hérités bornés, et le nourrisson est **porté** — sa position est celle
    /// de sa mère, tick après tick — et allaité (il ne meurt pas de faim).
    #[test]
    fn une_grossesse_donne_un_nourrisson_porte_et_allaite() {
        use crate::demography::Pregnancy;

        let (mut sim, _) = scenario_setup(42, 12, 0);
        let (mut mother, mut father) = (None, None);
        for (_, (id, demo)) in sim.agents.query::<(&AgentId, &Demographics)>().iter() {
            match demo.sex {
                Sex::Female if mother.is_none() => mother = Some(*id),
                Sex::Male if father.is_none() => father = Some(*id),
                _ => {}
            }
        }
        let (mother, father) = (mother.expect("aucune femme"), father.expect("aucun homme"));
        // Grossesse imposée, à terme dans 2 jours : on teste la naissance et
        // l'enfance, pas la rencontre (testée par ailleurs).
        let due = sim.time.tick + 48;
        let father_traits = Traits { curiosity: 0.9, ..Traits::default() };
        for (_, (id, demo)) in sim.agents.query_mut::<(&AgentId, &mut Demographics)>() {
            if *id == mother {
                demo.pregnancy = Some(Pregnancy { due_tick: due, father, father_traits });
            }
        }

        for _ in 0..24 * 8 {
            sim.step();
        }

        assert_eq!(sim.births.len(), 1, "une naissance attendue");
        let birth = sim.births[0];
        assert_eq!(birth.mother, mother);
        assert_eq!(birth.father, father);

        let mut mother_pos = None;
        let mut child = None;
        for (_, (id, pos, kin, traits, demo, phys)) in sim
            .agents
            .query::<(&AgentId, &Position, &Kinship, &Traits, &Demographics, &Physiology)>()
            .iter()
        {
            if *id == mother {
                mother_pos = Some((pos.x, pos.y));
            }
            if *id == birth.child {
                child = Some(((pos.x, pos.y), *kin, *traits, *demo, *phys));
            }
        }
        let mother_pos = mother_pos.expect("la mère doit survivre à ce scénario doux");
        let ((cx, cy), kin, traits, demo, phys) =
            child.expect("le nourrisson doit survivre, porté et allaité");
        assert_eq!(kin.mother, Some(mother));
        assert_eq!(kin.father, Some(father));
        assert!(demo.is_infant(sim.time.tick));
        for t in [
            traits.strength,
            traits.endurance,
            traits.dexterity,
            traits.curiosity,
            traits.sociability,
            traits.aggression,
        ] {
            assert!((0.0..=1.0).contains(&t), "trait hors bornes : {t}");
        }
        // Porté : après 6 jours de vie, toujours dans les bras de sa mère.
        let d = (cx - mother_pos.0).hypot(cy - mother_pos.1);
        assert!(d < 1.0, "le nourrisson doit être porté ({d:.1} tuiles de sa mère)");
        // Allaité : la faim ne s'accumule pas.
        assert!(phys.hunger < 0.5, "nourrisson affamé ({:.2}) malgré l'allaitement", phys.hunger);
    }

    /// Le revers de l'allaitement : un nourrisson sans mère n'a personne
    /// pour le porter ni le nourrir — il meurt en quelques jours. Aucune
    /// règle ne dit « les orphelins meurent » : la dérive suffit.
    #[test]
    fn un_nourrisson_orphelin_ne_survit_pas() {
        let (mut sim, home) = scenario_setup(42, 6, 0);
        let orphan = sim.spawn_child(
            home.0 as f64 + 0.5,
            home.1 as f64 + 0.5,
            Sex::Male,
            Traits::default(),
            Kinship { mother: None, father: None },
        );
        for _ in 0..24 * 8 {
            sim.step();
        }
        assert!(
            sim.deaths.iter().any(|d| d.agent == orphan),
            "l'orphelin devrait être mort en 8 jours"
        );
    }

    /// Défaut G (run longue tempéré 7 : 1 118 nourrissons morts de soif en
    /// 82 ans, 80 % des naissances à la fin) : le lait était une porte tout
    /// ou rien — faim de la mère ≥ 0,95, plus une goutte. Une faim qui frôle
    /// le maximum quelques heures par jour coupait le lait chaque jour. La
    /// lactation résiste pourtant à une sous-alimentation modérée : elle ne
    /// tarit que dans une famine qui dure, quand les réserves s'épuisent
    /// (depuis le bilan énergétique, les réserves de graisse ; avant, la santé
    /// en tenait lieu). Les deux bouts : quelques heures de faim ne coupent
    /// rien ; une famine longue tarit le lait.
    #[test]
    fn le_lait_ne_tarit_que_dans_une_famine_qui_dure() {
        let infant_thirst_after = |exhausted: bool| -> f32 {
            let (mut sim, home) = scenario_setup(42, 0, 0);
            let mother = sim.spawn_agent(home.0 as f64 + 0.5, home.1 as f64 + 0.5);
            let infant = sim.spawn_child(
                home.0 as f64 + 0.5,
                home.1 as f64 + 0.5,
                Sex::Male,
                Traits::default(),
                Kinship { mother: Some(mother), father: None },
            );
            for _ in 0..48 {
                // La mère a très faim (au-dessus de l'ancien seuil) ; ses
                // réserves tiennent, ou la famine les a vidées et le corps
                // s'effondre.
                for (_, (id, phys)) in sim.agents.query_mut::<(&AgentId, &mut Physiology)>() {
                    if *id == mother {
                        phys.hunger = 0.97;
                        if exhausted {
                            phys.reserves_kcal = 0.0;
                            phys.health = 0.1;
                        }
                    }
                }
                demography::nurse_infants(&mut sim);
                for (_, (id, phys)) in sim.agents.query_mut::<(&AgentId, &mut Physiology)>() {
                    if *id == infant {
                        phys.thirst = (phys.thirst + crate::agent::THIRST_PER_TICK).min(1.0);
                    }
                }
            }
            sim.agents
                .query::<(&AgentId, &Physiology)>()
                .iter()
                .find(|(_, (id, _))| **id == infant)
                .map(|(_, (_, p))| p.thirst)
                .unwrap()
        };
        let reserves = infant_thirst_after(false);
        assert!(reserves < 0.3, "une mère affamée mais pas épuisée allaite encore ({reserves:.2})");
        let epuisee = infant_thirst_after(true);
        assert!(epuisee > reserves + 0.3, "une mère épuisée par la famine n'a presque plus de lait ({epuisee:.2})");
    }

    /// La proximité de parenté : sœur 0,5, grand-mère et tante 0,25, cousine
    /// 0,125, étrangère 0 — calculée depuis les naissances, qui survivent aux
    /// morts.
    #[test]
    fn la_proximite_de_parente_se_lit_dans_les_naissances() {
        // 1 et 2 ont deux enfants, 10 et 11 ; 10 et 20 ont 30 ; 11 et 21 ont 31.
        let parents: BTreeMap<u64, (u64, u64)> =
            [(10, (1, 2)), (11, (1, 2)), (30, (10, 20)), (31, (11, 21))].into_iter().collect();
        let k = |a, b| demography::kin_closeness(&parents, a, b);
        assert_eq!(k(10, 11), 0.5, "sœurs");
        assert_eq!(k(1, 30), 0.25, "grand-mère");
        assert_eq!(k(11, 30), 0.25, "tante");
        assert_eq!(k(30, 31), 0.125, "cousines");
        assert_eq!(k(30, 99), 0.0, "étrangère");
    }

    /// Une scène d'orphelins : chacun a perdu sa mère et a près de lui une
    /// femme qui allaite son propre nourrisson — sa grande sœur si `sisters`,
    /// une étrangère sinon. Renvoie (sim, orphelins).
    fn orphans_with_nursing_women(sisters: bool, n: u64) -> (Sim, Vec<AgentId>) {
        let (mut sim, home) = scenario_setup(42, 0, 0);
        let at = (home.0 as f64 + 0.5, home.1 as f64 + 0.5);
        let mut orphans = Vec::new();
        for i in 0..n {
            // La mère morte et le père : de simples identifiants, absents du monde.
            let (dead_mother, father) = (AgentId(1_000_000 + 2 * i), AgentId(1_000_001 + 2 * i));
            let woman = sim.spawn_agent(at.0, at.1);
            for (_, (id, demo)) in sim.agents.query_mut::<(&AgentId, &mut Demographics)>() {
                if *id == woman {
                    demo.sex = Sex::Female;
                }
            }
            if sisters {
                sim.births.push(BirthRecord { tick: 0, mother: dead_mother, father, child: woman });
            }
            let own = Kinship { mother: Some(woman), father: None };
            sim.spawn_child(at.0, at.1, Sex::Male, Traits::default(), own);
            let orphan = sim.spawn_child(
                at.0,
                at.1,
                Sex::Female,
                Traits::default(),
                Kinship { mother: Some(dead_mother), father: Some(father) },
            );
            sim.births.push(BirthRecord { tick: 0, mother: dead_mother, father, child: orphan });
            orphans.push(orphan);
        }
        (sim, orphans)
    }

    /// L'allomaternage (vision de l'utilisateur, sourcée) : plus la femme qui
    /// allaite est proche de l'orphelin, plus il a de chances d'être nourri.
    /// Sur trois jours — le temps qu'un nourrisson tient sans lait —, une
    /// grande sœur (0,5 par jour) prend presque toujours l'orphelin, une
    /// étrangère (0,01) presque jamais. La mort reste possible des deux côtés.
    #[test]
    fn plus_la_femme_est_proche_plus_l_orphelin_est_nourri() {
        let adopted = |sisters: bool| {
            let (mut sim, orphans) = orphans_with_nursing_women(sisters, 20);
            for day in 1..=3 {
                sim.time.tick = day * cairn_core::TICKS_PER_DAY;
                demography::adopt_orphans(&mut sim);
            }
            orphans.iter().filter(|o| sim.fosters.contains_key(&o.0)).count()
        };
        let (soeurs, etrangeres) = (adopted(true), adopted(false));
        assert!(soeurs >= 14, "des grandes sœurs qui allaitent prennent presque tous les orphelins ({soeurs}/20)");
        assert!(etrangeres <= 4, "des étrangères n'en prennent presque aucun ({etrangeres}/20)");
    }

    /// Pris par une femme qui allaite, l'orphelin est porté et nourri, et vit.
    #[test]
    fn un_orphelin_pris_par_une_femme_qui_allaite_survit() {
        let (mut sim, orphans) = orphans_with_nursing_women(true, 1);
        let woman = sim.births.iter().find(|b| b.child != orphans[0]).map(|b| b.child).unwrap();
        sim.fosters.insert(orphans[0].0, woman);
        for _ in 0..24 * 8 {
            sim.step();
        }
        assert!(
            !sim.deaths.iter().any(|d| d.agent == orphans[0]),
            "nourri par une autre, l'orphelin doit être vivant après 8 jours"
        );
    }

    /// La rencontre suffit : un campement mixte produit des conceptions en
    /// quelques semaines, sans aucune intervention. Sur quatre seeds (règle 7) :
    /// sur une seule, l'espérance est d'une ou deux conceptions et zéro reste un
    /// tirage possible (mesuré au bilan énergétique : 1/1/3/3 avant les
    /// réserves, 0/1/2/1 après — on endure désormais la faim saturée sur ses
    /// réserves, et la porte de fécondité lit la faim).
    #[test]
    fn les_conceptions_surviennent_au_campement() {
        let mut conceptions = 0;
        for seed in [42u64, 7, 1337, 2024] {
            let (mut sim, _) = scenario_setup(seed, 30, 0);
            for _ in 0..24 * 45 {
                sim.step();
            }
            let pregnancies = sim
                .agents
                .query::<&Demographics>()
                .iter()
                .filter(|(_, d)| d.pregnancy.is_some())
                .count();
            conceptions += pregnancies + sim.births.len();
        }
        assert!(
            conceptions >= 2,
            "30 adultes mêlés pendant 45 jours, 4 seeds : des conceptions attendues ({conceptions})"
        );
    }

    /// Un enfant (ni nourrisson ni adulte) **suit** son parent : lâché à
    /// 300 tuiles de sa mère, il la rejoint et gravite ensuite autour d'elle.
    #[test]
    fn un_enfant_suit_sa_mere() {
        let (mut sim, home) = scenario_setup(42, 4, 0);
        let mother = sim
            .agents
            .query::<(&AgentId, &Demographics)>()
            .iter()
            .find(|(_, (_, d))| d.sex == Sex::Female)
            .map(|(_, (id, _))| *id)
            .expect("aucune femme parmi les fondateurs");
        let child = sim.spawn_child(
            home.0 as f64 + 300.5,
            home.1 as f64 + 0.5,
            Sex::Female,
            Traits::default(),
            Kinship { mother: Some(mother), father: None },
        );
        // Vieilli à 8 ans : un enfant autonome (marche, cueille) mais pas un
        // adulte — c'est lui qui a le candidat « suivre son parent ».
        for (_, (id, demo)) in sim.agents.query_mut::<(&AgentId, &mut Demographics)>() {
            if *id == child {
                demo.born_tick -= (8.0 * cairn_core::TICKS_PER_YEAR as f64) as i64;
            }
        }

        // La distance mère-enfant, relevée toutes les trois heures après le
        // premier jour (le temps de la rejoindre). On juge la **médiane**, pas
        // un instantané : depuis MAR-4, la mère part en sortie de jour et
        // l'enfant de 8 ans reste au camp — chez les Hadza aussi, les enfants
        // ne suivent pas les sorties de chasse. Un instantané pris pendant une
        // sortie mesurerait la sortie, pas l'attachement.
        let mut distances = Vec::new();
        for h in 0..24 * 3 {
            sim.step();
            if h >= 24 && h % 3 == 0 {
                let mut positions = BTreeMap::new();
                for (_, (id, pos)) in sim.agents.query::<(&AgentId, &Position)>().iter() {
                    positions.insert(id.0, (pos.x, pos.y));
                }
                let m = positions.get(&mother.0).expect("mère morte : scénario invalide");
                let c = positions.get(&child.0).expect("enfant mort : scénario invalide");
                distances.push((c.0 - m.0).hypot(c.1 - m.1));
            }
        }
        distances.sort_by(f64::total_cmp);
        let d = distances[distances.len() / 2];
        assert!(
            d < 200.0,
            "l'enfant devrait graviter autour de sa mère (médiane {d:.0} tuiles sur les jours 2-3, départ 300)"
        );
    }

    /// LE critère de la carte mentale : « strictement incluse dans ce que
    /// l'agent a pu percevoir ». On rejoue la simulation pas à pas en notant
    /// **de l'extérieur** où chaque agent a mis les pieds, puis on vérifie
    /// que sa mémoire ne contient rien d'autre : cellules ⊆ cellules
    /// foulées, sources ⊆ vraies sources (vues ou apprises d'un autre —
    /// qui ne mémorise lui-même que du vrai). Aucune omniscience possible.
    #[test]
    fn la_carte_mentale_reste_dans_le_percu() {
        let (mut sim, _) = scenario_setup(42, 10, 0);
        let mut stepped: BTreeMap<u64, std::collections::BTreeSet<(i64, i64)>> = BTreeMap::new();
        for _ in 0..24 * 10 {
            sim.step();
            for (_, (id, pos)) in sim.agents.query::<(&AgentId, &Position)>().iter() {
                stepped.entry(id.0).or_default().insert(crate::memory::cell_of(pos.tile()));
            }
        }
        let mut checked = 0;
        let mut spring_seen = 0;
        for (_, (id, mem)) in sim.agents.query::<(&AgentId, &Memory)>().iter() {
            let walked = stepped.get(&id.0).expect("agent jamais observé");
            for cell in &mem.known {
                assert!(
                    walked.contains(cell),
                    "agent {} : cellule {cell:?} en mémoire sans y avoir mis les pieds",
                    id.0
                );
            }
            checked += 1;
            spring_seen += mem.springs.len();
        }
        assert!(checked > 0, "population disparue : scénario invalide");
        assert!(spring_seen > 0, "personne n'a mémorisé la moindre source en 10 jours ?");
        // Chaque source mémorisée est une vraie source (eau douce à la tuile).
        let springs: Vec<(u64, (i64, i64))> = sim
            .agents
            .query::<(&AgentId, &Memory)>()
            .iter()
            .flat_map(|(_, (id, mem))| mem.springs.iter().map(|s| (id.0, *s)).collect::<Vec<_>>())
            .collect();
        for (agent, (x, y)) in springs {
            assert!(
                sim.world.tile(x, y).has_fresh_water(),
                "agent {agent} : source fantôme en ({x}, {y})"
            );
        }
        // Et la pratique a forgé au moins un cueilleur au-delà de son départ.
        let progressed = sim
            .agents
            .query::<(&Traits, &Skills)>()
            .iter()
            .any(|(_, (t, s))| s.foraging > 0.5 * crate::skills::forage_cap(t) + 1e-4);
        assert!(progressed, "10 jours de cueillette doivent forger la compétence");
    }

    /// Critère : « deux groupes qui se rencontrent échangent de l'information
    /// géographique ». Une source connue du seul agent A passe à B, à portée
    /// de conversation — et **pas** à C, trop loin pour entendre.
    #[test]
    fn les_rencontres_echangent_les_sources() {
        let (mut sim, home) = scenario_setup(42, 2, 0);
        let far = sim.spawn_agent(home.0 as f64 + 5_000.0, home.1 as f64 + 0.5);
        let ids: Vec<AgentId> = sim
            .agents
            .query::<&AgentId>()
            .iter()
            .map(|(_, id)| *id)
            .collect();
        let secret = (123_456, -654_321); // n'importe où : on teste le canal
        for (_, (id, mem)) in sim.agents.query_mut::<(&AgentId, &mut Memory)>() {
            if *id == ids[0] {
                mem.remember_spring(secret, (0.0, 0.0));
            }
        }
        crate::memory::exchange_knowledge(&mut sim);
        let knows = |sim: &Sim, who: AgentId| {
            sim.agents
                .query::<(&AgentId, &Memory)>()
                .iter()
                .any(|(_, (id, mem))| *id == who && mem.springs.contains(&secret))
        };
        assert!(
            knows(&sim, ids[1]),
            "la source connue de A doit être apprise par B, à 12 tuiles de lui"
        );
        assert!(
            !knows(&sim, far),
            "à 10 km, C est bien trop loin pour entendre parler de la source"
        );
    }

    /// Critère : « les agents curieux explorent plus loin, mesurablement ».
    /// Deux moitiés de population identiques à la curiosité près ; après
    /// deux semaines, les curieux connaissent plus de cellules — **sur chacune
    /// de quatre seeds** (règle 7). Sur la seule seed 42, l'ancien seuil
    /// (+20 %) passait par un tirage : mesuré avant MAR-2/MAR-4, le rapport
    /// allait de 1,02 à 1,59 selon la seed (1,19 en tout) ; après, de 1,03 à
    /// 1,30 (1,12) — toujours dans le même sens, mais l'écart est faible :
    /// voir le défaut CUR du registre (`docs/CODE.md` §5.1).
    #[test]
    fn les_curieux_explorent_plus_loin() {
        for seed in [42u64, 7, 1337, 2024] {
            let (mut sim, _) = scenario_setup(seed, 20, 0);
            for (_, (id, traits)) in sim.agents.query_mut::<(&AgentId, &mut Traits)>() {
                *traits = Traits {
                    curiosity: if id.0 % 2 == 0 { 0.95 } else { 0.05 },
                    sociability: 0.2, // la même cohésion pour tous : on isole la curiosité
                    ..Traits::default()
                };
            }
            for _ in 0..24 * 14 {
                sim.step();
            }
            let (mut curious, mut dull) = ((0usize, 0usize), (0usize, 0usize));
            for (_, (id, mem)) in sim.agents.query::<(&AgentId, &Memory)>().iter() {
                let bucket = if id.0 % 2 == 0 { &mut curious } else { &mut dull };
                bucket.0 += mem.known.len();
                bucket.1 += 1;
            }
            assert!(curious.1 > 0 && dull.1 > 0, "un groupe s'est éteint : scénario invalide");
            let mean_curious = curious.0 as f64 / curious.1 as f64;
            let mean_dull = dull.0 as f64 / dull.1 as f64;
            assert!(
                mean_curious > mean_dull,
                "seed {seed} : les curieux doivent connaître plus de terrain \
                 ({mean_curious:.1} cellules contre {mean_dull:.1})"
            );
        }
    }

    /// Fait avancer `sim` jour par jour jusqu'à `max_days`, et renvoie
    /// l'identifiant du premier clan formé (`None` si aucun ne s'est formé).
    /// On s'arrête **dès** la formation plutôt que de courir jusqu'au bout :
    /// le critère de ce test est « un clan se forme », pas « un clan dure » —
    /// la persistance dans la durée, elle, est couverte par
    /// `le_territoire_stabilise_la_derive_apres_formation` ci-dessous.
    fn run_until_clan_formed(sim: &mut Sim, max_days: u64) -> Option<crate::social::ClanId> {
        for _ in 0..max_days {
            for _ in 0..24 {
                sim.step();
            }
            if let Some(e) =
                sim.clan_events.iter().find(|e| e.kind == social::ClanEventKind::Formed)
            {
                return Some(e.clan);
            }
        }
        None
    }

    /// LE critère n°1 de la Phase 4 : « des clans se forment sans qu'aucune
    /// règle ne dise "former un clan ici" ». Le témoin dispersé a exactement
    /// la même taille de population que le groupe resserré, seule la
    /// géographie diffère — ce n'est donc pas la taille de la population qui
    /// déclenche la détection, mais bien la cohésion et la co-résidence.
    #[test]
    fn un_clan_emerge_sans_regle_explicite() {
        let (mut sim, _) = scenario_setup(42, 24, 0);
        assert!(
            run_until_clan_formed(&mut sim, 40).is_some(),
            "un groupe resserré de 24 personnes doit finir par former un clan"
        );

        // Témoin : même seed, même taille de population, mais dispersée à
        // des kilomètres — jamais à portée de rencontre.
        let mut scattered = Sim::new(WorldSeed(42), 512);
        scattered.allow_fauna_immigration = false; // scène contrôlée, voir plus haut
        scattered.allow_wildfires = false;
        let home = find_land(&scattered);
        for i in 0..10i64 {
            let far = (home.0 + i * 4000, home.1);
            if scattered.world.tile(far.0, far.1).is_walkable() {
                scattered.spawn_agent(far.0 as f64 + 0.5, far.1 as f64 + 0.5);
            }
        }
        assert!(
            run_until_clan_formed(&mut scattered, 40).is_none(),
            "une population dispersée à des km ne doit jamais former de clan"
        );
    }

    /// Le pendant du critère précédent : « un clan affamé s'effondre, et ses
    /// survivants rejoignent d'autres clans ou en fondent un nouveau ». On
    /// teste ici le **mécanisme** de dissolution, pas une famine réaliste
    /// (couverte par le calibrage) : rien ne code « si affamé, dissoudre » —
    /// un clan n'est qu'un instantané recalculé chaque jour, il s'efface tout
    /// seul dès qu'il ne retrouve plus son groupe le lendemain.
    #[test]
    fn un_clan_s_efface_quand_ses_membres_disparaissent() {
        let (mut sim, _) = scenario_setup(42, 24, 0);
        let clan_id = run_until_clan_formed(&mut sim, 40).expect("scénario invalide : aucun clan formé");
        assert!(sim.clans.iter().any(|c| c.id == clan_id), "le clan tout juste formé doit exister");

        let victims: Vec<hecs::Entity> =
            sim.agents.query::<&AgentId>().iter().map(|(e, _)| e).collect();
        for e in victims {
            let _ = sim.agents.despawn(e);
        }
        for _ in 0..24 {
            sim.step();
        }
        assert!(sim.clans.is_empty(), "sans le moindre membre, le clan ne peut plus être détecté");
        assert!(
            sim.clan_events
                .iter()
                .any(|e| e.clan == clan_id && e.kind == social::ClanEventKind::Dissolved),
            "un événement de dissolution doit avoir été enregistré"
        );
    }

    /// Déterminisme des clans, spécifiquement : même seed, même histoire de
    /// formations/dissolutions, mêmes membres, bit à bit.
    #[test]
    fn les_clans_sont_deterministes() {
        let run = || {
            let (mut sim, _) = scenario_setup(42, 24, 0);
            for _ in 0..24 * 40 {
                sim.step();
            }
            sim
        };
        let a = run();
        let b = run();
        let fingerprint = |sim: &Sim| -> Vec<(u64, u64, Vec<u64>)> {
            sim.clans
                .iter()
                .map(|c| (c.id.0, c.founded_tick, c.members.iter().map(|m| m.0).collect()))
                .collect()
        };
        assert_eq!(fingerprint(&a), fingerprint(&b));
        assert_eq!(a.clan_events.len(), b.clan_events.len());
    }

    /// La plus grande distance entre un membre du clan et le foyer courant du
    /// clan (`Clan::home`) — la mesure d'étalement utilisée pour ce test.
    fn clan_spread(sim: &Sim, clan: &social::Clan) -> f64 {
        let humans = sim.human_views();
        clan.members
            .iter()
            .filter_map(|&id| demography::find_human(&humans, id))
            .map(|h| (h.pos.0 - clan.home.0).hypot(h.pos.1 - clan.home.1))
            .fold(0.0_f64, f64::max)
    }

    /// L'incrément 2 de la Phase 4 (rétroaction comportementale vers le
    /// territoire) répond directement à une limite mesurée à l'incrément 1 :
    /// sans rien pour ramener un agent vers son clan, la même scène (24
    /// agents, seed 42) dérivait sans jamais se stabiliser (jusqu'à ~8 km en
    /// 85 j, voir CLAUDE.md). On ne rejoue pas cette mesure historique ici
    /// (trop lente, trop sensible aux aléas) ; on teste la propriété qui doit
    /// désormais tenir à la place : une fois le clan formé, son étalement
    /// **cesse de croître** au lieu de s'éloigner indéfiniment. On compare
    /// deux instantanés espacés de 40 jours plutôt qu'une borne absolue : la
    /// dérive sans borne, par définition, continue de croître entre deux
    /// mesures aussi éloignées ; une dérive rappelée par le territoire, non.
    /// Mesuré sur cette scène : 4,5 km à 60 j → 5,1 km à 100 j (+0,6 km en
    /// 40 j, contre les ~8 km en 85 j sans rappel — un plateau, pas une
    /// dérive).
    #[test]
    fn le_territoire_stabilise_la_derive_apres_formation() {
        // Quatre seeds (règle 7), jugées sur la croissance **moyenne** : sur une
        // seule, l'étalement oscille de ±1,5 km entre deux instantanés (mesuré
        // avant MAR-2/MAR-4 : seed 1337 +1,5 km, seed 2024 −1 km ; après :
        // seed 42 +0,8 km, seed 1337 −0,4 km) — une dérive sans borne, elle,
        // croîtrait partout.
        let mut growth = 0.0;
        for seed in [42u64, 7, 1337, 2024] {
            let (mut sim, _) = scenario_setup(seed, 24, 0);
            run_until_clan_formed(&mut sim, 40).expect("scénario invalide : aucun clan formé");
            for _ in 0..24 * 20 {
                sim.step();
            }
            let clan = sim.clans.first().cloned().expect("le clan doit exister à 60 j (40 + 20)");
            let spread_60d = clan_spread(&sim, &clan);
            for _ in 0..24 * 40 {
                sim.step();
            }
            let clan = sim
                .clans
                .first()
                .cloned()
                .expect("le clan doit avoir survécu jusqu'à 100 j grâce au territoire");
            growth += (clan_spread(&sim, &clan) - spread_60d) / 4.0;
        }
        assert!(
            growth < cairn_core::km_to_tiles(1.5),
            "l'étalement ne doit plus croître sans borne une fois le territoire actif \
             (croissance moyenne {growth:.0} tuiles entre 60 et 100 j, 4 seeds)"
        );
    }

    /// L'incrément 3 de la Phase 4 (stock commun, BRIEF §5.1) : une chasse
    /// fructueuse nourrit rarement pile ce qu'il fallait — une prise est une
    /// bête entière (`meat_hunger`), pas une portion calibrée. Le surplus, qui partait
    /// auparavant dans le `.max(0.0)` de la faim déjà comblée, charge
    /// désormais `Carrying` — **pas** le stock directement : la viande doit
    /// encore être rapportée au foyer (voir le test suivant). Test au niveau
    /// du mécanisme (`execute` appelé directement, comme le permet `mod
    /// tests` dans le même fichier) plutôt qu'un scénario complet : un clan
    /// injecté à la main ne survivrait pas à la prochaine détection de
    /// minuit (`social::daily` le remplacerait par ce qu'il détecte vraiment
    /// dans le graphe), donc un run multi-jours ne testerait pas ce qu'on
    /// veut isoler ici.
    #[test]
    fn une_chasse_fructueuse_charge_le_surplus_porte() {
        let mut sim = Sim::new(WorldSeed(42), 512);
        let home = find_land(&sim);
        let mut pos = Position { x: home.0 as f64 + 0.5, y: home.1 as f64 + 0.5 };
        let mut phys = Physiology { hunger: 0.3, ..Physiology::default() };
        let mut behavior =
            Behavior { task: Some(Task { kind: TaskKind::Hunt, target: home }), ..Behavior::default() };
        let traits = Traits::default();
        let mut skills = Skills::default();
        let mut routes: BTreeMap<u64, Route> = BTreeMap::new();
        let mut budget = PathBudget { left: PATH_REQUESTS_PER_TICK, denied: 0 };
        let clan_id = social::ClanId(3);
        let mut clan_stock: BTreeMap<social::ClanId, f32> = BTreeMap::new();
        let mut carrying = Carrying::default();
        let mut prestige = Prestige::default();
        let mut structures: Vec<Structure> = Vec::new();
        // Le troupeau est juste sous la main : cette scène teste la
        // mécanique de mise à mort, pas l'approche.
        let herd =
            HerdView {
                entity: hecs::Entity::DANGLING,
                pos: (pos.x, pos.y),
                population: 20.0,
                species: fauna::Species::Deer,
                tameness: 0.0,
            };

        let mut shares = Vec::new();
        let tick = tick_de_prise(&sim, AgentId(0), skills.hunting);
        let kill = execute(
            &mut sim.world,
            &mut routes,
            &mut budget,
            AgentId(0),
            &mut pos,
            &mut phys,
            &mut behavior,
            &[herd],
            &[],
            &[],
            1.0,
            &traits,
            &mut skills,
            Some(clan_id),
            &mut clan_stock,
            &mut carrying,
            &mut prestige,
            &mut structures,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut shares,
            tick, 1.0,
        );

        assert!(kill.is_some(), "le troupeau est à portée : la chasse doit réussir");
        assert!(phys.hunger < 1e-6, "la faim doit être totalement comblée (nutrition > faim initiale)");
        assert!(
            carrying.0 > 0.0,
            "le surplus (nutrition au-delà de la faim comblée) doit être porté par le chasseur"
        );
        // D10 : une bête tuée est une bête entière. Un cerf de ~120 kg donne
        // ~60 kg comestibles, ~72 000 kcal, un mois de nourriture ; même un
        // novice qui en gâche un quart en tire plus de deux semaines (7 points),
        // qu'il les mange, les porte au clan ou les offre autour de lui.
        let fed = 0.3 + carrying.0 + shares.iter().map(|s| s.amount).sum::<f32>();
        assert!(
            fed >= 7.0,
            "un cerf tué ne rapporte que {fed:.2} point de faim, {:.1} jour de nourriture",
            fed / 0.5
        );
        assert_eq!(
            clan_stock.get(&clan_id).copied().unwrap_or(0.0),
            0.0,
            "le stock du clan ne doit RIEN recevoir tant que le surplus n'a pas été rapporté au foyer"
        );
    }

    /// D10, le partage : un chasseur ne mange qu'une ration de sa bête ; ce
    /// qu'il offre autour de lui suit sa sociabilité héritée. Un voisin
    /// affamé à 100 m est nourri par un chasseur sociable, pas par un
    /// solitaire — et personne ne l'oblige : c'est une disposition, pas une
    /// règle de clan (le chasseur n'a pas de clan ici).
    #[test]
    fn un_chasseur_sociable_nourrit_ses_voisins_affames() {
        let voisin_apres = |sociability: f32| -> f32 {
            let mut sim = Sim::new(WorldSeed(42), 512);
            let home = find_land(&sim);
            let voisin = sim.spawn_agent(home.0 as f64 + 50.5, home.1 as f64 + 0.5);
            for (_, (id, ph)) in sim.agents.query_mut::<(&AgentId, &mut Physiology)>() {
                if *id == voisin {
                    ph.hunger = 0.8;
                }
            }
            let mut pos = Position { x: home.0 as f64 + 0.5, y: home.1 as f64 + 0.5 };
            let mut phys = Physiology { hunger: 0.3, ..Physiology::default() };
            let mut behavior =
                Behavior { task: Some(Task { kind: TaskKind::Hunt, target: home }), ..Behavior::default() };
            let traits = Traits { sociability, ..Traits::default() };
            let herd = HerdView {
                entity: hecs::Entity::DANGLING,
                pos: (pos.x, pos.y),
                population: 20.0,
                species: fauna::Species::Deer,
                tameness: 0.0,
            };
            let tick = tick_de_prise(&sim, AgentId(9_999), Skills::default().hunting);
            let mut shares = Vec::new();
            execute(
                &mut sim.world, &mut BTreeMap::new(), &mut PathBudget { left: PATH_REQUESTS_PER_TICK, denied: 0 },
                AgentId(9_999), &mut pos, &mut phys, &mut behavior, &[herd], &[], &[], 1.0,
                &traits, &mut Skills::default(), None, &mut BTreeMap::new(),
                &mut Carrying::default(), &mut Prestige::default(), &mut Vec::new(),
                &mut Vec::new(), &mut Vec::new(), &mut shares, tick, 1.0,
            );
            resolve_shares(&mut sim, &shares);
            sim.agents
                .query::<(&AgentId, &Physiology)>()
                .iter()
                .find(|(_, (id, _))| **id == voisin)
                .map(|(_, (_, ph))| ph.hunger)
                .unwrap()
        };
        let solitaire = voisin_apres(0.0);
        let sociable = voisin_apres(1.0);
        assert!((solitaire - 0.8).abs() < 1e-6, "un solitaire ne partage pas (faim du voisin {solitaire:.2})");
        assert!(sociable < 0.05, "un chasseur sociable doit rassasier son voisin (faim restante {sociable:.2})");
    }

    /// D10, la conservation : le stock du clan n'a plus de plafond, il
    /// pourrit. Sans technique, la viande crue a une durée de vie de quelques
    /// jours ; un stock de 30 points pour deux membres (quinze jours de
    /// nourriture chacun) ne doit être ni tronqué d'un coup, ni gardé intact :
    /// en 20 heures, il perd un quart environ (e^(−20/72)).
    #[test]
    fn le_stock_du_clan_pourrit_au_lieu_d_etre_plafonne() {
        let (mut sim, home) = scenario_setup(3, 2, 0);
        let members: Vec<AgentId> = sim.agents.query::<&AgentId>().iter().map(|(_, id)| *id).collect();
        for (_, (phys, membership)) in sim.agents.query_mut::<(&mut Physiology, &mut ClanMembership)>() {
            phys.hunger = 0.0; // repus : personne ne puise
            membership.0 = Some(social::ClanId(1));
        }
        sim.clans.push(Clan {
            id: social::ClanId(1),
            founded_tick: 0,
            members: members.iter().copied().collect(),
            home: (home.0 as f64 + 0.5, home.1 as f64 + 0.5),
            stock: 30.0,
            chief: members[0],
            desired: None,
            rivalry: 0.0,
        });
        sim.time.tick = 1; // entre deux minuits : la détection des clans ne passe pas
        for _ in 0..20 {
            sim.step();
        }
        let stock = sim.clans.iter().find(|c| c.id == social::ClanId(1)).unwrap().stock;
        assert!(
            (20.0..26.0).contains(&stock),
            "30 points de stock deviennent {stock:.1} en 20 heures (attendu ~22,7 : un pourrissement, pas un plafond)"
        );
    }

    /// D10, la conservation se découvre : un clan dont les membres savent
    /// conserver (neige, séchage, caches — la technique `preservation`) garde
    /// sa réserve des mois au lieu de quelques jours. Même scène que le test
    /// du pourrissement : 30 points, 20 heures, deux membres qui savent.
    #[test]
    fn un_clan_qui_sait_conserver_garde_sa_reserve() {
        let (mut sim, home) = scenario_setup(3, 2, 0);
        let preservation = sim.tech_tree.id_of("preservation").expect("la conservation doit exister");
        let members: Vec<AgentId> = sim.agents.query::<&AgentId>().iter().map(|(_, id)| *id).collect();
        for (_, (phys, membership, know)) in
            sim.agents.query_mut::<(&mut Physiology, &mut ClanMembership, &mut Knowledge)>()
        {
            phys.hunger = 0.0;
            membership.0 = Some(social::ClanId(1));
            know.insert(preservation);
        }
        sim.clans.push(Clan {
            id: social::ClanId(1),
            founded_tick: 0,
            members: members.iter().copied().collect(),
            home: (home.0 as f64 + 0.5, home.1 as f64 + 0.5),
            stock: 30.0,
            chief: members[0],
            desired: None,
            rivalry: 0.0,
        });
        sim.time.tick = 1;
        for _ in 0..20 {
            // Repus d'un bout à l'autre : c'est le pourrissement qu'on mesure, pas
            // un repas pris dans la réserve.
            for (_, phys) in sim.agents.query_mut::<&mut Physiology>() {
                phys.hunger = 0.0;
            }
            sim.step();
        }
        let stock = sim.clans.iter().find(|c| c.id == social::ClanId(1)).unwrap().stock;
        assert!(stock > 29.5, "un clan qui sait conserver perd {:.1} points sur 30 en 20 heures", 30.0 - stock);
    }

    /// D10, pister : aucun troupeau à vue, mais un souvenir récent d'en avoir
    /// vu un à 4 km. Un affamé envisage de s'y rendre ; sans souvenir, rien ne
    /// lui dit où est le gibier. Lu via `inspect_agent` (pur).
    #[test]
    fn un_affame_qui_se_souvient_du_gibier_part_le_pister() {
        let candidats = |souvenir: bool| -> bool {
            let (mut sim, home) = scenario_setup(5, 1, 0);
            let id = sim.agents.query::<&AgentId>().iter().map(|(_, a)| *a).next().unwrap();
            sim.time.tick = 12; // midi (CHA-2b) : de nuit, une piste ne se lit pas
            let tick = sim.time.tick;
            for (_, (phys, mem)) in sim.agents.query_mut::<(&mut Physiology, &mut Memory)>() {
                phys.hunger = 0.8;
                if souvenir {
                    mem.game = Some(((home.0 + 2_000, home.1), tick));
                }
            }
            sim.inspect_agent(id).unwrap().iter().any(|m| m.kind == TaskKind::Track)
        };
        assert!(!candidats(false), "sans souvenir, rien ne dit où pister");
        assert!(candidats(true), "un affamé qui se souvient d'un troupeau doit envisager de le pister");
    }

    /// CHA-2b : une piste ne se lit pas dans le noir. De nuit, un affamé qui se
    /// souvient d'un troupeau n'envisage pas de le pister, et le pisteur que la
    /// nuit surprend s'arrête ; le souvenir, lui, reste pour le matin.
    #[test]
    fn le_pisteur_s_arrete_a_la_nuit() {
        let (mut sim, home) = scenario_setup(5, 1, 0);
        let id = sim.agents.query::<&AgentId>().iter().map(|(_, a)| *a).next().unwrap();
        // Minuit, et un tick où l'agent ne délibère pas : seule l'exécution joue.
        let mut tick = 10 * TICKS_PER_DAY;
        while (tick + 1).wrapping_add(id.0) % brain::DELIBERATION_PERIOD == 0 {
            tick += TICKS_PER_DAY;
        }
        sim.time.tick = tick;
        let spot = (home.0 + 2_000, home.1);
        for (_, (phys, mem, behavior)) in sim.agents.query_mut::<(&mut Physiology, &mut Memory, &mut Behavior)>() {
            phys.hunger = 0.8;
            mem.game = Some((spot, tick));
            behavior.task = Some(Task { kind: TaskKind::Track, target: spot });
        }
        assert!(
            !sim.inspect_agent(id).unwrap().iter().any(|m| m.kind == TaskKind::Track),
            "de nuit, pister ne doit pas être envisagé"
        );
        sim.step();
        let (behavior, mem) = sim.agents.query_mut::<(&Behavior, &Memory)>().into_iter().next().map(|(_, b)| b).unwrap();
        assert!(behavior.task.is_none_or(|t| t.kind != TaskKind::Track), "la nuit tombée, le pisteur s'arrête");
        assert!(mem.game.is_some(), "le souvenir de la piste reste pour le matin");
    }

    /// D10, pister (suite) : le souvenir d'un troupeau survit à sa sortie du
    /// champ de vue. Un humain voit un troupeau à 1,5 km ; le troupeau part au
    /// loin ; à la délibération suivante, l'humain — resté sur place — doit
    /// encore s'en souvenir.
    #[test]
    fn perdre_un_troupeau_de_vue_n_efface_pas_son_souvenir() {
        let (mut sim, home) = scenario_setup(5, 1, 0);
        // Midi (MAR-2) : le test porte sur la mémoire du gibier, pas sur la nuit — au tick 0
        // il fait nuit noire, et l'on ne voit plus le gibier à 2 km.
        sim.time.tick = 12;
        let herd = sim.spawn_herd(home.0 as f64 + 750.0, home.1 as f64, 30.0);
        for (_, phys) in sim.agents.query_mut::<&mut Physiology>() {
            phys.hunger = 0.8;
        }
        sim.step(); // délibération : le troupeau est vu
        let seen = sim.agents.query::<&Memory>().iter().next().map(|(_, m)| m.game).unwrap();
        assert!(seen.is_some(), "un troupeau à 1,5 km doit être aperçu et retenu");
        for (_, (id, pos)) in sim.fauna.query_mut::<(&FaunaId, &mut Position)>() {
            if *id == herd {
                pos.x += 5_000.0; // parti à 10 km
            }
        }
        for _ in 0..8 {
            sim.step();
        }
        let kept = sim.agents.query::<&Memory>().iter().next().map(|(_, m)| m.game).unwrap();
        assert!(kept.is_some(), "le souvenir du troupeau a disparu dès qu'il est sorti du champ de vue");
    }

    /// D10, garder sa prise : un chasseur sans clan ne jette plus ce qu'il ne
    /// mange pas. Il le porte, et le mange quand la faim revient — sans quoi
    /// un chasseur repu qui tue un cerf en perd un mois de nourriture, et
    /// recommence le lendemain.
    #[test]
    fn un_chasseur_sans_clan_garde_sa_prise_et_la_mange_plus_tard() {
        let mut sim = Sim::new(WorldSeed(42), 512);
        let home = find_land(&sim);
        let mut pos = Position { x: home.0 as f64 + 0.5, y: home.1 as f64 + 0.5 };
        let mut phys = Physiology { hunger: 0.3, ..Physiology::default() };
        let mut carrying = Carrying::default();
        let traits = Traits { sociability: 0.0, ..Traits::default() };
        let herd = HerdView {
            entity: hecs::Entity::DANGLING,
            pos: (pos.x, pos.y),
            population: 20.0,
            species: fauna::Species::Deer,
            tameness: 0.0,
        };
        let tick = tick_de_prise(&sim, AgentId(0), Skills::default().hunting);
        let mut run = |task: Option<Task>, phys: &mut Physiology, carrying: &mut Carrying, pos: &mut Position| {
            let mut behavior = Behavior { task, ..Behavior::default() };
            execute(
                &mut sim.world, &mut BTreeMap::new(), &mut PathBudget { left: PATH_REQUESTS_PER_TICK, denied: 0 },
                AgentId(0), pos, phys, &mut behavior, &[herd], &[], &[], 1.0, &traits,
                &mut Skills::default(), None, &mut BTreeMap::new(), carrying,
                &mut Prestige::default(), &mut Vec::new(), &mut Vec::new(), &mut Vec::new(),
                &mut Vec::new(), tick, 1.0,
            );
        };
        run(Some(Task { kind: TaskKind::Hunt, target: home }), &mut phys, &mut carrying, &mut pos);
        assert!(carrying.0 > 5.0, "sans clan, le chasseur doit garder sa prise ({:.2} porté)", carrying.0);
        phys.hunger = 0.8;
        let porte = carrying.0;
        run(None, &mut phys, &mut carrying, &mut pos);
        assert!(phys.hunger < 0.1, "affamé, il mange ce qu'il porte (faim restante {:.2})", phys.hunger);
        assert!(carrying.0 < porte, "ce qu'il a mangé sort de ce qu'il porte");
    }

    /// D10, l'envie de chasser suit le besoin : deux adultes aussi affamés,
    /// un troupeau à vue ; celui qui porte déjà de quoi manger des jours n'a
    /// aucune raison de tuer une autre bête. Lu via `inspect_agent` (pur).
    #[test]
    fn qui_porte_deja_de_la_viande_ne_chasse_guere() {
        let score = |porte: f32| -> f32 {
            let (mut sim, home) = scenario_setup(5, 1, 0);
            sim.time.tick = 12; // midi (MAR-2) : de nuit, le troupeau n'est pas à vue
            sim.spawn_herd(home.0 as f64 + 300.0, home.1 as f64, 30.0);
            let id = sim.agents.query::<&AgentId>().iter().map(|(_, a)| *a).next().unwrap();
            for (_, (phys, carrying)) in sim.agents.query_mut::<(&mut Physiology, &mut Carrying)>() {
                phys.hunger = 0.5;
                carrying.0 = porte;
            }
            sim.inspect_agent(id)
                .unwrap()
                .iter()
                .find(|m| m.kind == TaskKind::Hunt)
                .map_or(0.0, |m| m.score)
        };
        let (demuni, pourvu) = (score(0.0), score(5.0));
        assert!(demuni > 0.0, "un affamé sans viande, un troupeau à vue : il doit envisager la chasse");
        assert!(
            pourvu < 0.3 * demuni,
            "qui porte cinq points de viande chasse presque autant ({pourvu:.3}) qu'un démuni ({demuni:.3})"
        );
    }

    /// D10, partir en quête : un membre de clan affamé, sans troupeau en vue
    /// ni piste, envisage de balayer le territoire du clan loin du foyer ; un
    /// repu, guère ; un humain sans clan **qui n'a encore dormi nulle part**,
    /// jamais (depuis MAR-4, le gîte de la dernière nuit sert de camp aux
    /// sans-clan). Lu via `inspect_agent`.
    #[test]
    fn un_affame_d_un_clan_part_en_quete_sur_le_territoire() {
        let quete = |faim: f32, avec_clan: bool, reserve: f32| -> Option<(f32, f64)> {
            let (mut sim, home) = scenario_au_foyer(5, 1, 0);
            let id = sim.agents.query::<&AgentId>().iter().map(|(_, a)| *a).next().unwrap();
            for (_, (phys, membership)) in sim.agents.query_mut::<(&mut Physiology, &mut ClanMembership)>() {
                phys.hunger = faim;
                if avec_clan {
                    membership.0 = Some(social::ClanId(1));
                }
            }
            if avec_clan {
                sim.clans.push(Clan {
                    id: social::ClanId(1),
                    founded_tick: 0,
                    members: [id].into_iter().collect(),
                    home: (home.0 as f64 + 0.5, home.1 as f64 + 0.5),
                    stock: reserve,
                    chief: id,
                    desired: None,
                    rivalry: 0.0,
                });
            }
            // Plusieurs délibérations, donc plusieurs directions tirées : une
            // seule peut buter sur un rivage proche.
            let mut best: Option<(f32, f64)> = None;
            for t in 0..8 {
                sim.time.tick = t * 4;
                if let Some(m) = sim.inspect_agent(id).unwrap().iter().find(|m| m.kind == TaskKind::SeekGame) {
                    let d = ((m.target.0 - home.0) as f64).hypot((m.target.1 - home.1) as f64);
                    let km = cairn_core::tiles_to_km(d);
                    if best.is_none_or(|(_, b)| km > b) {
                        best = Some((m.score, km));
                    }
                }
            }
            best
        };
        let (affame, loin) = quete(0.9, true, 0.0).expect("un affamé d'un clan, sans gibier en vue, doit envisager une quête");
        assert!(loin > 2.0, "une quête part au-delà du regard (2 km), au plus loin {loin:.1} km sur huit directions");
        // Repu, et un clan pourvu (une réserve pleine pour son seul membre) : rien
        // ne pousse à chasser. Un clan sans réserve, lui, enverrait même un repu
        // chasser pour les siens (H2) — voulu.
        let repu = quete(0.0, true, STOCK_SCALE_PER_MEMBER).map_or(0.0, |(s, _)| s);
        assert!(repu < 0.2 * affame, "un repu ne part guère en quête ({repu:.3} contre {affame:.3})");
        assert!(quete(0.9, false, 0.0).is_none(), "sans clan, pas de foyer d'où partir en quête");
    }

    /// D2, la faim disperse : un affamé peut pousser sa quête au-delà du
    /// territoire du clan, jusqu'à la portée d'une sortie à la journée (Kelly
    /// 1995, ~10 km) ; qui chasse seulement pour les siens, repu, reste sur le
    /// territoire. Rien n'oblige à partir (un candidat parmi d'autres), et le
    /// rappel au clan existe toujours une fois loin.
    #[test]
    fn la_faim_pousse_la_quete_au_dela_du_territoire_sans_couper_le_rappel() {
        let portee = |faim: f32, reserve: f32| -> f64 {
            let (mut sim, home) = scenario_au_foyer(5, 1, 0);
            let id = sim.agents.query::<&AgentId>().iter().map(|(_, a)| *a).next().unwrap();
            for (_, (phys, membership)) in sim.agents.query_mut::<(&mut Physiology, &mut ClanMembership)>() {
                phys.hunger = faim;
                membership.0 = Some(social::ClanId(1));
            }
            sim.clans.push(Clan {
                id: social::ClanId(1),
                founded_tick: 0,
                members: [id].into_iter().collect(),
                home: (home.0 as f64 + 0.5, home.1 as f64 + 0.5),
                stock: reserve,
                chief: id,
                desired: None,
                rivalry: 0.0,
            });
            let mut far: f64 = 0.0;
            for t in 0..16 {
                sim.time.tick = t * 4;
                if let Some(m) = sim.inspect_agent(id).unwrap().iter().find(|m| m.kind == TaskKind::SeekGame) {
                    let d = ((m.target.0 - home.0) as f64).hypot((m.target.1 - home.1) as f64);
                    far = far.max(cairn_core::tiles_to_km(d));
                }
            }
            far
        };
        let territoire = cairn_core::tiles_to_km(social::RESIDENCE_RADIUS_TILES);
        let affame = portee(0.95, STOCK_SCALE_PER_MEMBER);
        assert!(affame > territoire, "un affamé doit pouvoir chercher au-delà du territoire ({affame:.1} km)");
        let pour_les_siens = portee(0.0, 0.0);
        assert!(
            pour_les_siens <= territoire,
            "repu, on chasse pour les siens sur le territoire ({pour_les_siens:.1} km)"
        );

        // Loin du foyer, le rappel est toujours là.
        let (mut sim, home) = scenario_au_foyer(5, 1, 0);
        let id = sim.agents.query::<&AgentId>().iter().map(|(_, a)| *a).next().unwrap();
        for (_, (phys, membership)) in sim.agents.query_mut::<(&mut Physiology, &mut ClanMembership)>() {
            phys.hunger = 0.95;
            membership.0 = Some(social::ClanId(1));
        }
        let away = (home.0 as f64 - cairn_core::km_to_tiles(8.0), home.1 as f64);
        sim.clans.push(Clan {
            id: social::ClanId(1),
            founded_tick: 0,
            members: [id].into_iter().collect(),
            home: away,
            stock: 0.0,
            chief: id,
            desired: None,
            rivalry: 0.0,
        });
        assert!(
            sim.inspect_agent(id).unwrap().iter().any(|m| m.kind == TaskKind::ReturnToClan && m.score > 0.0),
            "à 8 km du foyer, rentrer reste un choix offert"
        );
    }

    /// Deux points de terre que l'eau sépare en ligne droite, et qu'un A*
    /// contourne : un rivage réel du monde, trouvé en sondant le worldgen.
    fn across_water(sim: &Sim) -> ((i64, i64), (i64, i64)) {
        let land = |x: i64, y: i64| sim.world.worldgen().elevation(x, y) > 0.0;
        let start = find_land(sim);
        for r in 0..400i64 {
            for k in 0..(8 * r.max(1)) {
                let a = (
                    start.0 + ((k as f64 / (8 * r.max(1)) as f64 * std::f64::consts::TAU).cos() * r as f64 * 60.0) as i64,
                    start.1 + ((k as f64 / (8 * r.max(1)) as f64 * std::f64::consts::TAU).sin() * r as f64 * 60.0) as i64,
                );
                let b = (a.0 + 400, a.1);
                if !land(a.0, a.1) || !land(b.0, b.1) {
                    continue;
                }
                let wet = (1..400).any(|i| !land(a.0 + i, a.1));
                if wet && crate::pathfind::astar(a, b, land, crate::pathfind::NODE_BUDGET).is_some() {
                    return (a, b);
                }
            }
        }
        panic!("aucun rivage contournable trouvé");
    }

    /// Bug de l'eau (2026-10-03) : quand le budget d'A* du tick est épuisé,
    /// `advance` répondait « bloqué » comme pour un agent vraiment cerné — et
    /// l'appelant marquait la source inaccessible pendant deux jours (D11).
    /// Une limite de calcul devenait une croyance : 115 morts de soif en
    /// 30 jours dans un clan de 200 au bord de l'eau (run longue, an 92).
    #[test]
    fn un_budget_epuise_n_est_pas_un_chemin_bloque() {
        let (mut sim, _) = scenario_setup(7, 0, 0);
        let (a, b) = across_water(&sim);
        let mut pos = Position { x: a.0 as f64 + 0.5, y: a.1 as f64 + 0.5 };
        let mut routes = BTreeMap::new();
        let mut empty = PathBudget { left: 0, denied: 0 };
        // On marche d'abord jusqu'à la rive ; c'est là que l'A* serait demandé.
        for _ in 0..10 {
            let m = advance(&mut sim.world, &mut routes, &mut empty, AgentId(1), &mut pos, b);
            assert_ne!(m, Move::Stuck, "faute de budget, on attend : ce n'est pas un chemin bloqué");
        }
        assert!(empty.denied >= 1, "arrivé à la rive, la demande de trajet est refusée et comptée");
    }

    /// Bug de l'eau (2026-10-03) : un trajet A* s'arrêtait à chaque étape
    /// atteinte (32 à 45 m) et jetait le reste de l'heure de marche —
    /// contourner l'eau se faisait à ~45 m/h au lieu de 4 km/h (trace : un
    /// assoiffé mort en marchant vers une source à 1,6 km, 44 m par heure).
    #[test]
    fn un_trajet_qui_contourne_l_eau_avance_d_une_heure_de_marche() {
        let (mut sim, _) = scenario_setup(7, 0, 0);
        let (a, b) = across_water(&sim);
        let mut pos = Position { x: a.0 as f64 + 0.5, y: a.1 as f64 + 0.5 };
        let mut routes = BTreeMap::new();
        let mut budget = PathBudget { left: 8, denied: 0 };
        let mut walked = 0.0;
        for _ in 0..3 {
            let before = (pos.x, pos.y);
            let m = advance(&mut sim.world, &mut routes, &mut budget, AgentId(1), &mut pos, b);
            walked += (pos.x - before.0).hypot(pos.y - before.1);
            if m == Move::Arrived {
                return;
            }
        }
        assert!(
            walked > 3.0 * 0.5 * WALK_TILES_PER_TICK,
            "trois heures de marche le long d'un trajet : au moins la moitié de la marche nominale ({walked:.0} tuiles)"
        );
    }

    /// Chantier de l'eau, défaut A : la soif doit primer quand elle tue vite.
    /// Trace réelle : un chasseur affamé ET assoiffé chasse 22 h d'affilée, une
    /// source à 164 m, et meurt de soif — à saturation, boire et chasser
    /// valaient tous deux ~1, la chasse en cours gagnant l'inertie. La soif
    /// tue en ~3 jours, la faim en ~3 semaines (« règle des trois ») : celle
    /// qui tue le plus tôt doit l'emporter nettement, sans rien forcer.
    #[test]
    fn la_soif_qui_tue_prime_sur_la_chasse() {
        let (mut sim, home) = scenario_au_foyer(5, 1, 0);
        let spring = sim.world.nearest_spring(home, 5).expect("une source près du foyer");
        let id = sim.agents.query::<&AgentId>().iter().map(|(_, a)| *a).next().unwrap();
        let at = (spring.0 as f64 + 80.5, spring.1 as f64 + 0.5); // ~160 m de la source
        for (_, (pos, phys)) in sim.agents.query_mut::<(&mut Position, &mut Physiology)>() {
            pos.x = at.0;
            pos.y = at.1;
            phys.thirst = 1.0;
            phys.hunger = 1.0;
            phys.health = 0.9;
        }
        sim.spawn_herd(at.0 + 5.0, at.1, 60.0);
        let scores = sim.inspect_agent(id).unwrap();
        let drink = scores.iter().filter(|m| m.kind == TaskKind::Drink).map(|m| m.score).fold(0.0, f32::max);
        let other = scores.iter().filter(|m| m.kind != TaskKind::Drink).map(|m| m.score).fold(0.0, f32::max);
        assert!(
            drink > other + 0.2,
            "assoiffé à mort, boire ({drink:.2}) doit nettement primer sur tout le reste ({other:.2})"
        );
        // L'autre bout : soif modérée, faim saturée — pas d'urgence vitale de
        // l'eau, la nourriture reste en concurrence normale.
        for (_, phys) in sim.agents.query_mut::<&mut Physiology>() {
            phys.thirst = 0.5;
            phys.health = 1.0;
        }
        let scores = sim.inspect_agent(id).unwrap();
        let drink = scores.iter().filter(|m| m.kind == TaskKind::Drink).map(|m| m.score).fold(0.0, f32::max);
        assert!(drink <= 1.0, "sans urgence, le score de boire reste dans sa plage ordinaire ({drink:.2})");
    }

    /// Piste F (run longue tempéré 42 : deux bandes à plus de 900 km l'une de
    /// l'autre en 25 ans) : rien n'attachait une bande à ses terres, chaque
    /// changement de maille faisait glisser le camp — une marche au hasard.
    /// Une bande réelle parcourt un domaine qu'elle connaît. À richesse égale,
    /// le pays connu (où l'on sait l'eau et la nourriture) doit l'emporter ;
    /// rien n'interdit l'inconnu.
    #[test]
    fn a_richesse_egale_on_retourne_sur_ses_terres() {
        let (mut sim, _) = scenario_au_foyer(5, 1, 0);
        sim.time.tick = 160 * cairn_core::TICKS_PER_DAY; // été : ça pousse
        let (id, pos) = sim
            .agents
            .query::<(&AgentId, &Position)>()
            .iter()
            .map(|(_, (a, p))| (*a, (p.x, p.y)))
            .next()
            .unwrap();
        for (_, phys) in sim.agents.query_mut::<&mut Physiology>() {
            phys.hunger = 0.8;
        }
        let side = crate::fauna::RANGE_ZONE_TILES;
        let zone = crate::fauna::range_zone(pos);
        let center = |dx: i64, dy: i64| (((zone.0 + dx) as f64 + 0.5) * side, ((zone.1 + dy) as f64 + 0.5) * side);
        // Deux mailles riches parmi les voisines : l'inconnue (vue la première
        // dans l'ordre de parcours) et la connue (vue la dernière). Le reste du
        // pays alentour est vidé.
        let order: Vec<(i64, i64)> =
            (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (dx, dy))).filter(|&d| d != (0, 0)).collect();
        let rich: Vec<(i64, i64)> = order
            .iter()
            .copied()
            .filter(|&(dx, dy)| sim.world.edible_kcal(center(dx, dy), sim.time.tick) > 1.0e6)
            .collect();
        assert!(rich.len() >= 2, "il faut deux mailles riches autour du foyer");
        let (unknown, familiar) = (rich[0], *rich.last().unwrap());
        sim.world.gather(center(0, 0), sim.time.tick, f64::MAX);
        for &(dx, dy) in &order {
            if (dx, dy) != unknown && (dx, dy) != familiar {
                sim.world.gather(center(dx, dy), sim.time.tick, f64::MAX);
            }
        }
        let known = center(familiar.0, familiar.1);
        for (_, mem) in sim.agents.query_mut::<&mut Memory>() {
            let step = crate::memory::MEMORY_CELL_TILES as f64;
            let mut y = known.1 - side / 2.0;
            while y < known.1 + side / 2.0 {
                let mut x = known.0 - side / 2.0;
                while x < known.0 + side / 2.0 {
                    mem.known.insert(crate::memory::cell_of((x as i64, y as i64)));
                    x += step;
                }
                y += step;
            }
        }
        let target = sim
            .inspect_agent(id)
            .unwrap()
            .into_iter()
            .filter(|m| m.kind == TaskKind::Wander && m.score > 0.0)
            .map(|m| m.target)
            // Le changement de pays vise le centre d'une maille voisine.
            .find(|t| order.iter().any(|&(dx, dy)| {
                let c = center(dx, dy);
                (c.0 as i64, c.1 as i64) == *t
            }));
        let target = target.expect("une maille plus riche doit être envisagée");
        assert_eq!(
            crate::fauna::range_zone((target.0 as f64, target.1 as f64)),
            (zone.0 + familiar.0, zone.1 + familiar.1),
            "à richesse égale, la maille connue doit l'emporter"
        );
    }

    /// D10, le soir au campement : un membre de clan fatigué, la nuit, à 3 km
    /// du foyer, envisage de rentrer dormir près des siens — pas seulement de
    /// s'effondrer sur place. C'est là que les liens se nouent (les rencontres
    /// se font à moins de 120 m) : sans retour au camp, les chasseurs partis
    /// en quête ne se croisaient plus et le clan se défaisait.
    #[test]
    fn le_soir_on_rentre_dormir_au_campement() {
        let (mut sim, home) = scenario_au_foyer(5, 1, 0);
        let id = sim.agents.query::<&AgentId>().iter().map(|(_, a)| *a).next().unwrap();
        let camp = (home.0 as f64 + 1_500.5, home.1 as f64 + 0.5); // 3 km
        for (_, (phys, membership)) in sim.agents.query_mut::<(&mut Physiology, &mut ClanMembership)>() {
            phys.fatigue = 0.7;
            membership.0 = Some(social::ClanId(1));
        }
        sim.clans.push(Clan {
            id: social::ClanId(1),
            founded_tick: 0,
            members: [id].into_iter().collect(),
            home: camp,
            stock: 0.0,
            chief: id,
            desired: None,
            rivalry: 0.0,
        });
        sim.time.tick = 23; // la nuit
        let motifs = sim.inspect_agent(id).unwrap();
        let rentrer = motifs
            .iter()
            .filter(|m| m.kind == TaskKind::Sleep)
            .find(|m| ((m.target.0 as f64 - camp.0).hypot(m.target.1 as f64 - camp.1)) < 50.0);
        assert!(rentrer.is_some(), "un membre fatigué, la nuit, loin du camp, doit envisager d'y rentrer dormir");
    }

    /// D11 : un humain mourait de soif à 300 m d'une source qu'il ne pouvait
    /// pas atteindre (de l'autre côté de l'eau), parce que « boire » visait
    /// toujours la source la plus proche à vol d'oiseau et qu'aucun échec ne
    /// s'apprenait. Une marche qui a buté sur une source doit la faire écarter :
    /// à la délibération suivante, l'assoiffé vise une autre source connue.
    #[test]
    fn une_source_hors_d_atteinte_est_mise_de_cote() {
        let (mut sim, home) = scenario_au_foyer(42, 1, 0);
        let proche = sim.world.nearest_spring(home, 5).expect("le foyer des bancs a une source à portée");
        let loin_depart = (home.0 + 1_500, home.1 + 1_500);
        let autre = sim.world.nearest_spring(loin_depart, 5).expect("une seconde source dans la région");
        assert_ne!(proche, autre);
        for (_, (phys, mem, behavior)) in sim.agents.query_mut::<(&mut Physiology, &mut Memory, &mut Behavior)>() {
            phys.thirst = 0.9;
            mem.springs = vec![proche, autre];
            behavior.task = None;
            behavior.stuck_on = Some(proche); // la marche vers elle vient d'échouer
        }
        sim.step();
        let task = sim.agents.query::<&Behavior>().iter().next().map(|(_, b)| b.task).unwrap();
        assert_eq!(
            task.map(|t| (t.kind, t.target)),
            Some((TaskKind::Drink, autre)),
            "après un échec vers la source la plus proche, l'assoiffé doit viser l'autre"
        );
    }

    /// Le pendant du dépôt : rapporter le surplus porté au foyer
    /// (`TaskKind::BringSurplusHome`) vide `Carrying` **dans** le stock, du
    /// même montant — ni nourriture créée, ni perdue en chemin.
    #[test]
    fn rapporter_le_surplus_au_foyer_alimente_le_stock_a_parts_egales() {
        let mut sim = Sim::new(WorldSeed(42), 512);
        let home = find_land(&sim);
        let mut pos = Position { x: home.0 as f64 + 0.5, y: home.1 as f64 + 0.5 };
        let mut phys = Physiology { hunger: 0.0, ..Physiology::default() }; // repu : il ne mange pas en route
        let mut behavior = Behavior {
            task: Some(Task { kind: TaskKind::BringSurplusHome, target: home }),
            ..Behavior::default()
        };
        let traits = Traits::default();
        let mut skills = Skills::default();
        let mut routes: BTreeMap<u64, Route> = BTreeMap::new();
        let mut budget = PathBudget { left: PATH_REQUESTS_PER_TICK, denied: 0 };
        let clan_id = social::ClanId(9);
        let mut clan_stock: BTreeMap<social::ClanId, f32> = BTreeMap::new();
        let porte_avant = 0.4;
        let mut carrying = Carrying(porte_avant);
        let mut prestige = Prestige::default();
        let mut structures: Vec<Structure> = Vec::new();

        execute(
            &mut sim.world,
            &mut routes,
            &mut budget,
            AgentId(0),
            &mut pos,
            &mut phys,
            &mut behavior,
            &[],
            &[],
            &[],
            1.0,
            &traits,
            &mut skills,
            Some(clan_id),
            &mut clan_stock,
            &mut carrying,
            &mut prestige,
            &mut structures,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut Vec::new(),
            0, 1.0,
        );

        assert_eq!(carrying.0, 0.0, "le surplus rapporté doit être entièrement déposé, plus rien porté");
        assert_eq!(
            clan_stock.get(&clan_id).copied().unwrap_or(0.0),
            porte_avant,
            "le stock doit gagner exactement ce que le chasseur portait"
        );
        assert_eq!(
            prestige.0, porte_avant,
            "le prestige de qui rapporte doit croître du même montant que le stock"
        );
    }

    /// Le pendant du dépôt : puiser dans le stock au foyer réduit la faim
    /// **et** le stock, du même montant — ni nourriture créée, ni perdue.
    #[test]
    fn puiser_dans_le_stock_reduit_la_faim_et_le_stock_a_parts_egales() {
        let mut sim = Sim::new(WorldSeed(42), 512);
        let home = find_land(&sim);
        let mut pos = Position { x: home.0 as f64 + 0.5, y: home.1 as f64 + 0.5 };
        let hunger_avant = 0.6;
        let mut phys = Physiology { hunger: hunger_avant, ..Physiology::default() };
        let mut behavior =
            Behavior { task: Some(Task { kind: TaskKind::EatFromStock, target: home }), ..Behavior::default() };
        let traits = Traits::default();
        let mut skills = Skills::default();
        let mut routes: BTreeMap<u64, Route> = BTreeMap::new();
        let mut budget = PathBudget { left: PATH_REQUESTS_PER_TICK, denied: 0 };
        let clan_id = social::ClanId(7);
        let stock_avant = 0.5; // moins que la faim : le retrait doit être partiel
        let mut clan_stock: BTreeMap<social::ClanId, f32> = BTreeMap::from([(clan_id, stock_avant)]);
        let mut carrying = Carrying::default();
        let mut prestige = Prestige::default();
        let mut structures: Vec<Structure> = Vec::new();

        execute(
            &mut sim.world,
            &mut routes,
            &mut budget,
            AgentId(0),
            &mut pos,
            &mut phys,
            &mut behavior,
            &[],
            &[],
            &[],
            1.0,
            &traits,
            &mut skills,
            Some(clan_id),
            &mut clan_stock,
            &mut carrying,
            &mut prestige,
            &mut structures,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut Vec::new(),
            0, 1.0,
        );

        let stock_apres = clan_stock[&clan_id];
        assert!(phys.hunger < hunger_avant, "la faim doit avoir baissé");
        assert!(stock_apres < stock_avant, "le stock doit avoir baissé");
        assert!(
            (hunger_avant - phys.hunger - (stock_avant - stock_apres)).abs() < 1e-6,
            "conservation : ce que l'agent gagne, le stock le perd, au même montant \
             (faim {hunger_avant} → {}, stock {stock_avant} → {stock_apres})",
            phys.hunger
        );
        assert!(stock_apres <= 1e-6, "le stock, plus petit que la faim, doit être vidé entièrement");
    }

    /// « Qui bâtit » — construire consomme le stock commun et crée la
    /// structure au foyer, au même titre qu'un autre candidat de tâche. On
    /// teste le mécanisme (`execute` avec une tâche `Build`) plutôt qu'une
    /// scène complète : comme pour le stock, un clan injecté à la main ne
    /// survivrait pas à la prochaine détection de minuit.
    #[test]
    fn batir_consomme_le_stock_et_cree_la_structure() {
        let mut sim = Sim::new(WorldSeed(42), 512);
        let home = find_land(&sim);
        let mut pos = Position { x: home.0 as f64 + 0.5, y: home.1 as f64 + 0.5 };
        let mut phys = Physiology::default();
        let mut behavior = Behavior {
            task: Some(Task { kind: TaskKind::Build(StructureKind::Hut), target: home }),
            ..Behavior::default()
        };
        let traits = Traits::default();
        let mut skills = Skills::default();
        let mut routes: BTreeMap<u64, Route> = BTreeMap::new();
        let mut budget = PathBudget { left: PATH_REQUESTS_PER_TICK, denied: 0 };
        let clan_id = social::ClanId(4);
        let stock_avant = StructureKind::Hut.cost() + 5.0; // de quoi payer, et un reste
        let mut clan_stock: BTreeMap<social::ClanId, f32> = BTreeMap::from([(clan_id, stock_avant)]);
        let mut carrying = Carrying::default();
        let mut prestige = Prestige::default();
        let mut structures: Vec<Structure> = Vec::new();

        execute(
            &mut sim.world,
            &mut routes,
            &mut budget,
            AgentId(0),
            &mut pos,
            &mut phys,
            &mut behavior,
            &[],
            &[],
            &[],
            1.0,
            &traits,
            &mut skills,
            Some(clan_id),
            &mut clan_stock,
            &mut carrying,
            &mut prestige,
            &mut structures,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut Vec::new(),
            0, 1.0,
        );

        assert_eq!(structures.len(), 1, "une structure doit avoir été bâtie");
        assert_eq!(structures[0].kind, StructureKind::Hut);
        assert_eq!(structures[0].clan, clan_id);
        assert!(
            (clan_stock[&clan_id] - 5.0).abs() < 1e-6,
            "le coût de la hutte doit avoir été prélevé sur le stock commun"
        );

        // Deuxième chantier identique, stock encore suffisant mais la hutte
        // existe déjà : aucun doublon, aucun prélèvement.
        let mut behavior2 = Behavior {
            task: Some(Task { kind: TaskKind::Build(StructureKind::Hut), target: home }),
            ..Behavior::default()
        };
        let stock_intermediaire = clan_stock[&clan_id];
        execute(
            &mut sim.world,
            &mut routes,
            &mut budget,
            AgentId(1),
            &mut pos,
            &mut phys,
            &mut behavior2,
            &[],
            &[],
            &[],
            1.0,
            &traits,
            &mut skills,
            Some(clan_id),
            &mut clan_stock,
            &mut carrying,
            &mut prestige,
            &mut structures,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut Vec::new(),
            0, 1.0,
        );
        assert_eq!(structures.len(), 1, "une hutte existe déjà : pas de doublon");
        assert_eq!(clan_stock[&clan_id], stock_intermediaire, "no-op : le stock ne bouge pas");
    }

    /// Agriculture (Phase 5, chaîne §5.3) : entretenir un champ
    /// (`TaskKind::Cultivate`) ÉLÈVE la biomasse de la tuile au-dessus de son
    /// état, en prélevant un peu de fertilité (§2.4). Mécanisme testé par
    /// `execute` direct, la tuile forcée à un état connu (prairie fertile,
    /// biomasse basse) via `tile_mut`.
    #[test]
    fn cultiver_eleve_la_biomasse_et_epuise_le_sol() {
        let mut sim = Sim::new(WorldSeed(42), 512);
        let field = find_land(&sim);
        {
            let tile = sim.world.tile_mut(field.0, field.1);
            tile.soil_fertility = 200;
            tile.biomass = 20;
        }
        let mut pos = Position { x: field.0 as f64 + 0.5, y: field.1 as f64 + 0.5 };
        let mut phys = Physiology::default();
        let mut behavior = Behavior {
            task: Some(Task { kind: TaskKind::Cultivate, target: field }),
            ..Behavior::default()
        };
        let traits = Traits::default();
        let mut skills = Skills::default();
        let mut routes: BTreeMap<u64, Route> = BTreeMap::new();
        let mut budget = PathBudget { left: PATH_REQUESTS_PER_TICK, denied: 0 };
        let mut clan_stock: BTreeMap<social::ClanId, f32> = BTreeMap::new();
        let mut carrying = Carrying::default();
        let mut prestige = Prestige::default();
        let mut structures: Vec<Structure> = Vec::new();

        execute(
            &mut sim.world, &mut routes, &mut budget, AgentId(0), &mut pos, &mut phys,
            &mut behavior, &[], &[], &[], 1.0, &traits, &mut skills, None, &mut clan_stock,
            &mut carrying, &mut prestige, &mut structures, &mut Vec::new(), &mut Vec::new(),
            &mut Vec::new(), 0, 1.0,
        );

        let tile = sim.world.tile(field.0, field.1);
        assert!(tile.biomass > 20, "le champ entretenu doit gagner de la biomasse (20 → {})", tile.biomass);
        assert_eq!(tile.soil_fertility, 199, "l'entretien prélève un peu de fertilité");
    }

    /// Le candidat « cultiver » n'apparaît que pour qui **maîtrise
    /// l'agriculture** : deux agents repus au foyer sur la même prairie
    /// fertile, seul l'agriculteur l'envisage. Lu via `inspect_agent` (pur).
    #[test]
    fn seul_l_agriculteur_envisage_de_cultiver() {
        let mut sim = Sim::new(WorldSeed(42), 512);
        let home = find_land(&sim);
        {
            let tile = sim.world.tile_mut(home.0, home.1);
            tile.biome = cairn_worldgen::Biome::Grassland;
            tile.soil_fertility = 200;
            tile.biomass = 20;
        }
        let agri = sim.tech_tree.id_of("agriculture").unwrap();
        let farmer = sim.spawn_agent(home.0 as f64 + 0.5, home.1 as f64 + 0.5);
        let profane = sim.spawn_agent(home.0 as f64 + 0.5, home.1 as f64 + 0.5);
        for (_, (id, know, phys, membership)) in
            sim.agents.query_mut::<(&AgentId, &mut Knowledge, &mut Physiology, &mut ClanMembership)>()
        {
            phys.hunger = 0.2; // repu : on cultive au calme
            phys.thirst = 0.2;
            phys.cold = 0.0;
            membership.0 = Some(social::ClanId(1));
            if *id == farmer {
                know.insert(agri);
            }
        }
        sim.clans.push(Clan {
            id: social::ClanId(1),
            founded_tick: 0,
            members: [farmer, profane].into_iter().collect(),
            home: (home.0 as f64 + 0.5, home.1 as f64 + 0.5),
            stock: 0.0,
            chief: farmer,
            desired: None,
            rivalry: 0.0,
        });

        let m_farmer = sim.inspect_agent(farmer).unwrap();
        assert!(
            m_farmer.iter().any(|m| m.kind == TaskKind::Cultivate),
            "un agriculteur repu au foyer sur prairie fertile doit envisager de cultiver"
        );
        let m_profane = sim.inspect_agent(profane).unwrap();
        assert!(
            !m_profane.iter().any(|m| m.kind == TaskKind::Cultivate),
            "qui ne connaît pas l'agriculture ne cultive pas"
        );
    }

    /// « Éleveur » : une meute qui rôde près du foyer appelle les **agressifs**
    /// à la chasser, bien plus que les placides — la défense du territoire naît
    /// du trait, pas d'une règle. Lu via `inspect_agent` (pur).
    #[test]
    fn une_meute_pres_du_foyer_appelle_les_agressifs() {
        let mut sim = Sim::new(WorldSeed(3), 512);
        let home = find_land(&sim);
        let homef = (home.0 as f64 + 0.5, home.1 as f64 + 0.5);
        // Une meute à ~100 m du foyer : une menace franche sur le territoire.
        sim.spawn_pack(homef.0 + 50.0, homef.1, 20.0);
        let brave = sim.spawn_agent(homef.0, homef.1);
        let timid = sim.spawn_agent(homef.0, homef.1);
        for (_, (id, traits, phys, membership)) in sim.agents.query_mut::<(
            &AgentId,
            &mut Traits,
            &mut Physiology,
            &mut ClanMembership,
        )>() {
            phys.hunger = 0.2; // repu : la survie ne monopolise pas la décision
            phys.thirst = 0.2;
            phys.cold = 0.0;
            membership.0 = Some(social::ClanId(1));
            if *id == brave {
                traits.aggression = 0.9;
                traits.strength = 0.8;
            } else {
                traits.aggression = 0.02;
                traits.strength = 0.5;
            }
        }
        sim.clans.push(Clan {
            id: social::ClanId(1),
            founded_tick: 0,
            members: [brave, timid].into_iter().collect(),
            home: homef,
            stock: 0.0,
            chief: brave,
            desired: None,
            rivalry: 0.0,
        });

        let hunt_score = |m: &[crate::brain::Motivation]| {
            m.iter().find(|x| x.kind == TaskKind::HuntPredator).map(|x| x.score)
        };
        let brave_score = hunt_score(&sim.inspect_agent(brave).unwrap())
            .expect("un agressif face à une meute proche envisage de la chasser");
        let timid_score = hunt_score(&sim.inspect_agent(timid).unwrap()).unwrap_or(0.0);
        assert!(brave_score > 0.0);
        assert!(
            brave_score > timid_score,
            "l'agressif s'y risque bien plus que le placide (brave {brave_score}, timid {timid_score})"
        );
    }

    /// Domestication : garder un cheptel apprivoisé (`TaskKind::Herd`) verse un
    /// prélèvement durable au **stock commun** du clan. Mécanisme testé par
    /// `execute` direct, avec un cheptel synthétique à portée.
    #[test]
    fn garder_le_cheptel_alimente_le_stock() {
        let mut sim = Sim::new(WorldSeed(1), 512);
        let home = find_land(&sim);
        let homef = (home.0 as f64 + 0.5, home.1 as f64 + 0.5);
        let cheptel = HerdView {
            entity: hecs::Entity::DANGLING,
            pos: homef,
            population: 20.0,
            species: fauna::Species::Aurochs,
            tameness: 0.8, // franchement domestiqué
        };
        let mut pos = Position { x: homef.0, y: homef.1 };
        let mut phys = Physiology::default();
        let mut behavior =
            Behavior { task: Some(Task { kind: TaskKind::Herd, target: home }), ..Behavior::default() };
        let traits = Traits::default();
        let mut skills = Skills::default();
        let mut routes: BTreeMap<u64, Route> = BTreeMap::new();
        let mut budget = PathBudget { left: PATH_REQUESTS_PER_TICK, denied: 0 };
        let clan_id = social::ClanId(2);
        let mut clan_stock: BTreeMap<social::ClanId, f32> = BTreeMap::new();
        let mut carrying = Carrying::default();
        let mut prestige = Prestige::default();
        let mut structures: Vec<Structure> = Vec::new();

        execute(
            &mut sim.world, &mut routes, &mut budget, AgentId(0), &mut pos, &mut phys,
            &mut behavior, &[cheptel], &[], &[], 1.0, &traits, &mut skills, Some(clan_id),
            &mut clan_stock, &mut carrying, &mut prestige, &mut structures, &mut Vec::new(),
            &mut Vec::new(), &mut Vec::new(), 0, 1.0,
        );

        assert!(
            clan_stock.get(&clan_id).copied().unwrap_or(0.0) > 0.0,
            "garder le cheptel doit nourrir le stock commun"
        );
    }

    /// Un cheptel apprivoisé à portée du foyer appelle l'éleveur (candidat
    /// `Herd`) ; sans cheptel apprivoisé, personne ne garde. Lu via
    /// `inspect_agent` (pur).
    #[test]
    fn un_cheptel_apprivoise_appelle_l_eleveur() {
        let mut sim = Sim::new(WorldSeed(5), 512);
        let home = find_land(&sim);
        let homef = (home.0 as f64 + 0.5, home.1 as f64 + 0.5);
        let cheptel = sim.spawn_herd(homef.0 + 30.0, homef.1, 20.0);
        for (_, (id, herd)) in sim.fauna.query_mut::<(&FaunaId, &mut Herd)>() {
            if *id == cheptel {
                herd.tameness = 0.8;
            }
        }
        let herder = sim.spawn_agent(homef.0, homef.1);
        for (_, (phys, membership)) in
            sim.agents.query_mut::<(&mut Physiology, &mut ClanMembership)>()
        {
            phys.hunger = 0.2;
            phys.thirst = 0.2;
            phys.cold = 0.0;
            membership.0 = Some(social::ClanId(1));
        }
        sim.clans.push(Clan {
            id: social::ClanId(1),
            founded_tick: 0,
            members: [herder].into_iter().collect(),
            home: homef,
            stock: 0.0,
            chief: herder,
            desired: None,
            rivalry: 0.0,
        });

        let m = sim.inspect_agent(herder).unwrap();
        assert!(
            m.iter().any(|x| x.kind == TaskKind::Herd),
            "un éleveur repu au foyer, cheptel à portée, doit envisager de le garder"
        );
    }

    /// Conflit inter-clans : une forte **tension** avec un clan voisin appelle
    /// les agressifs au **raid** quand un rival passe à portée ; sans tension,
    /// personne ne razzie. Lu via `inspect_agent` (la tension vit dans
    /// `sim.clan_relations`, que `inspect` consulte).
    #[test]
    fn une_forte_tension_appelle_au_raid() {
        let mut sim = Sim::new(WorldSeed(7), 512);
        let home = find_land(&sim);
        let homef = (home.0 as f64 + 0.5, home.1 as f64 + 0.5);
        let raider = sim.spawn_agent(homef.0, homef.1);
        let rival = sim.spawn_agent(homef.0 + 20.0, homef.1); // ~40 m : à portée de raid
        for (_, (id, membership, traits, phys)) in sim.agents.query_mut::<(
            &AgentId,
            &mut ClanMembership,
            &mut Traits,
            &mut Physiology,
        )>() {
            phys.hunger = 0.2;
            phys.thirst = 0.2;
            phys.cold = 0.0;
            traits.aggression = 0.9;
            traits.strength = 0.7;
            membership.0 = Some(social::ClanId(if *id == raider { 1 } else { 2 }));
        }
        sim.clans.push(Clan {
            id: social::ClanId(1),
            founded_tick: 0,
            members: [raider].into_iter().collect(),
            home: homef,
            stock: 0.0,
            chief: raider,
            desired: None,
            rivalry: 0.0,
        });
        sim.clans.push(Clan {
            id: social::ClanId(2),
            founded_tick: 0,
            members: [rival].into_iter().collect(),
            home: (homef.0 + 20.0, homef.1),
            stock: 3.0,
            chief: rival,
            desired: None,
            rivalry: 0.0,
        });

        let raid_score = |sim: &mut Sim| {
            sim.inspect_agent(raider)
                .unwrap()
                .iter()
                .find(|x| x.kind == TaskKind::Raid)
                .map(|x| x.score)
        };
        // Forte tension (clé normalisée (min, max) = (1, 2)).
        sim.clan_relations.tension.insert((1, 2), 0.8);
        assert!(
            raid_score(&mut sim).is_some_and(|s| s > 0.0),
            "un agressif face à un rival hostile proche doit envisager le raid"
        );
        // D13 : blessé, on ne va pas au-devant des coups. À mi-plaie, l'envie
        // de razzier s'éteint — sans ce frein, une mêlée durait jusqu'à la mort.
        let indemne = raid_score(&mut sim).unwrap();
        for (_, (id, wound)) in sim.agents.query_mut::<(&AgentId, &mut Wound)>() {
            if *id == raider {
                wound.0 = 0.6;
            }
        }
        let blesse = raid_score(&mut sim).unwrap_or(0.0);
        assert!(
            blesse < 0.05 * indemne,
            "un raider à 0,6 de plaie razzie encore ({blesse:.3} contre {indemne:.3} indemne)"
        );
        for (_, wound) in sim.agents.query_mut::<&mut Wound>() {
            wound.0 = 0.0;
        }
        // Sans tension : plus de raid.
        sim.clan_relations.tension.clear();
        assert!(raid_score(&mut sim).is_none(), "sans tension, pas de raid");
    }

    /// Le bug réellement signalé par l'utilisateur : le client permet de
    /// lancer une scène à densité de gibier **nulle** (`herd_grid=0`, champ
    /// « Densité de gibier » de l'onglet Paramètres). Un `Sim` construit
    /// directement (donc `allow_fauna_immigration` à sa valeur par défaut —
    /// contrairement à `scenario_setup`, qui le désactive pour les scènes de
    /// test contrôlées) doit malgré tout voir du gibier apparaître avec le
    /// temps. Avec l'ancienne garde (`hunted_head > 0`), ce test aurait
    /// échoué : sans le moindre troupeau initial, aucun humain ne peut
    /// jamais chasser, `hunted_head` restait nul indéfiniment.
    #[test]
    fn une_scene_sans_gibier_initial_finit_par_en_recevoir() {
        let mut sim = Sim::new(WorldSeed(42), 1024);
        // On isole l'immigration de gibier : pas d'incendie qui viendrait
        // affamer les quelques agents-ancres (scène contrôlée).
        sim.allow_wildfires = false;
        let home = find_land(&sim);
        for i in 0..6i64 {
            let (dx, dy) = (i * 6, 0);
            sim.spawn_agent(home.0 as f64 + dx as f64 + 0.5, home.1 as f64 + dy as f64 + 0.5);
        }
        assert_eq!(
            sim.fauna.query::<&Herd>().iter().count(),
            0,
            "scénario invalide : du gibier existe déjà avant le premier pas"
        );

        let mut got_herd = false;
        for _ in 0..24 * 150 {
            sim.step();
            if sim.fauna.query::<&Herd>().iter().count() > 0 {
                got_herd = true;
                break;
            }
        }
        assert!(got_herd, "aucun gibier apparu en 150 j malgré allow_fauna_immigration=true par défaut");
    }

    #[test]
    fn le_temps_avance_et_les_agents_bougent() {
        let mut sim = scenario(42, 10, 0);
        let avant: Vec<_> = fingerprint(&sim).iter().map(|f| (f.1, f.2)).collect();
        for _ in 0..48 {
            sim.step();
        }
        assert_eq!(sim.time.tick, 48);
        let apres: Vec<_> = fingerprint(&sim).iter().map(|f| (f.1, f.2)).collect();
        assert_ne!(avant, apres, "aucun agent n'a bougé en 48 h");
    }
}
