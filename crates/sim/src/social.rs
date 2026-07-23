//! Le graphe social et l'émergence du clan (BRIEF §5.1, Phase 4 « LE CLAN »,
//! incréments 1, 2 et 3).
//!
//! C'est la phase la plus délicate en conception (BRIEF §9) : **le clan ne
//! doit être qu'une conséquence détectée, jamais une cause**. Il est donc
//! hors de question d'écrire « si la population dépasse X, former un
//! village ». Ce module ne fait que deux choses, toutes deux
//! **observationnelles** :
//!
//! 1. **Tenir un graphe d'affinités** entre agents : chaque rencontre (à
//!    portée de conversation, comme l'échange de savoirs — `crate::memory`)
//!    renforce le lien entre deux agents, un peu plus vite s'ils sont
//!    parents. Sans entretien, un lien se relâche et finit par s'oublier.
//! 2. **Détecter, chaque jour, les groupes qui se sont formés** dans ce
//!    graphe — et seulement ceux qui dépassent *simultanément* un seuil de
//!    cohésion (densité du graphe d'affinités) et de co-résidence (les
//!    membres vivent proches les uns des autres). Ces groupes deviennent des
//!    `Clan`. Rien d'autre ne les crée.
//!
//! ## Une limite de bande passante sociale, pas un seuil de taille
//!
//! Le brief demande aussi qu'« un clan trop nombreux fissionne ». Écrire
//! « si taille > N, couper en deux » serait exactement l'anti-pattern à
//! refuser (BRIEF §11). À la place, chaque agent ne retient qu'un nombre
//! borné de liens forts (`MAX_BONDS_PER_AGENT` — l'hypothèse de Dunbar :
//! l'attention sociale est une ressource finie). Comme la cohésion exigée
//! est une **densité** (proportion de paires effectivement liées), et que le
//! nombre de liens qu'un groupe de `n` membres peut porter est plafonné à
//! `n · MAX_BONDS_PER_AGENT / 2`, la densité maximale atteignable décroît
//! mécaniquement en `1/n` — un groupe qui grossit **ne peut plus, au-delà
//! d'une certaine taille, satisfaire le seuil de cohésion**, quelle que soit
//! la façon dont ses membres se lient. La borne de taille est donc une
//! **conséquence arithmétique** de deux règles locales (attention bornée,
//! cohésion exigée), pas un cas spécial.
//!
//! ## Persistance de l'identité
//!
//! La détection est **recalculée en entier chaque jour** — aucun état de
//! « clan » n'est modifié incrémentalement. Un clan qui existait la veille
//! survit s'il retrouve, dans les nouveaux groupes détectés, un groupe qui
//! partage la **majorité** de ses membres (dans les deux sens) ; sinon, il
//! s'efface — ses membres redeviennent simplement des agents sans clan, prêts
//! à en rejoindre ou fonder un autre au prochain passage. C'est ce mécanisme,
//! et lui seul, qui donne « un clan affamé s'effondre » : si la faim disperse
//! ses membres ou les tue, le groupe ne se reforme plus assez fort le
//! lendemain, et il n'y a rien de plus à coder.
//!
//! ## Le territoire (incrément 2) : de la co-résidence constatée à la
//! co-résidence *voulue*
//!
//! L'incrément 1 avait une limite mesurée : rien, dans les comportements des
//! Phases 2-3, ne poussait un agent à rester près de ses liens sociaux —
//! `TaskKind::Socialize` ne fait que rejoindre le **plus proche** congénère
//! quand on est isolé, pas revenir vers un clan précis. Résultat mesuré sur
//! la scène de calibrage (24 agents) : la population diffusait sans borne,
//! au point qu'un clan formé pouvait se dissoudre de lui-même par pure
//! dérive spatiale — pas par un vrai effondrement au sens du brief.
//!
//! `Clan::home` (le centroïde des membres, recalculé par `detect_clans` en
//! même temps que tout le reste) devient ce **territoire** au sens le plus
//! simple : pas encore un champ diffusé sur la grille (ça viendra avec les
//! structures, qui touchent enfin les tuiles — BRIEF §2.3), juste un point
//! qui **attire**. `brain::decide` lit ce point (`AgentCtx::clan` +
//! `clan_views`, passés par `Sim::step`) et propose un candidat
//! `TaskKind::ReturnToClan` dès qu'un membre s'éloigne du foyer de plus que
//! `RESIDENCE_RADIUS_TILES` — **le même rayon** que celui utilisé par la
//! détection pour juger de la co-résidence, pour que l'attraction et le
//! critère se répondent : rester en dessous du seuil, c'est rester
//! détectable. Mesuré sur la même scène (`sim::tests::
//! le_territoire_stabilise_la_derive_apres_formation`) : l'étalement du clan
//! **plafonne** (4,5 km à 60 j → 5,1 km à 100 j) là où il grossissait sans
//! fin auparavant (~8 km à 85 j, et toujours en croissance).
//!
//! ## Le stock commun (incrément 3)
//!
//! BRIEF §5.1 : « stock commun, ressources mises en commun ». Rien, avant
//! cet incrément, ne faisait vivre une réserve de nourriture plus loin que
//! l'instant présent — chasser et cueillir rechargent directement la faim de
//! l'individu, sans jamais rien laisser derrière. `Clan::stock` change ça
//! *a minima* : une chasse fructueuse nourrit rarement pile ce qu'il fallait
//! (`HUNT_NUTRITION` est une bête tuée, pas une portion calibrée) — le
//! surplus, qui partait auparavant dans le `.max(0.0)` de la faim déjà à
//! zéro, est désormais **porté** par le chasseur (`Carrying`, `sim::execute`,
//! bras `Hunt`) puis rapporté au foyer de son clan pour y rejoindre le stock
//! (`TaskKind::BringSurplusHome`) — la viande ne se téléporte pas depuis le
//! lieu de la chasse : le dépôt exige d'y être, exactement comme le retrait.
//! Un chasseur qui meurt ou change durablement de priorité en chemin perd
//! simplement ce qu'il portait, sans code dédié pour ce cas. À l'autre bout,
//! un membre affamé sans cueillette ni gibier à portée (même garde que le
//! désespoir d'errance) peut rentrer au foyer du clan pour y puiser
//! (`TaskKind::EatFromStock`) — le stock n'est pas un porte-monnaie magique,
//! il faut physiquement y être. **Rien n'a été ajouté au budget de
//! délibération** : `EatFromStock` et `BringSurplusHome` sont deux candidats
//! de plus dans le même softmax que tous les autres, pas un système à part.
//! La cueillette, elle, n'alimente pas le stock : son prélèvement est déjà
//! borné à ce que la faim du moment réclame (`wanted = hunger.min(bite)`
//! dans `sim::execute`), il n'y a structurellement pas de surplus à y
//! capter sans changer aussi *comment* on cueille — hors scope ici.
//!
//! ## La fission (incrément 5)
//!
//! BRIEF §9 : « un clan trop nombreux fissionne ». Jusqu'ici, un groupe
//! connexe qui ne passait pas le seuil de cohésion était simplement
//! **abandonné** — un clan qui grossissait trop finissait par se dissoudre
//! plutôt que de se scinder. Écrire « si `members.len() > N`, couper en
//! deux » serait, encore une fois, l'anti-pattern que le brief interdit
//! (§11) : la taille ne doit jamais être lue directement, seulement ses
//! conséquences sur la cohésion (voir plus haut, « une limite de bande
//! passante sociale »).
//!
//! `resolve_cluster` cherche donc, dans un groupe qui échoue le test de
//! cohésion, s'il cache en réalité **deux sous-groupes plus densément
//! liés** : on relève progressivement le seuil de lien utilisé pour la
//! recherche (jamais celui qui sert à *mesurer* la cohésion — ce dernier
//! reste `BOND_THRESHOLD` pour tout le monde, tout le temps, sans quoi la
//! notion même de « clan cohésif » deviendrait mouvante). C'est la coupure
//! d'un dendrogramme à seuil (single-linkage) — une technique connue, pas
//! une heuristique inventée pour l'occasion. Si le sous-graphe des liens
//! *les plus forts* se sépare en plusieurs morceaux, chacun est réévalué
//! **récursivement** par les mêmes règles que n'importe quel candidat clan
//! (taille, cohésion, co-résidence) : rien de spécifique à la fission, la
//! fonction qui valide un clan ordinaire est aussi celle qui valide chaque
//! fille. Un groupe trop petit pour produire deux clans viables
//! (`< 2 · MIN_CLAN_SIZE`), ou dont aucun seuil ne le sépare jamais
//! proprement, s'efface — exactement le comportement d'avant cet
//! incrément, préservé comme cas limite plutôt que remplacé.
//!
//! **Aucun changement nécessaire à la réconciliation d'identité** (voir
//! plus haut) : quand un groupe scinde, chaque fille est comparée aux
//! clans existants comme n'importe quel nouveau cluster. La fille qui
//! retrouve la majorité des membres de l'ancien clan **hérite** de son
//! identité (elle *est* le clan, juste plus petit) ; l'autre est traitée
//! comme un `Formed` ordinaire. Une fission se lit donc dans les
//! événements comme « le clan continue (amoindri) + un nouveau clan
//! apparaît » — un signal déjà présent, pas une mécanique à ajouter.
//!
//! ## Les relations inter-clans (incrément 6)
//!
//! BRIEF §9 : « deux clans voisins sur une ressource rare entrent en
//! tension de manière observable ». Le mot clé est *observable* : la
//! tension n'est **jamais décidée**, seulement **mesurée** — exactement le
//! même principe que la détection de clan elle-même. `update_relations`
//! (appelée à la même cadence quotidienne que `detect_clans`, juste après)
//! calcule, pour chaque paire de clans dont les foyers sont assez proches
//! pour se disputer un territoire (`CONTACT_RADIUS_TILES`), le gibier
//! disponible dans la zone qui les sépare (même notion d'effectif de
//! troupeau que la chasse, `fauna::Herd::population`, sommée dans un rayon
//! autour du point médian des deux foyers). Rapporté au nombre de bouches à
//! nourrir des deux clans réunis, ce gibier par tête est soit rare
//! (`SCARCITY_PER_CAPITA`), soit abondant. **Une seule règle, sans cas
//! spécial** : rare ET en contact → la tension monte (nudge saturant,
//! comme les liens sociaux) ; sinon (hors de contact, ou ressource
//! abondante) → elle redescend. Aucune notion de conflit, de guerre ou de
//! combat n'est ajoutée ici — `force`/`agressivité` restent en attente
//! (voir Phase 3) : c'est une portée délibérément limitée à ce que le
//! critère BRIEF demande, l'observabilité, pas la conclusion narrative
//! qu'un joueur pourrait en tirer (Phase 5+, Chronique).
//!
//! `ClanRelations::tension` est reconstruite en entier chaque jour à partir
//! des seuls clans qui existent encore (même logique que `Clan::home`) :
//! un clan qui s'efface (fission, effondrement) emporte ses tensions avec
//! lui sans code de nettoyage dédié — la paire disparaît simplement de la
//! prochaine reconstruction.
//!
//! ## Le territoire diffusé (incrément 7)
//!
//! L'incrément 2 avait fait de `Clan::home` un point qui **attire** les
//! membres. BRIEF §5.1 demande plus : un vrai « champ d'influence diffusé
//! sur la grille » — pas juste un point, une valeur interrogeable à
//! n'importe quelle position, qui décroît avec la distance.
//!
//! **Décision d'implémentation qui s'écarte du texte initial du projet** (qui
//! anticipait un champ `Tile::claim` écrit en dur) : au vu de l'échelle
//! réelle du monde (1 tuile = 2 m, un territoire de rayon
//! `RESIDENCE_RADIUS_TILES` ≈ 2250 tuiles couvre à lui seul près de 16
//! millions de tuiles), **écrire** cette valeur dans chaque tuile d'un
//! disque de cette taille, chaque jour, pour chaque clan, serait
//! incompatible avec l'architecture chunkée/LRU de `sim` (`Tile` reste
//! volontairement à 16 octets, et l'éviction ne sait sauvegarder que deux
//! champs mutables, voir `world::ChunkDelta`) — l'anti-pattern exact que le
//! chunking existe pour éviter (BRIEF §8.2). `claim_at` est donc une
//! fonction **pure et calculée à la demande** (`(position, clans) →
//! Option<ClanId>`), exactement dans l'esprit du worldgen lui-même (baseline
//! pur, jamais stocké) plutôt qu'une mutation de `Tile` : le champ existe
//! au sens mathématique — interrogeable en tout point — sans jamais être
//! matérialisé sur la grille.
//!
//! **La diffusion elle-même** : chaque clan projette une force
//! `membres × (1 − distance / RESIDENCE_RADIUS_TILES)`, nulle au-delà de ce
//! rayon (même seuil que le territoire-attracteur et la co-résidence — un
//! seul rayon partagé par toute la Phase 4, pas un de plus à recaler). La
//! tuile appartient au clan de force maximale en ce point, si elle est
//! positive — sinon elle est libre. **Aucune règle de taille scriptée** :
//! un clan plus peuplé projette naturellement plus loin dans les zones de
//! recouvrement (conséquence arithmétique du facteur `membres`, comme la
//! borne de fission), sans qu'aucun seuil de population ne soit jamais lu
//! directement. Aux confins de deux territoires qui se chevauchent, la
//! frontière est donc la même zone que celle où `update_relations` détecte
//! une tension — les deux mécanismes lisent la même géométrie sans être
//! couplés entre eux.
//!
//! **Portée limitée à ce que le critère demande** : ce champ ne modifie
//! encore aucun comportement (personne ne le consulte pour se déplacer,
//! chasser ou refuser l'accès) — il est désormais mesurable et testable,
//! prêt à être consulté par un futur incrément (structures : où a-t-on le
//! droit de bâtir ?) sans qu'aucun changement supplémentaire à ce module ne
//! soit nécessaire.
//!
//! ## Le chef (incrément 8)
//!
//! BRIEF §5.1 : « chef : `max(oratoire × prestige)`, contestable ». Deux
//! facteurs, mesurés séparément, jamais un rôle attribué directement :
//!
//! - **Oratoire** (`skills::Skills::oratory`) : une compétence de plus, avec
//!   le même patron que cueillette/chasse (plafond conditionné par un trait
//!   — ici `sociabilité`, déjà le trait qui pousse `TaskKind::Socialize`, pas
//!   un nouveau trait inventé pour l'occasion — et courbe saturante par la
//!   pratique). Sa pratique n'a pas besoin d'une tâche dédiée : chaque
//!   conversation (`encounter`, la même passe qui renforce les liens
//!   d'affinité) est déjà l'occasion de la pratiquer.
//! - **Prestige** (`agent::Prestige`) : contrairement aux compétences, ne
//!   sature ni ne décroît — une réserve d'estime accumulée à vie. Un seul
//!   déclencheur pour cet incrément : ce qu'un agent a effectivement
//!   rapporté à son clan (`TaskKind::BringSurplusHome`, voir le commentaire
//!   de module sur le stock commun) grossit son prestige du même montant
//!   que le stock. Un compagnon qui a nourri le groupe pendant des années
//!   garde son ascendant même le jour où il chasse moins bien qu'un jeune
//!   loup — voulu, pas un oubli de décroissance.
//!
//! `elect_chiefs` (appelée en dernier dans la passe quotidienne, après que
//! les clans et leurs relations du jour sont connus) désigne, pour chaque
//! clan, le membre qui maximise `oratoire × prestige` — ties départagées
//! par le plus petit `AgentId`, même convention que `claim_at`. **La
//! « contestation » n'est pas une mécanique à part** : puisque le calcul est
//! refait en entier chaque jour à partir des scores du jour, la position
//! change de mains dès qu'un autre membre dépasse le titulaire — exactement
//! le même principe que la détection de clan elle-même (recalcul complet,
//! pas d'état modifié incrémentalement). **Portée limitée** : comme la
//! tension et le territoire, être chef ne déclenche encore aucun
//! comportement particulier (pas de privilège, pas d'autorité) — le titre
//! est mesurable et observable, prêt pour un futur système qui voudrait s'en
//! servir.

