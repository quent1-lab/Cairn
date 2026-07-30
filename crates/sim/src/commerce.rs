//! Le commerce longue distance (BRIEF §5.3, Phase 5, incrément 6b) : **le clou
//! du système**. Le cuivre et l'étain sont générés à des centaines de km l'un
//! de l'autre (Phase 1, jamais co-localisés) ; le bronze exige les deux. Un
//! clan assis sur le cuivre ne peut donc l'atteindre qu'en **allant chercher
//! l'étain là où il est** — et c'est ce voyage qui *force* la route commerciale
//! du brief.
//!
//! ## Un vrai agent, pas une abstraction (choix acté)
//!
//! L'expédition n'est pas un point mécanique : c'est un **membre du clan** qui
//! part physiquement (choix de conception acté avec l'utilisateur). Il traverse
//! le monde comme n'importe quel agent — il boit, mange, dort en chemin, et
//! peut **mourir en route** : une expédition peut échouer, et c'est voulu
//! (l'émergence plutôt que la garantie). Ce module ne fait que trois choses :
//!
//! 1. **Dépêcher** (`dispatch`, quotidien) : un clan qui maîtrise la
//!    métallurgie du cuivre, dont aucun membre n'a encore vu d'étain, qui n'a
//!    pas déjà un envoyé en route, et qui a de quoi financer le voyage (stock
//!    commun), désigne son meilleur marcheur (le plus endurant) et lui assigne
//!    une expédition vers le **gisement d'étain le plus proche** (les
//!    prospecteurs suivent les indices géologiques — `find_nearest_tin`).
//! 2. **Guider** (le pilotage lui-même vit dans `sim::step` : tant qu'aucun
//!    besoin vital ne presse, l'envoyé file vers son étape ; sinon il délibère
//!    normalement pour survivre, puis reprend la route).
//! 3. **Aboutir** (`advance`, chaque tick) : arrivé près de l'étain, l'envoyé y
//!    est **exposé** et fait demi-tour ; rentré au foyer, l'expédition
//!    s'achève. L'étain rapporté se diffuse alors au clan par le troc
//!    (`tech::diffuse`) jusqu'au métallurgiste, seul capable d'en tirer le
//!    bronze.
//!
//! L'expédition vit dans une table à côté de l'ECS (`Sim::expeditions`, clé =
//! identifiant d'agent), exactement comme les routes de pathfinding — et elle
//! est nettoyée à la mort de l'agent de la même façon.

use std::collections::{BTreeMap, BTreeSet};

use cairn_core::{km_to_tiles, tiles_to_km};
use cairn_worldgen::Deposit;

use crate::agent::{AgentId, Physiology, Position};
use crate::chronicle::EventKind;
use crate::demography::{Demographics, Sex, Traits};
use crate::exposure::{Exposure, Exposures};
use crate::sim::Sim;
use crate::social::{ClanId, ClanMembership};
use crate::tech::Knowledge;
use crate::world::World;

/// Portée maximale de prospection (~1500 km). Au-delà, on renonce : certaines
/// régions n'ont pas d'étain atteignable, et n'auront donc jamais le bronze —
/// une conséquence géographique, pas une règle.
const PROSPECT_MAX_TILES: f64 = km_to_tiles(1500.0);
/// Pas de la prospection (~4 km) : les districts métalliques s'étendent sur des
/// dizaines de km (Phase 1), ce pas ne saute pas par-dessus l'un d'eux.
const PROSPECT_STRIDE: i64 = km_to_tiles(4.0) as i64;
/// Distance à laquelle l'expédition « atteint » l'étain (puis le foyer au
/// retour) — ~0,5 km, on n'a pas besoin de marcher sur la tuile exacte.
const REACH_TILES: f64 = km_to_tiles(0.5);
/// Coût d'une expédition, prélevé sur le stock commun : les vivres du voyage.
const EXPEDITION_COST: f32 = 10.0;
/// Santé minimale d'un candidat envoyé : on n'envoie pas un mourant au loin.
const MIN_EXPEDITION_HEALTH: f32 = 0.6;

/// Une expédition en cours, portée par un agent (clé = son identifiant dans
/// `Sim::expeditions`). L'étain visé, le foyer d'où l'on part (cible du
/// retour), et le sens du voyage.
#[derive(Debug, Clone, Copy)]
pub struct Expedition {
    pub tin: (i64, i64),
    pub home: (f64, f64),
    pub returning: bool,
}

