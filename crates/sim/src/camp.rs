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
//! d'eau que connaît l'un d'eux, hors du pays déjà battu, là où les plantes
//! sont les plus abondantes par bouche et où l'on a vu du gibier ces derniers
//! jours — le trajet pesé à l'échelle d'une journée de marche. Rien ne dit
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
/// Une semaine de nourriture pour une bouche (kcal) : l'unité dans laquelle on
/// juge l'abondance d'un pays (même idiome que `brain::edible_share`).
const WEEK_KCAL: f64 = 7.0 * 2_500.0;

/// Ce que la bande sait et a vécu ce jour, par clan.
#[derive(Default)]
struct BandDay {
    adults: u32,
    day_h: f32,
    day_net: f32,
    habit_h: f32,
    habit_net: f32,
    springs: BTreeSet<(i64, i64)>,
    game: Vec<(i64, i64)>,
}

/// La passe de minuit : relève le jour, et lève le camp des bandes dont le
/// pays rend moins que l'habitude.
pub(crate) fn daily(sim: &mut Sim) {
    let tick = sim.time.tick;
    let mut bands: BTreeMap<u64, BandDay> = BTreeMap::new();
    let mut people_per_zone: BTreeMap<(i64, i64), u32> = BTreeMap::new();
    for (_, (membership, demo, yields, mem, pos)) in sim
        .agents
        .query_mut::<(&ClanMembership, &Demographics, &mut Yields, &Memory, &crate::agent::Position)>()
    {
        *people_per_zone.entry(crate::fauna::range_zone((pos.x, pos.y))).or_default() += 1;
        let (dh, dn) = yields.take_day();
        let Some(clan) = membership.0 else { continue };
        if !demo.is_adult(tick) {
            continue;
        }
        let band = bands.entry(clan.0).or_default();
        let (hh, hn) = yields.habit();
        band.adults += 1;
        band.day_h += dh;
        band.day_net += dn;
        band.habit_h += hh;
        band.habit_net += hn;
        band.springs.extend(mem.springs.iter().copied());
        if let Some((spot, seen)) = mem.game
            && tick.saturating_sub(seen) < GAME_FRESH_TICKS
        {
            band.game.push(spot);
        }
    }

    let keep = 0.5f32.powf(1.0 / RECENT_HALF_LIFE_DAYS as f32);
    let mut clans = std::mem::take(&mut sim.clans);
    for clan in &mut clans {
        let Some(band) = bands.get(&clan.id.0) else { continue };
        clan.camp.h = clan.camp.h * keep + band.day_h;
        clan.camp.net = clan.camp.net * keep + band.day_net;
        let evidence = RECENT_EVIDENCE_HOURS * band.adults as f32;
        if clan.camp.h < evidence || band.habit_h < evidence || band.habit_net <= 0.0 {
            continue;
        }
        let now = clan.camp.net / clan.camp.h;
        let habit = band.habit_net / band.habit_h;
        if now >= habit {
            continue;
        }
        if let Some(dest) = choose_destination(sim, clan.home, clan.members.len(), band, &people_per_zone) {
            clan.home = dest;
            clan.camp = Default::default();
        }
    }
    sim.clans = clans;
}

/// Le lieu du prochain camp : une source connue hors du pays battu (au-delà
/// du rayon de résidence), la plus riche en plantes par bouche et en gibier
/// vu, pesée par le trajet à l'échelle d'une journée de marche.
fn choose_destination(
    sim: &mut Sim,
    home: (f64, f64),
    band_size: usize,
    band: &BandDay,
    people_per_zone: &BTreeMap<(i64, i64), u32>,
) -> Option<(f64, f64)> {
    let tick = sim.time.tick;
    let mut best: Option<(f64, (f64, f64))> = None;
    for &spring in &band.springs {
        let p = (spring.0 as f64 + 0.5, spring.1 as f64 + 0.5);
        let d = (p.0 - home.0).hypot(p.1 - home.1);
        if d < RESIDENCE_RADIUS_TILES {
            continue; // le pays qu'on quitte
        }
        let zone = crate::fauna::range_zone(p);
        let mouths = people_per_zone.get(&zone).copied().unwrap_or(0) as f64 + band_size as f64;
        let plants = (sim.world.edible_kcal(p, tick) / (mouths * WEEK_KCAL)).min(1.0);
        let game = band.game.iter().filter(|g| ((g.0 - spring.0) as f64).hypot((g.1 - spring.1) as f64) <= GAME_NEAR_TILES).count();
        let day = 1.0 / (1.0 + d / (WALK_TILES_PER_TICK * 24.0));
        let score = plants * (1.0 + game as f64) * day;
        if score > 0.0 && best.is_none_or(|(b, _)| score > b) {
            best = Some((score, p));
        }
    }
    best.map(|(_, p)| p)
}
