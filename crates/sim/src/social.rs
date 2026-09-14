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
//! ## Une ligne de faille, pas un seuil de taille
//!
//! Le brief demande aussi qu'« un clan trop nombreux fissionne ». Écrire
//! « si taille > N, couper en deux » serait exactement l'anti-pattern à
//! refuser (BRIEF §11).
//!
//! **Une première version a échoué à l'éviter**, et il vaut la peine de dire
//! comment, parce que l'erreur était invisible. Elle exigeait une **densité**
//! absolue (`COHESION_THRESHOLD`), en s'appuyant sur l'idée que le plafond de
//! liens par agent (`MAX_BONDS_PER_AGENT` — l'hypothèse de Dunbar) ferait
//! décroître la densité atteignable en `1/n`, si bien qu'un groupe trop gros
//! ne pourrait plus la satisfaire : une borne de taille présentée comme une
//! conséquence arithmétique. C'en était une — mais c'était **quand même une
//! règle de taille**, simplement déguisée en arithmétique. Calibré à 0,28 sur
//! une scène de 24 agents (où le maximum atteignable valait 0,65), le seuil
//! devenait *mathématiquement* impossible au-delà de 54 membres. Mesuré sur un
//! vrai run (`example diagnose`, 9 857 verdicts) : les groupes refusés pour
//! cohésion faisaient **58 membres en moyenne**, et les clans vivaient
//! **3,6 jours** en médiane — la population entière formait un seul groupe
//! connexe, refusé chaque nuit, recoupé chaque nuit à un endroit différent.
//!
//! La question posée est donc désormais l'inverse : non pas « ce groupe est-il
//! assez dense ? » (une question dont la réponse dépend de sa taille), mais
//! **« ce groupe a-t-il une ligne de faille ? »** — mesurée par la
//! **modularité** (Newman), qui teste si une partition explique la structure
//! du graphe mieux que le hasard. Une ligne de faille nette : le groupe se
//! scinde le long de cette ligne. Aucune : c'est un peuple, quelle que soit sa
//! taille.
//!
//! C'est là que la borne de taille émerge **vraiment**, sans qu'aucun effectif
//! ne soit lu nulle part : avec au plus `MAX_BONDS_PER_AGENT` relations par
//! personne, un groupe de deux cents *ne peut pas* être sans structure interne
//! — il porte forcément des communautés, donc il se scinde. Un groupe de
//! quarante qui se connaît vraiment, lui, n'en porte aucune, et reste entier.
//! La différence avec la version précédente est que la coupure suit maintenant
//! **quelque chose de réel** : comme la parenté renforce les liens plus vite
//! que la simple rencontre (`KIN_GAIN` > `ENCOUNTER_GAIN`), les communautés
//! détectées sont les lignages — et une famille étendue est encore la même
//! demain, là où une coupure arbitraire changeait toutes les nuits.
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
use crate::chronicle::EventKind;
use crate::fauna::Herd;
use crate::demography::{Kinship, Traits, find_human};
use crate::memory::TALK_RADIUS_TILES;
use crate::sim::Sim;
use crate::skills::{self, Skills};
use crate::structures::{self, StructureKind};
use cairn_core::{TICKS_PER_DAY, km_to_tiles};

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
/// Modularité minimale pour qu'une partition compte comme une **ligne de
/// faille** et scinde le groupe (voir [`modularity`] et l'en-tête de module).
///
/// 0,3 est la valeur conventionnelle en détection de communautés : en dessous,
/// la partition n'explique guère mieux la structure du graphe qu'un tirage
/// aléatoire de mêmes degrés — autrement dit, le groupe est **uni**, et le
/// couper serait inventer une frontière qui n'existe pas. C'est ce qui arrivait
/// avec le critère de densité qu'elle remplace : faute de pouvoir satisfaire un
/// seuil devenu inatteignable, un peuple entier était recoupé chaque nuit à un
/// endroit différent.
///
/// Contrairement à une densité, ce seuil est **adimensionnel** : il ne se
/// dégrade pas quand la population grandit, et n'aura donc pas à être
/// recalibré à chaque changement d'échelle.
const MODULARITY_THRESHOLD: f64 = 0.3;

/// Modularité exigée pour scinder un groupe qui **prolonge un clan existant**.
///
/// L'hystérésis, et la raison d'être de ce seuil : un critère unique comparé
/// chaque nuit à une valeur qui fluctue fait **basculer** la décision d'un jour
/// à l'autre. Mesuré (`example diagnose`, 5 ans) : 118 fissions produisaient
/// 161 clans éteints, alors que la composition des groupes était stable à
/// **96 %** d'un jour sur l'autre — le peuple ne changeait pas, seule la
/// découpe changeait d'avis. Un peuple déjà reconnu demande donc une faille
/// franche pour se briser, là où un groupe qui vient de se former se juge au
/// seuil ordinaire.
///
/// C'est le même principe que le cadrage à hystérésis de la caméra du client :
/// on ne réagit pas à un franchissement, on réagit à un franchissement **net**.
const MODULARITY_ESTABLISHED: f64 = 0.42;

/// Part d'un clan existant qu'un groupe doit contenir pour qu'on le considère
/// comme **le prolongement** de ce clan (et non un rassemblement neuf) — la
/// condition d'entrée de l'hystérésis ci-dessus.
const CONTINUITY_NUM: usize = 1;
const CONTINUITY_DEN: usize = 2;

