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
//! membres (`Yields::habit`), **hors chasse** l'un et l'autre : ce sont les
//! plantes qui s'épuisent autour d'un camp. Sous l'habitude, il lève le camp vers une source
//! d'eau que connaît l'un d'eux, hors du pays déjà battu, là où **la bande se
//! souvient** que la subsistance rendait le mieux (MEM-3 : la carte vécue de
//! ses membres, `Memory::places`, et non plus le stock réel des plantes) — le
//! trajet pesé à l'échelle d'une journée de marche ; la chasse y entre par ce
//! qu'on a tiré du lieu, pas par le gibier vu ces derniers jours. Un lieu dont personne ne se souvient assez vaut ce que rendent
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

/// Une levée du camp, relevée pour les bancs (pré-mortem de MEM-3) : où, et
/// sur quoi le choix s'est fait.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CampMove {
    pub tick: u64,
    pub clan: u64,
    pub from: (f64, f64),
    pub to: (f64, f64),
    /// Sources candidates hors du pays battu, dont celles dont le lieu est
    /// connu (un membre y est passé de jour : comparable).
    pub candidates: u32,
    pub known: u32,
    /// Le lieu retenu a-t-il été travaillé (sinon : seulement vu) ? Ce qu'on en
    /// attend, les kcal/h qu'apprend une unité de fourrage vu, le gibier vu
    /// alentour (relevé, sans poids) et le facteur du trajet.
    pub chosen_known: bool,
    pub remembered: f64,
    pub prior: f64,
    pub game: u32,
    pub day: f64,
    /// Ce qui a déclenché la levée (kcal nettes par jour d'adulte) : ce que
    /// rendait le camp et l'habitude, et la part de la chasse dans chacun ;
    /// jours depuis la levée précédente du clan (ou sa fondation).
    pub now: f64,
    pub habit: f64,
    pub now_hunt: f64,
    pub habit_hunt: f64,
    pub days_at_camp: f64,
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
    day_net_hunt: f32,
    habit_net_hunt: f32,
    springs: BTreeSet<(i64, i64)>,
    /// Le gibier vu récemment, par maille de 2 km : un troupeau vu par dix
    /// adultes reste un troupeau.
    game: BTreeSet<(i64, i64)>,
    /// MEM-3 : ce que ses membres savent de chaque lieu de 2 km où l'un d'eux
    /// est passé.
    places: BTreeMap<(i64, i64), BandPlace>,
}

/// Ce que la bande sait d'un lieu : ce qu'on en attend, pesé par les heures de
/// subsistance qu'on y a vécues ; le fourrage qu'on y a vu, pesé par les heures
/// de jour qu'on y a passées.
#[derive(Debug, Clone, Copy, Default)]
struct BandPlace {
    worked_value: f64,
    /// Le rendement de long terme, pesé de même : c'est sur lui que s'apprend
    /// ce que vaut une unité de fourrage vu (un camp qu'on vient d'épuiser ne
    /// doit pas faire paraître épuisé tout pays vu).
    long_value: f64,
    worked_h: f64,
    seen_sum: f64,
    seen_h: f64,
}

impl BandPlace {
    fn seen_mean(&self) -> f64 {
        if self.seen_h > 0.0 { self.seen_sum / self.seen_h } else { 0.0 }
    }
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
        let hunt_today = yields.day_net_by()[crate::yields::Pursuit::Hunt as usize];
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
            band.day_net_hunt += hunt_today;
        }
        band.habit_h += hh;
        band.habit_net += hn;
        band.habit_net_hunt += yields.habit_net_by()[crate::yields::Pursuit::Hunt as usize];
        band.springs.extend(mem.springs.iter().copied());
        // Ce que chaque lieu rend **à ce moment de l'année** (MEM-3 par saison,
        // décision de l'utilisateur, 2026-10-23).
        for (key, place) in &mem.places {
            let e = band.places.entry(*key).or_default();
            if let (Some(v), Some((long, h))) = (place.expected_in_season(tick), place.season_long(tick)) {
                e.worked_value += v * h;
                e.long_value += long * h;
                e.worked_h += h;
            }
            if let Some((mean, h)) = place.season_seen(tick) {
                e.seen_sum += mean * h;
                e.seen_h += h;
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
        clan.camp.net_hunt = clan.camp.net_hunt * keep + band.day_net_hunt;
        let evidence = band.adults as f32 * RECENT_EVIDENCE_HOURS / 24.0;
        // Hors chasse, de part et d'autre (décision de l'utilisateur,
        // 2026-10-20) : ce qui s'épuise autour d'un camp, ce sont les plantes ;
        // le gibier bouge, et une prise de 60 000 kcal dans l'habitude du mois
        // faisait paraître épuisé tout camp sans prise récente.
        if clan.camp.h < evidence || band.habit_h < 2.0 * evidence {
            continue;
        }
        let (now, habit) = (clan.camp.net / clan.camp.h, band.habit_net / band.habit_h);
        // MAR-6b c (décision de l'utilisateur, 2026-10-20, solution 3) : on ne
        // part que si un lieu connu promet mieux que le camp, trajet compris —
        // la valeur marginale contre ce qu'on peut attendre **ailleurs**, plus
        // contre l'habitude du mois (qui contient l'automne : en hiver, tout le
        // pays rend peu, et la bande partait vers un lieu qui ne rendait pas mieux).
        let Some(here) = band.places.get(&crate::memory::place_of(clan.home)).filter(|p| p.worked_h > 0.0) else {
            continue;
        };
        let here_value = here.worked_value / here.worked_h;
        if let Some((dest, mut choice)) = choose_destination(clan.home, band)
            && choice.remembered * choice.day > here_value
        {
            choice.tick = tick;
            choice.clan = clan.id.0;
            choice.now = f64::from(now);
            choice.habit = f64::from(habit);
            choice.now_hunt = f64::from(clan.camp.net_hunt / clan.camp.h);
            choice.habit_hunt = f64::from(band.habit_net_hunt / band.habit_h);
            let since = sim.camp_moves.iter().rev().find(|m| m.clan == clan.id.0).map_or(clan.founded_tick, |m| m.tick);
            choice.days_at_camp = tick.saturating_sub(since) as f64 / TICKS_PER_DAY as f64;
            sim.camp_moves.push(choice);
            clan.home = dest;
            clan.camp = Default::default();
        }
    }
    sim.clans = clans;
}