use std::collections::{BTreeMap, BTreeSet};

use crate::agent::{AgentId, Position, Prestige};
use crate::fauna::Herd;
use crate::demography::{Kinship, Traits, find_human};
use crate::memory::TALK_RADIUS_TILES;
use crate::sim::Sim;
use crate::skills::{self, Skills};
use crate::structures::{self, StructureKind};
use cairn_core::km_to_tiles;

// — Affinités —

/// Gain d'une rencontre ordinaire : nudge saturant vers 1,0 (même idiome que
/// `skills::practice`). Calibré à la hausse (voir le commentaire de module
/// sur la dispersion) : la population n'étant retenue par rien de plus
/// qu'un évitement de l'isolement (`brain::decide`, tâche `Socialize`), elle
/// se disperse sur des kilomètres en quelques semaines — les liens doivent
/// se former **avant**, pendant que le groupe est encore proche.
const ENCOUNTER_GAIN: f32 = 0.15;
/// Gain d'une rencontre entre parenté directe (mère/enfant, fratrie) : les
/// liens du sang prennent plus vite que l'amitié.
const KIN_GAIN: f32 = 0.35;
/// Relâchement quotidien multiplicatif d'un lien non entretenu : un lien
/// entretenu plusieurs fois par jour continue de croître malgré lui, un lien
/// abandonné s'efface en quelques semaines.
const DAILY_DECAY: f32 = 0.02;
/// En dessous de ce poids, un lien est oublié (borne mémoire du graphe).
const FORGET_THRESHOLD: f32 = 0.03;
/// Liens fiables retenus par agent au plus : l'hypothèse de Dunbar réduite à
/// l'os. C'est ce plafond, combiné à l'exigence de cohésion, qui borne
/// arithmétiquement la taille d'un clan viable (voir le commentaire de
/// module).
const MAX_BONDS_PER_AGENT: usize = 15;
/// Au-delà de ce poids, un lien compte comme une vraie relation pour la
/// détection de clan (en deçà, c'est une connaissance passagère).
pub const BOND_THRESHOLD: f32 = 0.5;

// — Détection de clan —