/// De combien un écart de prestige pèse dans l'attention qu'on accorde.
///
/// **La force centrifuge**, en trois tentatives dont deux ratées — elles valent
/// d'être racontées, chacune ayant échoué pour une raison différente :
///
/// 1. *Accélérer l'attachement vers un notable.* Le graphe s'est **densifié**
///    (92-96 % des liens au-dessus du seuil, contre 79-87 %) au lieu de se
///    structurer : s'attacher à quelqu'un ne coûtait rien aux autres relations.
///    Une étoile là où l'on voulait deux constellations.
/// 2. *Rendre l'attention finie* — chacun répartit une attention constante
///    entre ceux qu'il croise, si bien que s'attacher à un notable en retire
///    aux autres. Le défaut de densification a bien disparu (73-79 %), mais le
///    clan a **grossi** (110 → 137) : le prestige passait par une saturation
///    absolue `p/(p+K)`, qui écrasait les écarts là même où ils devaient
///    discriminer (40 et 60 en ressortaient à 0,83 et 0,88). Comme le prestige
///    ne décroît jamais, tous les anciens finissaient au plafond : une
///    gérontocratie indifférenciée, sans rivalité.
/// 3. *Comparer localement* (ici) : ce qui compte n'est pas le prestige en
///    valeur absolue mais l'**écart au prestige ambiant de l'assemblée** — voir
///    [`attention_weight`]. La comparaison étant refaite à chaque rencontre,
///    donc à portée de voix, un chef unique n'écrase plus tout le clan : là où
///    il n'est pas, quelqu'un d'autre domine.
///
/// Ce sont ces dominations **locales** qui font des factions spatialement
/// distinctes, donc des lignes de faille que la modularité peut lire — et
/// finalement la fission par rivalité de leadership, la plus documentée en
/// ethnographie des bandes, sans qu'aucune taille ni aucun rôle ne soit lu.
const PRESTIGE_PULL: f32 = 1.0;

/// Échelle de prestige qui borne l'effet d'un écart quand l'assemblée est
/// modeste : sans elle, un seul point de prestige parmi des gens à zéro ferait
/// un demi-dieu (voir [`attention_weight`]).
const PRESTIGE_HALF: f32 = 8.0;

/// Poids plancher : même écrasé par la présence de plus illustre que soi, on ne
/// devient jamais tout à fait transparent.
const MIN_ATTENTION_WEIGHT: f32 = 0.25;

/// Ce qu'un interlocuteur pèse dans l'attention qu'on lui accorde : la parenté
/// d'abord (elle prime, comme l'encodent déjà `KIN_GAIN` vs `ENCOUNTER_GAIN`),
/// puis **ce qu'il représente par rapport aux autres personnes présentes**.
///
/// C'est ce dernier point qui a demandé une seconde version. La première passait
/// le prestige dans une saturation absolue `p/(p+K)` — et cette saturation
/// écrasait les écarts exactement là où ils auraient dû discriminer : un ancien
/// à 40 et un autre à 60 en ressortaient à 0,83 et 0,88, indiscernables. Comme
/// le prestige ne décroît jamais et s'accumule à vie, tous les anciens
/// finissaient au plafond : une gérontocratie indifférenciée, aucune rivalité,
/// et un clan **plus** soudé qu'avant (taille médiane 110 → 137).
///
/// On compare donc au **prestige moyen de l'assemblée** (`local_mean`), pas à
/// une échelle absolue. Deux conséquences, toutes deux voulues :
///
/// - un notable entouré de ses pairs n'attire pas plus que sa part — et une
///   assemblée où personne ne se distingue rend exactement le comportement
///   d'avant ce mécanisme, écart nul, poids 1,0 (testé) ;
/// - la comparaison étant **locale à chaque rencontre** (portée de voix), un
///   chef unique n'écrase pas tout le clan : là où il n'est pas, quelqu'un
///   d'autre domine. Ce sont ces dominations locales qui font des factions
///   spatialement distinctes — donc des lignes de faille que la modularité peut
///   lire, ce qui est tout l'objet de la manœuvre.
fn attention_weight(kin: bool, prestige_of_other: f32, local_mean: f32) -> f32 {
    let base = if kin { KIN_GAIN / ENCOUNTER_GAIN } else { 1.0 };
    // Écart au prestige ambiant, ramené à cette échelle-là. `PRESTIGE_HALF` au
    // dénominateur borne l'effet quand l'assemblée est modeste (sans lui, un
    // seul point de prestige parmi des gens à zéro ferait un demi-dieu).
    let rel = (prestige_of_other - local_mean) / (local_mean + PRESTIGE_HALF);
    (base * (1.0 + PRESTIGE_PULL * rel)).max(MIN_ATTENTION_WEIGHT)
}

