//! Le combat homme ↔ prédateur (BRIEF §3.1 « combat », « blessures ») :
//! l'incrément « éleveur ». Les humains ne subissent plus seulement les
//! meutes — ils vont les **affronter** pour défendre leur territoire et le
//! gibier dont ils vivent (et, à terme, leur cheptel). C'est le premier usage
//! des traits **force** et **agressivité**, en attente depuis la Phase 3.
//!
//! ## Un affrontement de groupe, résolu après coup
//!
//! Comme les prises de chasse (`fauna::Kill`), le combat ne se résout pas au
//! fil de la délibération : chaque assaillant au contact **enregistre** une
//! passe d'armes (`Engagement`) pendant l'exécution, et tout se dénoue ensuite
//! d'un bloc (`resolve`). Ce détour est ce qui permet le choix « groupe
//! émergent » : plusieurs membres d'un clan peuvent frapper la **même** meute
//! au même tick — elle encaisse la **somme** de leurs coups, et sa riposte se
//! **partage** entre eux. Se ruer seul sur un lion des cavernes est mortel ;
//! s'y mettre à cinq dilue le danger — sans qu'aucune règle ne dise « chassez
//! en groupe », c'est l'agressivité de plusieurs voisins qui converge.
//!
//! ## Blesser, être blessé
//!
//! Un assaillant inflige à la meute des dégâts ∝ force × compétence de combat.
//! La meute riposte ∝ sa **dangerosité** (`Species::danger`) × son effectif,
//! partagée entre les assaillants et atténuée par la force et le combat de
//! chacun — le reste s'inscrit comme **plaie** (`agent::Wound`, persistante).
//! À plaie pleine, la blessure est mortelle (`DeathCause::Predation`). Un
//! tirage seedé donne sa part d'incertitude au fracas (le costaud gagne le plus
//! souvent, pas toujours), sans casser le déterminisme.

use std::collections::{BTreeMap, BTreeSet};

use cairn_core::{Pcg32, splitmix64};

use crate::agent::{AgentId, DeathCause, Physiology, Wound};
use crate::demography::Traits;
use crate::fauna::Pack;
use crate::salt;
use crate::sim::Sim;
use crate::skills::{self, Skills};
use crate::social::{ClanId, ClanMembership};

/// Dégâts qu'un assaillant inflige à la meute, par point de force × compétence.
/// Calibré pour qu'un bon guerrier entame nettement une meute, et qu'un groupe
/// la mette en déroute — à affiner hors ligne comme les autres constantes vives.
/// `pub(crate)` : la passe d'armes est *composée* dans `sim::execute` (qui a la
/// force et le combat de l'agent sous la main), puis *résolue* ici.
pub(crate) const ATTACK_POWER: f32 = 3.0;
/// Échelle de la riposte : convertit (dangerosité × effectif) partagé en plaie.
const RETALIATION_SCALE: f32 = 0.03;
/// Plafond d'atténuation de la riposte par la force et le combat : même le
/// meilleur guerrier encaisse toujours un peu.
const MAX_MITIGATION: f32 = 0.85;
/// Ce qu'une passe d'armes enseigne du combat, en heures-équivalent (un affront
/// forge le geste, comme une mise à mort forge la chasse — cf. `skills`).
const COMBAT_PRACTICE_HOURS: f32 = 8.0;

// — Conflit inter-clans (raids) : le duel homme ↔ homme. —
/// Échelle de la plaie d'un coup porté (∝ force × compétence de combat).
/// Calibrée pour qu'un duel blesse sensiblement sans être systématiquement
/// mortel — la mort vient de l'accumulation (raids répétés, mise en surnombre).
const RAID_WOUND_SCALE: f32 = 0.4;
/// La riposte du **défenseur** est amoindrie : il est surpris, il réagit
/// (l'agresseur a l'initiative).
const DEFENDER_FACTOR: f32 = 0.6;
/// Butin razzié au clan rival par passe d'armes réussie (portions de stock) :
/// on razzie la ressource disputée, cause même de la tension.
const PILLAGE_PER_CLASH: f32 = 0.5;

/// Une passe d'armes enregistrée pendant l'exécution, à dénouer dans [`resolve`].
#[derive(Debug, Clone, Copy)]
pub struct Engagement {
    /// Qui frappe (pour lui appliquer plaie et pratique après coup).
    pub attacker: AgentId,
    /// La meute visée (entité du monde faune).
    pub pack: hecs::Entity,
    /// Dégâts portés à la meute (∝ force × compétence de combat).
    pub power: f32,
    /// Atténuation de la riposte encaissée, dans [0, 1) (force + combat).
    pub defense: f32,
}

