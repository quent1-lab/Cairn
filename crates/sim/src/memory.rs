//! La mémoire spatiale individuelle (BRIEF §3.1, Phase 3) : « l'agent ne
//! connaît que les tuiles qu'il a vues, ou dont on lui a parlé ».
//!
//! Deux savoirs, aux granularités différentes :
//!
//! - **Les sources d'eau connues** : des points précis, peu nombreux, vitaux.
//!   C'est l'information *actionnable* — un agent assoiffé loin de tout
//!   marche vers la source qu'il a vue il y a dix jours au lieu de mourir à
//!   800 m d'elle. C'est aussi ce qui **s'échange** quand deux agents se
//!   croisent : on se dit où boire, pas sa carte entière.
//! - **Les cellules explorées** : une grille grossière (512 m de côté) des
//!   lieux où l'agent a mis les pieds. Personnelle, jamais transmise — c'est
//!   le vécu, pas le su. Elle nourrit le drive « comprendre » : explorer,
//!   c'est viser une cellule qui n'y est pas.
//!
//! L'exploration devient ainsi une **acquisition d'information** qui change
//! les décisions (où boire, où aller), pas une révélation de brouillard.
//! Critère d'acceptation : la carte mentale est **strictement incluse** dans
//! ce que l'agent a pu percevoir ou entendre — garanti par construction (on
//! n'écrit dedans qu'aux passages et aux rencontres), testé par les bornes.

use std::collections::BTreeSet;

use crate::agent::AgentId;
use crate::sim::Sim;

/// Côté d'une cellule de mémoire : 256 tuiles = 512 m. Assez fin pour que
/// « explorer » ait un sens local, assez gros pour que la carte d'une vie
/// tienne en quelques milliers d'entrées.
pub const MEMORY_CELL_TILES: i64 = 256;
/// Sources retenues au plus : les plus proches restent, les lointaines
/// s'oublient — une mémoire, pas un atlas.
pub const MAX_KNOWN_SPRINGS: usize = 16;
/// Plafond de cellules mémorisées (~1 000 km² vécus). Au-delà, on ne note
/// plus : la borne mémoire prime sur l'exhaustivité du souvenir.
pub const MAX_KNOWN_CELLS: usize = 4096;
/// Portée d'une conversation (~120 m) : en deçà, deux agents « se croisent »
/// et se partagent leurs sources (une fois par jour).
pub const TALK_RADIUS_TILES: f64 = 60.0;
/// Sources mises de côté retenues au plus.
const MAX_BLOCKED_SPRINGS: usize = 4;
/// Combien de temps une source où l'on a buté reste écartée : deux jours —
/// le lac ne bouge pas, mais l'échec peut venir d'un budget de calcul épuisé
/// ce tick-là, et il reste d'autres sources.
pub const BLOCKED_SPRING_TICKS: u64 = 2 * cairn_core::TICKS_PER_DAY;

/// La cellule de mémoire couvrant une tuile.
pub fn cell_of(tile: (i64, i64)) -> (i64, i64) {
    (tile.0.div_euclid(MEMORY_CELL_TILES), tile.1.div_euclid(MEMORY_CELL_TILES))
}

/// La carte mentale d'un agent. Composant à allocations (contrairement à
/// `Behavior`) : `hecs` l'accepte sans peine, on évite juste de le copier.
#[derive(Debug, Clone, Default)]
pub struct Memory {
    /// Sources d'eau connues (tuiles), vues soi-même ou apprises d'autrui.
    pub springs: Vec<(i64, i64)>,
    /// Cellules où l'agent a mis les pieds. Jamais transmises.
    pub known: BTreeSet<(i64, i64)>,
    /// Le dernier troupeau aperçu : tuile et tick. Un seul souvenir, le plus
    /// récent — le gibier bouge, un vieux souvenir ne vaut rien (voir
    /// `brain::GAME_MEMORY_DAYS`).
    pub game: Option<((i64, i64), u64)>,
    /// Sources mises de côté jusqu'à un tick : la marche y a buté (de l'eau
    /// sans contournement trouvé). Sans ce souvenir, un assoiffé revisait la
    /// même source inaccessible jusqu'à en mourir à 300 m (D11).
    pub blocked: Vec<((i64, i64), u64)>,
    /// Où l'on a dormi en dernier : le gîte d'où part et où revient une sortie
    /// de chasse quand on n'a pas de clan (MAR-4).
    pub lodge: Option<(i64, i64)>,
    /// Dernière cellule notée — évite une insertion par tick quand on
    /// piétine dans la même cellule (le cas de très loin le plus fréquent).
    last_cell: Option<(i64, i64)>,
}