/// Part des membres d'un clan disparu qu'il faut retrouver **sous une même
/// bannière** pour parler de fusion plutôt que de dispersion. La moitié : en
/// dessous, ce sont quelques rescapés qui ont trouvé refuge, pas un peuple qui
/// en rejoint un autre.
const MERGE_NUM_NUM: usize = 1;
const MERGE_NUM_DEN: usize = 2;
/// Rayon de résidence, depuis le centroïde du groupe (~4,5 km — un
/// territoire de bande semi-nomade, pas une seule clairière : mesuré sur la
/// scène de calibrage, voir `sim::tests::un_clan_emerge_sans_regle_explicite`
/// — une seule jambe d'errance ou d'exploration dépasse déjà les 120 m de
/// portée de conversation, et rien ne ramène la population vers un point fixe).
/// C'est aussi le rayon au-delà duquel `brain::decide` tire un membre vers le
/// foyer de son clan — même seuil des deux côtés, pour que l'attraction
/// comportementale et le critère de détection se répondent. `pub` : le client
/// s'en sert pour dessiner l'étendue du territoire diffusé (`claim_at`).
pub const RESIDENCE_RADIUS_TILES: f64 = km_to_tiles(4.5);
/// Fraction des membres qui doivent être dans ce rayon : pas tous — un
/// chasseur ou un éclaireur temporairement loin reste du clan. En dessous de
/// cette proportion, le groupe n'est plus « co-résident », il est dispersé.
const RESIDENCE_FRACTION: f32 = 0.7;
/// Un nouveau groupe hérite de l'identité d'un ancien clan si son **noyau**
/// survit : au moins cette fraction des anciens membres s'y retrouve. Un peuple
/// qui perd la moitié des siens à la famine reste ce peuple.
const IDENTITY_CORE_NUM: usize = 1;
const IDENTITY_CORE_DEN: usize = 2;
/// …et si les anciens membres pèsent encore au moins cette fraction du nouveau
/// groupe, pour qu'une poignée de survivants ne s'empare pas du nom d'un
/// ensemble bien plus vaste qui les a simplement absorbés.
///
/// **Asymétrique, et c'est le point** : la version précédente exigeait la
/// majorité **des deux côtés**, si bien qu'un clan qui grandissait — en
/// accueillant des isolés, ou simplement par ses propres naissances — cessait
/// d'être lui-même dès qu'il avait doublé. Un quart laisse un peuple
/// quadrupler sans changer de nom, ce qu'il faut pour que la Chronique puisse
/// écrire « *An 389 — les Ashkar et les Orum se disputent le gué* » quarante-sept
/// ans après leur première mention (BRIEF §6.4).
const IDENTITY_SHARE_NUM: usize = 1;
const IDENTITY_SHARE_DEN: usize = 4;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
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
    /// Ce que pèse le second prétendant face au chef, dans [0, 1] : la
    /// **rivalité interne**, mesurée chaque jour par `elect_chiefs`. Proche de
    /// 1, deux personnages se valent et le peuple a deux pôles. Purement
    /// observationnelle : rien ne la lit pour décider quoi que ce soit.
    pub rivalry: f32,
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
    /// Effectif — ce qui donne sa portée au territoire (`claim_from_views`) :
    /// un peuple nombreux revendique plus loin, sans qu'aucun seuil de taille
    /// ne soit lu (même arithmétique que `territory_strength`).
    pub members: usize,
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
    /// Le peuple a cessé d'exister sans que ses membres se retrouvent ailleurs
    /// ensemble : morts, ou dispersés chacun de son côté.
    Dissolved,
    /// Le peuple a été **absorbé** par un autre — ses membres vont bien, ils
    /// vivent désormais sous une autre bannière. Distinguer ce cas de la
    /// dissolution n'est pas cosmétique : la plupart des « extinctions »
    /// mesurées étaient en réalité des fusions (voir `detect_clans`).
    Merged { into: ClanId },
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
        prestige: f32,
    }
    let mut views: Vec<View> = sim
        .agents
        .query::<(&AgentId, &Position, &Kinship, &Prestige)>()
        .iter()
        .map(|(_, (id, pos, kin, prestige))| View {
            id: *id,
            pos: (pos.x, pos.y),
            kin: *kin,
            prestige: prestige.0,
        })
        .collect();
    views.sort_unstable_by_key(|v| v.id.0);

    // — Passe 1 : qui croise qui. On ne pondère rien encore : le poids d'un
    // interlocuteur se juge **relativement à l'assemblée**, qu'il faut donc
    // avoir recensée en entier (voir `attention_weight`). —
    let mut met: Vec<(usize, usize)> = Vec::new();
    let mut prestige_sum: Vec<f32> = vec![0.0; views.len()];
    let mut degree: Vec<u32> = vec![0; views.len()];
    for i in 0..views.len() {
        for j in (i + 1)..views.len() {
            let (a, b) = (&views[i], &views[j]);
            let d2 = (a.pos.0 - b.pos.0).powi(2) + (a.pos.1 - b.pos.1).powi(2);
            if d2 > TALK_RADIUS_TILES * TALK_RADIUS_TILES {
                continue;
            }
            met.push((i, j));
            prestige_sum[i] += b.prestige;
            prestige_sum[j] += a.prestige;
            degree[i] += 1;
            degree[j] += 1;
        }
    }

    // — Passe 2 : ce que chacun accorde à l'autre, comparé au prestige ambiant
    // de sa propre assemblée. —
    let local_mean = |i: usize| {
        if degree[i] == 0 { 0.0 } else { prestige_sum[i] / degree[i] as f32 }
    };
    let mut pairs: Vec<(usize, usize, f32, f32)> = Vec::with_capacity(met.len());
    let mut total: Vec<f32> = vec![0.0; views.len()];
    for &(i, j) in &met {
        let (a, b) = (&views[i], &views[j]);
        let kin = are_kin(a.id, &a.kin, b.id, &b.kin);
        // Ce que `a` accorde à `b` dépend de ce que *b* pèse aux yeux de `a`, et
        // réciproquement — d'où deux poids par paire, et deux moyennes locales.
        let w_ab = attention_weight(kin, b.prestige, local_mean(i));
        let w_ba = attention_weight(kin, a.prestige, local_mean(j));
        pairs.push((i, j, w_ab, w_ba));
        total[i] += w_ab;
        total[j] += w_ba;
    }

    // — Passe 3 : répartir. —
    let mut spoke: BTreeSet<u64> = BTreeSet::new();
    for &(i, j, w_ab, w_ba) in &pairs {
        let (a, b) = (&views[i], &views[j]);
        let base = if are_kin(a.id, &a.kin, b.id, &b.kin) { KIN_GAIN } else { ENCOUNTER_GAIN };
        // La part de son attention que chacun consacre à l'autre, ramenée à ce
        // qu'elle vaudrait si tous ses interlocuteurs se valaient : 1,0 quand
        // personne ne sort du lot — le comportement d'avant ce mécanisme, à
        // l'identique — puis au-dessus vers un notable, **et en dessous vers
        // tous les autres**. C'est cette seconde moitié qui manquait : sans
        // elle, s'attacher à quelqu'un ne coûtait rien à personne, et le graphe
        // se densifiait au lieu de se structurer.
        let share = |w: f32, sum: f32, n: u32| {
            if sum <= 0.0 || n == 0 { 1.0 } else { (n as f32) * w / sum }
        };
        let invest_a = share(w_ab, total[i], degree[i]);
        let invest_b = share(w_ba, total[j], degree[j]);
        // Un lien se noue à deux : il vaut la moyenne des deux investissements,
        // pas celui du plus enthousiaste.
        sim.social.reinforce(a.id, b.id, base * 0.5 * (invest_a + invest_b));
        spoke.insert(a.id.0);
        spoke.insert(b.id.0);
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
    humans: &[crate::demography::HumanView],
) -> Result<(f64, f64), ClanReject> {
    if members.len() < MIN_CLAN_SIZE {
        return Err(ClanReject::TooSmall { members: members.len() });
    }
    // Aucun test de densité absolue ici : voir « Une ligne de faille, pas un
    // seuil de taille » en tête de module. La cohésion se juge à l'**absence de
    // ligne de faille** (`best_split`, appelée avant nous par `resolve_cluster`),
    // ce qui est indépendant de l'effectif — une densité seuil, elle, devenait
    // inatteignable au-delà de 54 membres et hachait les clans en trois jours.
    let positions: Vec<(f64, f64)> =
        members.iter().filter_map(|&id| find_human(humans, AgentId(id)).map(|h| h.pos)).collect();
    if positions.len() != members.len() {
        return Err(ClanReject::Missing); // sécurité : ne devrait pas arriver (agent introuvable)
    }
    let (sx, sy) = positions.iter().fold((0.0, 0.0), |(sx, sy), &(x, y)| (sx + x, sy + y));
    let (cx, cy) = (sx / positions.len() as f64, sy / positions.len() as f64);
    let resident_count =
        positions.iter().filter(|&&(x, y)| (x - cx).hypot(y - cy) <= RESIDENCE_RADIUS_TILES).count();
    let resident_fraction = resident_count as f32 / positions.len() as f32;
    if resident_fraction < RESIDENCE_FRACTION {
        return Err(ClanReject::Scattered { members: members.len(), resident_fraction });
    }
    Ok((cx, cy))
}

