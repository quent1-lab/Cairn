//! Le graphe social et l'émergence du clan (BRIEF §5.1, Phase 4 « LE CLAN »,
//! incrément 1).
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
//! ## Limite connue : la co-résidence n'est pas encore *voulue*
//!
//! Calibré (voir les tests) sur une scène de 24 agents : rien, dans les
//! comportements des Phases 2-3, ne pousse un agent à rester près de ses
//! liens sociaux — `TaskKind::Socialize` ne fait que rejoindre le **plus
//! proche** congénère quand on est isolé, pas revenir vers un clan précis.
//! Résultat mesuré : la population diffuse sans borne (plusieurs km en
//! quelques semaines, sans jamais se stabiliser), si bien qu'un clan formé
//! peut se dissoudre de lui-même quelques semaines plus tard, simplement
//! parce que ses membres ont continué à errer chacun de leur côté — pas
//! parce qu'il « s'est effondré » au sens du brief. Un vrai territoire qui
//! **attire** ses membres (au lieu de seulement être constaté après coup)
//! est le sujet de l'incrément suivant.

use std::collections::{BTreeMap, BTreeSet};

use crate::agent::{AgentId, Position};
use crate::demography::{Kinship, find_human};
use crate::memory::TALK_RADIUS_TILES;
use crate::sim::Sim;
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
const RESIDENCE_RADIUS_TILES: f64 = km_to_tiles(4.5);
/// Fraction des membres qui doivent être dans ce rayon : pas tous — un
/// chasseur ou un éclaireur temporairement loin reste du clan. En dessous de
/// cette proportion, le groupe n'est plus « co-résident », il est dispersé.
const RESIDENCE_FRACTION: f32 = 0.7;
/// Un nouveau groupe hérite de l'identité d'un ancien clan s'ils partagent la
/// **majorité** de leurs membres, dans les deux sens (le clan n'a pas trop
/// changé, et il ne s'est pas noyé dans quelque chose de bien plus gros).
const IDENTITY_OVERLAP_NUM: usize = 1;
const IDENTITY_OVERLAP_DEN: usize = 2;

/// Identifiant stable d'un clan, monotone — comme `AgentId`/`FaunaId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClanId(pub u64);

/// Un clan détecté : pour l'instant, une identité et un rôle. Territoire,
/// stock, chef et normes viendront avec les incréments suivants de la
/// Phase 4 — ce socle suffit à tester l'émergence elle-même.
#[derive(Debug, Clone)]
pub struct Clan {
    pub id: ClanId,
    pub founded_tick: u64,
    pub members: BTreeSet<AgentId>,
}

/// Le clan d'appartenance d'un agent, `None` s'il n'en a pas. Composant à
/// part (et non un champ dans `Demographics`) : il est recalculé en bloc
/// chaque jour par `daily`, indépendamment de tout ce qui touche l'individu.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClanMembership(pub Option<ClanId>);

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

    for i in 0..views.len() {
        for j in (i + 1)..views.len() {
            let (a, b) = (&views[i], &views[j]);
            let d2 = (a.pos.0 - b.pos.0).powi(2) + (a.pos.1 - b.pos.1).powi(2);
            if d2 > TALK_RADIUS_TILES * TALK_RADIUS_TILES {
                continue;
            }
            let rate = if are_kin(a.id, &a.kin, b.id, &b.kin) { KIN_GAIN } else { ENCOUNTER_GAIN };
            sim.social.reinforce(a.id, b.id, rate);
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

/// La passe quotidienne : entretien du graphe, puis détection des clans.
/// Appelée à la même cadence que `demography::daily` — c'est le rythme
/// « administratif » de la simulation.
pub(crate) fn daily(sim: &mut Sim) {
    let alive: BTreeSet<u64> = sim.agents.query::<&AgentId>().iter().map(|(_, id)| id.0).collect();
    sim.social.prune_dead(&alive);
    sim.social.decay(DAILY_DECAY, FORGET_THRESHOLD);
    sim.social.cap_bonds(MAX_BONDS_PER_AGENT);
    detect_clans(sim);
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
    let mut clusters: Vec<BTreeSet<AgentId>> = Vec::new();
    for members in groups.values() {
        if members.len() < MIN_CLAN_SIZE {
            continue;
        }
        let member_set: BTreeSet<u64> = members.iter().copied().collect();
        let internal_edges =
            edges.iter().filter(|(a, b)| member_set.contains(a) && member_set.contains(b)).count();
        let possible = members.len() * (members.len() - 1) / 2;
        let density = internal_edges as f32 / possible as f32;
        if density < COHESION_THRESHOLD {
            continue;
        }
        let positions: Vec<(f64, f64)> = members
            .iter()
            .filter_map(|&id| find_human(&humans, AgentId(id)).map(|h| h.pos))
            .collect();
        if positions.len() != members.len() {
            continue; // sécurité : ne devrait pas arriver (agent introuvable)
        }
        let (sx, sy) = positions.iter().fold((0.0, 0.0), |(sx, sy), &(x, y)| (sx + x, sy + y));
        let (cx, cy) = (sx / positions.len() as f64, sy / positions.len() as f64);
        let home = positions.iter().filter(|&&(x, y)| (x - cx).hypot(y - cy) <= RESIDENCE_RADIUS_TILES).count();
        if (home as f32) < RESIDENCE_FRACTION * positions.len() as f32 {
            continue;
        }
        clusters.push(member_set.into_iter().map(AgentId).collect());
    }
    // Ordre déterministe et stable pour l'attribution des nouveaux
    // identifiants : par plus petit membre.
    clusters.sort_by_key(|c| c.iter().next().copied());

    // Réconciliation avec les clans existants : chacun cherche, parmi les
    // clusters encore libres, celui avec lequel il partage le plus de
    // membres. S'ils se recouvrent majoritairement dans les deux sens,
    // c'est le même clan qui continue ; sinon il s'efface.
    let mut matched = vec![false; clusters.len()];
    let mut next: Vec<Clan> = Vec::new();
    for clan in std::mem::take(&mut sim.clans) {
        let best = clusters
            .iter()
            .enumerate()
            .filter(|(i, _)| !matched[*i])
            .map(|(i, c)| (i, clan.members.intersection(c).count()))
            .filter(|&(_, overlap)| overlap > 0)
            .max_by_key(|&(_, overlap)| overlap);
        let kept = best.is_some_and(|(i, overlap)| {
            let cluster = &clusters[i];
            let majority_old = overlap * IDENTITY_OVERLAP_DEN >= clan.members.len() * IDENTITY_OVERLAP_NUM;
            let majority_new = overlap * IDENTITY_OVERLAP_DEN >= cluster.len() * IDENTITY_OVERLAP_NUM;
            if majority_old && majority_new {
                matched[i] = true;
                next.push(Clan { id: clan.id, founded_tick: clan.founded_tick, members: cluster.clone() });
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
    for (i, cluster) in clusters.into_iter().enumerate() {
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
        next.push(Clan { id, founded_tick: sim.time.tick, members: cluster });
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
}