impl Memory {
    /// Note le passage sur une tuile (une insertion par changement de
    /// cellule seulement).
    pub fn note_visit(&mut self, tile: (i64, i64)) {
        let cell = cell_of(tile);
        if self.last_cell == Some(cell) {
            return;
        }
        self.last_cell = Some(cell);
        if self.known.len() < MAX_KNOWN_CELLS {
            self.known.insert(cell);
        }
    }

    /// Mémorise une source. Au-delà du plafond, la plus lointaine de `from`
    /// est oubliée : on retient l'eau *utile*.
    pub fn remember_spring(&mut self, spring: (i64, i64), from: (f64, f64)) {
        if self.springs.contains(&spring) {
            return;
        }
        self.springs.push(spring);
        if self.springs.len() > MAX_KNOWN_SPRINGS {
            let d2 = |s: &(i64, i64)| {
                (s.0 as f64 + 0.5 - from.0).powi(2) + (s.1 as f64 + 0.5 - from.1).powi(2)
            };
            let (farthest, _) = self
                .springs
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| d2(a).total_cmp(&d2(b)))
                .expect("liste non vide");
            self.springs.swap_remove(farthest);
        }
    }

    /// La source connue la plus proche de `from`. Départage déterministe :
    /// distance, puis ordre d'insertion.
    pub fn nearest_known_spring(&self, from: (f64, f64)) -> Option<(i64, i64)> {
        let mut best: Option<(f64, (i64, i64))> = None;
        for &s in &self.springs {
            let d2 =
                (s.0 as f64 + 0.5 - from.0).powi(2) + (s.1 as f64 + 0.5 - from.1).powi(2);
            if best.is_none_or(|(bd, _)| d2 < bd) {
                best = Some((d2, s));
            }
        }
        best.map(|(_, s)| s)
    }

    /// Met `spring` de côté jusqu'au tick `until`. Au plus quelques entrées :
    /// la plus ancienne s'oublie.
    pub fn block(&mut self, spring: (i64, i64), until: u64) {
        self.blocked.retain(|(s, _)| *s != spring);
        self.blocked.push((spring, until));
        if self.blocked.len() > MAX_BLOCKED_SPRINGS {
            self.blocked.remove(0);
        }
    }

    /// `spring` est-elle mise de côté au tick `tick` ?
    pub fn is_blocked(&self, spring: (i64, i64), tick: u64) -> bool {
        self.blocked.iter().any(|(s, until)| *s == spring && tick < *until)
    }

    /// La source connue la plus proche qui n'est pas mise de côté.
    pub fn nearest_open_spring(&self, from: (f64, f64), tick: u64) -> Option<(i64, i64)> {
        let mut best: Option<(f64, (i64, i64))> = None;
        for &s in &self.springs {
            if self.is_blocked(s, tick) {
                continue;
            }
            let d2 =
                (s.0 as f64 + 0.5 - from.0).powi(2) + (s.1 as f64 + 0.5 - from.1).powi(2);
            if best.is_none_or(|(bd, _)| d2 < bd) {
                best = Some((d2, s));
            }
        }
        best.map(|(_, s)| s)
    }

    /// L'agent connaît-il cette cellule ?
    pub fn knows_cell(&self, cell: (i64, i64)) -> bool {
        self.known.contains(&cell)
    }
}