/// Pourquoi un groupe connexe n'est **pas** un clan. Purement observationnel :
/// la logique de décision est inchangée (les trois portes sont les mêmes, dans
/// le même ordre) — seule la raison du refus est désormais dite au lieu d'être
/// perdue dans un `None`. Sert à `ClanDiagnostics`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClanReject {
    /// Moins de `MIN_CLAN_SIZE` membres.
    TooSmall { members: usize },
    /// Densité du graphe interne sous `COHESION_THRESHOLD`.
    LowCohesion { members: usize, density: f32 },
    /// Moins de `RESIDENCE_FRACTION` des membres à portée du centroïde.
    Scattered { members: usize, resident_fraction: f32 },
    /// Un membre introuvable dans l'instantané (ne devrait pas arriver).
    Missing,
}

/// Compteurs de diagnostic sur la détection de clan — **rien ne les lit dans la
/// simulation**, ils n'existent que pour répondre à une question de calibrage :
/// quand un groupe échoue à être un clan, quelle porte lui a-t-elle été fermée ?
///
/// Les moyennes sont accumulées en sommes pour rester incrémentales (on ne garde
/// pas les échantillons). Cumulatifs sur toute la durée du monde.
#[derive(Debug, Clone, Default)]
pub struct ClanDiagnostics {
    /// Groupes validés comme clans.
    pub validated: u64,
    /// Rejets par taille, avec la somme des effectifs concernés.
    pub too_small: u64,
    pub too_small_members: u64,
    /// Rejets par cohésion, avec les sommes d'effectif et de densité mesurée.
    pub low_cohesion: u64,
    pub low_cohesion_members: u64,
    pub low_cohesion_density: f64,
    /// Rejets par dispersion, avec les sommes d'effectif et de fraction résidente.
    pub scattered: u64,
    pub scattered_members: u64,
    pub scattered_fraction: f64,
    /// Groupes trop petits ou trop lâches pour même tenter une fission.
    pub unsplittable: u64,
    /// Groupes coupés le long d.une ligne de faille (fission).
    pub split: u64,
}

impl ClanDiagnostics {
    fn note(&mut self, reject: &ClanReject) {
        match *reject {
            ClanReject::TooSmall { members } => {
                self.too_small += 1;
                self.too_small_members += members as u64;
            }
            ClanReject::LowCohesion { members, density } => {
                self.low_cohesion += 1;
                self.low_cohesion_members += members as u64;
                self.low_cohesion_density += f64::from(density);
            }
            ClanReject::Scattered { members, resident_fraction } => {
                self.scattered += 1;
                self.scattered_members += members as u64;
                self.scattered_fraction += f64::from(resident_fraction);
            }
            ClanReject::Missing => {}
        }
    }
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
    established: &[BTreeSet<u64>],
    diag: &mut ClanDiagnostics,
) -> Vec<(BTreeSet<AgentId>, (f64, f64))> {
    // 1. Ce groupe porte-t-il une ligne de faille ? La question vient **avant**
    //    la validation : un groupe scindé n'a pas à être jugé comme un tout.
    //    Un groupe qui prolonge un peuple déjà reconnu exige une faille plus
    //    franche pour se briser — voir `MODULARITY_ESTABLISHED`.
    let required = if continues_a_clan(&members, established) {
        MODULARITY_ESTABLISHED
    } else {
        MODULARITY_THRESHOLD
    };
    if let Some(parts) = best_split(&members, bonds, edges, required) {
        diag.split += 1;
        // Chaque morceau est réévalué par exactement les mêmes règles — la
        // fonction qui valide un clan ordinaire est celle qui valide les filles.
        return parts
            .into_iter()
            .flat_map(|g| resolve_cluster(g, bonds, edges, humans, established, diag))
            .collect();
    }
    // 2. Pas de ligne de faille : c'est un tout. Reste à savoir s'il est
    //    assez nombreux et assez rassemblé pour être un peuple.
    match validate_cluster(&members, humans) {
        Ok(home) => {
            diag.validated += 1;
            vec![(members.into_iter().map(AgentId).collect(), home)]
        }
        Err(reject) => {
            diag.note(&reject);
            Vec::new()
        }
    }
}

