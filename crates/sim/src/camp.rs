//! Lever le camp (MAR-6b) : une bande quitte le pays qu'elle a épuisé.
//!
//! Le théorème de la valeur marginale (Charnov) décrit bien les camps des
//! chasseurs-cueilleurs : on part quand ce que rend le lieu **en ce moment**
//! tombe sous ce qu'on obtient d'habitude (Batek : 8,2 jours par camp, 3 à
//! 24 ; la MVT explique 56 % de la variance des séjours, et le départ anticipe
//! le rendement décroissant — Venkataraman et al. 2017). La décision est
//! collective : c'est la perception commune de l'épuisement qui fait partir.
//!
//! Ici, chaque nuit, le clan relève ce que ses adultes ont tiré du pays
//! (`Yields::take_day`) dans le registre de son camp (`CampLedger`, demi-vie
//! de quelques jours, remis à zéro au départ) et le compare à l'habitude de ses
//! membres (`Yields::habit`). Sous l'habitude, il lève le camp vers une source
//! d'eau que connaît l'un d'eux, hors du pays déjà battu, là où **la bande se
//! souvient** que la subsistance rendait le mieux (MEM-3 : la carte vécue de
//! ses membres, `Memory::places`, et non plus le stock réel des plantes) et où l'on a
//! vu du gibier ces derniers jours — le trajet pesé à l'échelle d'une journée
//! de marche. Un lieu dont personne ne se souvient assez vaut ce que rendent
//! en moyenne les lieux connus : ni préféré, ni écarté. Rien ne dit
//! « partez » : sans source connue hors du pays, la bande reste.
//!
//! Le foyer (`Clan::home`) est ce camp : la délibération du lendemain y ramène
//! les membres (`ReturnToClan`, dormir au camp) et c'est de là que partent les
//! sorties. La bande se déplace en y allant.

use std::collections::{BTreeMap, BTreeSet};

use cairn_core::{TICKS_PER_DAY, km_to_tiles};

use crate::agent::WALK_TILES_PER_TICK;
use crate::demography::Demographics;
use crate::memory::Memory;
use crate::sim::Sim;
use crate::social::{ClanMembership, RESIDENCE_RADIUS_TILES};
use crate::yields::{RECENT_EVIDENCE_HOURS, RECENT_HALF_LIFE_DAYS, Yields};

/// Distance (tuiles) en deçà de laquelle un troupeau vu près d'un lieu compte
/// pour ce lieu : la portée du regard d'un chasseur (~2 km) et un peu plus.
const GAME_NEAR_TILES: f64 = km_to_tiles(3.0);
/// Fraîcheur d'un troupeau vu, pour qu'il compte dans le choix d'un lieu :
/// le gibier bouge (même échelle que `brain::GAME_MEMORY_DAYS`).
const GAME_FRESH_TICKS: u64 = 3 * TICKS_PER_DAY;
/// En deçà de ces heures de subsistance vécues sur un lieu, la bande ne le
/// juge pas encore (même seuil que `yields::EVIDENCE_HOURS`).
const PLACE_EVIDENCE_HOURS: f64 = crate::yields::EVIDENCE_HOURS as f64;

/// Une levée du camp, relevée pour les bancs (pré-mortem de MEM-3) : où, et
/// sur quoi le choix s'est fait.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CampMove {
    pub tick: u64,
    pub clan: u64,
    pub from: (f64, f64),
    pub to: (f64, f64),
    /// Sources candidates hors du pays battu, dont celles dont le lieu est
    /// jugé sur la mémoire (assez d'heures vécues).
    pub candidates: u32,
    pub known: u32,
    /// Le lieu retenu est-il jugé sur la mémoire ? Ses trois facteurs, et la
    /// valeur par défaut des lieux inconnus.
    pub chosen_known: bool,
    pub remembered: f64,
    pub prior: f64,
    pub game: u32,
    pub day: f64,
}

