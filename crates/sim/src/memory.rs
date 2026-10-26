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
/// Sources retenues au plus : une mémoire, pas un atlas. Au-delà, on oublie
/// celle qu'on a vue le moins récemment (MAR-6d : l'oubli par désusage).
/// « Des centaines » (MAR-6d, décision de l'utilisateur du 2026-10-24) : un
/// homme du désert de l'Ouest en liste plus de 50 qu'il a utilisés, son groupe
/// en connaît plus de 1 200, et un point d'eau se retrouve après 50-60 ans
/// sans visite (SOURCES de la cognition). À 16, chacun était toujours au
/// plafond et le pays quitté sortait de la mémoire en quelques camps.
pub const MAX_KNOWN_SPRINGS: usize = 256;
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
    /// Quand chaque source a été vue pour la dernière fois — par soi, ou par
    /// celui qui en a parlé (on transmet aussi l'âge de ce qu'on sait). C'est
    /// sur lui que se fait l'oubli (MAR-6d, décision de l'utilisateur du
    /// 2026-10-24) : une source s'oublie faute d'avoir été revue, pas parce
    /// qu'on s'en est éloigné — sinon le pays quitté s'efface à mesure qu'on
    /// avance, et la bande ne peut jamais y revenir.
    pub spring_seen: BTreeMap<(i64, i64), u64>,
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
    /// MEM-3 : la même chose, récente — kcal acquises et heures de
    /// subsistance qui s'effacent (demi-vie `crate::yields::RECENT_HALF_LIFE_DAYS`,
    /// celle du registre du camp : ce que le lieu rend en ce moment), à la
    /// date de `last`.
    pub recent_kcal: f32,
    pub recent_h: f32,
    /// Ce qu'on y a vu pousser : le fourrage de la tuile où l'on se tenait,
    /// en moyenne sur les heures de jour passées là (lu en passant, à la
    /// chasse comme en explorant : voir, c'est connaître ; la nuit, on ne voit
    /// pas les plantes), et ces heures.
    pub seen_forage: f32,
    pub seen_h: u32,
    /// MEM-3 par saison (décision de l'utilisateur, 2026-10-23) : la même
    /// chose par quart d'année vécu (`quarter_of`) — kcal acquises, heures de
    /// subsistance, fourrage vu (somme et heures de jour). Un lieu riche en
    /// plantes l'été ne dit rien de ce qu'il rend l'hiver.
    pub q_kcal: [f32; 4],
    pub q_h: [f32; 4],
    pub q_seen_sum: [f32; 4],
    pub q_seen_h: [f32; 4],
    /// Heures à y boire.
    pub drinks: u16,
    /// Nuits à y dormir.
    pub nights: u16,
}

/// Un lieu quitté repousse (MEM-3, plan validé par l'utilisateur) : ce qu'on
/// attend d'un lieu revient de son rendement récent vers son rendement de long
/// terme avec cette demi-vie — celle de la repousse du modèle (une prairie
/// rasée remonte à mi-hauteur en ~2 mois, `ecology::GROWTH_RATE_PER_DAY`).
pub const REGROW_HALF_LIFE_DAYS: f64 = 60.0;

impl Place {
    /// Heures de subsistance vécues sur le lieu.
    pub fn work_hours(&self) -> f32 {
        self.pursuit_hours.iter().sum()
    }

    /// Le rendement de long terme du lieu (kcal par heure de subsistance).
    pub fn long_term(&self) -> f64 {
        let h = f64::from(self.work_hours());
        if h <= 0.0 { 0.0 } else { f64::from(self.kcal.iter().sum::<f32>()) / h }
    }

    /// Ce qu'on attend du lieu au tick `tick`, en kcal acquises par heure de
    /// subsistance : le rendement récent juste après l'avoir quitté, le
    /// rendement de long terme une fois qu'il a repoussé.
    pub fn expected(&self, tick: u64) -> f64 {
        let h = f64::from(self.work_hours());
        if h <= 0.0 {
            return 0.0;
        }
        let long = f64::from(self.kcal.iter().sum::<f32>()) / h;
        if self.recent_h <= 0.0 {
            return long;
        }
        let recent = f64::from(self.recent_kcal / self.recent_h);
        let days = tick.saturating_sub(self.last) as f64 / cairn_core::TICKS_PER_DAY as f64;
        long + (recent - long) * 0.5f64.powf(days / REGROW_HALF_LIFE_DAYS)
    }
}