/// Cherche la **meilleure ligne de faille** d'un groupe, s'il en a une.
///
/// On explore les partitions obtenues en relevant progressivement le seuil
/// d'intimité (`FISSION_THRESHOLD_STEP`) : à chaque cran, ne restent que les
/// liens les plus forts, et le groupe se sépare peut-être en morceaux. C'est la
/// coupure d'un dendrogramme à seuil (single-linkage) — une technique connue,
/// pas une heuristique inventée pour l'occasion.
///
/// **Ce qui change tout, c'est le juge** : chaque partition candidate est notée
/// par sa [`modularity`], calculée sur le graphe de référence (les liens à
/// `BOND_THRESHOLD`, jamais ceux du seuil de recherche). On retient la
/// meilleure, et on ne coupe que si elle dépasse `MODULARITY_THRESHOLD`. Un
/// groupe uni n'a pas de partition modulaire : on ne le coupe pas, **quelle que
/// soit sa taille**.
///
/// `None` s'il n'y a aucune ligne de faille — le cas le plus fréquent.
fn best_split(
    members: &BTreeSet<u64>,
    bonds: &BTreeMap<(u64, u64), f32>,
    edges: &[(u64, u64)],
    required: f64,
) -> Option<Vec<BTreeSet<u64>>> {
    // Un groupe qui ne pourrait pas donner deux clans viables n'a pas à être
    // coupé : ce n'est pas une lecture de taille pour *décider* d'un
    // comportement, c'est l'arrêt d'une recherche qui n'a plus d'objet.
    if members.len() < 2 * MIN_CLAN_SIZE {
        return None;
    }
    let mut best: Option<(f64, Vec<BTreeSet<u64>>)> = None;
    let mut threshold = BOND_THRESHOLD + FISSION_THRESHOLD_STEP;
    while threshold <= FISSION_MAX_THRESHOLD {
        let stronger: Vec<(u64, u64)> = bonds
            .iter()
            .filter(|&(&(a, b), &w)| {
                w >= threshold && members.contains(&a) && members.contains(&b)
            })
            .map(|(&k, _)| k)
            .collect();
        let parts = connected_components(members, &stronger);
        if parts.len() > 1 {
            let q = modularity(&parts, edges, members);
            if best.as_ref().is_none_or(|(bq, _)| q > *bq) {
                best = Some((q, parts));
            }
        }
        threshold += FISSION_THRESHOLD_STEP;
    }
    best.filter(|(q, _)| *q >= required).map(|(_, parts)| parts)
}

/// Ce groupe est-il **le prolongement** d'un clan déjà reconnu ? Vrai dès qu'il
/// contient au moins la moitié des membres d'un clan existant : c'est le même
/// peuple qui continue, avec des départs et des arrivées.
fn continues_a_clan(members: &BTreeSet<u64>, established: &[BTreeSet<u64>]) -> bool {
    established.iter().any(|clan| {
        let kept = clan.iter().filter(|id| members.contains(id)).count();
        !clan.is_empty() && kept * CONTINUITY_DEN >= clan.len() * CONTINUITY_NUM
    })
}

