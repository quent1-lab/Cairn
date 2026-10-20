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

use std::collections::{BTreeMap, BTreeSet};

use crate::agent::AgentId;
use crate::sim::Sim;

/// Côté d'une cellule de mémoire : 256 tuiles = 512 m. Assez fin pour que
/// « explorer » ait un sens local, assez gros pour que la carte d'une vie
/// tienne en quelques milliers d'entrées.
pub const MEMORY_CELL_TILES: i64 = 256;
/// Sources retenues au plus (MEM-4a : ~40 points d'eau d'usage historique à
/// Nyae Nyae, chez les Ju/'hoansi). Au-delà, la moins forte s'oublie.
pub const MAX_KNOWN_SPRINGS: usize = 64;
/// MEM-4a, la force du souvenir d'une source (forme validée par
/// l'utilisateur le 2026-10-18, constantes non sourcées, à mesurer) : chaque
/// fois qu'on la voit, qu'on en entend parler ou qu'on y boit, elle se
/// renforce d'une part de ce qui lui manque ; elle s'efface ensuite avec une
/// constante de temps `SPRING_TAU_DAYS` vers un **plancher qui monte avec
/// l'usage** (jours où l'on y a bu : `uses / (uses + SPRING_FLOOR_USES)`). Une
/// source vue une fois s'oublie en quelques mois ; celle où l'on boit souvent
/// se garde à vie.
const SPRING_TAU_DAYS: f64 = 60.0;
const SPRING_FLOOR_USES: f32 = 10.0;
pub const SPRING_SEEN: f32 = 0.2;
pub const SPRING_HEARD: f32 = 0.1;
const SPRING_USED: f32 = 0.5;
/// Sous cette force, la source est oubliée.
const SPRING_FORGET: f32 = 0.02;

/// Le souvenir d'une source : force à la dernière mise à jour, jours d'usage,
/// tick de la mise à jour, dernier jour où l'on y a bu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringTrace {
    pub strength: f32,
    pub uses: u16,
    pub at: u64,
    last_use_day: u64,
}

impl SpringTrace {
    fn floor(&self) -> f32 {
        f32::from(self.uses) / (f32::from(self.uses) + SPRING_FLOOR_USES)
    }

    /// La force au tick `tick`.
    pub fn strength_at(&self, tick: u64) -> f32 {
        let days = tick.saturating_sub(self.at) as f64 / cairn_core::TICKS_PER_DAY as f64;
        let f = self.floor();
        f + (self.strength - f) * (-days / SPRING_TAU_DAYS).exp() as f32
    }

    fn reinforce(&mut self, tick: u64, boost: f32) {
        let s = self.strength_at(tick);
        self.strength = s + boost * (1.0 - s);
        self.at = tick;
    }
}
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
    /// La force du souvenir de chacune, dans le même ordre (MEM-4a).
    pub spring_traces: Vec<SpringTrace>,
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
    /// MEM-1 : la carte vécue, par maille de 2 km (celle des plantes et des
    /// domaines du gibier) — ce que chaque lieu a été pour soi.
    pub places: BTreeMap<(i64, i64), Place>,
}

/// Côté d'un lieu de la carte vécue : 2 km, la maille des plantes et des
/// domaines du gibier (MEM, décision de l'utilisateur du 2026-10-18).
pub const PLACE_TILES: f64 = cairn_core::km_to_tiles(2.0);
/// Lieux retenus au plus. Un territoire hadza fait ~1 000 mailles de 2 km,
/// un territoire aché ~5 000 : le plafond n'est atteint que par un grand
/// voyageur. Au-delà, le lieu le plus anciennement visité s'oublie (MEM-1 ;
/// la force du souvenir viendra avec MEM-4).
pub const MAX_PLACES: usize = 5000;

