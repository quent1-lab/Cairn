//! La domestication (BRIEF §5.3, « élevage ») : la suite naturelle de
//! l'éleveur. Après avoir écarté les prédateurs (`crate::combat`), un clan qui
//! garde un troupeau **domesticable** (aurochs, renne) près de son foyer
//! l'**apprivoise** peu à peu — c'est le cheptel.
//!
//! ## Émergence comportementale (choix acté)
//!
//! Pas de technologie à découvrir : les bêtes s'habituent aux humains le temps
//! qu'on les protège et qu'on les garde à proximité. L'apprivoisement
//! (`Herd::tameness`) **monte** pour une espèce domesticable près d'un foyer
//! **et** peu harcelée par les prédateurs (c'est l'éleveur qui les a chassés),
//! et **redescend** sinon — un troupeau délaissé ou harcelé **redevient
//! sauvage** (réversion férale, exactement comme un champ retourne en friche).
//! Tant qu'il est gardé, il est **ancré** au foyer (`Herd::anchor`) : il ne
//! migre plus, ne dérive plus (voir `fauna::update_herds`). La récolte, elle,
//! passe par un vrai éleveur (`TaskKind::Herd`, voir `brain`/`sim::execute`).
//!
//! Cette passe ne fait que **mesurer** (proximité, pression prédatrice) et en
//! dériver l'apprivoisement — jamais un « ce clan possède ce troupeau » scripté.

use cairn_core::km_to_tiles;

use crate::agent::Position;
use crate::chronicle::EventKind;
use crate::fauna::{Herd, Pack};
use crate::sim::Sim;
use crate::social::ClanId;

/// Rayon autour d'un foyer dans lequel un troupeau peut s'apprivoiser (~2 km) :
/// le territoire du clan.
const TAME_RADIUS: f64 = km_to_tiles(2.0);
/// Rayon où l'on pèse la menace des prédateurs sur le troupeau (~3 km).
const PROTECT_RADIUS: f64 = km_to_tiles(3.0);
/// Effectif de prédateurs au-delà duquel le troupeau est trop harcelé pour
/// s'apprivoiser : l'éleveur doit d'abord les avoir écartés.
const PROTECT_MAX_PREDATORS: f32 = 8.0;
/// Gain d'apprivoisement par jour gardé (~50 jours pour un cheptel franc).
const TAME_RATE: f32 = 0.02;
/// Perte d'apprivoisement par jour délaissé ou harcelé (réversion férale).
const FERAL_RATE: f32 = 0.02;

/// Apprivoisement à partir duquel le troupeau est un **cheptel** qu'un éleveur
/// garde et récolte (`TaskKind::Herd`). `pub` : lu par `brain` et `execute`.
pub const DOMESTICATED_THRESHOLD: f32 = 0.5;

/// Passe quotidienne : apprivoise (ou refarouche) chaque troupeau domesticable
/// selon la proximité d'un foyer et la protection contre les prédateurs. Pure
/// mesure ; aucun RNG (déterministe par construction).
pub(crate) fn daily(sim: &mut Sim) {
    // Instantanés (lecture) avant de muter les troupeaux : foyers des clans et
    // menace prédatrice.
    // Le clan est retenu à côté de son foyer : la Chronique veut savoir *qui*
    // a domestiqué la bête, pas seulement où.
    let homes: Vec<(ClanId, (f64, f64))> = sim.clans.iter().map(|c| (c.id, c.home)).collect();
    let packs: Vec<((f64, f64), f32)> = sim
        .fauna
        .query::<(&Pack, &Position)>()
        .iter()
        .map(|(_, (p, pos))| ((pos.x, pos.y), p.population))
        .collect();

    // Les franchissements du seuil de cheptel, relevés puis journalisés hors de
    // l'emprunt de `sim.fauna`.
    let mut tamed: Vec<((i64, i64), ClanId, crate::fauna::Species)> = Vec::new();

    for (_, (herd, pos)) in sim.fauna.query_mut::<(&mut Herd, &Position)>() {
        if !herd.species.domesticable() {
            continue; // le gibier sauvage ne s'apprivoise pas
        }
        let here = (pos.x, pos.y);
        // Le foyer le plus proche à portée d'apprivoisement, s'il en est un.
        let home = homes
            .iter()
            .copied()
            .filter(|(_, h)| dist2(*h, here) <= TAME_RADIUS * TAME_RADIUS)
            .min_by(|(_, a), (_, b)| dist2(*a, here).total_cmp(&dist2(*b, here)));
        // Pression prédatrice autour du troupeau.
        let predators: f32 = packs
            .iter()
            .filter(|(pp, _)| dist2(*pp, here) <= PROTECT_RADIUS * PROTECT_RADIUS)
            .map(|(_, pop)| pop)
            .sum();

        let before = herd.tameness;
        if let Some((clan, h)) = home
            && predators < PROTECT_MAX_PREDATORS
        {
            herd.tameness = (herd.tameness + TAME_RATE).min(1.0);
            herd.anchor = Some(h);
            // Le jour où l'apprivoisement franchit le seuil : ce troupeau n'est
            // plus du gibier, c'est un cheptel. Un fait qui fait date, et qu'on
            // ne peut lire que du dedans (le seuil ne se franchit qu'une fois).
            if before < DOMESTICATED_THRESHOLD && herd.tameness >= DOMESTICATED_THRESHOLD {
                tamed.push(((here.0.floor() as i64, here.1.floor() as i64), clan, herd.species));
            }
        } else {
            herd.tameness = (herd.tameness - FERAL_RATE).max(0.0);
            if herd.tameness <= 0.0 {
                herd.anchor = None; // redevenu sauvage : il repart avec les saisons
            }
        }
    }

    for (pos, clan, species) in tamed {
        sim.record(pos, EventKind::Domesticated { clan, species });
    }
}