/// La **modularité** d'une partition (Newman, 2004) : à quel point les arêtes
/// tombent-elles à l'intérieur des communautés plutôt qu'entre elles, comparé à
/// ce qu'un graphe aléatoire de mêmes degrés donnerait ?
///
/// `Q = Σ_c [ L_c/L − (d_c/2L)² ]`, où `L` est le nombre d'arêtes internes au
/// groupe, `L_c` celles internes à la communauté `c`, et `d_c` la somme des
/// degrés de ses membres. `Q ≈ 0` quand la partition n'explique rien de plus
/// que le hasard ; `Q` monte vers 0,5 et au-delà quand les communautés sont
/// nettes. C'est **adimensionnel** : contrairement à une densité, le seuil
/// n'est pas à recalibrer quand la population grandit — la raison même pour
/// laquelle on est passé à cette mesure.
fn modularity(parts: &[BTreeSet<u64>], edges: &[(u64, u64)], members: &BTreeSet<u64>) -> f64 {
    let internal: Vec<(u64, u64)> = edges
        .iter()
        .filter(|(a, b)| members.contains(a) && members.contains(b))
        .copied()
        .collect();
    let total = internal.len() as f64;
    if total == 0.0 {
        return 0.0; // aucun lien : aucune structure à trouver
    }
    // Degré de chaque membre dans le sous-graphe.
    let mut degree: BTreeMap<u64, f64> = BTreeMap::new();
    for &(a, b) in &internal {
        *degree.entry(a).or_default() += 1.0;
        *degree.entry(b).or_default() += 1.0;
    }
    let mut q = 0.0;
    for part in parts {
        let inside =
            internal.iter().filter(|(a, b)| part.contains(a) && part.contains(b)).count() as f64;
        let d: f64 = part.iter().filter_map(|id| degree.get(id)).sum();
        q += inside / total - (d / (2.0 * total)).powi(2);
    }
    q
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
        let mut ranked: Vec<f32> =
            clan.members.iter().map(|m| scores.get(&m.0).copied().unwrap_or(0.0)).collect();
        if let Some(&chief) = clan.members.iter().max_by(|a, b| {
            let sa = scores.get(&a.0).copied().unwrap_or(0.0);
            let sb = scores.get(&b.0).copied().unwrap_or(0.0);
            sa.total_cmp(&sb).then(b.0.cmp(&a.0)) // égalité : plus petit AgentId l'emporte
        }) {
            clan.chief = chief;
        }
        // La rivalité : ce que pèse le second face au premier. Pure **mesure**,
        // elle ne déclenche rien — la scission, quand elle vient, vient du
        // graphe social (deux halos de prestige que la modularité sépare), pas
        // de ce nombre. Il sert à voir venir : c'est l'indicateur qu'un peuple
        // est sur le point de se diviser, et le futur matériau d'un fait de
        // Chronique (« la rivalité entre X et Y déchire les Z »).
        ranked.sort_by(|a, b| b.total_cmp(a));
        clan.rivalry = match (ranked.first(), ranked.get(1)) {
            (Some(&first), Some(&second)) if first > 0.0 => (second / first).clamp(0.0, 1.0),
            _ => 0.0,
        };
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

/// Le même champ de territoire que [`claim_at`], mais lu depuis les
/// **instantanés** que la délibération a sous la main (`brain::decide` ne voit
/// pas `sim.clans`). Même formule, même convention de départage.
///
/// C'est ce qui donne enfin un **consommateur comportemental** au territoire
/// diffusé. Depuis la Phase 4 il était calculé, affiché par le client, consulté
/// par les structures — mais rien ne le lisait pour se déplacer, et le projet le
/// notait comme tel. Conséquence mesurée sur 15 ans : rien n'éloignait jamais
/// deux peuples. Ils vivaient à 2,1 km les uns des autres (p90 : 3,5 km) quand
/// leur rayon de rappel en fait 4,5 — donc des territoires presque confondus,
/// des membres qui se croisent en permanence, et des liens qui se reformaient
/// aussitôt coupés. D'où 554 fusions pour 423 fissions : un oscillateur, pas une
/// société.
pub fn claim_from_views(
    point: (f64, f64),
    clans: &BTreeMap<ClanId, crate::social::ClanView>,
) -> Option<ClanId> {
    clans
        .iter()
        .map(|(&id, v)| {
            let dist = (point.0 - v.home.0).hypot(point.1 - v.home.1);
            let strength =
                v.members as f32 * (1.0 - (dist / RESIDENCE_RADIUS_TILES) as f32).max(0.0);
            (id, strength)
        })
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
    // Les compteurs de diagnostic sont sortis de `sim` le temps de la boucle :
    // `resolve_cluster` les emprunte mutablement, or `sim.social.bonds` est
    // emprunté immuablement au même moment.
    let mut diag = std::mem::take(&mut sim.clan_diagnostics);
    // Les peuples d'hier, pour l'hystérésis de fission : un groupe qui prolonge
    // l'un d'eux ne se brise qu'à une faille franche (`MODULARITY_ESTABLISHED`).
    let established: Vec<BTreeSet<u64>> =
        sim.clans.iter().map(|c| c.members.iter().map(|a| a.0).collect()).collect();
    for members in groups.into_values() {
        let member_set: BTreeSet<u64> = members.into_iter().collect();
        clusters.extend(resolve_cluster(
            member_set,
            &sim.social.bonds,
            &edges,
            &humans,
            &established,
            &mut diag,
        ));
    }
    sim.clan_diagnostics = diag;
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
    // Les clans qui n'ont pas retrouvé leur identité — leur sort se décide une
    // fois tous les clans du jour connus (fusion ou dispersion, voir plus bas).
    let mut lost: Vec<Clan> = Vec::new();
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
            let majority_old = overlap * IDENTITY_CORE_DEN >= clan.members.len() * IDENTITY_CORE_NUM;
            let majority_new = overlap * IDENTITY_SHARE_DEN >= cluster.len() * IDENTITY_SHARE_NUM;
            if majority_old && majority_new {
                matched[i] = true;
                next.push(Clan {
                    id: clan.id,
                    founded_tick: clan.founded_tick,
                    members: cluster.clone(),
                    home: *home,
                    stock: clan.stock,
                    rivalry: clan.rivalry,
                    chief: clan.chief, // provisoire : `elect_chiefs` le recalcule juste après
                    desired: clan.desired, // reporté ; `structures::plan` le recalcule à minuit
                });
                true
            } else {
                false
            }
        });
        if !kept {
            // Rien n'est journalisé ici : on ne sait pas encore **ce qui est
            // arrivé** à ce peuple. Ses membres sont peut-être morts, ou
            // dispersés — ou bien ils ont rejoint un autre clan, et il faut
            // pour le dire connaître les clans du jour, qui n'ont pas encore
            // tous reçu leur identité. Verdict rendu plus bas.
            lost.push(clan);
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
        // Rien dans la Chronique ici : un clan tout juste détecté n'est pas
        // encore un peuple. C'est le jour où il atteint `CLAN_NOTABLE_DAYS`
        // qu'on peut l'affirmer — voir la fin de cette fonction.
        next.push(Clan {
            id,
            founded_tick: sim.time.tick,
            chief: *cluster.iter().next().unwrap(), // provisoire, voir la doc du champ
            members: cluster,
            home,
            stock: 0.0,
            rivalry: 0.0,
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

    // Qu'est-il arrivé aux clans qui ont perdu leur identité ? Le modèle ne
    // savait qu'**éteindre**, et comptait donc comme une mort ce qui est le
    // plus souvent une **fusion** : un petit groupe se détache, puis rejoint le
    // gros — ses gens vont très bien, mais la Chronique écrivait « ils se
    // dispersent ». On regarde donc où sont passés ses membres : si la moitié
    // d'entre eux se retrouve sous une même bannière, ce peuple n'est pas mort,
    // il a été absorbé.
    for clan in std::mem::take(&mut lost) {
        let mut tally: BTreeMap<u64, usize> = BTreeMap::new();
        for member in &clan.members {
            if let Some(host) = membership.get(&member.0) {
                *tally.entry(host.0).or_default() += 1;
            }
        }
        // Égalité départagée par le plus petit identifiant, comme partout
        // ailleurs (`claim_at`, `elect_chiefs`) — jamais par l'ordre d'itération.
        let into = tally
            .iter()
            .max_by_key(|&(id, n)| (*n, std::cmp::Reverse(*id)))
            .filter(|&(_, n)| n * MERGE_NUM_DEN >= clan.members.len() * MERGE_NUM_NUM)
            .map(|(id, _)| ClanId(*id));
        sim.clan_events.push(ClanEvent {
            tick: sim.time.tick,
            clan: clan.id,
            kind: match into {
                Some(host) => ClanEventKind::Merged { into: host },
                None => ClanEventKind::Dissolved,
            },
            members: clan.members.len(),
        });
        // La fin d'un peuple fait date (BRIEF §6.4) — mais seulement d'un
        // peuple dont on avait annoncé la naissance : voir
        // `chronicle::CLAN_NOTABLE_DAYS`. Un groupe détecté le matin et reperdu
        // le soir n'a pas d'histoire à clore.
        let days = (sim.time.tick.saturating_sub(clan.founded_tick)) / TICKS_PER_DAY;
        if days > crate::chronicle::CLAN_NOTABLE_DAYS {
            let home = (clan.home.0.floor() as i64, clan.home.1.floor() as i64);
            let members = clan.members.len();
            sim.record(
                home,
                match into {
                    Some(host) => EventKind::ClanMerged { clan: clan.id, into: host, members },
                    None => EventKind::ClanDissolved { clan: clan.id, members },
                },
            );
        }
    }

    // Les peuples qui viennent d'atteindre l'ancienneté notable : c'est
    // aujourd'hui qu'ils entrent dans la Chronique (voir `CLAN_NOTABLE_DAYS`).
    // L'égalité stricte suffit et ne peut pas se manquer — cette passe tombe
    // exactement une fois par jour.
    let born: Vec<(ClanId, (i64, i64), usize)> = sim
        .clans
        .iter()
        .filter(|c| {
            (sim.time.tick.saturating_sub(c.founded_tick)) / TICKS_PER_DAY
                == crate::chronicle::CLAN_NOTABLE_DAYS
        })
        .map(|c| {
            (c.id, (c.home.0.floor() as i64, c.home.1.floor() as i64), c.members.len())
        })
        .collect();
    for (clan, pos, members) in born {
        sim.record(pos, EventKind::ClanFormed { clan, members });
    }
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
        HumanView { id: AgentId(id), pos: (x, y), sex: Sex::Female, adult: true, clan: None }
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
            validate_cluster(&members, &humans).is_err(),
            "les 20 pris en bloc ne doivent PAS passer le seuil de cohésion (c'est le point de départ du test)"
        );

        let result = resolve_cluster(members, &bonds, &edges, &humans, &[], &mut ClanDiagnostics::default());
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
    fn un_groupe_uni_reste_un_seul_peuple_quelle_que_soit_sa_densite() {
        let mut bonds: BTreeMap<(u64, u64), f32> = BTreeMap::new();
        let mut humans: Vec<HumanView> = Vec::new();
        for i in 0..20u64 {
            humans.push(human_at(i, i as f64 * 2.0, 0.0));
            bonds.insert(SocialGraph::key(AgentId(i), AgentId((i + 1) % 20)), 0.9);
        }
        humans.sort_by_key(|h| h.id.0);
        let members: BTreeSet<u64> = (0..20).collect();
        let edges: Vec<(u64, u64)> = bonds.keys().copied().collect();

        // Un anneau n'a **aucune** ligne de faille : tous les liens s'y valent,
        // aucune partition n'explique la structure mieux que le hasard.
        assert!(
            best_split(&members, &bonds, &edges, MODULARITY_THRESHOLD).is_none(),
            "un anneau homogène n'a pas de ligne de faille : il ne doit pas être coupé"
        );
        // Et sa densité (~0,1) ne lui est plus opposée : c'est un peuple uni,
        // pas un groupe trop lâche. C'est le changement de conception mesuré à
        // l'`example diagnose` — voir « Une ligne de faille, pas un seuil de
        // taille » en tête de module.
        let result =
            resolve_cluster(members, &bonds, &edges, &humans, &[], &mut ClanDiagnostics::default());
        assert_eq!(result.len(), 1, "l'anneau doit former un seul clan");
        assert_eq!(result[0].0.len(), 20, "et n'y perdre personne");
    }

    /// La modularité fait ce qu'on attend d'elle : elle note haut une partition
    /// qui suit une vraie frontière, et bas une coupure arbitraire.
    #[test]
    fn la_modularite_distingue_une_vraie_frontiere_d_une_coupure_arbitraire() {
        // Deux cliques de 6, reliées par une seule arête.
        let mut edges: Vec<(u64, u64)> = Vec::new();
        for group in 0..2u64 {
            let base = group * 6;
            for a in base..base + 6 {
                for b in (a + 1)..base + 6 {
                    edges.push((a, b));
                }
            }
        }
        edges.push((5, 6)); // le pont
        let members: BTreeSet<u64> = (0..12).collect();
        let vraie: Vec<BTreeSet<u64>> = vec![(0..6).collect(), (6..12).collect()];
        // Une coupure qui traverse les deux cliques au lieu de les séparer.
        let arbitraire: Vec<BTreeSet<u64>> =
            vec![[0, 1, 2, 6, 7, 8].into(), [3, 4, 5, 9, 10, 11].into()];

        let q_vraie = modularity(&vraie, &edges, &members);
        let q_arbitraire = modularity(&arbitraire, &edges, &members);
        assert!(
            q_vraie >= MODULARITY_THRESHOLD,
            "deux cliques reliées par un pont sont une vraie frontière (Q = {q_vraie:.3})"
        );
        assert!(
            q_arbitraire < q_vraie,
            "une coupure qui traverse les cliques doit noter moins ({q_arbitraire:.3} < {q_vraie:.3})"
        );
    }

    /// L'attention est finie : s'attacher à un notable en retire aux autres.
    /// C'est **le coût** qui manquait à la première tentative — sans lui, le
    /// graphe se densifiait au lieu de se structurer.
    #[test]
    fn s_attacher_a_un_notable_coute_aux_autres() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        // Un observateur, deux anonymes, et un notable — tous à portée de voix.
        let watcher = sim.spawn_agent(0.0, 0.0);
        let plain = sim.spawn_agent(1.0, 0.0);
        let _other = sim.spawn_agent(1.5, 0.0);
        let notable = sim.spawn_agent(2.0, 0.0);
        for (_, (id, prestige)) in sim.agents.query_mut::<(&AgentId, &mut Prestige)>() {
            if *id == notable {
                prestige.0 = 40.0; // des années à nourrir les siens
            }
        }
        encounter(&mut sim);
        let bond = |x: AgentId, y: AgentId| sim.social.affinity(x, y);
        assert!(
            bond(watcher, notable) > bond(watcher, plain),
            "le notable capte plus ({:.4}) qu'un anonyme ({:.4})",
            bond(watcher, notable),
            bond(watcher, plain)
        );

        // La contre-épreuve, celle qui distingue ce mécanisme du précédent :
        // sans notable dans l'assemblée, le même anonyme reçoit **davantage**.
        let mut plat = Sim::new(WorldSeed(1), 64);
        let w2 = plat.spawn_agent(0.0, 0.0);
        let p2 = plat.spawn_agent(1.0, 0.0);
        let _o2 = plat.spawn_agent(1.5, 0.0);
        let _n2 = plat.spawn_agent(2.0, 0.0);
        encounter(&mut plat);
        assert!(
            plat.social.affinity(w2, p2) > bond(watcher, plain),
            "un anonyme perd de l'attention quand un notable est là ({:.4} sans, {:.4} avec)",
            plat.social.affinity(w2, p2),
            bond(watcher, plain)
        );
    }

    /// À assemblée plate — personne ne sort du lot — le mécanisme doit rendre
    /// **exactement** le comportement d'avant : sinon il aurait rééquilibré tout
    /// le modèle social au passage, sans qu'on l'ait demandé.
    #[test]
    fn sans_notable_l_attachement_est_celui_d_avant() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let a = sim.spawn_agent(0.0, 0.0);
        let b = sim.spawn_agent(1.0, 0.0);
        let _c = sim.spawn_agent(1.5, 0.0);
        encounter(&mut sim);
        // `reinforce` est un nudge saturant depuis 0 : le premier gain vaut
        // exactement le taux appliqué.
        let expected = ENCOUNTER_GAIN;
        let got = sim.social.affinity(a, b);
        assert!(
            (got - expected).abs() < 1e-5,
            "sans prestige, le gain doit valoir ENCOUNTER_GAIN ({expected:.4}), obtenu {got:.4}"
        );
    }

    /// Une fusion n'est pas une fin. Mesuré : 86 des 118 disparitions de clans
    /// étaient en réalité des absorptions — le modèle comptait comme des morts
    /// des gens qui allaient très bien, sous une autre bannière.
    #[test]
    fn un_peuple_absorbe_n_est_pas_un_peuple_mort() {
        // Le seuil de fusion : la moitié des membres retrouvés sous une même
        // bannière suffit à dire « ils les ont rejoints ».
        let fusionne = |retrouves: usize, effectif: usize| {
            retrouves * MERGE_NUM_DEN >= effectif * MERGE_NUM_NUM
        };
        assert!(fusionne(10, 20), "la moitié d'un peuple sous une bannière : une fusion");
        assert!(fusionne(20, 20), "tous : une fusion, évidemment");
        assert!(!fusionne(3, 20), "trois rescapés qui trouvent refuge : pas une fusion");
    }

    /// Le cœur du volet « identité » : un peuple qui **grandit** reste lui-même.
    /// L'ancienne règle (majorité des deux côtés) lui retirait son nom dès qu'il
    /// avait doublé — c'est ce qui empêchait tout clan d'avoir une histoire.
    #[test]
    fn un_peuple_qui_grandit_garde_son_nom() {
        // 10 anciens membres, tous retrouvés dans un groupe qui en compte 30.
        let (overlap, old_size, new_size) = (10usize, 10usize, 30usize);
        let core = overlap * IDENTITY_CORE_DEN >= old_size * IDENTITY_CORE_NUM;
        let share = overlap * IDENTITY_SHARE_DEN >= new_size * IDENTITY_SHARE_NUM;
        assert!(core && share, "un clan qui triple garde son identité");

        // Mais une poignée de survivants ne s'empare pas du nom d'une foule.
        let (overlap, old_size, new_size) = (5usize, 10usize, 100usize);
        let core = overlap * IDENTITY_CORE_DEN >= old_size * IDENTITY_CORE_NUM;
        let share = overlap * IDENTITY_SHARE_DEN >= new_size * IDENTITY_SHARE_NUM;
        assert!(core && !share, "5 rescapés noyés dans 100 ne donnent pas leur nom au tout");
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
            rivalry: 0.0,
        });
        sim.clans.push(Clan {
            id: ClanId(2),
            founded_tick: 0,
            members: (100..110u64).map(AgentId).collect(),
            home: home_b,
            stock: 0.0,
            chief: AgentId(100),
            desired: None,
            rivalry: 0.0,
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
            rivalry: 0.0,
        }
    }

    /// Le champ de territoire (incrément 7) revendique le foyer lui-même :
    /// la force d'un clan y est maximale (distance nulle).
    #[test]
    fn un_point_au_foyer_est_revendique_par_son_clan() {
        let clan = synthetic_clan(1, (100.0, 200.0), 10);
        assert_eq!(claim_at((100.0, 200.0), std::slice::from_ref(&clan)), Some(clan.id));
    }

    /// Le champ lu depuis les instantanés de la délibération doit rendre le
    /// **même** verdict que celui lu depuis les clans : c'est la même frontière
    /// des deux côtés, sans quoi un agent se croirait chez lui là où le reste du
    /// modèle le dit chez autrui.
    #[test]
    fn le_territoire_vu_par_la_deliberation_est_le_meme_que_le_vrai() {
        let clans =
            vec![synthetic_clan(1, (0.0, 0.0), 10), synthetic_clan(2, (2000.0, 0.0), 10)];
        let views: BTreeMap<ClanId, ClanView> = clans
            .iter()
            .map(|c| {
                (
                    c.id,
                    ClanView {
                        home: c.home,
                        stock: c.stock,
                        desired: None,
                        members: c.members.len(),
                    },
                )
            })
            .collect();
        // De part et d'autre de la frontière, et au-delà de toute portée.
        for point in [(0.0, 0.0), (500.0, 0.0), (1500.0, 0.0), (2000.0, 0.0), (60_000.0, 0.0)] {
            assert_eq!(
                claim_from_views(point, &views),
                claim_at(point, &clans),
                "verdicts divergents en {point:?}"
            );
        }
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
            rivalry: 0.0,
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
            rivalry: 0.0,
        });

        elect_chiefs(&mut sim);

        assert_eq!(sim.clans[0].chief, a, "égalité stricte : le plus petit AgentId doit l'emporter");
    }
}