/// Le quart d'année d'un tick (0 à 3) : le moment de l'année qu'un humain
/// reconnaît par ce qu'il vit (la durée du jour, le froid), pas l'étiquette
/// de saison du climat, que la simulation ne lit pas.
pub fn quarter_of(tick: u64) -> usize {
    ((tick / cairn_core::TICKS_PER_DAY) % cairn_core::DAYS_PER_YEAR * 4 / cairn_core::DAYS_PER_YEAR) as usize
}

impl Place {
    /// Ce que le lieu rend à ce moment de l'année (kcal par heure de
    /// subsistance), s'il y a été travaillé : le rendement récent juste après
    /// l'avoir quitté, celui de la saison une fois qu'il a repoussé. `None` :
    /// on ne l'a pas travaillé à ce moment de l'année.
    pub fn expected_in_season(&self, tick: u64) -> Option<f64> {
        let q = quarter_of(tick);
        if self.q_h[q] <= 0.0 {
            return None;
        }
        let long = f64::from(self.q_kcal[q] / self.q_h[q]);
        if self.recent_h <= 0.0 {
            return Some(long);
        }
        let recent = f64::from(self.recent_kcal / self.recent_h);
        let days = tick.saturating_sub(self.last) as f64 / cairn_core::TICKS_PER_DAY as f64;
        Some(long + (recent - long) * 0.5f64.powf(days / REGROW_HALF_LIFE_DAYS))
    }

    /// Le rendement de la saison seul, et ses heures.
    pub fn season_long(&self, tick: u64) -> Option<(f64, f64)> {
        let q = quarter_of(tick);
        (self.q_h[q] > 0.0).then(|| (f64::from(self.q_kcal[q] / self.q_h[q]), f64::from(self.q_h[q])))
    }

    /// Le fourrage vu à ce moment de l'année : (moyenne, heures de jour).
    pub fn season_seen(&self, tick: u64) -> Option<(f64, f64)> {
        let q = quarter_of(tick);
        (self.q_seen_h[q] > 0.0).then(|| (f64::from(self.q_seen_sum[q] / self.q_seen_h[q]), f64::from(self.q_seen_h[q])))
    }
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

    /// Mémorise une source. Au-delà du plafond, la plus lointaine de `from`
    /// est oubliée : on retient l'eau *utile*.
    pub fn remember_spring(&mut self, spring: (i64, i64), seen: u64) {
        // L'âge se tient pour chaque source connue : le chercher là d'abord
        // évite de parcourir la liste (256 entrées) pour chacune des sources
        // qu'on se redit chaque jour — presque toutes déjà connues.
        let known = self.spring_seen.contains_key(&spring) || self.springs.contains(&spring);
        let last = self.spring_seen.entry(spring).or_insert(seen);
        *last = (*last).max(seen);
        if known {
            return;
        }
        self.springs.push(spring);
        if self.springs.len() > MAX_KNOWN_SPRINGS {
            // La moins récemment vue ; à égalité, la plus anciennement apprise.
            let seen_at = |s: &(i64, i64)| self.spring_seen.get(s).copied().unwrap_or(0);
            let (oldest, _) = self
                .springs
                .iter()
                .enumerate()
                .min_by_key(|(_, s)| seen_at(s))
                .expect("liste non vide");
            let forgotten = self.springs.remove(oldest);
            self.spring_seen.remove(&forgotten);
        }
    }