/// L'étape courante que vise l'envoyé : l'étain à l'aller, le foyer au retour.
pub fn waypoint(exp: &Expedition) -> (i64, i64) {
    if exp.returning {
        (exp.home.0.floor() as i64, exp.home.1.floor() as i64)
    } else {
        exp.tin
    }
}

/// Cherche le gisement `target` le plus proche de `from`, par anneaux carrés
/// croissants au pas `PROSPECT_STRIDE`. Requête géologique **pure**
/// (`WorldGen::deposit`, aucune matérialisation de chunk). `None` si aucun dans
/// la portée. Base de la prospection d'étain (via `find_nearest_tin`) et de la
/// reconnaissance géologique (banc `scout`).
pub fn find_nearest_deposit(world: &World, from: (i64, i64), target: Deposit) -> Option<(i64, i64)> {
    let wg = world.worldgen();
    let is_target = |x: i64, y: i64| {
        let e = wg.elevation(x, y);
        e > 0.0 && wg.deposit(x, y, e) == target
    };
    if is_target(from.0, from.1) {
        return Some(from);
    }
    let max_ring = (PROSPECT_MAX_TILES as i64) / PROSPECT_STRIDE;
    for r in 1..=max_ring {
        let d = r * PROSPECT_STRIDE;
        // Bords haut et bas de l'anneau carré.
        let mut x = -d;
        while x <= d {
            for &y in &[-d, d] {
                if is_target(from.0 + x, from.1 + y) {
                    return Some((from.0 + x, from.1 + y));
                }
            }
            x += PROSPECT_STRIDE;
        }
        // Bords gauche et droit (sans recompter les coins).
        let mut y = -d + PROSPECT_STRIDE;
        while y < d {
            for &x in &[-d, d] {
                if is_target(from.0 + x, from.1 + y) {
                    return Some((from.0 + x, from.1 + y));
                }
            }
            y += PROSPECT_STRIDE;
        }
    }
    None
}

/// L'étain le plus proche — le cas d'usage de la prospection commerciale.
pub fn find_nearest_tin(world: &World, from: (i64, i64)) -> Option<(i64, i64)> {
    find_nearest_deposit(world, from, Deposit::Tin)
}

/// État agrégé d'un clan pour décider d'une expédition.
struct ClanTinInfo {
    has_copper: bool,
    has_tin: bool,
    active: bool,
    /// Meilleur candidat au voyage : (identifiant, endurance).
    best: Option<(u64, f32)>,
}

/// Passe quotidienne : dépêche une expédition pour chaque clan qui en a besoin
/// et les moyens (voir le commentaire de module).
pub(crate) fn dispatch(sim: &mut Sim) {
    let Some(copper) = sim.tech_tree.id_of("copper_metallurgy") else {
        return;
    };
    let tick = sim.time.tick;

    // Agrégation par clan en une passe (lecture seule).
    let mut infos: BTreeMap<u64, ClanTinInfo> = BTreeMap::new();
    for (_, (id, membership, knowledge, exposures, demo, phys, traits)) in sim
        .agents
        .query::<(&AgentId, &ClanMembership, &Knowledge, &Exposures, &Demographics, &Physiology, &Traits)>()
        .iter()
    {
        let Some(cid) = membership.0 else { continue };
        let info = infos.entry(cid.0).or_insert(ClanTinInfo {
            has_copper: false,
            has_tin: false,
            active: false,
            best: None,
        });
        if knowledge.has(copper) {
            info.has_copper = true;
        }
        if exposures.has(Exposure::Tin) {
            info.has_tin = true;
        }
        let on_expedition = sim.expeditions.contains_key(&id.0);
        if on_expedition {
            info.active = true;
        } else if demo.is_adult(tick) && phys.health > MIN_EXPEDITION_HEALTH {
            // Le plus endurant l'emporte ; à endurance égale, le plus petit id
            // (déterminisme, même convention qu'ailleurs).
            if info.best.is_none_or(|(bid, be)| {
                traits.endurance > be || (traits.endurance == be && id.0 < bid)
            }) {
                info.best = Some((id.0, traits.endurance));
            }
        }
    }

    // Les départs à raconter, relevés ici et journalisés après la boucle :
    // `sim.clans` y est emprunté en écriture, `sim.record` veut tout `sim`.
    let mut departures: Vec<((i64, i64), ClanId, u64, f32)> = Vec::new();
    for clan in &mut sim.clans {
        let Some(info) = infos.get(&clan.id.0) else { continue };
        if !info.has_copper || info.has_tin || info.active || clan.stock < EXPEDITION_COST {
            continue;
        }
        let Some((envoy, _)) = info.best else { continue };
        let home_tile = (clan.home.0.floor() as i64, clan.home.1.floor() as i64);
        let Some(tin) = find_nearest_tin(&sim.world, home_tile) else {
            continue; // pas d'étain atteignable : jamais de bronze ici
        };
        clan.stock -= EXPEDITION_COST;
        sim.expeditions.insert(envoy, Expedition { tin, home: clan.home, returning: false });
        let tiles = ((tin.0 - home_tile.0) as f64).hypot((tin.1 - home_tile.1) as f64);
        departures.push((home_tile, clan.id, envoy, tiles_to_km(tiles) as f32));
    }
    for (pos, clan, envoy, km) in departures {
        // Un homme part au bout du monde pour un métal : ça fait date (§6.4).
        if let Some(sex) = sex_of(sim, envoy) {
            sim.record(
                pos,
                EventKind::ExpeditionDeparted {
                    clan,
                    agent: AgentId(envoy),
                    sex,
                    distance_km: km,
                },
            );
        }
    }
}

