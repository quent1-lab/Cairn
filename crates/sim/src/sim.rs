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

use cairn_core::{Pcg32, SimTime, TICKS_PER_DAY, WorldSeed};
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
/// Portée d'une mise à mort : le chasseur doit être à ~200 m du troupeau.
const HUNT_REACH_TILES: f64 = cairn_core::km_to_tiles(0.2);
/// Têtes prélevées par chasse réussie.
const HUNT_YIELD_HEAD: f32 = 1.0;
/// Ce qu'une prise retire de faim : une bête nourrit bien mieux que des
/// baies — c'est tout l'intérêt du risque et du trajet.
const HUNT_NUTRITION: f32 = 0.7;
/// Plafond du stock commun d'un clan, par membre : quelques portions de
/// réserve, pas un grenier sans fond. `HUNT_NUTRITION` vaut jusqu'à ~0,88
/// (au meilleur skill) — 3 portions, c'est de quoi absorber un mauvais jour
/// de chasse pour chaque membre, pas accumuler indéfiniment. `pub` (même
/// patron que `memory::MEMORY_CELL_TILES`) : le client en a besoin pour
/// afficher un stock relatif à son plafond plutôt qu'un nombre nu.
pub const STOCK_CAP_PER_MEMBER: f32 = 3.0;
/// Requêtes A* autorisées par tick (BRIEF §8.2 : « pathfinding budgété »).
/// Seuls les agents que l'eau bloque en consomment ; les autres marchent en
/// ligne droite pour rien.
const PATH_REQUESTS_PER_TICK: u32 = 8;