/// Ce que la bande sait et a vécu ce jour, par clan.
#[derive(Default)]
struct BandDay {
    adults: u32,
    day_net: f32,
    /// Adultes qui ont passé la nuit au camp (à moins du rayon de résidence).
    at_camp: u32,
    /// Jours d'adulte et kcal nettes de l'habitude (le mois).
    habit_h: f32,
    habit_net: f32,
    springs: BTreeSet<(i64, i64)>,
    /// Le gibier vu récemment, par maille de 2 km : un troupeau vu par dix
    /// adultes reste un troupeau.
    game: BTreeSet<(i64, i64)>,
    /// MEM-3 : ce que ses membres attendent de chaque lieu de 2 km (kcal par
    /// heure, pesées par les heures qu'ils y ont vécues) : (somme pesée, heures).
    places: BTreeMap<(i64, i64), (f64, f64)>,
}

impl BandDay {
    fn see_game(&mut self, spot: (i64, i64)) {
        self.game.insert(crate::fauna::range_zone((spot.0 as f64 + 0.5, spot.1 as f64 + 0.5)));
    }
}

/// La passe de minuit : relève le jour, et lève le camp des bandes dont le
/// pays rend moins que l'habitude.
pub(crate) fn daily(sim: &mut Sim) {
    let tick = sim.time.tick;
    let mut bands: BTreeMap<u64, BandDay> = BTreeMap::new();
    let homes: BTreeMap<u64, (f64, f64)> = sim.clans.iter().map(|c| (c.id.0, c.home)).collect();
    for (_, (membership, demo, yields, mem, pos)) in sim
        .agents
        .query_mut::<(&ClanMembership, &Demographics, &mut Yields, &Memory, &crate::agent::Position)>()
    {
        let (_, dn) = yields.take_day();
        let Some(clan) = membership.0 else { continue };
        if !demo.is_adult(tick) {
            continue;
        }
        let band = bands.entry(clan.0).or_default();
        let (hh, hn) = yields.habit_per_day();
        band.adults += 1;
        // Le jour ne compte pour le camp que si l'on y est : le jour de marche
        // vers un nouveau camp pèse dans l'habitude (trajets compris), pas
        // dans ce que rend le lieu où l'on arrive.
        if homes.get(&clan.0).is_some_and(|h| (pos.x - h.0).hypot(pos.y - h.1) <= RESIDENCE_RADIUS_TILES) {
            band.at_camp += 1;
            band.day_net += dn;
        }
        band.habit_h += hh;
        band.habit_net += hn;
        band.springs.extend(mem.springs.iter().copied());
        for (key, place) in &mem.places {
            let h = f64::from(place.work_hours());
            if h > 0.0 {
                let e = band.places.entry(*key).or_default();
                e.0 += place.expected(tick) * h;
                e.1 += h;
            }
        }
        if let Some((spot, seen)) = mem.game
            && tick.saturating_sub(seen) < GAME_FRESH_TICKS
        {
            band.see_game(spot);
        }
    }

    let keep = 0.5f32.powf(1.0 / RECENT_HALF_LIFE_DAYS as f32);
    let mut clans = std::mem::take(&mut sim.clans);
    for clan in &mut clans {
        let Some(band) = bands.get(&clan.id.0) else { continue };
        // Par jour d'adulte, et non par heure de travail : la valeur marginale
        // juge contre la moyenne **trajets compris** (Charnov) — un jour de
        // marche vers un nouveau camp ne rapporte rien, et il compte dans
        // l'habitude ; le camp, lui, ne compte que les jours qu'on y a passés.
        clan.camp.h = clan.camp.h * keep + band.at_camp as f32;
        clan.camp.net = clan.camp.net * keep + band.day_net;
        let evidence = band.adults as f32 * RECENT_EVIDENCE_HOURS / 24.0;
        if clan.camp.h < evidence || band.habit_h < 2.0 * evidence || band.habit_net <= 0.0 {
            continue;
        }
        let now = clan.camp.net / clan.camp.h;
        let habit = band.habit_net / band.habit_h;
        if now >= habit {
            continue;
        }
        if let Some((dest, mut choice)) = choose_destination(clan.home, band) {
            choice.tick = tick;
            choice.clan = clan.id.0;
            sim.camp_moves.push(choice);
            clan.home = dest;
            clan.camp = Default::default();
        }
    }
    sim.clans = clans;
}