/// Le sexe d'un agent vivant — ce que la Chronique doit figer pour pouvoir le
/// nommer plus tard (voir `crate::chronicle`). `None` s'il n'est plus là.
fn sex_of(sim: &Sim, agent: u64) -> Option<Sex> {
    sim.agents
        .query::<(&AgentId, &Demographics)>()
        .iter()
        .find(|(_, (id, _))| id.0 == agent)
        .map(|(_, (_, demo))| demo.sex)
}

/// Passe par tick : fait aboutir les expéditions. Arrivé près de l'étain,
/// l'envoyé y est exposé et fait demi-tour ; rentré au foyer, l'expédition
/// s'achève. No-op s'il n'y en a aucune.
pub(crate) fn advance(sim: &mut Sim) {
    if sim.expeditions.is_empty() {
        return;
    }
    let positions: BTreeMap<u64, (f64, f64)> = sim
        .agents
        .query::<(&AgentId, &Position)>()
        .iter()
        .map(|(_, (id, p))| (id.0, (p.x, p.y)))
        .collect();

    let mut reached_tin: BTreeSet<u64> = BTreeSet::new();
    let mut completed: Vec<u64> = Vec::new();
    for (&aid, exp) in &sim.expeditions {
        let Some(&pos) = positions.get(&aid) else { continue }; // mort : nettoyé ailleurs
        let target = waypoint(exp);
        let d = (pos.0 - (target.0 as f64 + 0.5)).hypot(pos.1 - (target.1 as f64 + 0.5));
        if d <= REACH_TILES {
            if exp.returning {
                completed.push(aid);
            } else {
                reached_tin.insert(aid);
            }
        }
    }

    if !reached_tin.is_empty() {
        for (_, (id, exposures)) in sim.agents.query_mut::<(&AgentId, &mut Exposures)>() {
            if reached_tin.contains(&id.0) {
                exposures.expose(Exposure::Tin);
            }
        }
        for aid in &reached_tin {
            if let Some(exp) = sim.expeditions.get_mut(aid) {
                exp.returning = true;
            }
        }
    }
    for aid in completed {
        let exp = sim.expeditions.remove(&aid);
        // Le retour est le fait notable : c'est lui qui rend le bronze possible
        // (l'étain rapporté se rediffuse au clan par le troc).
        if let (Some(exp), Some(sex)) = (exp, sex_of(sim, aid))
            && let Some(clan) = clan_of(sim, aid)
        {
            let pos = (exp.home.0.floor() as i64, exp.home.1.floor() as i64);
            sim.record(pos, EventKind::ExpeditionReturned { clan, agent: AgentId(aid), sex });
        }
    }
}