    /// Ce qu'on a entendu dire en une passe de parole (MAR-6d, étape 4) : les
    /// sources de tous ses interlocuteurs réunies, chacune avec l'âge le plus
    /// récent qu'on en a entendu — **triées par source, sans doublon**. On les
    /// ajoute à ce qu'on sait, puis on oublie les moins récemment vues
    /// au-delà de la capacité : ce qu'on retient ne dépend plus de l'ordre dans
    /// lequel on a parlé à chacun.
    pub fn learn(&mut self, heard: &[((i64, i64), u64)]) {
        for &(spring, seen) in heard {
            match self.spring_seen.get_mut(&spring) {
                Some(last) => *last = (*last).max(seen),
                None => {
                    if !self.springs.contains(&spring) {
                        self.springs.push(spring);
                    }
                    self.spring_seen.insert(spring, seen);
                }
            }
        }
        let excess = self.springs.len().saturating_sub(MAX_KNOWN_SPRINGS);
        if excess > 0 {
            // Les moins récemment vues ; à égalité, les plus anciennement apprises.
            let mut order: Vec<(u64, usize)> =
                self.springs.iter().enumerate().map(|(i, s)| (self.spring_seen_at(*s), i)).collect();
            order.sort_unstable();
            let mut drop = vec![false; self.springs.len()];
            for &(_, i) in &order[..excess] {
                drop[i] = true;
                self.spring_seen.remove(&self.springs[i]);
            }
            let mut i = 0;
            self.springs.retain(|_| {
                i += 1;
                !drop[i - 1]
            });
        }
    }

    /// Quand cette source a été vue pour la dernière fois (0 : inconnue).
    pub fn spring_seen_at(&self, spring: (i64, i64)) -> u64 {
        self.spring_seen.get(&spring).copied().unwrap_or(0)
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
        forage: Option<u8>,
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
        let place = self.places.entry(key).or_insert(Place { first: tick, last: tick, ..Place::default() });
        let hours_since = tick.saturating_sub(place.last) as f64;
        let keep = 0.5f64.powf(hours_since / (crate::yields::RECENT_HALF_LIFE_DAYS * 24.0)) as f32;
        place.recent_kcal *= keep;
        place.recent_h *= keep;
        if pursuit.is_some() {
            place.recent_h += 1.0;
        }
        place.recent_kcal += kcal.iter().sum::<f64>() as f32;
        place.last = tick;
        place.hours = place.hours.saturating_add(1);
        let q = quarter_of(tick);
        if let Some(f) = forage {
            place.seen_h += 1;
            place.seen_forage += (f32::from(f) - place.seen_forage) / place.seen_h as f32;
            place.q_seen_sum[q] += f32::from(f);
            place.q_seen_h[q] += 1.0;
        }
        if pursuit.is_some() {
            place.q_h[q] += 1.0;
        }
        place.q_kcal[q] += kcal.iter().sum::<f64>() as f32;
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
    type Shown = ([f32; PURSUITS], [f32; PURSUITS]);
    struct View {
        entity: hecs::Entity,
        id: AgentId,
        pos: (f64, f64),
        springs: Vec<((i64, i64), u64)>,
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
            springs: {
                let mut v: Vec<_> = mem.springs.iter().map(|&s| (s, mem.spring_seen_at(s))).collect();
                v.sort_unstable();
                v
            },
            shown: yields.shown(),
        })
        .collect();
    views.sort_unstable_by_key(|v| v.id.0);