/// Le lieu du prochain camp : une source connue hors du pays battu (au-delà
/// du rayon de résidence), là où la bande se souvient que la subsistance
/// rendait le mieux par heure, pesée par le trajet à l'échelle d'une journée
/// de marche.
fn choose_destination(home: (f64, f64), band: &BandDay) -> Option<((f64, f64), CampMove)> {
    // Ce que vaut, en kcal par heure, une unité de fourrage vu : appris sur les
    // lieux que la bande a à la fois vus et travaillés. C'est ce qui permet de
    // comparer un lieu seulement vu (en chassant, en explorant) à un lieu exploité.
    let (num, den) = band.places.values().filter(|p| p.worked_h > 0.0 && p.seen_h > 0.0).fold((0.0, 0.0), |a, p| {
        (a.0 + p.long_value, a.1 + p.seen_mean() * p.worked_h)
    });
    let per_forage = (den > 0.0).then(|| num / den);
    let mut best: Option<(f64, (f64, f64), CampMove)> = None;
    let (mut candidates, mut known) = (0u32, 0u32);
    for &spring in &band.springs {
        let p = (spring.0 as f64 + 0.5, spring.1 as f64 + 0.5);
        let d = (p.0 - home.0).hypot(p.1 - home.1);
        if d < RESIDENCE_RADIUS_TILES {
            continue; // le pays qu'on quitte
        }
        candidates += 1;
        // On ne compare que ce qu'on connaît (décision de l'utilisateur,
        // 2026-10-20) : une source dont on a seulement entendu parler, dans un
        // pays où personne n'est allé, n'a pas de valeur — ni bonne ni moyenne.
        let Some(place) = band.places.get(&crate::memory::place_of(p)) else { continue };
        let (remembered, worked) = if place.worked_h > 0.0 {
            (place.worked_value / place.worked_h, true)
        } else if let Some(k) = per_forage
            && place.seen_h > 0.0
        {
            (k * place.seen_mean(), false)
        } else {
            continue; // passé de nuit seulement : rien vu
        };
        known += 1;
        let side = crate::fauna::RANGE_ZONE_TILES;
        let game = band
            .game
            .iter()
            .filter(|z| ((z.0 as f64 + 0.5) * side - p.0).hypot((z.1 as f64 + 0.5) * side - p.1) <= GAME_NEAR_TILES)
            .count();
        let day = 1.0 / (1.0 + d / (WALK_TILES_PER_TICK * 24.0));
        // Le gibier vu ne s'y ajoute pas (décision de l'utilisateur, 2026-10-20) :
        // la chasse compte déjà dans ce que la bande se souvient d'y avoir tiré,
        // et un troupeau vu ces trois derniers jours pesait plus que tout ce
        // qu'on savait du pays (étape 1 : gibier au lieu retenu 91 %, ×3,9).
        // Il reste relevé (`CampMove::game`).
        let score = remembered * day;
        if score > 0.0 && best.is_none_or(|(b, _, _)| score > b) {
            let choice = CampMove {
                tick: 0,
                clan: 0,
                from: home,
                to: p,
                candidates: 0,
                known: 0,
                chosen_known: worked,
                remembered,
                prior: per_forage.unwrap_or(0.0),
                game: game as u32,
                day,
                now: 0.0,
                habit: 0.0,
                now_hunt: 0.0,
                habit_hunt: 0.0,
                days_at_camp: 0.0,
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

    /// Un lieu travaillé 100 h qui rendait `kcal_h`, où l'on voyait `forage`.
    fn worked(kcal_h: f64, forage: f64) -> BandPlace {
        BandPlace { worked_value: kcal_h * 100.0, long_value: kcal_h * 100.0, worked_h: 100.0, seen_sum: forage * 100.0, seen_h: 100.0 }
    }

    fn at(s: (i64, i64)) -> (i64, i64) {
        place_of((s.0 as f64 + 0.5, s.1 as f64 + 0.5))
    }

    /// MEM-3 : la bande part là où elle se souvient que le pays rendait, pas
    /// vers celui qu'elle sait pauvre ; sans rien savoir, elle ne part pas.
    #[test]
    fn le_camp_part_vers_le_lieu_dont_on_se_souvient_qu_il_rendait() {
        let home = (0.0, 0.0);
        let d = 2.5 * PLACE_TILES; // hors du pays battu, à égale distance
        let (rich, poor) = ((d as i64, 0), (-d as i64, 0));
        let mut band = BandDay::default();
        band.springs.extend([rich, poor]);
        assert!(choose_destination(home, &band).is_none(), "rien de vécu : on ne part pas");
        band.places.insert(at(rich), worked(2000.0, 100.0));
        band.places.insert(at(poor), worked(100.0, 100.0));
        let (dest, _) = choose_destination(home, &band).expect("une destination");
        assert_eq!(place_of(dest), at(rich), "vers le lieu qui rendait");
    }

    /// On ne compare pas un pays où personne n'est allé : une source connue
    /// par ouï-dire n'y attire pas la bande, même si les lieux connus sont
    /// pauvres (décision de l'utilisateur, 2026-10-20).
    #[test]
    fn on_ne_part_pas_vers_un_pays_ou_personne_n_est_alle() {
        let home = (0.0, 0.0);
        let d = 2.5 * PLACE_TILES;
        let (known_poor, heard, rich_home) = ((d as i64, 0), (-d as i64, 0), (0, 0));
        let mut band = BandDay::default();
        band.springs.extend([known_poor, heard]);
        band.places.insert(at(rich_home), worked(3000.0, 100.0)); // le pays qu'on quitte
        band.places.insert(at(known_poor), worked(200.0, 30.0));
        let (dest, _) = choose_destination(home, &band).expect("une destination");
        assert_eq!(place_of(dest), at(known_poor), "le connu, même pauvre, plutôt que l'inconnu");
        band.places.remove(&at(known_poor));
        assert!(choose_destination(home, &band).is_none(), "seul l'inconnu reste : on ne part pas");
    }

    /// Un lieu seulement vu (en chassant, en explorant) se compare aux lieux
    /// travaillés par ce qu'on y a vu pousser.
    #[test]
    fn un_lieu_vu_se_compare_par_ce_qu_on_y_a_vu_pousser() {
        let home = (0.0, 0.0);
        let d = 2.5 * PLACE_TILES;
        let (seen_rich, poor, rich_home) = ((d as i64, 0), (-d as i64, 0), (0, 0));
        let mut band = BandDay::default();
        band.springs.extend([seen_rich, poor]);
        band.places.insert(at(rich_home), worked(500.0, 50.0)); // 10 kcal/h par unité de fourrage
        band.places.insert(at(poor), worked(200.0, 20.0));
        band.places.insert(at(seen_rich), BandPlace { seen_sum: 100.0 * 10.0, seen_h: 10.0, ..BandPlace::default() });
        let (dest, choice) = choose_destination(home, &band).expect("une destination");
        assert_eq!(place_of(dest), at(seen_rich), "le pays vu riche l'emporte sur le pays travaillé pauvre");
        assert!(!choice.chosen_known);
        assert!((choice.remembered - 1000.0).abs() < 1.0, "100 de fourrage vu × 10 kcal/h : {}", choice.remembered);
    }

    /// Le gibier vu ces derniers jours ne fait pas préférer un lieu : la
    /// chasse compte déjà dans ce que la bande se souvient d'y avoir tiré
    /// (décision de l'utilisateur, 2026-10-20, choix 1).
    #[test]
    fn le_gibier_vu_ne_l_emporte_pas_sur_ce_dont_on_se_souvient() {
        let home = (0.0, 0.0);
        let d = 2.5 * PLACE_TILES;
        let (rich, poor) = ((d as i64, 0), (-d as i64, 0));
        let mut band = BandDay::default();
        band.springs.extend([rich, poor]);
        band.places.insert(at(rich), worked(1000.0, 100.0));
        band.places.insert(at(poor), worked(600.0, 100.0));
        band.see_game(poor); // un troupeau vu près du lieu le moins bon
        let (dest, _) = choose_destination(home, &band).expect("une destination");
        assert_eq!(place_of(dest), at(rich), "on va où le pays rendait, pas où l'on a vu passer un troupeau");
    }
}