/// Ce qu'un lieu a été pour soi (MEM-1, observation : aucune décision ne le
/// lit encore).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Place {
    /// Premier et dernier passage (ticks).
    pub first: u64,
    pub last: u64,
    /// Heures passées sur le lieu, dont à chaque activité de subsistance.
    pub hours: u32,
    pub pursuit_hours: [f32; crate::yields::PURSUITS],
    /// Kcal acquises sur le lieu, par activité.
    pub kcal: [f32; crate::yields::PURSUITS],
    /// Heures à y boire.
    pub drinks: u16,
    /// Nuits à y dormir.
    pub nights: u16,
}

/// Le lieu (maille de 2 km) d'une position.
pub fn place_of(pos: (f64, f64)) -> (i64, i64) {
    ((pos.0 / PLACE_TILES).floor() as i64, (pos.1 / PLACE_TILES).floor() as i64)
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

    /// Mémorise une source, vue (`SPRING_SEEN`) ou entendue (`SPRING_HEARD`),
    /// ou renforce son souvenir. Au-delà du plafond, la moins forte s'oublie
    /// (MEM-4a : par l'usage, plus par la distance).
    pub fn remember_spring(&mut self, spring: (i64, i64), tick: u64, boost: f32) {
        if let Some(i) = self.springs.iter().position(|s| *s == spring) {
            self.spring_traces[i].reinforce(tick, boost);
            return;
        }
        self.springs.push(spring);
        self.spring_traces.push(SpringTrace { strength: boost, uses: 0, at: tick, last_use_day: u64::MAX });
        if self.springs.len() > MAX_KNOWN_SPRINGS {
            let (weakest, _) = self
                .spring_traces
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.strength_at(tick).total_cmp(&b.strength_at(tick)))
                .expect("liste non vide");
            self.springs.swap_remove(weakest);
            self.spring_traces.swap_remove(weakest);
        }
    }

    /// On a bu à cette source : un jour d'usage de plus (une fois par jour),
    /// et un fort renfort.
    pub fn use_spring(&mut self, spring: (i64, i64), tick: u64) {
        self.remember_spring(spring, tick, SPRING_USED);
        let day = tick / cairn_core::TICKS_PER_DAY;
        if let Some(i) = self.springs.iter().position(|s| *s == spring) {
            let t = &mut self.spring_traces[i];
            if t.last_use_day != day {
                t.last_use_day = day;
                t.uses = t.uses.saturating_add(1);
            }
        }
    }

    /// Oublie les sources dont le souvenir est trop faible.
    pub fn forget_springs(&mut self, tick: u64) {
        let mut i = 0;
        while i < self.springs.len() {
            if self.spring_traces[i].strength_at(tick) < SPRING_FORGET {
                self.springs.swap_remove(i);
                self.spring_traces.swap_remove(i);
            } else {
                i += 1;
            }
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

    /// MEM-1 : une heure vécue en `pos` — l'activité de subsistance qu'on y a
    /// menée, les kcal qu'elle y a acquises, si l'on y a bu ou dormi.
    pub fn live_hour(
        &mut self,
        pos: (f64, f64),
        tick: u64,
        pursuit: Option<crate::yields::Pursuit>,
        kcal: [f64; crate::yields::PURSUITS],
        drank: bool,
        slept: bool,
    ) {
        let key = place_of(pos);
        if !self.places.contains_key(&key) && self.places.len() >= MAX_PLACES {
            let oldest = self.places.iter().min_by_key(|(k, p)| (p.last, **k)).map(|(k, _)| *k);
            if let Some(k) = oldest {
                self.places.remove(&k);
            }
        }
        let place = self.places.entry(key).or_insert(Place { first: tick, ..Place::default() });
        place.last = tick;
        place.hours = place.hours.saturating_add(1);
        if let Some(p) = pursuit {
            place.pursuit_hours[p as usize] += 1.0;
        }
        for (i, k) in kcal.iter().enumerate() {
            place.kcal[i] += *k as f32;
        }
        place.drinks = place.drinks.saturating_add(u16::from(drank));
        place.nights = place.nights.saturating_add(u16::from(slept));
    }
}

/// L'échange quotidien : chaque paire d'agents à portée de conversation met
/// ses sources en commun. Deux passes — on lit tout, puis on écrit tout —
/// pour ne jamais emprunter deux mémoires à la fois, et pour que le résultat
/// ne dépende pas de l'ordre de traitement des paires.
pub(crate) fn exchange_knowledge(sim: &mut Sim) {
    use crate::yields::{PURSUITS, Yields};
    let tick = sim.time.tick;
    for (_, mem) in sim.agents.query_mut::<&mut Memory>() {
        mem.forget_springs(tick);
    }
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
                mem.remember_spring(spring, tick, SPRING_HEARD);
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
    fn une_source_s_oublie_par_l_usage_pas_par_la_distance() {
        use cairn_core::TICKS_PER_DAY;
        let mut mem = Memory::default();
        // La source du pays, à 20 km, où l'on a bu trente jours.
        let home_spring = (10_000, 0);
        for d in 0..30 {
            mem.use_spring(home_spring, d * TICKS_PER_DAY);
        }
        // Puis un voyage : on voit en passant un plein de sources proches.
        let t = 40 * TICKS_PER_DAY;
        for i in 0..MAX_KNOWN_SPRINGS as i64 {
            mem.remember_spring((i * 10, 5), t + i as u64, SPRING_SEEN);
        }
        assert_eq!(mem.springs.len(), MAX_KNOWN_SPRINGS);
        assert!(mem.springs.contains(&home_spring), "la source du pays reste, loin ou pas");
        // Dédoublonnage.
        let len = mem.springs.len();
        mem.remember_spring((0, 5), t, SPRING_SEEN);
        assert_eq!(mem.springs.len(), len);
        // Un an plus tard, les sources vues en passant sont oubliées ; celle du
        // pays tient sur son plancher (30 jours d'usage : 0,75).
        mem.forget_springs(t + 360 * TICKS_PER_DAY);
        assert_eq!(mem.springs, vec![home_spring]);
        let s = mem.spring_traces[0].strength_at(t + 360 * TICKS_PER_DAY);
        assert!((0.7..0.8).contains(&s), "plancher de l'usage : {s}");
    }

    #[test]
    fn la_source_connue_la_plus_proche() {
        let mut mem = Memory::default();
        assert_eq!(mem.nearest_known_spring((0.0, 0.0)), None);
        mem.remember_spring((100, 0), 0, SPRING_SEEN);
        mem.remember_spring((10, 0), 0, SPRING_SEEN);
        mem.remember_spring((-500, 0), 0, SPRING_SEEN);
        assert_eq!(mem.nearest_known_spring((0.0, 0.0)), Some((10, 0)));
        assert_eq!(mem.nearest_known_spring((-400.0, 0.0)), Some((-500, 0)));
    }

    #[test]
    fn la_carte_vecue_retient_ce_que_chaque_lieu_a_rendu() {
        use crate::yields::Pursuit;
        let mut mem = Memory::default();
        let camp = (10.0, 10.0);
        let bush = (10.0 + PLACE_TILES * 3.0, 10.0);
        for h in 0..10 {
            mem.live_hour(bush, h, Some(Pursuit::Gather), [500.0, 0.0], false, false);
        }
        mem.live_hour(camp, 10, None, [0.0; 2], true, false);
        mem.live_hour(camp, 11, None, [0.0; 2], false, true);
        assert_eq!(mem.places.len(), 2, "deux lieux de 2 km distincts");
        let b = mem.places[&place_of(bush)];
        assert_eq!((b.first, b.last, b.hours), (0, 9, 10));
        assert_eq!(b.pursuit_hours[Pursuit::Gather as usize], 10.0);
        assert_eq!(b.kcal[Pursuit::Gather as usize], 5000.0, "la cueillette du lieu");
        let c = mem.places[&place_of(camp)];
        assert_eq!((c.drinks, c.nights, c.kcal), (1, 1, [0.0; 2]));
    }
}