/// Dénoue tous les affrontements du tick : blesse les meutes de la somme des
/// coups, partage et distribue leur riposte en plaies (mortelles à 1). Pur
/// vis-à-vis de l'ordre : agrégation par `BTreeMap`, tirage dérivé de
/// (seed, tick, id de l'assaillant). No-op s'il n'y a pas eu de combat.
pub(crate) fn resolve(sim: &mut Sim, engagements: &[Engagement]) {
    if engagements.is_empty() {
        return;
    }
    let seed = sim.world.seed();
    let tick = sim.time.tick;

    // 1. Regrouper les passes d'armes par meute (clé = bits de l'entité, ordre
    //    déterministe) : indices dans `engagements`.
    let mut by_pack: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
    for (i, e) in engagements.iter().enumerate() {
        by_pack.entry(e.pack.to_bits().get()).or_default().push(i);
    }

    // 2. Pour chaque meute : encaisser la somme des coups, puis répartir sa
    //    riposte entre les assaillants → plaie à infliger, accumulée par id.
    let mut hurt: BTreeMap<u64, f32> = BTreeMap::new();
    for indices in by_pack.values() {
        let entity = engagements[indices[0]].pack;
        let total_power: f32 = indices.iter().map(|&i| engagements[i].power).sum();
        // La meute encaisse ; on retient son état *avant* pour la riposte.
        let Ok(pack) = sim.fauna.query_one_mut::<&mut Pack>(entity) else {
            // Meute déjà disparue ce tick : les assaillants pratiquent quand
            // même (ils ont chargé), mais nul n'est blessé.
            for &i in indices {
                hurt.entry(engagements[i].attacker.0).or_insert(0.0);
            }
            continue;
        };
        let pop_before = pack.population;
        let danger = pack.species.danger();
        pack.population = (pop_before - total_power).max(0.0);

        // Riposte totale de la meute, partagée entre ses assaillants.
        let threat = danger * pop_before;
        let share = threat / indices.len() as f32;
        for &i in indices {
            let e = &engagements[i];
            let mut rng = Pcg32::new(seed.derive(salt::COMBAT) ^ splitmix64(tick), e.attacker.0);
            let roll = 0.5 + rng.next_f32(); // 0,5–1,5 : la part d'incertitude
            let mitigation = e.defense.min(MAX_MITIGATION);
            let wound = share * RETALIATION_SCALE * (1.0 - mitigation) * roll;
            *hurt.entry(e.attacker.0).or_insert(0.0) += wound;
        }
    }

    // 3. Appliquer aux assaillants : plaie (mortelle à 1), et pratique du
    //    combat (qu'on ait saigné ou non — on s'est battu).
    for (_, (id, traits, skills, wound, phys)) in sim
        .agents
        .query_mut::<(&AgentId, &Traits, &mut Skills, &mut Wound, &mut Physiology)>()
    {
        let Some(&dmg) = hurt.get(&id.0) else { continue };
        skills::practice(&mut skills.combat, skills::combat_cap(traits), COMBAT_PRACTICE_HOURS);
        wound.0 = (wound.0 + dmg).min(1.0);
        if wound.0 >= 1.0 {
            // Plaie mortelle : on passe par le pipeline de mort ordinaire
            // (santé nulle + cause), la passe de démographie fera le reste.
            phys.health = 0.0;
            phys.last_damage = Some(DeathCause::Predation);
        }
    }
}

/// Une passe d'armes **entre deux humains** — un raid inter-clans, à dénouer
/// dans [`resolve_clashes`]. Enregistrée par `sim::execute` quand un agresseur
/// (`TaskKind::Raid`) atteint un rival ; l'issue se dénoue en groupe après coup.
#[derive(Debug, Clone, Copy)]
pub struct Clash {
    /// Celui qui a l'initiative (il porte le premier coup, encaisse la riposte).
    pub attacker: AgentId,
    /// Le rival visé (il est frappé, et riposte en défense).
    pub target: AgentId,
}