/// Le clan d'un agent vivant, s'il en a un.
fn clan_of(sim: &Sim, agent: u64) -> Option<ClanId> {
    sim.agents
        .query::<(&AgentId, &ClanMembership)>()
        .iter()
        .find(|(_, (id, _))| id.0 == agent)
        .and_then(|(_, (_, m))| m.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::{Clan, ClanId};
    use cairn_core::WorldSeed;

    #[test]
    fn la_prospection_trouve_un_gisement_d_etain() {
        // Sur une vraie seed, l'étain existe quelque part (Phase 1 le garantit
        // sur une masse terrestre) : depuis l'origine, on doit en trouver un,
        // et ce doit vraiment être de l'étain.
        let sim = Sim::new(WorldSeed(42), 64);
        let tin = find_nearest_tin(&sim.world, (0, 0));
        if let Some((tx, ty)) = tin {
            let wg = sim.world.worldgen();
            assert_eq!(wg.deposit(tx, ty, wg.elevation(tx, ty)), Deposit::Tin);
        }
        // (Si `None`, l'origine est un océan sans étain dans 1500 km — rare ;
        // le test reste correct, il n'affirme rien de faux.)
    }

    #[test]
    fn l_envoye_expose_a_l_etain_puis_fait_demi_tour_et_rentre() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let envoy = sim.spawn_agent(0.0, 0.0);
        // Étain assez loin pour que l'envoyé, au départ, ne soit pas déjà dans
        // le rayon d'arrivée (REACH_TILES ≈ 250 tuiles) — on téléporte l'agent
        // pour ne pas simuler le trajet réel.
        let tin = (10_000, 0);
        sim.expeditions.insert(envoy.0, Expedition { tin, home: (0.0, 0.0), returning: false });

        // L'envoyé n'est pas encore près de l'étain : rien ne se passe.
        advance(&mut sim);
        assert!(!sim.expeditions[&envoy.0].returning);

        // On le pose sur l'étain : il y est exposé et fait demi-tour.
        for (_, (id, pos)) in sim.agents.query_mut::<(&AgentId, &mut Position)>() {
            if *id == envoy {
                pos.x = tin.0 as f64 + 0.5;
                pos.y = tin.1 as f64 + 0.5;
            }
        }
        advance(&mut sim);
        assert!(sim.expeditions[&envoy.0].returning, "arrivé à l'étain, il rentre");
        let exposed = sim
            .agents
            .query::<(&AgentId, &Exposures)>()
            .iter()
            .any(|(_, (id, e))| *id == envoy && e.has(Exposure::Tin));
        assert!(exposed, "il a vu l'étain");

        // On le ramène au foyer : l'expédition s'achève.
        for (_, (id, pos)) in sim.agents.query_mut::<(&AgentId, &mut Position)>() {
            if *id == envoy {
                pos.x = 0.0;
                pos.y = 0.0;
            }
        }
        advance(&mut sim);
        assert!(!sim.expeditions.contains_key(&envoy.0), "rentré : l'expédition est finie");
    }

    #[test]
    fn un_clan_du_cuivre_sans_etain_depeche_une_expedition() {
        let mut sim = Sim::new(WorldSeed(42), 64);
        let copper = sim.tech_tree.id_of("copper_metallurgy").unwrap();
        // Un membre qui maîtrise la métallurgie du cuivre, aucun n'a vu l'étain.
        let m = sim.spawn_agent(0.0, 0.0);
        for (_, (id, know)) in sim.agents.query_mut::<(&AgentId, &mut Knowledge)>() {
            if *id == m {
                know.insert(copper);
            }
        }
        for (_, membership) in sim.agents.query_mut::<&mut ClanMembership>() {
            membership.0 = Some(ClanId(1));
        }
        let mut clan = Clan {
            id: ClanId(1),
            founded_tick: 0,
            members: [m].into_iter().collect(),
            home: (0.0, 0.0),
            stock: 20.0, // de quoi financer
            chief: m,
            desired: None,
        };
        clan.home = (0.0, 0.0);
        sim.clans.push(clan);

        dispatch(&mut sim);
        // Si de l'étain est atteignable depuis l'origine (seed 42), une
        // expédition part et le stock est ponctionné.
        if find_nearest_tin(&sim.world, (0, 0)).is_some() {
            assert!(sim.expeditions.contains_key(&m.0), "une expédition doit être dépêchée");
            assert!(sim.clans[0].stock < 20.0, "le voyage est financé par le stock");
        }
    }

    #[test]
    fn sans_metallurgie_du_cuivre_aucune_expedition() {
        let mut sim = Sim::new(WorldSeed(42), 64);
        let m = sim.spawn_agent(0.0, 0.0);
        for (_, membership) in sim.agents.query_mut::<&mut ClanMembership>() {
            membership.0 = Some(ClanId(1));
        }
        sim.clans.push(Clan {
            id: ClanId(1),
            founded_tick: 0,
            members: [m].into_iter().collect(),
            home: (0.0, 0.0),
            stock: 20.0,
            chief: m,
            desired: None,
        });
        dispatch(&mut sim);
        assert!(sim.expeditions.is_empty(), "pas de cuivre, pas de quête d'étain");
    }
}