/// Taille minimale d'un groupe pour compter comme clan : en dessous, c'est
/// une famille, pas une structure sociale.
const MIN_CLAN_SIZE: usize = 8;
/// Densité minimale du sous-graphe interne (paires liées / paires possibles)
/// pour qu'un groupe connecté soit reconnu comme un clan, et pas une simple
/// chaîne de connaissances qui se prolonge de proche en proche. Calibré sur
/// une scène de 24 agents (voir `sim::tests::un_clan_emerge_sans_regle_explicite`) :
/// les groupes réels observés plafonnent autour de 0,2 à 0,45 — personne ne
/// se lie fortement à tout le monde, la densité d'un vrai village reste
/// modeste, pas proche de 1.
const COHESION_THRESHOLD: f32 = 0.28;
/// Rayon de résidence, depuis le centroïde du groupe (~4,5 km — un
/// territoire de bande semi-nomade, pas une seule clairière : mesuré sur la
/// scène de calibrage, voir `sim::tests::un_clan_emerge_sans_regle_explicite`
/// — une seule jambe d'errance ou d'exploration dépasse déjà les 120 m de
/// portée de conversation, et rien ne ramène la population vers un point fixe).
/// `pub(crate)` : c'est aussi le rayon au-delà duquel `brain::decide` tire un
/// membre vers le foyer de son clan — même seuil des deux côtés, pour que
/// l'attraction comportementale et le critère de détection se répondent.
pub(crate) const RESIDENCE_RADIUS_TILES: f64 = km_to_tiles(4.5);
/// Fraction des membres qui doivent être dans ce rayon : pas tous — un
/// chasseur ou un éclaireur temporairement loin reste du clan. En dessous de
/// cette proportion, le groupe n'est plus « co-résident », il est dispersé.
const RESIDENCE_FRACTION: f32 = 0.7;
/// Un nouveau groupe hérite de l'identité d'un ancien clan s'ils partagent la
/// **majorité** de leurs membres, dans les deux sens (le clan n'a pas trop
/// changé, et il ne s'est pas noyé dans quelque chose de bien plus gros).
const IDENTITY_OVERLAP_NUM: usize = 1;
const IDENTITY_OVERLAP_DEN: usize = 2;

// — Fission —

/// Pas de relèvement du seuil de lien à chaque tentative de scission d'un
/// groupe trop lâche pour être un seul clan. Assez fin pour trouver la
/// coupure naturelle entre deux sous-groupes réels sans la rater (voir le
/// commentaire de module) ; assez grossier pour rester bon marché (au plus
/// une poignée de paliers avant `FISSION_MAX_THRESHOLD`).
const FISSION_THRESHOLD_STEP: f32 = 0.05;
/// Au-delà de ce seuil de recherche, il ne reste plus que des paires ou des
/// individus isolés dans le sous-graphe des liens les plus forts — inutile
/// de chercher plus loin, aucune scission ne produira deux clans viables.
const FISSION_MAX_THRESHOLD: f32 = 0.95;

// — Relations inter-clans —

/// Distance entre deux foyers de clan en dessous de laquelle leurs
/// territoires sont assez proches pour se disputer une même zone. Deux fois
/// le rayon de résidence : le territoire de chacun s'étend déjà sur
/// `RESIDENCE_RADIUS_TILES`, donc au-delà de deux fois cette distance, les
/// deux zones ne se touchent structurellement plus.
const CONTACT_RADIUS_TILES: f64 = RESIDENCE_RADIUS_TILES * 2.0;
/// Rayon de dépouillement du gibier autour du point médian de deux foyers en
/// contact : la même échelle que le territoire d'un clan (`RESIDENCE_RADIUS_TILES`),
/// pas une zone à part — la ressource contestée, c'est ce que chacun aurait
/// pu chasser depuis chez lui.
const RESOURCE_SURVEY_RADIUS_TILES: f64 = RESIDENCE_RADIUS_TILES;
/// Gibier disponible par personne (les deux clans réunis) en dessous duquel
/// la zone disputée compte comme rare. Estimation de premier ordre (pas
/// minée sur une scène multi-jours comme `COHESION_THRESHOLD` — un clan de
/// quelques dizaines de membres a historiquement besoin d'un ordre de
/// grandeur de plusieurs têtes de gibier par personne dans son rayon de
/// chasse habituel pour ne jamais tomber à la famine, voir le calibrage
/// Phase 2) ; à affiner si une vraie scène à deux clans le contredit.
const SCARCITY_PER_CAPITA: f32 = 3.0;
/// Nudge saturant quotidien de la tension quand la zone est rare et les
/// clans en contact — même idiome que `ENCOUNTER_GAIN`/`skill::practice`.
const TENSION_RISE_RATE: f32 = 0.12;
/// Relâchement quotidien multiplicatif quand ce n'est plus le cas (hors de
/// contact, ou ressource redevenue abondante) — même idiome que
/// `DAILY_DECAY` du graphe d'affinités.
const TENSION_DECAY_RATE: f32 = 0.08;
/// En dessous de ce niveau, une tension résiduelle est oubliée plutôt que
/// portée indéfiniment (borne mémoire, comme `FORGET_THRESHOLD`).
const TENSION_FORGET_THRESHOLD: f32 = 0.02;

/// Identifiant stable d'un clan, monotone — comme `AgentId`/`FaunaId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClanId(pub u64);

/// Un clan détecté : identité, membres, **territoire** (BRIEF §5.1) et
/// **stock commun**. Chef et normes viendront avec les incréments suivants
/// de la Phase 4.
#[derive(Debug, Clone)]
pub struct Clan {
    pub id: ClanId,
    pub founded_tick: u64,
    pub members: BTreeSet<AgentId>,
    /// Centre du territoire (moyenne des positions des membres à la dernière
    /// détection). Lu par `brain::decide` pour attirer les membres qui s'en
    /// éloignent — voir le commentaire de module sur la co-résidence *voulue*.
    pub home: (f64, f64),
    /// Réserve alimentaire commune (mêmes unités que `Physiology::hunger` :
    /// combien de faim elle peut éponger). Alimentée par le surplus des
    /// chasses fructueuses, puisée par les membres affamés sans ressource
    /// locale — voir le commentaire de module. Persiste d'un jour à l'autre
    /// tant que le clan garde son identité (`detect_clans` le reporte) ; un
    /// clan qui s'efface perd son stock avec lui, comme un campement
    /// abandonné — aucune règle de redistribution n'est nécessaire pour un
    /// cas déjà rare.
    pub stock: f32,
    /// Le membre qui maximise `oratoire × prestige` (BRIEF §5.1, voir le
    /// commentaire de module « Le chef »). Provisoire à la construction du
    /// clan (le plus petit membre, un choix arbitraire mais déterministe) —
    /// `elect_chiefs`, appelée juste après dans la même passe quotidienne,
    /// le recalcule toujours avant que quiconque d'autre ne lise `sim.clans`.
    pub chief: AgentId,
    /// La structure que le clan désire bâtir, s'il en désire une
    /// (`crate::structures::plan`, passe quotidienne) — `None` s'il est au
    /// chaud, son stock vide et sans voisin menaçant. Lue par `brain::decide`
    /// pour proposer un candidat `TaskKind::Build`.
    pub desired: Option<StructureKind>,
}

/// Instantané minimal d'un clan pour la délibération (`brain::decide`) : ce
/// qu'un membre a besoin de savoir sur **son** clan pour décider d'y
/// retourner, d'y puiser ou d'y bâtir, sans lui donner accès à la liste des
/// membres.
#[derive(Debug, Clone, Copy)]
pub struct ClanView {
    pub home: (f64, f64),
    pub stock: f32,
    pub desired: Option<StructureKind>,
}

/// Le clan d'appartenance d'un agent, `None` s'il n'en a pas. Composant à
/// part (et non un champ dans `Demographics`) : il est recalculé en bloc
/// chaque jour par `daily`, indépendamment de tout ce qui touche l'individu.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClanMembership(pub Option<ClanId>);

/// Tension mesurée entre paires de clans (BRIEF §9, voir le commentaire de
/// module « Les relations inter-clans »). `BTreeMap` à clé normalisée
/// `(min, max)`, exactement le même patron que `SocialGraph::bonds` — une
/// relation non orientée, jamais itérée dans un ordre non déterministe.
/// Reconstruite en entier chaque jour par `update_relations` : aucune paire
/// n'est modifiée à la main, aucun nettoyage dédié n'est nécessaire quand un
/// clan s'efface (voir le commentaire de module).
#[derive(Debug, Clone, Default)]
pub struct ClanRelations {
    pub tension: BTreeMap<(u64, u64), f32>,
}

impl ClanRelations {
    fn key(a: ClanId, b: ClanId) -> (u64, u64) {
        if a.0 < b.0 { (a.0, b.0) } else { (b.0, a.0) }
    }