/// Dénoue les raids du tick (BRIEF §5, la tension inter-clans qui trouve sa
/// conclusion). Chaque passe d'armes est **mutuelle** : l'agresseur blesse la
/// cible (∝ force × combat), la cible **riposte** en défense (amoindrie). À
/// plusieurs sur une même cible, on la submerge (les plaies s'additionnent →
/// mort par `Violence`). Un raid réussi **razzie** le stock du clan rival. Pur
/// vis-à-vis de l'ordre : agrégation par `BTreeMap`, tirage seedé (salt COMBAT).
pub(crate) fn resolve_clashes(sim: &mut Sim, clashes: &[Clash]) {
    if clashes.is_empty() {
        return;
    }
    let seed = sim.world.seed();
    let tick = sim.time.tick;

    // 1. Instantané des combattants (force, compétence de combat, clan).
    let involved: BTreeSet<u64> =
        clashes.iter().flat_map(|c| [c.attacker.0, c.target.0]).collect();
    let mut stats: BTreeMap<u64, (f32, f32, Option<ClanId>)> = BTreeMap::new();
    for (_, (id, traits, skills, membership)) in sim
        .agents
        .query::<(&AgentId, &Traits, &Skills, &ClanMembership)>()
        .iter()
    {
        if involved.contains(&id.0) {
            stats.insert(id.0, (traits.strength, skills.combat, membership.0));
        }
    }

    // 2. Coups mutuels + butin par paire de clans.
    let mut hurt: BTreeMap<u64, f32> = BTreeMap::new();
    let mut fought: BTreeSet<u64> = BTreeSet::new();
    let mut pillage: Vec<(ClanId, ClanId)> = Vec::new();
    for c in clashes {
        let (Some(&(af, ac, aclan)), Some(&(tf, tc, tclan))) =
            (stats.get(&c.attacker.0), stats.get(&c.target.0))
        else {
            continue; // l'un des deux a déjà quitté la scène ce tick
        };
        let mut rng = Pcg32::new(seed.derive(salt::COMBAT) ^ splitmix64(tick), c.attacker.0);
        // Le coup de l'agresseur sur la cible.
        let blow = af * (0.5 + ac);
        let mit_t = (0.3 * tf + 0.4 * tc).min(MAX_MITIGATION);
        *hurt.entry(c.target.0).or_default() +=
            blow * RAID_WOUND_SCALE * (1.0 - mit_t) * (0.5 + rng.next_f32());
        // La riposte du défenseur (amoindrie).
        let riposte = tf * (0.5 + tc) * DEFENDER_FACTOR;
        let mit_a = (0.3 * af + 0.4 * ac).min(MAX_MITIGATION);
        *hurt.entry(c.attacker.0).or_default() +=
            riposte * RAID_WOUND_SCALE * (1.0 - mit_a) * (0.5 + rng.next_f32());
        fought.insert(c.attacker.0);
        fought.insert(c.target.0);
        if let (Some(a), Some(t)) = (aclan, tclan)
            && a != t
        {
            pillage.push((a, t));
        }
    }

    // 3. Appliquer plaies (mortelles à 1 → Violence) et pratique du combat.
    for (_, (id, traits, skills, wound, phys)) in sim
        .agents
        .query_mut::<(&AgentId, &Traits, &mut Skills, &mut Wound, &mut Physiology)>()
    {
        if !fought.contains(&id.0) {
            continue;
        }
        skills::practice(&mut skills.combat, skills::combat_cap(traits), COMBAT_PRACTICE_HOURS);
        if let Some(&dmg) = hurt.get(&id.0) {
            wound.0 = (wound.0 + dmg).min(1.0);
            if wound.0 >= 1.0 {
                phys.health = 0.0;
                phys.last_damage = Some(DeathCause::Violence);
            }
        }
    }

    // 4. Butin : chaque razzia transfère du stock du clan rival vers
    //    l'assaillant, borné par ce que le rival possède (on ne razzie pas plus
    //    qu'il n'y a). C'est la tension — née d'une ressource rare — qui se
    //    dénoue en la prenant de force.
    for (a, t) in pillage {
        let take = sim
            .clans
            .iter()
            .find(|c| c.id == t)
            .map_or(0.0, |c| c.stock.min(PILLAGE_PER_CLASH));
        if take <= 0.0 {
            continue;
        }
        if let Some(tc) = sim.clans.iter_mut().find(|c| c.id == t) {
            tc.stock -= take;
        }
        if let Some(ac) = sim.clans.iter_mut().find(|c| c.id == a) {
            ac.stock += take;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Position;
    use crate::fauna::{FaunaId, Species};
    use crate::social::Clan;
    use cairn_core::WorldSeed;

    /// Fait naître une meute de loups d'effectif connu et renvoie son entité.
    fn spawn_wolf_pack(sim: &mut Sim, pop: f32) -> hecs::Entity {
        sim.fauna
            .spawn((FaunaId(999), Position { x: 0.0, y: 0.0 }, Pack::new(pop, Species::Wolf)))
    }

    /// Le premier affrontement de l'histoire du projet : la meute encaisse le
    /// coup, l'assaillant en ressort blessé et un peu meilleur au combat.
    #[test]
    fn affronter_une_meute_la_blesse_et_blesse_l_assaillant() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let pack = spawn_wolf_pack(&mut sim, 30.0);
        let attacker = sim.spawn_agent(0.0, 0.0);
        resolve(&mut sim, &[Engagement { attacker, pack, power: 5.0, defense: 0.2 }]);

        let pop = sim.fauna.query_one_mut::<&Pack>(pack).unwrap().population;
        assert!((pop - 25.0).abs() < 1e-3, "la meute perd la somme des coups (30 − 5 = {pop})");

        let (mut wound, mut combat) = (0.0, 0.0);
        for (_, (id, w, s)) in sim.agents.query::<(&AgentId, &Wound, &Skills)>().iter() {
            if *id == attacker {
                wound = w.0;
                combat = s.combat;
            }
        }
        assert!(wound > 0.0, "affronter une meute laisse une plaie");
        assert!(combat > 0.0, "on apprend à se battre en se battant");
    }

    /// « Groupe émergent » : la riposte se partage — le premier assaillant
    /// encaisse deux fois moins à deux que seul (même tirage, part divisée).
    #[test]
    fn le_groupe_dilue_le_risque() {
        let first_wound = |attackers: usize| -> f32 {
            let mut sim = Sim::new(WorldSeed(1), 64);
            let pack = spawn_wolf_pack(&mut sim, 30.0);
            let ids: Vec<AgentId> = (0..attackers).map(|_| sim.spawn_agent(0.0, 0.0)).collect();
            let engs: Vec<Engagement> = ids
                .iter()
                .map(|&a| Engagement { attacker: a, pack, power: 2.0, defense: 0.2 })
                .collect();
            resolve(&mut sim, &engs);
            let mut w = 0.0;
            for (_, (id, wound)) in sim.agents.query::<(&AgentId, &Wound)>().iter() {
                if *id == ids[0] {
                    w = wound.0;
                }
            }
            w
        };
        let (solo, duo) = (first_wound(1), first_wound(2));
        assert!(solo > 0.0 && duo > 0.0);
        assert!(
            (duo - solo / 2.0).abs() < 1e-4,
            "à deux, le premier encaisse la moitié (seul {solo}, à deux {duo})"
        );
    }

    /// La dangerosité pèse : un lion des cavernes (0,7) blesse bien plus qu'un
    /// loup (0,3), à assaut égal.
    #[test]
    fn un_lion_est_plus_dangereux_qu_un_loup() {
        let wound_from = |species: Species| -> f32 {
            let mut sim = Sim::new(WorldSeed(1), 64);
            let pack = sim.fauna.spawn((
                FaunaId(1),
                Position { x: 0.0, y: 0.0 },
                Pack::new(20.0, species),
            ));
            let attacker = sim.spawn_agent(0.0, 0.0);
            resolve(&mut sim, &[Engagement { attacker, pack, power: 2.0, defense: 0.2 }]);
            let mut w = 0.0;
            for (_, (id, wound)) in sim.agents.query::<(&AgentId, &Wound)>().iter() {
                if *id == attacker {
                    w = wound.0;
                }
            }
            w
        };
        assert!(
            wound_from(Species::CaveLion) > wound_from(Species::Wolf),
            "un lion des cavernes doit blesser plus qu'un loup"
        );
    }

    /// Le conflit inter-clans : un raid blesse **les deux** combattants (la
    /// cible riposte) et **razzie** le stock du clan rival vers l'assaillant.
    #[test]
    fn un_raid_blesse_les_deux_et_razzie_le_rival() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let raider = sim.spawn_agent(0.0, 0.0);
        let victim = sim.spawn_agent(0.0, 0.0);
        for (_, (id, membership, traits)) in
            sim.agents.query_mut::<(&AgentId, &mut ClanMembership, &mut Traits)>()
        {
            traits.strength = 0.7; // de quoi frapper et riposter
            membership.0 = Some(ClanId(if *id == raider { 1 } else { 2 }));
        }
        let clan = |id, chief, stock| Clan {
            id: ClanId(id),
            founded_tick: 0,
            members: [chief].into_iter().collect(),
            home: (0.0, 0.0),
            stock,
            chief,
            desired: None,
        };
        sim.clans.push(clan(1, raider, 0.0));
        sim.clans.push(clan(2, victim, 5.0));

        resolve_clashes(&mut sim, &[Clash { attacker: raider, target: victim }]);

        let wound_of = |sim: &Sim, who: AgentId| {
            let mut w = 0.0;
            for (_, (id, wound)) in sim.agents.query::<(&AgentId, &Wound)>().iter() {
                if *id == who {
                    w = wound.0;
                }
            }
            w
        };
        assert!(wound_of(&sim, raider) > 0.0, "l'agresseur encaisse la riposte");
        assert!(wound_of(&sim, victim) > 0.0, "la cible est blessée");
        let stock = |id| sim.clans.iter().find(|c| c.id == ClanId(id)).unwrap().stock;
        assert!(stock(2) < 5.0, "le clan rival est razzié");
        assert!((stock(1) + stock(2) - 5.0).abs() < 1e-4, "le butin est transféré, pas créé");
    }
}