// — Agriculture (Phase 5, chaîne §5.3) : entretenir un champ élève la biomasse
//   d'une prairie au-dessus de sa capacité sauvage. La récolte passe par la
//   cueillette ordinaire (tuile plus riche → meilleur rendement) ; l'écologie
//   ramène un champ abandonné vers la friche. `pub` (comme `STOCK_CAP...`) : la
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
/// nourriture obtenue (au taux d'une prise, `HUNT_NUTRITION`) va au stock
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
#[derive(PartialEq, Eq)]
enum Move {
    /// Arrivé à moins d'`ARRIVAL_TILES` de la cible.
    Arrived,
    /// A progressé ce tick.
    Moved,
    /// Bloqué : cerné par l'eau, ou budget d'A* épuisé pour ce tick.
    Stuck,
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
    /// Têtes de gibier prélevées par les humains depuis le début : le compteur
    /// de la pression de chasse.
    pub hunted_head: f32,
    /// Nombre cumulé d'appels A* (observabilité du coût de pathfinding).
    pub path_calls: u64,
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
    /// **La Chronique** (Phase 6, BRIEF §6.4) : le journal narratif du monde,
    /// append-only et ordonné par tick (on n'y pousse qu'au tick courant).
    /// Ne reçoit que ce qui **fait date** — pas les naissances et les morts
    /// ordinaires, qui restent des statistiques dans `births`/`deaths`. Voir
    /// `crate::chronicle` pour la distinction journal / log.
    pub chronicle: Vec<crate::chronicle::Event>,
    /// Les expéditions commerciales en cours (Phase 5, incrément 6b), par
    /// identifiant d'agent — une table à côté de l'ECS, comme `routes`,
    /// nettoyée à la mort de l'envoyé. Voir `crate::commerce`.
    pub expeditions: BTreeMap<u64, Expedition>,
    /// Les feux de forêt sont-ils actifs pour cette simulation ? Vrai par
    /// défaut (le monde brûle parfois). Mis à faux par les scènes de test
    /// contrôlées qui isolent une autre mécanique — même patron explicite que
    /// `allow_fauna_immigration`.
    pub allow_wildfires: bool,
    /// L'immigration de gibier (`fauna::daily_immigration`) est-elle active
    /// pour cette simulation ? Vrai par défaut (le monde est censé être
    /// habité) ; les scènes de test qui veulent isoler une mécanique de
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
            hunted_head: 0.0,
            path_calls: 0,
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
            chronicle: Vec::new(),
            expeditions: BTreeMap::new(),
            allow_wildfires: true,
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
        self.agents.spawn((
            id,
            Position { x, y },
            Physiology::default(),
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
        self.agents.spawn((
            id,
            Position { x, y },
            Physiology {
                hunger: 0.1,
                thirst: 0.1,
                fatigue: 0.0,
                cold: 0.0,
                health: 1.0,
                last_damage: None,
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
            .map(|c| (c.id, ClanView { home: c.home, stock: c.stock, desired: c.desired }))
            .collect();
        // On extrait les composants de l'agent (copies, plus un clone de la
        // mémoire) pour relâcher l'emprunt de `self.agents` avant d'appeler
        // `brain::inspect`, qui veut `&mut self.world`.
        let agriculture = self.tech_tree.id_of("agriculture");
        let (pos, phys, traits, demo, kin, clan, carrying, current, knows_agriculture, mem) = self
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
            )>()
            .iter()
            .find(|(_, (aid, ..))| aid.0 == id.0)
            .map(|(_, (_, pos, phys, traits, demo, kin, membership, carrying, behavior, knowledge, mem))| {
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
        };
        Some(brain::inspect(
            &mut self.world, time, &ctx, &mem, current, &herds, &packs, &humans, &clan_views,
            &self.clan_relations,
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
        // *au début* du tick. Tout le monde délibère sur la même photo.
        // Le territoire de chaque clan (voir `social::detect_clans`) est
        // recalculé une fois par jour ; on n'en prend ici qu'une lecture.
        let herds = self.herd_views();
        let packs = self.pack_views();
        let humans = self.human_views();
        let clan_views: BTreeMap<ClanId, ClanView> = self
            .clans
            .iter()
            .map(|c| (c.id, ClanView { home: c.home, stock: c.stock, desired: c.desired }))
            .collect();

        // L'id de l'agriculture, résolu une seule fois : sert à déballer un
        // `knows_agriculture: bool` par agent pour la délibération (`brain`
        // reste ainsi découplé de l'arbre technologique).
        let agriculture = self.tech_tree.id_of("agriculture");

        // 1. Délibération — bucketée : l'agent i ne repense sa tâche qu'aux
        // ticks (tick + i) % période == 0, ou dès qu'il n'a plus de tâche.
        // Les nourrissons ne délibèrent pas : ils sont portés.
        for (_, (id, pos, phys, traits, demo, kin, membership, carrying, knowledge, behavior, mem)) in self
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
                &mut Behavior,
                &mut Memory,
            )>()
        {
            if demo.is_infant(time.tick) {
                continue;
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
                    );
                }
            }
        }

        // 2. Exécution des tâches. Les chasses réussies sont collectées : on
        // n'entame pas le gibier pendant que les autres délibèrent dessus.
        // `path_budget` borne le nombre d'A* lancés ce tick (agents bloqués
        // par l'eau). `clan_stock` est une copie de travail des réserves —
        // dépôts (chasse) et retraits (`EatFromStock`) s'y accumulent au fil
        // des agents, reportée sur `self.clans` une fois la boucle finie.
        let mut kills: Vec<Kill> = Vec::new();
        let mut path_budget = PATH_REQUESTS_PER_TICK;
        let mut clan_stock: BTreeMap<ClanId, f32> =
            self.clans.iter().map(|c| (c.id, c.stock)).collect();
        // Copie de travail des structures : `Build` y pousse la structure
        // achevée (financée par `clan_stock`), reportée sur `self` en fin de
        // boucle. Clairsemé — quelques éléments par clan, clone négligeable.
        let mut structures = self.structures.clone();
        let mut engagements: Vec<Engagement> = Vec::new();
        let mut clashes: Vec<Clash> = Vec::new();
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
                time.tick,
            );
            // Où que la tâche l'ait mené, l'agent note où il a mis les pieds.
            mem.note_visit(pos.tile());
            if let Some(kill) = outcome {
                kills.push(kill);
            }
        }
        // Dénouement des affrontements du tick : les meutes encaissent la somme
        // des coups, leur riposte se partage en plaies (mortelles à 1).
        combat::resolve(self, &engagements);
        // Puis les raids inter-clans : coups mutuels, butin, morts par violence.
        combat::resolve_clashes(self, &clashes);
        self.hunted_head += kills.iter().map(|k| k.head).sum::<f32>();
        self.path_calls += u64::from(PATH_REQUESTS_PER_TICK - path_budget);
        structures.sort_by_key(|s| (s.clan.0, s.kind));
        self.structures = structures;
        // Report des dépôts/retraits de la boucle, plafonné par membre
        // (`STOCK_CAP_PER_MEMBER`) plus la capacité des greniers du clan. On
        // efface aussi le désir déjà satisfait ce tick, pour ne pas faire
        // marcher inutilement d'autres membres vers un chantier déjà achevé
        // (le prochain `structures::plan` de minuit le referait de toute
        // façon, mais autant couper court tout de suite).
        for clan in &mut self.clans {
            if let Some(&stock) = clan_stock.get(&clan.id) {
                let cap = STOCK_CAP_PER_MEMBER * clan.members.len() as f32
                    + structures::granary_bonus(clan.id, &self.structures);
                clan.stock = stock.min(cap);
            }
            if let Some(kind) = clan.desired
                && self.structures.iter().any(|s| s.clan == clan.id && s.kind == kind)
            {
                clan.desired = None;
            }
        }

        // 2 ter. Expéditions : maintenant que les positions sont à jour, un
        // envoyé arrivé près de l'étain y est exposé et fait demi-tour ; rentré
        // au foyer, sa quête s'achève (voir `crate::commerce`).
        commerce::advance(self);

        // 2 bis. Les nourrissons : portés par leur mère, allaités par elle.
        demography::nurse_infants(self);

        // 3. Physiologie et morts. On collecte d'abord (on ne peut pas
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
        let mut dead = Vec::new();
        let mut sheltered = 0u64;
        for (entity, (id, pos, phys, traits, demo, behavior, membership, exposures, wound)) in
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
            phys.drift(felt, behavior.activity, traits.endurance);
            // La plaie se referme lentement, quoi qu'il arrive (§3.1) — elle
            // handicape le temps de guérir, mais ne s'infecte pas ici.
            wound.0 = (wound.0 - WOUND_HEAL_PER_TICK).max(0.0);
            // Une grossesse se nourrit : le surcoût s'ajoute à la dérive.
            if demo.pregnancy.is_some() {
                phys.hunger = (phys.hunger + demography::PREGNANCY_HUNGER_PER_TICK).min(1.0);
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

        // 3 bis. Démographie quotidienne à minuit : naissances, conceptions,
        // sénescence. Puis entretien du graphe social et détection des
        // clans (Phase 4) : le graphe doit voir la population du jour, pas
        // celle d'hier (un mort ne doit pas peser sur la cohésion).
        if time.tick.is_multiple_of(TICKS_PER_DAY) {
            demography::daily(self);
            social::daily(self);
            // La pression du jour (Phase 5) : besoin des clans et de leur
            // tension fraîche (posée par `social::daily`), lue plus tard par
            // le moteur d'insight. Mesure pure, ne déclenche rien encore.
            pressure::measure(self);
            // L'insight (Phase 5) : chaque adulte oisif, au confort, dans un
            // clan sous pression, peut découvrir une tech dont les prérequis
            // sont réunis. Après la pression, qu'elle consomme.
            tech::insight(self);
            // L'oubli (Phase 5) : constate quelles techs n'ont plus aucun
            // porteur vivant aujourd'hui (les morts du jour sont déjà passées,
            // étape 3) et les journalise — « l'humanité peut régresser ».
            tech::forget(self);
            // Commerce (Phase 5) : un clan du cuivre sans étain dépêche un
            // envoyé le chercher au loin — la route qui, seule, mène au bronze.
            commerce::dispatch(self);
            // Après que les clans du jour sont connus : réattribuer les
            // structures à qui contrôle leur tuile (et ruiner les abandonnées),
            // mesurer ce que chaque clan désire bâtir, puis ancrer le foyer des
            // clans sédentarisés à leur hutte du chef (en dernier : c'est cette
            // valeur ancrée que la délibération du lendemain doit lire).
            structures::maintain(self, time.tick / TICKS_PER_DAY);
            structures::plan(self);
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
            fire::daily(self);
        }

        // 3 ter. Échange de savoirs et renforcement des liens sociaux toutes
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
        }

        // 4. Faune. Les meutes chassent d'abord (sur l'instantané), puis
        // toutes les prises — prédation et chasse humaine — sont appliquées
        // avant que les troupeaux ne fassent leurs petits : un troupeau
        // décimé ne doit pas engendrer comme s'il était intact.
        let (pred_kills, dead_packs) = fauna::update_packs(&mut self.fauna, &self.world, &herds);
        kills.extend(pred_kills);
        fauna::apply_kills(&mut self.fauna, &kills);
        for entity in dead_packs {
            let _ = self.fauna.despawn(entity);
        }

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

        let seed = self.world.seed();
        let (dead_herds, fissions) = fauna::update_herds(
            &mut self.fauna,
            &mut self.world,
            &self.climate,
            time,
            seed,
            &threats,
        );
        for entity in dead_herds {
            let _ = self.fauna.despawn(entity);
        }
        for (x, y, population, species) in fissions {
            self.spawn_herd_species(x, y, population, species);
        }

        // 4 bis. Immigration de gibier, quotidienne : sans elle, une zone
        // qui perd tous ses troupeaux (chasse sous `HERD_MIN`, prédation
        // comprise) reste vide pour toujours — rien ne fait *repousser* un
        // troupeau depuis zéro individu, contrairement à la végétation
        // (banque de graines). Gardée par `allow_fauna_immigration` — voir
        // le commentaire du champ : un choix explicite du contexte
        // d'exécution, pas une heuristique déduite de l'état de la sim.
        if time.tick.is_multiple_of(TICKS_PER_DAY) && self.allow_fauna_immigration {
            let human_positions: Vec<(f64, f64)> = humans.iter().map(|h| h.pos).collect();
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

        // 5. Écologie quotidienne, à minuit.
        if time.tick.is_multiple_of(TICKS_PER_DAY) {
            ecology::daily_regrowth(&mut self.world, &self.climate, time);
        }

        self.time.tick += 1;
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
    path_budget: &mut u32,
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
    tick: u64,
) -> Option<Kill> {
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
        // Un bon chasseur tire plus d'une bête (dépeçage, choix de la proie) —
        // et chaque mise à mort forge le geste bien plus qu'une heure d'affût.
        let nutrition = HUNT_NUTRITION * (0.75 + 0.5 * agent_skills.hunting);
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
        let surplus = (nutrition - phys.hunger).max(0.0);
        phys.hunger = (phys.hunger - nutrition).max(0.0);
        if surplus > 0.0 && clan.is_some() {
            carrying.0 += surplus;
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
            *clan_stock.entry(clan_id).or_insert(0.0) += HERD_HARVEST_HEAD * HUNT_NUTRITION;
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

    // Phase trajet : la cible est trop loin, on marche (budget d'une heure).
    if pos.distance_tiles(task.target) > ARRIVAL_TILES {
        behavior.activity = Activity::Walking;
        if advance(world, routes, path_budget, id, pos, task.target) == Move::Stuck {
            behavior.task = None; // vraiment cerné : on re-délibérera
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
            let bite = EAT_HUNGER_PER_TICK * work * (0.6 + 0.8 * agent_skills.foraging);
            skills::practice(&mut agent_skills.foraging, skills::forage_cap(traits), 1.0);
            let tile = world.tile_mut(task.target.0, task.target.1);
            let wanted = (phys.hunger.min(bite) / NUTRITION_PER_BIOMASS).ceil() as u8;
            let taken = wanted.min(tile.biomass);
            tile.biomass -= taken;
            phys.hunger = (phys.hunger - f32::from(taken) * NUTRITION_PER_BIOMASS).max(0.0);
            if phys.hunger <= 0.05 || tile.biomass == 0 {
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
                let taken = stock.min(phys.hunger);
                *stock -= taken;
                phys.hunger = (phys.hunger - taken).max(0.0);
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
        | TaskKind::ReturnToClan
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

/// Fait avancer l'agent vers `target` pour ce tick, en gérant l'évitement de
/// l'eau : ligne droite tant qu'elle passe, A* budgété (via une [`Route`]
/// persistante) dès qu'elle bute.
///
/// Le cas courant — terrain ouvert — ne touche jamais l'A* : on ne paie le
/// pathfinding que là où la géographie l'exige.
fn advance(
    world: &mut World,
    routes: &mut BTreeMap<u64, Route>,
    path_budget: &mut u32,
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
    if *path_budget == 0 {
        return Move::Stuck; // pas ce tick — on retentera, la tâche est gardée
    }
    *path_budget -= 1;
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
/// prochaine (ou vers le but exact une fois le trajet épuisé).
fn follow_route(world: &mut World, route: &mut Route, pos: &mut Position, target: (i64, i64)) -> Move {
    while route.cursor < route.waypoints.len()
        && pos.distance_tiles(route.waypoints[route.cursor]) <= ARRIVAL_TILES
    {
        route.cursor += 1;
    }
    let heading_to_goal = route.cursor >= route.waypoints.len();
    let step_target = if heading_to_goal {
        target
    } else {
        route.waypoints[route.cursor]
    };
    match walk_line(world, pos, step_target) {
        Move::Arrived if heading_to_goal => Move::Arrived,
        Move::Arrived => {
            route.cursor += 1; // étape atteinte, on enchaîne au prochain tick
            Move::Moved
        }
        other => other,
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
    let mut moved = false;
    while budget > 0.0 {
        let dx = target.0 as f64 + 0.5 - pos.x;
        let dy = target.1 as f64 + 0.5 - pos.y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist <= ARRIVAL_TILES {
            return Move::Arrived;
        }
        let step = WALK_SAMPLE_TILES.min(dist).min(budget);
        let next = (pos.x + dx / dist * step, pos.y + dy / dist * step);
        if world.worldgen().elevation(next.0.floor() as i64, next.1.floor() as i64) <= 0.0 {
            return if moved { Move::Moved } else { Move::Stuck };
        }
        pos.x = next.0;
        pos.y = next.1;
        budget -= step;
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
        for _ in 0..8 {
            let mut budget = PATH_REQUESTS_PER_TICK;
            advance(&mut sim.world, &mut routes, &mut budget, id, &mut pos, target);
        }
        assert!(routes.contains_key(&id.0), "un trajet A* d'évitement doit être créé");
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
        let jours = 24 * 15;
        let cheptel = |agents: u32| {
            let (mut sim, home) = scenario_setup(42, agents, 4);
            let depart = local_herbivores(&sim, home, 3.0);
            for _ in 0..jours {
                sim.step();
            }
            (depart, local_herbivores(&sim, home, 3.0), sim.hunted_head)
        };

        let (depart, temoin, tue_temoin) = cheptel(0);
        let (_, surchasse, tue_surchasse) = cheptel(60);

        assert_eq!(tue_temoin, 0.0, "témoin : personne ne chasse");
        assert!(
            temoin > depart * 0.5,
            "témoin : le gibier doit rester sur place ({depart:.0} → {temoin:.0} têtes) — \
             sinon le test ne mesure pas la chasse mais la dérive des troupeaux"
        );
        assert!(
            tue_surchasse > 20.0,
            "les chasseurs doivent prélever du gibier ({tue_surchasse:.0} têtes)"
        );
        assert!(
            surchasse < temoin * 0.2,
            "effondrement local attendu : {surchasse:.0} têtes à 3 km sous 60 chasseurs, \
             contre {temoin:.0} sans (départ {depart:.0})"
        );
    }

    #[test]
    fn la_chasse_nourrit_mieux_que_la_cueillette() {
        // Un chasseur au contact du gibier doit rassasier, et le troupeau
        // doit le payer : c'est le couplage « chassent quand ils ont faim ».
        let (mut sim, _) = scenario_setup(42, 20, 3);
        for (_, phys) in sim.agents.query_mut::<&mut Physiology>() {
            phys.hunger = 0.9; // affamés : la chasse doit dominer
        }
        for _ in 0..24 * 3 {
            sim.step();
        }
        assert!(sim.hunted_head > 0.0, "des affamés près du gibier doivent chasser");
        let faim: f32 = sim
            .agents
            .query::<&Physiology>()
            .iter()
            .map(|(_, p)| p.hunger)
            .sum::<f32>()
            / sim.population().max(1) as f32;
        assert!(faim < 0.8, "la chasse doit faire retomber la faim (moyenne {faim:.2})");
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

    /// La rencontre suffit : un campement mixte produit des conceptions en
    /// quelques semaines, sans aucune intervention.
    #[test]
    fn les_conceptions_surviennent_au_campement() {
        let (mut sim, _) = scenario_setup(42, 30, 0);
        for _ in 0..24 * 45 {
            sim.step();
        }
        let pregnancies = sim
            .agents
            .query::<&Demographics>()
            .iter()
            .filter(|(_, d)| d.pregnancy.is_some())
            .count();
        assert!(
            pregnancies + sim.births.len() > 0,
            "30 adultes mêlés pendant 45 jours : au moins une conception attendue"
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

        for _ in 0..24 * 3 {
            sim.step();
        }

        let mut positions = BTreeMap::new();
        for (_, (id, pos)) in sim.agents.query::<(&AgentId, &Position)>().iter() {
            positions.insert(id.0, (pos.x, pos.y));
        }
        let m = positions.get(&mother.0).expect("mère morte : scénario invalide");
        let c = positions.get(&child.0).expect("enfant mort : scénario invalide");
        let d = (c.0 - m.0).hypot(c.1 - m.1);
        assert!(
            d < 200.0,
            "l'enfant devrait graviter autour de sa mère (à {d:.0} tuiles après 3 jours, départ 300)"
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
    /// deux semaines, les curieux connaissent nettement plus de cellules.
    #[test]
    fn les_curieux_explorent_plus_loin() {
        let (mut sim, _) = scenario_setup(42, 20, 0);
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
            mean_curious > mean_dull * 1.2,
            "les curieux doivent connaître nettement plus de terrain \
             ({mean_curious:.1} cellules contre {mean_dull:.1})"
        );
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
        let (mut sim, _) = scenario_setup(42, 24, 0);
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
        let spread_100d = clan_spread(&sim, &clan);

        assert!(
            spread_100d < spread_60d + cairn_core::km_to_tiles(1.5),
            "l'étalement ne doit plus croître sans borne une fois le territoire actif \
             ({spread_60d:.0} tuiles à 60 j, {spread_100d:.0} tuiles à 100 j)"
        );
    }

    /// L'incrément 3 de la Phase 4 (stock commun, BRIEF §5.1) : une chasse
    /// fructueuse nourrit rarement pile ce qu'il fallait — `HUNT_NUTRITION`
    /// est une bête tuée, pas une portion calibrée. Le surplus, qui partait
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
        let mut budget = PATH_REQUESTS_PER_TICK;
        let clan_id = social::ClanId(3);
        let mut clan_stock: BTreeMap<social::ClanId, f32> = BTreeMap::new();
        let mut carrying = Carrying::default();
        let mut prestige = Prestige::default();
        let mut structures: Vec<Structure> = Vec::new();
        // Le troupeau est juste sous la main : cette scène teste la
        // mécanique de mise à mort, pas l'approche.
        let herd =
            HerdView { entity: hecs::Entity::DANGLING, pos: (pos.x, pos.y), population: 20.0, tameness: 0.0 };

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
            0,
        );

        assert!(kill.is_some(), "le troupeau est à portée : la chasse doit réussir");
        assert!(phys.hunger < 1e-6, "la faim doit être totalement comblée (nutrition > faim initiale)");
        assert!(
            carrying.0 > 0.0,
            "le surplus (nutrition au-delà de la faim comblée) doit être porté par le chasseur"
        );
        assert_eq!(
            clan_stock.get(&clan_id).copied().unwrap_or(0.0),
            0.0,
            "le stock du clan ne doit RIEN recevoir tant que le surplus n'a pas été rapporté au foyer"
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
        let mut phys = Physiology::default();
        let mut behavior = Behavior {
            task: Some(Task { kind: TaskKind::BringSurplusHome, target: home }),
            ..Behavior::default()
        };
        let traits = Traits::default();
        let mut skills = Skills::default();
        let mut routes: BTreeMap<u64, Route> = BTreeMap::new();
        let mut budget = PATH_REQUESTS_PER_TICK;
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
            0,
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
        let mut budget = PATH_REQUESTS_PER_TICK;
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
            0,
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
        let mut budget = PATH_REQUESTS_PER_TICK;
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
            0,
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
            0,
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
        let mut budget = PATH_REQUESTS_PER_TICK;
        let mut clan_stock: BTreeMap<social::ClanId, f32> = BTreeMap::new();
        let mut carrying = Carrying::default();
        let mut prestige = Prestige::default();
        let mut structures: Vec<Structure> = Vec::new();

        execute(
            &mut sim.world, &mut routes, &mut budget, AgentId(0), &mut pos, &mut phys,
            &mut behavior, &[], &[], &[], 1.0, &traits, &mut skills, None, &mut clan_stock,
            &mut carrying, &mut prestige, &mut structures, &mut Vec::new(), &mut Vec::new(), 0,
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
            tameness: 0.8, // franchement domestiqué
        };
        let mut pos = Position { x: homef.0, y: homef.1 };
        let mut phys = Physiology::default();
        let mut behavior =
            Behavior { task: Some(Task { kind: TaskKind::Herd, target: home }), ..Behavior::default() };
        let traits = Traits::default();
        let mut skills = Skills::default();
        let mut routes: BTreeMap<u64, Route> = BTreeMap::new();
        let mut budget = PATH_REQUESTS_PER_TICK;
        let clan_id = social::ClanId(2);
        let mut clan_stock: BTreeMap<social::ClanId, f32> = BTreeMap::new();
        let mut carrying = Carrying::default();
        let mut prestige = Prestige::default();
        let mut structures: Vec<Structure> = Vec::new();

        execute(
            &mut sim.world, &mut routes, &mut budget, AgentId(0), &mut pos, &mut phys,
            &mut behavior, &[cheptel], &[], &[], 1.0, &traits, &mut skills, Some(clan_id),
            &mut clan_stock, &mut carrying, &mut prestige, &mut structures, &mut Vec::new(),
            &mut Vec::new(), 0,
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