    /// La tension entre deux clans (0 s'ils ne se sont jamais côtoyés ou si
    /// elle s'est éteinte depuis).
    pub fn tension_between(&self, a: ClanId, b: ClanId) -> f32 {
        self.tension.get(&Self::key(a, b)).copied().unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClanEventKind {
    Formed,
    Dissolved,
}

/// Trace d'une naissance ou d'une mort de clan — le futur matériau de la
/// Chronique, et déjà l'observable qui prouve l'émergence dans les tests.
#[derive(Debug, Clone, Copy)]
pub struct ClanEvent {
    pub tick: u64,
    pub clan: ClanId,
    pub kind: ClanEventKind,
    pub members: usize,
}

/// Le graphe d'affinités : un poids par paire d'agents qui se sont
/// rencontrés, dans `[0, 1]`. `BTreeMap` (jamais `HashMap`) : la simulation
/// ne doit jamais dépendre d'un ordre d'itération non déterministe. Clé
/// normalisée `(min, max)` — le lien est non orienté.
#[derive(Debug, Clone, Default)]
pub struct SocialGraph {
    pub bonds: BTreeMap<(u64, u64), f32>,
}

impl SocialGraph {
    fn key(a: AgentId, b: AgentId) -> (u64, u64) {
        if a.0 < b.0 { (a.0, b.0) } else { (b.0, a.0) }
    }

    /// Le poids du lien entre deux agents (0 s'ils ne se sont jamais liés).
    pub fn affinity(&self, a: AgentId, b: AgentId) -> f32 {
        self.bonds.get(&Self::key(a, b)).copied().unwrap_or(0.0)
    }

    /// Rapproche un lien de 1,0 d'une fraction `rate` de ce qu'il reste à
    /// parcourir — saturant, comme `skills::practice` : les premières
    /// rencontres comptent plus que les suivantes.
    fn reinforce(&mut self, a: AgentId, b: AgentId, rate: f32) {
        let w = self.bonds.entry(Self::key(a, b)).or_insert(0.0);
        *w += rate * (1.0 - *w);
    }

    /// Retire les liens dont un membre a disparu (mort) : le graphe ne doit
    /// pas porter le poids d'agents qui n'existent plus.
    fn prune_dead(&mut self, alive: &BTreeSet<u64>) {
        self.bonds.retain(|&(a, b), _| alive.contains(&a) && alive.contains(&b));
    }

    /// Relâchement quotidien : chaque lien s'affaiblit un peu ; ceux tombés
    /// sous le seuil d'oubli disparaissent (borne la taille du graphe aux
    /// relations qui comptent encore).
    fn decay(&mut self, rate: f32, floor: f32) {
        self.bonds.retain(|_, w| {
            *w *= 1.0 - rate;
            *w > floor
        });
    }

    /// Ne garde, pour chaque agent, que ses `max_per_agent` liens les plus
    /// forts — et seulement les liens que **les deux côtés** retiennent
    /// mutuellement dans leur top. C'est le plafond de bande passante
    /// sociale : il borne le degré de chaque nœud, quelle que soit la taille
    /// de la population qui voudrait s'y agglutiner.
    fn cap_bonds(&mut self, max_per_agent: usize) {
        let mut adjacency: BTreeMap<u64, Vec<(f32, u64)>> = BTreeMap::new();
        for (&(a, b), &w) in &self.bonds {
            adjacency.entry(a).or_default().push((w, b));
            adjacency.entry(b).or_default().push((w, a));
        }
        let mut top: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
        for (agent, neighbors) in &mut adjacency {
            // Tri décroissant par poids ; égalité départagée par id pour un
            // résultat indépendant de l'ordre d'insertion.
            neighbors.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.1.cmp(&y.1)));
            let kept: BTreeSet<u64> = neighbors.iter().take(max_per_agent).map(|&(_, id)| id).collect();
            top.insert(*agent, kept);
        }
        self.bonds.retain(|&(a, b), _| {
            top.get(&a).is_some_and(|s| s.contains(&b)) && top.get(&b).is_some_and(|s| s.contains(&a))
        });
    }
}

/// Les liens de parenté directe comptent comme une rencontre à part : mère,
/// père, ou fratrie (même mère ou même père connu).
fn are_kin(a_id: AgentId, a: &Kinship, b_id: AgentId, b: &Kinship) -> bool {
    a.mother == Some(b_id)
        || a.father == Some(b_id)
        || b.mother == Some(a_id)
        || b.father == Some(a_id)
        || (a.mother.is_some() && a.mother == b.mother)
        || (a.father.is_some() && a.father == b.father)
}

/// La passe de rencontre, toutes les 4 h (même cadence que
/// `memory::exchange_knowledge`, pour la même raison : un instantané
/// quotidien raterait les croisements de la journée). Tout agent vivant
/// participe, y compris les enfants — c'est ce qui laisse un nourrisson
/// porté se lier à sa mère par la simple proximité continue, sans règle
/// spéciale.
pub(crate) fn encounter(sim: &mut Sim) {
    struct View {
        id: AgentId,
        pos: (f64, f64),
        kin: Kinship,
    }
    let mut views: Vec<View> = sim
        .agents
        .query::<(&AgentId, &Position, &Kinship)>()
        .iter()
        .map(|(_, (id, pos, kin))| View { id: *id, pos: (pos.x, pos.y), kin: *kin })
        .collect();
    views.sort_unstable_by_key(|v| v.id.0);

    // Qui a eu au moins une conversation cette passe : chacun pratique un
    // peu d'oratoire (voir plus bas), qu'il ait parlé une fois ou dix — pas
    // besoin d'un compte exact, juste « a-t-il eu l'occasion de parler ? ».
    let mut spoke: BTreeSet<u64> = BTreeSet::new();
    for i in 0..views.len() {
        for j in (i + 1)..views.len() {
            let (a, b) = (&views[i], &views[j]);
            let d2 = (a.pos.0 - b.pos.0).powi(2) + (a.pos.1 - b.pos.1).powi(2);
            if d2 > TALK_RADIUS_TILES * TALK_RADIUS_TILES {
                continue;
            }
            let rate = if are_kin(a.id, &a.kin, b.id, &b.kin) { KIN_GAIN } else { ENCOUNTER_GAIN };
            sim.social.reinforce(a.id, b.id, rate);
            spoke.insert(a.id.0);
            spoke.insert(b.id.0);
        }
    }
    if spoke.is_empty() {
        return;
    }
    // Pas de nouvelle tâche de délibération pour ça : parler, c'est déjà
    // pratiquer la rhétorique — même conversation, même passe, un candidat
    // de moins à ajouter au softmax de `brain::decide`.
    for (_, (id, traits, skills)) in sim.agents.query_mut::<(&AgentId, &Traits, &mut Skills)>() {
        if spoke.contains(&id.0) {
            skills::practice(&mut skills.oratory, skills::oratory_cap(traits), 1.0);
        }
    }
}

/// Un ensemble disjoint (union-find) à compression de chemin et union par
/// rang : la structure classique pour regrouper des éléments reliés par des
/// paires, en quasi-`O(1)` amorti par opération. Local au module — pas
/// besoin d'une dépendance pour ~20 lignes (BRIEF, règle non négociable n°2).
struct DisjointSet {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl DisjointSet {
    fn new(n: usize) -> Self {
        Self { parent: (0..n).collect(), rank: vec![0; n] }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]); // compression de chemin
        }
        self.parent[x]
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        match self.rank[ra].cmp(&self.rank[rb]) {
            std::cmp::Ordering::Less => self.parent[ra] = rb,
            std::cmp::Ordering::Greater => self.parent[rb] = ra,
            std::cmp::Ordering::Equal => {
                self.parent[rb] = ra;
                self.rank[ra] += 1;
            }
        }
    }
}

/// Sépare `members` en composantes connexes selon `edges` (restreintes aux
/// paires dont les deux bouts sont dans `members`). Tout membre sans arête
/// retenue forme sa propre composante à un seul élément — c'est voulu : un
/// individu qui ne tient plus au groupe qu'à un lien trop faible pour la
/// recherche de scission en cours n'appartient à aucune des deux filles.
fn connected_components(members: &BTreeSet<u64>, edges: &[(u64, u64)]) -> Vec<BTreeSet<u64>> {
    let ids: Vec<u64> = members.iter().copied().collect(); // BTreeSet → déjà trié
    let mut dsu = DisjointSet::new(ids.len());
    for &(a, b) in edges {
        if let (Ok(ia), Ok(ib)) = (ids.binary_search(&a), ids.binary_search(&b)) {
            dsu.union(ia, ib);
        }
    }
    let mut groups: BTreeMap<usize, BTreeSet<u64>> = BTreeMap::new();
    for (i, &id) in ids.iter().enumerate() {
        groups.entry(dsu.find(i)).or_default().insert(id);
    }
    groups.into_values().collect()
}