    // À qui chacun a parlé (il apprend l'union de leurs sources), et ce qu'il
    // voit faire (MAR-6a : la somme de leur expérience récente, et leur
    // nombre).
    let mut partners: Vec<Vec<usize>> = vec![Vec::new(); views.len()];
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
            partners[i].push(j);
            partners[j].push(i);
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
    let mut heard: Vec<((i64, i64), u64)> = Vec::new();
    for (view, others) in views.iter().zip(&partners) {
        if others.is_empty() {
            continue;
        }
        heard.clear();
        for &k in others {
            heard.extend_from_slice(&views[k].springs);
        }
        // Par source, l'âge le plus récent : triées par (source, âge), on garde
        // la dernière de chaque source.
        heard.sort_unstable();
        heard.reverse();
        heard.dedup_by_key(|e| e.0);
        heard.reverse();
        if let Ok(mem) = sim.agents.query_one_mut::<&mut Memory>(view.entity) {
            mem.learn(&heard);
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
    fn la_source_la_moins_recemment_vue_s_oublie() {
        let mut mem = Memory::default();
        // Une source vue il y a longtemps, puis un plein d'autres vues depuis.
        mem.remember_spring((0, 5), 0);
        for i in 0..MAX_KNOWN_SPRINGS as i64 {
            mem.remember_spring((100 + i * 10, 5), 10 + i as u64);
        }
        assert_eq!(mem.springs.len(), MAX_KNOWN_SPRINGS);
        assert!(!mem.springs.contains(&(0, 5)), "la source vue il y a le plus longtemps s'oublie");
        assert_eq!(mem.spring_seen.len(), MAX_KNOWN_SPRINGS, "son âge s'oublie avec elle");
        // Dédoublonnage, et la revoir la rajeunit.
        let len = mem.springs.len();
        mem.remember_spring((100, 5), 1_000);
        assert_eq!(mem.springs.len(), len);
        assert_eq!(mem.spring_seen_at((100, 5)), 1_000);
        // L'entendre dire d'une vieille nouvelle ne la vieillit pas.
        mem.remember_spring((100, 5), 3);
        assert_eq!(mem.spring_seen_at((100, 5)), 1_000);
    }

    /// MAR-6d : la source de l'ancien camp, loin d'ici mais vue récemment,
    /// reste en mémoire ; s'oublie une source proche qu'on n'a pas revue
    /// depuis longtemps. Avec l'oubli par la distance, le pays quitté
    /// s'effaçait à mesure qu'on avançait.
    #[test]
    fn le_pays_quitte_ne_s_oublie_pas_parce_qu_on_s_en_eloigne() {
        let mut mem = Memory::default();
        let old_camp = (20_000, 0); // 40 km
        // Des sources d'ici, apprises il y a longtemps.
        for i in 0..(MAX_KNOWN_SPRINGS as i64 - 1) {
            mem.remember_spring((i * 10, 5), 100 + i as u64);
        }
        // L'ancien camp, où l'on buvait encore la semaine dernière.
        mem.remember_spring(old_camp, 5_000);
        // Une source d'ici, découverte aujourd'hui : la mémoire déborde.
        mem.remember_spring((-500, 5), 5_100);
        assert!(mem.springs.contains(&old_camp), "l'ancien camp, vu récemment, reste connu");
        assert!(!mem.springs.contains(&(0, 5)), "s'oublie la source la moins récemment vue");
    }

    #[test]
    fn la_source_connue_la_plus_proche() {
        let mut mem = Memory::default();
        assert_eq!(mem.nearest_known_spring((0.0, 0.0)), None);
        mem.remember_spring((100, 0), 0);
        mem.remember_spring((10, 0), 0);
        mem.remember_spring((-500, 0), 0);
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
            mem.live_hour(bush, h, Some(Pursuit::Gather), [500.0, 0.0], Some(100), false, false);
        }
        mem.live_hour(camp, 10, None, [0.0; 2], Some(50), true, false);
        mem.live_hour(camp, 11, None, [0.0; 2], Some(50), false, true);
        assert_eq!(mem.places.len(), 2, "deux lieux de 2 km distincts");
        let b = mem.places[&place_of(bush)];
        assert_eq!((b.first, b.last, b.hours), (0, 9, 10));
        assert_eq!(b.pursuit_hours[Pursuit::Gather as usize], 10.0);
        assert_eq!(b.kcal[Pursuit::Gather as usize], 5000.0, "la cueillette du lieu");
        let c = mem.places[&place_of(camp)];
        assert_eq!((c.drinks, c.nights, c.kcal), (1, 1, [0.0; 2]));
    }

    #[test]
    fn un_lieu_epuise_vaut_peu_puis_repousse() {
        use crate::yields::Pursuit;
        use cairn_core::TICKS_PER_DAY;
        let mut mem = Memory::default();
        let spot = (10.0, 10.0);
        // Deux mois riches (1 000 kcal/h), puis un mois épuisé (100 kcal/h).
        let mut t = 0;
        for (days, kcal) in [(60, 1000.0), (30, 100.0)] {
            for _ in 0..days {
                for _ in 0..6 {
                    mem.live_hour(spot, t, Some(Pursuit::Gather), [kcal, 0.0], Some(100), false, false);
                    t += 1;
                }
                t += TICKS_PER_DAY - 6;
            }
        }
        let p = mem.places[&place_of(spot)];
        let now = p.expected(t);
        assert!(now < 400.0, "juste quitté, il est épuisé : {now:.0} kcal/h");
        let later = p.expected(t + 240 * TICKS_PER_DAY);
        let long = f64::from(p.kcal[0] / p.pursuit_hours[0]);
        assert!((later - long).abs() < 0.1 * long, "huit mois après, il a repoussé : {later:.0} contre {long:.0}");
    }

    /// Un lieu riche l'été et pauvre l'hiver ne vaut pas, l'hiver, un lieu
    /// pauvre l'été et riche l'hiver — ce que la mémoire de toute l'année
    /// (`expected`) disait à l'inverse.
    #[test]
    fn un_lieu_se_juge_sur_ce_qu_il_rend_a_ce_moment_de_l_annee() {
        use crate::yields::Pursuit;
        use cairn_core::TICKS_PER_DAY;
        let (a, b) = ((10.0, 10.0), (10.0 + PLACE_TILES * 3.0, 10.0));
        let mut mem = Memory::default();
        let day = |d: u64| d * TICKS_PER_DAY;
        // L'été (jours 180-209) puis l'hiver (jours 0-29 de l'année suivante).
        for d in 180..210 {
            for h in 0..6 {
                mem.live_hour(a, day(d) + h, Some(Pursuit::Gather), [1000.0, 0.0], Some(150), false, false);
                mem.live_hour(b, day(d) + 8 + h, Some(Pursuit::Gather), [200.0, 0.0], Some(60), false, false);
            }
        }
        for d in 360..390 {
            for h in 0..6 {
                mem.live_hour(a, day(d) + h, Some(Pursuit::Gather), [50.0, 0.0], Some(10), false, false);
                mem.live_hour(b, day(d) + 8 + h, Some(Pursuit::Gather), [400.0, 0.0], Some(80), false, false);
            }
        }
        let winter = day(360 + 365 + 10); // l'hiver d'après, un an plus tard
        let (pa, pb) = (mem.places[&place_of(a)], mem.places[&place_of(b)]);
        assert!(pa.expected(winter) > pb.expected(winter), "toute l'année, A paraît meilleur");
        let (va, vb) = (pa.expected_in_season(winter).unwrap(), pb.expected_in_season(winter).unwrap());
        assert!(vb > 3.0 * va, "l'hiver, B rend bien plus que A : {vb:.0} contre {va:.0}");
        assert!(pa.expected_in_season(day(360 + 90 + 10)).is_none(), "au printemps, on n'y a jamais travaillé");
    }

    /// MAR-6d, étape 4 : ce qu'on entend en une passe se réunit avant d'oublier
    /// — deux sources récentes entendues font oublier les deux plus anciennes,
    /// et une source ancienne entendue ne chasse pas une récente.
    #[test]
    fn ce_qu_on_entend_se_reunit_puis_on_oublie_les_plus_anciennes() {
        let mut mem = Memory::default();
        for i in 0..MAX_KNOWN_SPRINGS as i64 {
            mem.remember_spring((i * 10, 0), 100 + i as u64);
        }
        let mut heard = vec![((-10, 0), 50), ((-20, 0), 9_000), ((-30, 0), 9_001), ((10, 0), 8_000)];
        heard.sort_unstable();
        mem.learn(&heard);
        assert_eq!(mem.springs.len(), MAX_KNOWN_SPRINGS);
        assert_eq!(mem.spring_seen.len(), MAX_KNOWN_SPRINGS);
        assert!(mem.springs.contains(&(-20, 0)) && mem.springs.contains(&(-30, 0)), "les récentes entrent");
        assert!(!mem.springs.contains(&(-10, 0)), "l'ancienne entendue ne chasse personne");
        assert!(!mem.springs.contains(&(0, 0)) && !mem.springs.contains(&(20, 0)), "les deux plus anciennes s'oublient");
        assert_eq!(mem.spring_seen_at((10, 0)), 8_000, "une source connue rajeunit");
    }
}