/// Le lieu du prochain camp : une source connue hors du pays battu (au-delà
/// du rayon de résidence), là où la bande se souvient que la subsistance
/// rendait le mieux par heure, et près du gibier vu, pesée par le trajet à
/// l'échelle d'une journée de marche.
fn choose_destination(home: (f64, f64), band: &BandDay) -> Option<((f64, f64), CampMove)> {
    let (all_kcal, all_h) = band.places.values().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
    if all_h < PLACE_EVIDENCE_HOURS {
        return None; // la bande ne sait encore rien de ce que rendent les lieux
    }
    let prior = all_kcal / all_h;
    let mut best: Option<(f64, (f64, f64), CampMove)> = None;
    let (mut candidates, mut known) = (0u32, 0u32);
    for &spring in &band.springs {
        let p = (spring.0 as f64 + 0.5, spring.1 as f64 + 0.5);
        let d = (p.0 - home.0).hypot(p.1 - home.1);
        if d < RESIDENCE_RADIUS_TILES {
            continue; // le pays qu'on quitte
        }
        let (remembered, is_known) = match band.places.get(&crate::memory::place_of(p)) {
            Some(&(kcal, h)) if h >= PLACE_EVIDENCE_HOURS => (kcal / h, true),
            _ => (prior, false),
        };
        candidates += 1;
        known += u32::from(is_known);
        let side = crate::fauna::RANGE_ZONE_TILES;
        let game = band
            .game
            .iter()
            .filter(|z| ((z.0 as f64 + 0.5) * side - p.0).hypot((z.1 as f64 + 0.5) * side - p.1) <= GAME_NEAR_TILES)
            .count();
        let day = 1.0 / (1.0 + d / (WALK_TILES_PER_TICK * 24.0));
        let score = remembered * (1.0 + game as f64) * day;
        if score > 0.0 && best.is_none_or(|(b, _, _)| score > b) {
            let choice = CampMove {
                tick: 0,
                clan: 0,
                from: home,
                to: p,
                candidates: 0,
                known: 0,
                chosen_known: is_known,
                remembered,
                prior,
                game: game as u32,
                day,
            };
            best = Some((score, p, choice));
        }
    }
    best.map(|(_, p, mut c)| {
        c.candidates = candidates;
        c.known = known;
        (p, c)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{PLACE_TILES, place_of};

    /// MEM-3 : la bande part là où elle se souvient que le pays rendait, pas
    /// vers le lieu qu'elle n'a jamais exploité ni vers celui qu'elle sait
    /// pauvre ; sans rien savoir, elle ne part pas.
    #[test]
    fn le_camp_part_vers_le_lieu_dont_on_se_souvient_qu_il_rendait() {
        let home = (0.0, 0.0);
        let d = 2.5 * PLACE_TILES; // hors du pays battu, à égale distance
        let (rich, poor, unknown) = ((d as i64, 0), (-d as i64, 0), (0, d as i64));
        let mut band = BandDay::default();
        band.springs.extend([rich, poor, unknown]);
        let at = |s: (i64, i64)| place_of((s.0 as f64 + 0.5, s.1 as f64 + 0.5));
        assert!(choose_destination(home, &band).is_none(), "rien de vécu : on ne part pas");
        band.places.insert(at(rich), (200_000.0, 100.0)); // 2 000 kcal/h
        band.places.insert(at(poor), (10_000.0, 100.0)); // 100 kcal/h
        let (dest, _) = choose_destination(home, &band).expect("une destination");
        assert_eq!(place_of(dest), at(rich), "vers le lieu qui rendait");
    }

    /// Un troupeau vu par trente adultes reste un troupeau : il ne fait pas
    /// préférer le lieu pauvre où il paît au lieu riche.
    #[test]
    fn un_troupeau_vu_par_tous_compte_une_fois() {
        let home = (0.0, 0.0);
        let d = 2.5 * PLACE_TILES;
        let (rich, poor) = ((d as i64, 0), (-d as i64, 0));
        let mut band = BandDay::default();
        band.springs.extend([rich, poor]);
        let at = |s: (i64, i64)| place_of((s.0 as f64 + 0.5, s.1 as f64 + 0.5));
        band.places.insert(at(rich), (200_000.0, 100.0)); // 2 000 kcal/h
        band.places.insert(at(poor), (10_000.0, 100.0)); // 100 kcal/h
        for k in 0..30 {
            band.see_game((poor.0 + k, poor.1)); // le même troupeau, trente témoins
        }
        let (dest, _) = choose_destination(home, &band).expect("une destination");
        assert_eq!(place_of(dest), at(rich), "le gibier compte par troupeau, pas par témoin");
    }
}