/// Valide (ou non) un groupe candidat comme clan à part entière : taille,
/// cohésion **toujours mesurée à `BOND_THRESHOLD`** (jamais au seuil de
/// recherche, temporairement relevé, qui sert seulement à trouver une
/// scission — voir le commentaire de module) et co-résidence. `edges` est
/// la liste des relations réelles (poids ≥ `BOND_THRESHOLD`) de toute la
/// population ; seules celles internes à `members` comptent ici. Renvoie le
/// territoire (centroïde) du groupe s'il passe tous les tests.
fn validate_cluster(
    members: &BTreeSet<u64>,
    edges: &[(u64, u64)],
    humans: &[crate::demography::HumanView],
) -> Option<(f64, f64)> {
    if members.len() < MIN_CLAN_SIZE {
        return None;
    }
    let internal_edges = edges.iter().filter(|(a, b)| members.contains(a) && members.contains(b)).count();
    let possible = members.len() * (members.len() - 1) / 2;
    let density = internal_edges as f32 / possible as f32;
    if density < COHESION_THRESHOLD {
        return None;
    }
    let positions: Vec<(f64, f64)> =
        members.iter().filter_map(|&id| find_human(humans, AgentId(id)).map(|h| h.pos)).collect();
    if positions.len() != members.len() {
        return None; // sécurité : ne devrait pas arriver (agent introuvable)
    }
    let (sx, sy) = positions.iter().fold((0.0, 0.0), |(sx, sy), &(x, y)| (sx + x, sy + y));
    let (cx, cy) = (sx / positions.len() as f64, sy / positions.len() as f64);
    let resident_count =
        positions.iter().filter(|&&(x, y)| (x - cx).hypot(y - cy) <= RESIDENCE_RADIUS_TILES).count();
    if (resident_count as f32) < RESIDENCE_FRACTION * positions.len() as f32 {
        return None;
    }
    Some((cx, cy))
}

/// Résout un groupe connexe candidat en zéro, un ou plusieurs clans (voir le
/// commentaire de module, « La fission »). `bonds` est le graphe pondéré
/// complet (pour la recherche de scission à seuil relevé), `edges` la même
/// liste filtrée à `BOND_THRESHOLD` que `validate_cluster` utilise pour
/// mesurer la cohésion — jamais celle, temporaire, de la recherche.
fn resolve_cluster(
    members: BTreeSet<u64>,
    bonds: &BTreeMap<(u64, u64), f32>,
    edges: &[(u64, u64)],
    humans: &[crate::demography::HumanView],
    search_threshold: f32,
) -> Vec<(BTreeSet<AgentId>, (f64, f64))> {
    if let Some(home) = validate_cluster(&members, edges, humans) {
        return vec![(members.into_iter().map(AgentId).collect(), home)];
    }
    if members.len() < 2 * MIN_CLAN_SIZE || search_threshold > FISSION_MAX_THRESHOLD {
        return Vec::new(); // trop petit ou trop lâche pour se scinder : s'efface, comme avant cet incrément.
    }
    let stronger: Vec<(u64, u64)> = bonds
        .iter()
        .filter(|&(&(a, b), &w)| w >= search_threshold && members.contains(&a) && members.contains(&b))
        .map(|(&k, _)| k)
        .collect();
    let sub_groups = connected_components(&members, &stronger);
    if sub_groups.len() <= 1 {
        // Encore un seul morceau à ce seuil : essayer un cran plus haut.
        return resolve_cluster(members, bonds, edges, humans, search_threshold + FISSION_THRESHOLD_STEP);
    }
    sub_groups.into_iter().flat_map(|g| resolve_cluster(g, bonds, edges, humans, search_threshold)).collect()
}

/// La passe quotidienne : entretien du graphe, détection des clans, puis
/// mesure des relations inter-clans (qui a besoin des clans du jour, donc
/// appelée après `detect_clans`). Appelée à la même cadence que
/// `demography::daily` — c'est le rythme « administratif » de la simulation.
pub(crate) fn daily(sim: &mut Sim) {
    let alive: BTreeSet<u64> = sim.agents.query::<&AgentId>().iter().map(|(_, id)| id.0).collect();
    sim.social.prune_dead(&alive);
    sim.social.decay(DAILY_DECAY, FORGET_THRESHOLD);
    sim.social.cap_bonds(MAX_BONDS_PER_AGENT);
    detect_clans(sim);
    update_relations(sim);
    elect_chiefs(sim);
}

/// Désigne le chef de chaque clan (voir le commentaire de module « Le
/// chef ») : le membre qui maximise `oratoire × prestige`, ex æquo
/// départagés par le plus petit `AgentId` — même convention que `claim_at`.
/// Recalculé en entier à partir des scores du jour, jamais modifié
/// incrémentalement : c'est ce recalcul, et lui seul, qui rend la position
/// « contestable » (BRIEF §5.1) sans mécanique de contestation dédiée.
fn elect_chiefs(sim: &mut Sim) {
    if sim.clans.is_empty() {
        return;
    }
    let scores: BTreeMap<u64, f32> = sim
        .agents
        .query::<(&AgentId, &Skills, &Prestige)>()
        .iter()
        .map(|(_, (id, skills, prestige))| (id.0, skills.oratory * prestige.0))
        .collect();
    for clan in &mut sim.clans {
        if let Some(&chief) = clan.members.iter().max_by(|a, b| {
            let sa = scores.get(&a.0).copied().unwrap_or(0.0);
            let sb = scores.get(&b.0).copied().unwrap_or(0.0);
            sa.total_cmp(&sb).then(b.0.cmp(&a.0)) // égalité : plus petit AgentId l'emporte
        }) {
            clan.chief = chief;
        }
    }
}

/// Le gibier disponible (somme des effectifs de troupeaux, la même notion
/// que la chasse) dans un rayon de `RESOURCE_SURVEY_RADIUS_TILES` autour
/// d'un point — le proxy le plus simple pour « ce que ces deux clans
/// pourraient se disputer », sans rien inventer de nouveau.
fn contested_game(point: (f64, f64), herds: &[(f64, f64, f32)]) -> f32 {
    herds
        .iter()
        .filter(|&&(x, y, _)| (x - point.0).hypot(y - point.1) <= RESOURCE_SURVEY_RADIUS_TILES)
        .map(|&(_, _, population)| population)
        .sum()
}

/// Mesure, pour chaque paire de clans, si leur voisinage justifie une
/// tension — voir le commentaire de module. Reconstruit `sim.clan_relations`
/// en entier à partir des seuls clans qui existent encore aujourd'hui,
/// exactement comme `detect_clans` reconstruit `sim.clans`.
fn update_relations(sim: &mut Sim) {
    let herds: Vec<(f64, f64, f32)> = sim
        .fauna
        .query::<(&Herd, &Position)>()
        .iter()
        .map(|(_, (herd, pos))| (pos.x, pos.y, herd.population))
        .collect();

    let mut next: BTreeMap<(u64, u64), f32> = BTreeMap::new();
    for i in 0..sim.clans.len() {
        for j in (i + 1)..sim.clans.len() {
            let (a, b) = (&sim.clans[i], &sim.clans[j]);
            let dist = (a.home.0 - b.home.0).hypot(a.home.1 - b.home.1);
            let scarce = dist <= CONTACT_RADIUS_TILES && {
                let midpoint = ((a.home.0 + b.home.0) / 2.0, (a.home.1 + b.home.1) / 2.0);
                let mouths = (a.members.len() + b.members.len()) as f32;
                contested_game(midpoint, &herds) / mouths.max(1.0) < SCARCITY_PER_CAPITA
            };
            let key = ClanRelations::key(a.id, b.id);
            let previous = sim.clan_relations.tension_between(a.id, b.id);
            // Une palissade (de l'un ou l'autre) freine la montée : un clan
            // qui tient sa position escalade moins (voir `structures`).
            let defended = structures::has_palisade(a.id, &sim.structures)
                || structures::has_palisade(b.id, &sim.structures);
            let rise = if defended {
                TENSION_RISE_RATE * structures::PALISADE_TENSION_FACTOR
            } else {
                TENSION_RISE_RATE
            };
            let updated = if scarce {
                previous + rise * (1.0 - previous)
            } else {
                previous * (1.0 - TENSION_DECAY_RATE)
            };
            if updated > TENSION_FORGET_THRESHOLD {
                next.insert(key, updated);
            }
        }
    }
    sim.clan_relations.tension = next;
}

/// La force d'influence d'un clan en un point : maximale à son foyer,
/// décroît linéairement jusqu'à s'annuler à `RESIDENCE_RADIUS_TILES` — même
/// rayon que le territoire-attracteur et la co-résidence (voir le
/// commentaire de module). Pondérée par l'effectif : un clan plus peuplé
/// projette naturellement plus loin dans une zone disputée, sans qu'aucun
/// seuil de taille ne soit jamais lu directement (conséquence arithmétique,
/// comme la borne de fission).
fn territory_strength(point: (f64, f64), clan: &Clan) -> f32 {
    let dist = (point.0 - clan.home.0).hypot(point.1 - clan.home.1);
    clan.members.len() as f32 * (1.0 - (dist / RESIDENCE_RADIUS_TILES) as f32).max(0.0)
}

/// Le champ de territoire diffusé (voir le commentaire de module) : quel
/// clan, s'il en existe un, revendique ce point — le clan de force maximale
/// s'il en existe une strictement positive. Égalité départagée par le plus
/// petit `ClanId` (déterminisme, même convention que `cap_bonds`) : n'arrive
/// que si deux clans de même effectif ont leurs foyers exactement
/// équidistants du point, un cas de mesure nulle en pratique mais qui doit
/// rester reproductible.
pub fn claim_at(point: (f64, f64), clans: &[Clan]) -> Option<ClanId> {
    clans
        .iter()
        .map(|c| (c.id, territory_strength(point, c)))
        .filter(|&(_, strength)| strength > 0.0)
        .max_by(|(id_a, s_a), (id_b, s_b)| s_a.total_cmp(s_b).then(id_b.cmp(id_a)))
        .map(|(id, _)| id)
}