/// L'échange quotidien : chaque paire d'agents à portée de conversation met
/// ses sources en commun. Deux passes — on lit tout, puis on écrit tout —
/// pour ne jamais emprunter deux mémoires à la fois, et pour que le résultat
/// ne dépende pas de l'ordre de traitement des paires.
pub(crate) fn exchange_knowledge(sim: &mut Sim) {
    use crate::yields::{PURSUITS, Yields};
    type Shown = ([f32; PURSUITS], [f32; PURSUITS]);
    struct View {
        entity: hecs::Entity,
        id: AgentId,
        pos: (f64, f64),
        springs: Vec<(i64, i64)>,
        shown: Shown,
    }
    let mut views: Vec<View> = sim
        .agents
        .query::<(&crate::agent::AgentId, &crate::agent::Position, &Memory, &Yields)>()
        .iter()
        .map(|(entity, (id, pos, mem, yields))| View {
            entity,
            id: *id,
            pos: (pos.x, pos.y),
            springs: mem.springs.clone(),
            shown: yields.shown(),
        })
        .collect();
    views.sort_unstable_by_key(|v| v.id.0);

    // Ce que chacun apprend (l'union des sources de ses interlocuteurs), et ce
    // qu'il voit faire (MAR-6a : la somme de leur expérience récente, et leur
    // nombre).
    let mut heard: Vec<Vec<(i64, i64)>> = vec![Vec::new(); views.len()];
    let mut seen: Vec<(Shown, u32)> = vec![(([0.0; PURSUITS], [0.0; PURSUITS]), 0); views.len()];
    let add = |acc: &mut (Shown, u32), other: &Shown| {
        for p in 0..PURSUITS {
            acc.0.0[p] += other.0[p];
            acc.0.1[p] += other.1[p];
        }
        acc.1 += 1;
    };
    for i in 0..views.len() {
        for j in (i + 1)..views.len() {
            let (a, b) = (&views[i], &views[j]);
            let d2 = (a.pos.0 - b.pos.0).powi(2) + (a.pos.1 - b.pos.1).powi(2);
            if d2 > TALK_RADIUS_TILES * TALK_RADIUS_TILES {
                continue;
            }
            heard[i].extend(&b.springs);
            heard[j].extend(&a.springs);
            add(&mut seen[i], &b.shown);
            add(&mut seen[j], &a.shown);
        }
    }
    for (view, ((sum_h, sum_net), n)) in views.iter().zip(&seen) {
        if *n == 0 {
            continue;
        }
        if let Ok(yields) = sim.agents.query_one_mut::<&mut Yields>(view.entity) {
            let k = *n as f32;
            yields.observe(sum_h.map(|x| x / k), sum_net.map(|x| x / k));
        }
    }
    for (view, learned) in views.iter().zip(heard) {
        if learned.is_empty() {
            continue;
        }
        if let Ok(mem) = sim.agents.query_one_mut::<&mut Memory>(view.entity) {
            for spring in learned {
                mem.remember_spring(spring, view.pos);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_cellules_se_notent_une_fois() {
        let mut mem = Memory::default();
        for _ in 0..100 {
            mem.note_visit((10, 10)); // piétine la même cellule
        }
        assert_eq!(mem.known.len(), 1);
        mem.note_visit((10 + MEMORY_CELL_TILES, 10));
        assert_eq!(mem.known.len(), 2);
        // Le retour sur une ancienne cellule ne duplique pas.
        mem.note_visit((10, 10));
        assert_eq!(mem.known.len(), 2);
    }

    #[test]
    fn cell_of_est_stable_autour_de_zero() {
        // div_euclid : pas de cellule « double » à cheval sur l'origine.
        assert_eq!(cell_of((-1, -1)), (-1, -1));
        assert_eq!(cell_of((0, 0)), (0, 0));
        assert_eq!(cell_of((MEMORY_CELL_TILES - 1, 0)), (0, 0));
        assert_eq!(cell_of((MEMORY_CELL_TILES, 0)), (1, 0));
        assert_eq!(cell_of((-MEMORY_CELL_TILES, 0)), (-1, 0));
    }

    #[test]
    fn les_sources_lointaines_s_oublient() {
        let mut mem = Memory::default();
        let home = (0.0, 0.0);
        // Une source très lointaine, puis un plein d'autres proches.
        mem.remember_spring((100_000, 0), home);
        for i in 0..MAX_KNOWN_SPRINGS as i64 {
            mem.remember_spring((i * 10, 5), home);
        }
        assert_eq!(mem.springs.len(), MAX_KNOWN_SPRINGS);
        assert!(
            !mem.springs.contains(&(100_000, 0)),
            "la source à 200 km doit être la première oubliée"
        );
        // Dédoublonnage.
        let len = mem.springs.len();
        mem.remember_spring((0, 5), home);
        assert_eq!(mem.springs.len(), len);
    }

    #[test]
    fn la_source_connue_la_plus_proche() {
        let mut mem = Memory::default();
        assert_eq!(mem.nearest_known_spring((0.0, 0.0)), None);
        mem.remember_spring((100, 0), (0.0, 0.0));
        mem.remember_spring((10, 0), (0.0, 0.0));
        mem.remember_spring((-500, 0), (0.0, 0.0));
        assert_eq!(mem.nearest_known_spring((0.0, 0.0)), Some((10, 0)));
        assert_eq!(mem.nearest_known_spring((-400.0, 0.0)), Some((-500, 0)));
    }
}