fn dist2(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentId;
    use crate::fauna::{FaunaId, Species};
    use crate::social::{Clan, ClanId};
    use cairn_core::WorldSeed;

    fn spawn_herd(sim: &mut Sim, species: Species, x: f64, y: f64) -> hecs::Entity {
        sim.fauna.spawn((FaunaId(1), Position { x, y }, Herd::new(20.0, species)))
    }

    /// L'aurochs gardé près d'un foyer et protégé s'apprivoise et s'ancre ; loin
    /// de tout clan il reste sauvage, et un cerf (non domesticable) jamais.
    #[test]
    fn un_aurochs_garde_pres_du_foyer_s_apprivoise() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        sim.clans.push(Clan {
            id: ClanId(1),
            founded_tick: 0,
            members: [].into_iter().collect(),
            home: (0.0, 0.0),
            stock: 0.0,
            chief: AgentId(0),
            desired: None,
            rivalry: 0.0,
        });
        let near = spawn_herd(&mut sim, Species::Aurochs, 100.0, 0.0); // ~200 m du foyer
        let far = spawn_herd(&mut sim, Species::Aurochs, 5000.0, 0.0); // 10 km
        let wild = spawn_herd(&mut sim, Species::Deer, 120.0, 0.0); // domesticable ? non

        for _ in 0..40 {
            daily(&mut sim);
        }

        let tam = |sim: &mut Sim, e| sim.fauna.query_one_mut::<&Herd>(e).unwrap().tameness;
        assert!(tam(&mut sim, near) > DOMESTICATED_THRESHOLD, "l'aurochs gardé s'apprivoise");
        assert!(sim.fauna.query_one_mut::<&Herd>(near).unwrap().anchor.is_some(), "le cheptel est ancré");
        assert_eq!(tam(&mut sim, far), 0.0, "l'aurochs loin de tout reste sauvage");
        assert_eq!(tam(&mut sim, wild), 0.0, "un cerf ne s'apprivoise pas");
    }

    /// Les prédateurs empêchent l'apprivoisement : un aurochs près du foyer mais
    /// harcelé par une grosse meute ne se domestique pas (l'éleveur doit d'abord
    /// l'avoir dégagée — c'est le lien avec `crate::combat`).
    #[test]
    fn une_meute_proche_empeche_l_apprivoisement() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        sim.clans.push(Clan {
            id: ClanId(1),
            founded_tick: 0,
            members: [].into_iter().collect(),
            home: (0.0, 0.0),
            stock: 0.0,
            chief: AgentId(0),
            desired: None,
            rivalry: 0.0,
        });
        let cheptel = spawn_herd(&mut sim, Species::Aurochs, 100.0, 0.0);
        // Une meute nombreuse tout près : le troupeau est harcelé.
        sim.fauna.spawn((FaunaId(2), Position { x: 150.0, y: 0.0 }, Pack::new(30.0, Species::Wolf)));

        for _ in 0..40 {
            daily(&mut sim);
        }
        assert_eq!(
            sim.fauna.query_one_mut::<&Herd>(cheptel).unwrap().tameness,
            0.0,
            "harcelé par les prédateurs, il ne s'apprivoise pas"
        );
    }
}