/// Détecte les groupes du graphe d'affinités qui franchissent à la fois le
/// seuil de cohésion et de co-résidence, puis réconcilie avec les clans
/// existants (voir le commentaire de module sur la persistance d'identité).
fn detect_clans(sim: &mut Sim) {
    let humans = sim.human_views(); // trié par id, position courante de chacun

    // Arêtes « vraies relations » : au-dessus du seuil de lien. `cap_bonds`
    // borne déjà le degré, mais un agent presque sans contact garde ses
    // quelques liens même faibles — ce filtre ne garde que ceux qui comptent.
    let edges: Vec<(u64, u64)> =
        sim.social.bonds.iter().filter(|&(_, &w)| w >= BOND_THRESHOLD).map(|(&k, _)| k).collect();

    let mut ids: BTreeSet<u64> = BTreeSet::new();
    for &(a, b) in &edges {
        ids.insert(a);
        ids.insert(b);
    }
    let ids: Vec<u64> = ids.into_iter().collect(); // BTreeSet → déjà trié

    let mut dsu = DisjointSet::new(ids.len());
    for &(a, b) in &edges {
        let (ia, ib) = (ids.binary_search(&a).unwrap(), ids.binary_search(&b).unwrap());
        dsu.union(ia, ib);
    }

    let mut groups: BTreeMap<usize, Vec<u64>> = BTreeMap::new();
    for (i, &id) in ids.iter().enumerate() {
        let root = dsu.find(i);
        groups.entry(root).or_default().push(id);
    }

    // Chaque composante connexe est un **candidat** — encore faut-il qu'il
    // soit assez gros, assez dense (cohésion) et assez compact (résidence).
    // `resolve_cluster` valide le groupe tel quel, ou, s'il échoue,
    // cherche s'il cache plusieurs sous-groupes plus densément liés
    // (fission, voir le commentaire de module) : dans les deux cas, le
    // centroïde de chaque cluster retenu devient le **territoire** du clan
    // (`Clan::home`).
    let mut clusters: Vec<(BTreeSet<AgentId>, (f64, f64))> = Vec::new();
    for members in groups.into_values() {
        let member_set: BTreeSet<u64> = members.into_iter().collect();
        clusters.extend(resolve_cluster(
            member_set,
            &sim.social.bonds,
            &edges,
            &humans,
            BOND_THRESHOLD + FISSION_THRESHOLD_STEP,
        ));
    }
    // Ordre déterministe et stable pour l'attribution des nouveaux
    // identifiants : par plus petit membre.
    clusters.sort_by_key(|(c, _)| c.iter().next().copied());

    // Réconciliation avec les clans existants : chacun cherche, parmi les
    // clusters encore libres, celui avec lequel il partage le plus de
    // membres. S'ils se recouvrent majoritairement dans les deux sens,
    // c'est le même clan qui continue (et son territoire se recentre sur le
    // nouveau centroïde) ; sinon il s'efface.
    let mut matched = vec![false; clusters.len()];
    let mut next: Vec<Clan> = Vec::new();
    for clan in std::mem::take(&mut sim.clans) {
        let best = clusters
            .iter()
            .enumerate()
            .filter(|(i, _)| !matched[*i])
            .map(|(i, (c, _))| (i, clan.members.intersection(c).count()))
            .filter(|&(_, overlap)| overlap > 0)
            .max_by_key(|&(_, overlap)| overlap);
        let kept = best.is_some_and(|(i, overlap)| {
            let (cluster, home) = &clusters[i];
            let majority_old = overlap * IDENTITY_OVERLAP_DEN >= clan.members.len() * IDENTITY_OVERLAP_NUM;
            let majority_new = overlap * IDENTITY_OVERLAP_DEN >= cluster.len() * IDENTITY_OVERLAP_NUM;
            if majority_old && majority_new {
                matched[i] = true;
                next.push(Clan {
                    id: clan.id,
                    founded_tick: clan.founded_tick,
                    members: cluster.clone(),
                    home: *home,
                    stock: clan.stock,
                    chief: clan.chief, // provisoire : `elect_chiefs` le recalcule juste après
                    desired: clan.desired, // reporté ; `structures::plan` le recalcule à minuit
                });
                true
            } else {
                false
            }
        });
        if !kept {
            sim.clan_events.push(ClanEvent {
                tick: sim.time.tick,
                clan: clan.id,
                kind: ClanEventKind::Dissolved,
                members: clan.members.len(),
            });
        }
    }
    for (i, (cluster, home)) in clusters.into_iter().enumerate() {
        if matched[i] {
            continue;
        }
        let id = ClanId(sim.next_clan_id);
        sim.next_clan_id += 1;
        sim.clan_events.push(ClanEvent {
            tick: sim.time.tick,
            clan: id,
            kind: ClanEventKind::Formed,
            members: cluster.len(),
        });
        next.push(Clan {
            id,
            founded_tick: sim.time.tick,
            chief: *cluster.iter().next().unwrap(), // provisoire, voir la doc du champ
            members: cluster,
            home,
            stock: 0.0,
            desired: None, // un clan neuf n'a encore rien mesuré ; `structures::plan` décidera
        });
    }
    next.sort_by_key(|c| c.id.0);

    // Synchronise le composant `ClanMembership` de chaque agent avec le
    // verdict du jour.
    let mut membership: BTreeMap<u64, ClanId> = BTreeMap::new();
    for clan in &next {
        for &member in &clan.members {
            membership.insert(member.0, clan.id);
        }
    }
    for (_, (id, cm)) in sim.agents.query_mut::<(&AgentId, &mut ClanMembership)>() {
        cm.0 = membership.get(&id.0).copied();
    }
    sim.clans = next;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_rencontres_renforcent_le_lien_et_saturent() {
        let mut g = SocialGraph::default();
        let (a, b) = (AgentId(0), AgentId(1));
        assert_eq!(g.affinity(a, b), 0.0);
        for _ in 0..14 {
            g.reinforce(a, b, ENCOUNTER_GAIN);
        }
        assert!(g.affinity(a, b) >= BOND_THRESHOLD, "14 rencontres doivent franchir le seuil de lien");
        for _ in 0..500 {
            g.reinforce(a, b, ENCOUNTER_GAIN);
        }
        assert!(g.affinity(a, b) < 1.0 && g.affinity(a, b) > 0.99, "sature sous 1,0, ne l'atteint jamais");
    }

    #[test]
    fn la_parente_lie_plus_vite_que_la_rencontre_ordinaire() {
        let mut g = SocialGraph::default();
        let (a, b) = (AgentId(0), AgentId(1));
        g.reinforce(a, b, KIN_GAIN);
        let kin_after_one = g.affinity(a, b);
        let mut h = SocialGraph::default();
        h.reinforce(a, b, ENCOUNTER_GAIN);
        assert!(kin_after_one > h.affinity(a, b));
    }

    #[test]
    fn un_lien_neglige_s_efface() {
        let mut g = SocialGraph::default();
        let (a, b) = (AgentId(0), AgentId(1));
        for _ in 0..30 {
            g.reinforce(a, b, ENCOUNTER_GAIN);
        }
        assert!(g.affinity(a, b) > 0.5);
        for _ in 0..400 {
            g.decay(DAILY_DECAY, FORGET_THRESHOLD);
        }
        assert_eq!(g.affinity(a, b), 0.0, "le lien doit avoir été oublié (retiré de la carte)");
    }

    #[test]
    fn le_plafond_de_liens_ne_garde_que_les_plus_forts_mutuels() {
        let mut g = SocialGraph::default();
        let hub = AgentId(0);
        // Vingt liens forts, tous plus forts que MAX_BONDS_PER_AGENT ne peut
        // en garder : le plus faible doit tomber en premier.
        for i in 1..=20u64 {
            // Poids décroissant avec l'id : (1) est le plus fort, (20) le plus faible.
            let w = 1.0 - i as f32 * 0.01;
            g.bonds.insert(SocialGraph::key(hub, AgentId(i)), w);
            // L'autre bout doit aussi placer ce lien dans son propre top —
            // ici chacun n'a qu'un seul lien, donc c'est trivialement son n°1.
        }
        g.cap_bonds(MAX_BONDS_PER_AGENT);
        let degree = g.bonds.keys().filter(|&&(a, b)| a == hub.0 || b == hub.0).count();
        assert_eq!(degree, MAX_BONDS_PER_AGENT, "le hub ne doit garder que ses liens les plus forts");
        assert!(
            g.affinity(hub, AgentId(20)) == 0.0,
            "le lien le plus faible du hub doit avoir été coupé"
        );
        assert!(g.affinity(hub, AgentId(1)) > 0.0, "le lien le plus fort doit rester");
    }

    use cairn_core::WorldSeed;

    #[test]
    fn le_plafond_coupe_un_lien_non_mutuel() {
        // A considère B comme son ami le plus proche, mais B a par ailleurs
        // MAX_BONDS_PER_AGENT autres liens plus forts que celui avec A : le
        // lien A–B doit disparaître (il ne survit que si les deux le
        // retiennent dans leur top).
        let mut g = SocialGraph::default();
        let (a, b) = (AgentId(0), AgentId(1));
        g.bonds.insert(SocialGraph::key(a, b), 0.9);
        for i in 2..=(1 + MAX_BONDS_PER_AGENT as u64) {
            g.bonds.insert(SocialGraph::key(b, AgentId(i)), 0.95);
        }
        g.cap_bonds(MAX_BONDS_PER_AGENT);
        assert_eq!(g.affinity(a, b), 0.0, "lien non mutuel dans le top : doit être coupé");
    }

    use crate::demography::{HumanView, Sex};

    fn human_at(id: u64, x: f64, y: f64) -> HumanView {
        HumanView { id: AgentId(id), pos: (x, y), sex: Sex::Female, adult: true }
    }

    type SocialFixture = (BTreeSet<u64>, BTreeMap<(u64, u64), f32>, Vec<HumanView>);

    /// Deux sous-groupes de 10, chacun assez dense et compact pour être un
    /// clan à lui seul, reliés par une poignée de liens plus faibles qu'eux
    /// (mais tout de même au-dessus de `BOND_THRESHOLD`, donc comptés dans
    /// la mesure de cohésion de l'ensemble). Le tout, pris comme un seul
    /// groupe de 20, ne passe PAS le seuil de cohésion (les liens internes
    /// des deux moitiés ne suffisent pas à densifier 190 paires possibles) —
    /// exactement le cas que la fission doit résoudre.
    fn two_dense_halves_bridged() -> SocialFixture {
        let mut bonds: BTreeMap<(u64, u64), f32> = BTreeMap::new();
        let mut humans: Vec<HumanView> = Vec::new();
        for half in 0..2u64 {
            let base = half * 10;
            for i in 0..10u64 {
                let id = base + i;
                humans.push(human_at(id, half as f64 * 10_000.0 + i as f64 * 2.0, 0.0));
            }
            // Densité interne : chaînes de voisinage jusqu'à distance 3
            // (24 arêtes sur 45 paires possibles = 0,53, largement au-dessus
            // de COHESION_THRESHOLD).
            for k in 1..=3u64 {
                for i in 0..(10 - k) {
                    bonds.insert(SocialGraph::key(AgentId(base + i), AgentId(base + i + k)), 0.9);
                }
            }
        }
        // Deux ponts, plus faibles que les liens internes : ce sont eux, et
        // seulement eux, que la recherche de scission doit couper en premier.
        bonds.insert(SocialGraph::key(AgentId(4), AgentId(14)), 0.55);
        bonds.insert(SocialGraph::key(AgentId(7), AgentId(17)), 0.55);
        humans.sort_by_key(|h| h.id.0);
        let members: BTreeSet<u64> = (0..20).collect();
        (members, bonds, humans)
    }

    /// Le cœur de l'incrément 5 (BRIEF §9 : « un clan trop nombreux
    /// fissionne ») : un groupe qui échoue le test de cohésion pris en bloc
    /// se scinde en deux clans filles viables plutôt que de s'effacer,
    /// **sans qu'aucun seuil de taille ne soit jamais lu directement** —
    /// seule la structure du graphe (deux sous-groupes densément liés,
    /// faiblement reliés entre eux) décide.
    #[test]
    fn un_groupe_trop_lache_se_scinde_en_deux_clans_filles() {
        let (members, bonds, humans) = two_dense_halves_bridged();
        let edges: Vec<(u64, u64)> = bonds.keys().copied().collect(); // tous ≥ BOND_THRESHOLD ici

        assert!(
            validate_cluster(&members, &edges, &humans).is_none(),
            "les 20 pris en bloc ne doivent PAS passer le seuil de cohésion (c'est le point de départ du test)"
        );

        let result = resolve_cluster(members, &bonds, &edges, &humans, BOND_THRESHOLD + FISSION_THRESHOLD_STEP);
        assert_eq!(result.len(), 2, "doit se scinder en exactement deux clans filles");
        for (cluster, _) in &result {
            assert_eq!(cluster.len(), 10, "chaque fille doit retrouver sa moitié complète");
        }
        let mut all_members: BTreeSet<AgentId> = BTreeSet::new();
        for (cluster, _) in &result {
            all_members.extend(cluster.iter().copied());
        }
        assert_eq!(all_members.len(), 20, "aucun membre perdu ni dupliqué pendant la scission");
        let (a, _) = &result[0];
        let (b, _) = &result[1];
        assert!(a.is_disjoint(b), "les deux clans filles ne doivent partager aucun membre");
    }

    /// Contre-épreuve : un groupe trop lâche mais **sans** structure interne
    /// à exploiter (ici un simple anneau de voisinage, une seule force de
    /// lien partout) ne doit produire AUCUN clan — la recherche de scission
    /// ne doit pas fabriquer une coupure là où il n'y en a pas. Il finit par
    /// se réduire à des paires/individus isolés, tous trop petits pour être
    /// un clan : le même sort qu'avant l'incrément fission.
    #[test]
    fn un_groupe_sans_sous_structure_ne_fissionne_pas_et_s_efface() {
        let mut bonds: BTreeMap<(u64, u64), f32> = BTreeMap::new();
        let mut humans: Vec<HumanView> = Vec::new();
        for i in 0..20u64 {
            humans.push(human_at(i, i as f64 * 2.0, 0.0));
            bonds.insert(SocialGraph::key(AgentId(i), AgentId((i + 1) % 20)), 0.9);
        }
        humans.sort_by_key(|h| h.id.0);
        let members: BTreeSet<u64> = (0..20).collect();
        let edges: Vec<(u64, u64)> = bonds.keys().copied().collect();

        assert!(
            validate_cluster(&members, &edges, &humans).is_none(),
            "un anneau de 20 (densité ~0,1) ne doit pas passer le seuil de cohésion"
        );
        let result = resolve_cluster(members, &bonds, &edges, &humans, BOND_THRESHOLD + FISSION_THRESHOLD_STEP);
        assert!(
            result.is_empty(),
            "sans sous-groupe réellement plus dense, le groupe doit s'effacer, pas fissionner artificiellement"
        );
    }

    /// Deux clans synthétiques de 10 membres chacun, positionnés à distance
    /// contrôlée, avec un unique troupeau à leur point médian — assez pour
    /// injecter `sim.clans`/`sim.fauna` sans passer par une scène complète
    /// (même patron qu'à l'incrément 3 : tester le mécanisme directement).
    fn two_clans_and_one_herd(gap_tiles: f64, herd_population: f32) -> Sim {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let home_a = (0.0, 0.0);
        let home_b = (gap_tiles, 0.0);
        sim.clans.push(Clan {
            id: ClanId(1),
            founded_tick: 0,
            members: (0..10u64).map(AgentId).collect(),
            home: home_a,
            stock: 0.0,
            chief: AgentId(0),
            desired: None,
        });
        sim.clans.push(Clan {
            id: ClanId(2),
            founded_tick: 0,
            members: (100..110u64).map(AgentId).collect(),
            home: home_b,
            stock: 0.0,
            chief: AgentId(100),
            desired: None,
        });
        let midpoint = ((home_a.0 + home_b.0) / 2.0, (home_a.1 + home_b.1) / 2.0);
        sim.spawn_herd(midpoint.0, midpoint.1, herd_population);
        sim
    }

    /// LE critère n°2 de la Phase 4 (BRIEF §9) : « deux clans voisins sur
    /// une ressource rare entrent en tension de manière observable ». Deux
    /// clans en contact (foyers à moins de `CONTACT_RADIUS_TILES`) partagent
    /// un seul petit troupeau (10 têtes pour 20 bouches, largement sous
    /// `SCARCITY_PER_CAPITA`) : la tension doit monter, jour après jour,
    /// sans jamais dépasser 1,0 (nudge saturant, même idiome que les liens
    /// sociaux).
    #[test]
    fn deux_clans_voisins_sur_une_ressource_rare_entrent_en_tension() {
        let mut sim = two_clans_and_one_herd(CONTACT_RADIUS_TILES - 500.0, 10.0);
        let (a, b) = (ClanId(1), ClanId(2));
        assert_eq!(sim.clan_relations.tension_between(a, b), 0.0, "aucune tension avant toute mesure");

        let mut previous = 0.0;
        for day in 0..20 {
            update_relations(&mut sim);
            let tension = sim.clan_relations.tension_between(a, b);
            assert!(tension > previous, "jour {day} : la tension doit strictement monter tant que c'est rare");
            assert!(tension < 1.0, "jour {day} : la tension sature sous 1,0, ne l'atteint jamais");
            previous = tension;
        }
        assert!(previous > 0.6, "après 20 jours de disette partagée, la tension doit être clairement établie");
    }

    /// Contre-épreuve n°1 : les deux mêmes clans, mêmes foyers, mais un
    /// troupeau surabondant (1000 têtes pour 20 bouches) — la ressource
    /// n'est plus rare, donc **aucune** tension ne doit apparaître malgré la
    /// proximité. La cohabitation seule ne suffit pas.
    #[test]
    fn deux_clans_voisins_sur_une_ressource_abondante_restent_en_paix() {
        let mut sim = two_clans_and_one_herd(CONTACT_RADIUS_TILES - 500.0, 1000.0);
        let (a, b) = (ClanId(1), ClanId(2));
        for _ in 0..20 {
            update_relations(&mut sim);
        }
        assert_eq!(
            sim.clan_relations.tension_between(a, b),
            0.0,
            "une ressource abondante ne doit jamais faire monter la tension, même entre voisins"
        );
    }

    /// Contre-épreuve n°2 : les deux mêmes clans, mais hors de portée de
    /// contact (foyers à plus de `CONTACT_RADIUS_TILES`), avec le même
    /// troupeau minuscule qui les ferait entrer en tension s'ils étaient
    /// proches. Pas de tension non plus : la rareté seule ne suffit pas sans
    /// voisinage — les deux conditions du critère BRIEF comptent vraiment.
    #[test]
    fn deux_clans_trop_eloignes_ne_se_disputent_jamais_la_ressource() {
        let mut sim = two_clans_and_one_herd(CONTACT_RADIUS_TILES + 500.0, 10.0);
        let (a, b) = (ClanId(1), ClanId(2));
        for _ in 0..20 {
            update_relations(&mut sim);
        }
        assert_eq!(
            sim.clan_relations.tension_between(a, b),
            0.0,
            "hors de contact, deux clans ne doivent jamais entrer en tension, quelle que soit la ressource"
        );
    }

    fn synthetic_clan(id: u64, home: (f64, f64), members: usize) -> Clan {
        Clan {
            id: ClanId(id),
            founded_tick: 0,
            members: (0..members as u64).map(|m| AgentId(id * 1000 + m)).collect(),
            home,
            stock: 0.0,
            chief: AgentId(id * 1000),
            desired: None,
        }
    }

    /// Le champ de territoire (incrément 7) revendique le foyer lui-même :
    /// la force d'un clan y est maximale (distance nulle).
    #[test]
    fn un_point_au_foyer_est_revendique_par_son_clan() {
        let clan = synthetic_clan(1, (100.0, 200.0), 10);
        assert_eq!(claim_at((100.0, 200.0), std::slice::from_ref(&clan)), Some(clan.id));
    }

    /// Au-delà de `RESIDENCE_RADIUS_TILES`, la force de tout clan est nulle
    /// (clampée) — le point reste libre, pas revendiqué par défaut.
    #[test]
    fn un_point_hors_de_portee_de_tout_clan_reste_libre() {
        let clan = synthetic_clan(1, (0.0, 0.0), 10);
        let far = (RESIDENCE_RADIUS_TILES * 2.0, 0.0);
        assert_eq!(claim_at(far, &[clan]), None, "hors du rayon de territoire, personne ne revendique le point");
    }

    /// Le cœur de la diffusion : entre deux clans de même effectif, un point
    /// plus proche du foyer de A que de celui de B est revendiqué par A —
    /// la force décroît avec la distance, ce n'est pas une simple présence
    /// binaire.
    #[test]
    fn le_champ_favorise_le_clan_dont_le_foyer_est_le_plus_proche() {
        let a = synthetic_clan(1, (0.0, 0.0), 10);
        let b = synthetic_clan(2, (2000.0, 0.0), 10);
        let closer_to_a = (400.0, 0.0);
        assert_eq!(claim_at(closer_to_a, &[a, b]), Some(ClanId(1)));
    }

    /// Un clan plus peuplé projette une force plus forte à distance égale —
    /// conséquence arithmétique du facteur `membres` dans
    /// `territory_strength`, jamais un seuil de population lu directement
    /// (même philosophie que la borne de fission).
    #[test]
    fn un_clan_plus_peuple_l_emporte_a_distance_egale() {
        let small = synthetic_clan(1, (0.0, 0.0), 5);
        let big = synthetic_clan(2, (1000.0, 0.0), 50);
        let midpoint = (500.0, 0.0); // équidistant des deux foyers
        assert_eq!(
            claim_at(midpoint, &[small, big]),
            Some(ClanId(2)),
            "à distance strictement égale, le clan le plus peuplé doit l'emporter"
        );
    }

    /// Égalité exacte (même effectif, point équidistant) : départagée par le
    /// plus petit `ClanId`, et **indépendante de l'ordre du slice** — la
    /// détermination ne doit jamais dépendre d'un ordre d'itération
    /// accidentel (règle de déterminisme du projet).
    #[test]
    fn l_egalite_stricte_est_departagee_par_le_plus_petit_id_quel_que_soit_l_ordre() {
        let a = synthetic_clan(1, (0.0, 0.0), 10);
        let b = synthetic_clan(2, (1000.0, 0.0), 10);
        let midpoint = (500.0, 0.0);
        assert_eq!(claim_at(midpoint, &[a.clone(), b.clone()]), Some(ClanId(1)));
        assert_eq!(claim_at(midpoint, &[b, a]), Some(ClanId(1)), "le résultat ne doit pas dépendre de l'ordre du slice");
    }

    fn oratory_of(sim: &Sim, id: AgentId) -> f32 {
        sim.agents
            .query::<(&AgentId, &Skills)>()
            .iter()
            .find(|(_, (i, _))| i.0 == id.0)
            .map(|(_, (_, s))| s.oratory)
            .expect("agent introuvable")
    }

    /// Le cœur de l'incrément 8 côté compétence : parler (`encounter`, la
    /// même passe qui renforce les liens d'affinité) pratique l'oratoire —
    /// sans tâche dédiée, sans candidat de plus dans `brain::decide`.
    #[test]
    fn parler_pratique_l_oratoire() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let a = sim.spawn_agent(0.0, 0.0);
        sim.spawn_agent(1.0, 0.0); // à portée de conversation de `a`
        let before = oratory_of(&sim, a);
        for _ in 0..30 {
            encounter(&mut sim);
        }
        assert!(oratory_of(&sim, a) > before, "trente occasions de parler doivent faire progresser l'oratoire");
    }

    /// Contre-épreuve : un agent isolé, jamais à portée de qui que ce soit,
    /// ne pratique jamais l'oratoire — la compétence se forge en parlant
    /// pour de vrai, pas par simple écoulement du temps.
    #[test]
    fn un_agent_isole_ne_pratique_jamais_l_oratoire() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let lonely = sim.spawn_agent(0.0, 0.0);
        sim.spawn_agent(50_000.0, 0.0); // bien au-delà de la portée de conversation
        let before = oratory_of(&sim, lonely);
        for _ in 0..30 {
            encounter(&mut sim);
        }
        assert_eq!(oratory_of(&sim, lonely), before, "sans personne à portée, l'oratoire ne doit pas bouger");
    }

    fn set_scores(sim: &mut Sim, id: AgentId, oratory: f32, prestige: f32) {
        for (_, (agent_id, skills, agent_prestige)) in
            sim.agents.query_mut::<(&AgentId, &mut Skills, &mut Prestige)>()
        {
            if agent_id.0 == id.0 {
                skills.oratory = oratory;
                agent_prestige.0 = prestige;
            }
        }
    }

    /// Le cœur de l'incrément 8 : le chef est celui qui maximise
    /// `oratoire × prestige`, jamais le seul plus prestigieux ou le seul
    /// meilleur orateur pris isolément.
    #[test]
    fn le_chef_est_celui_qui_maximise_oratoire_fois_prestige() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let low = sim.spawn_agent(0.0, 0.0);
        let high = sim.spawn_agent(0.0, 0.0);
        let silent_but_prestigious = sim.spawn_agent(0.0, 0.0);
        set_scores(&mut sim, low, 0.3, 1.0); // score 0,3
        set_scores(&mut sim, high, 0.5, 2.0); // score 1,0 — doit gagner
        set_scores(&mut sim, silent_but_prestigious, 0.0, 100.0); // score nul malgré un immense prestige : sans oratoire, on ne mène pas
        sim.clans.push(Clan {
            id: ClanId(1),
            founded_tick: 0,
            members: [low, high, silent_but_prestigious].into_iter().collect(),
            home: (0.0, 0.0),
            stock: 0.0,
            chief: low, // valeur de départ arbitraire : doit changer
            desired: None,
        });

        elect_chiefs(&mut sim);

        assert_eq!(sim.clans[0].chief, high, "le membre au produit oratoire×prestige le plus élevé doit être élu");
    }

    /// Égalité stricte de score : départagée par le plus petit `AgentId`,
    /// même convention que `claim_at` — le déterminisme du projet ne doit
    /// jamais dépendre d'un tirage arbitraire.
    #[test]
    fn l_egalite_de_score_est_departagee_par_le_plus_petit_agent_id() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let a = sim.spawn_agent(0.0, 0.0);
        let b = sim.spawn_agent(0.0, 0.0);
        set_scores(&mut sim, a, 0.4, 1.0);
        set_scores(&mut sim, b, 0.4, 1.0);
        sim.clans.push(Clan {
            id: ClanId(1),
            founded_tick: 0,
            members: [a, b].into_iter().collect(),
            home: (0.0, 0.0),
            stock: 0.0,
            chief: b,
            desired: None,
        });

        elect_chiefs(&mut sim);

        assert_eq!(sim.clans[0].chief, a, "égalité stricte : le plus petit AgentId doit l'emporter");
    }
}
