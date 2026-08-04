//! La pression : « famine, froid, guerre, surpopulation — mesurés, agrégés au
//! clan » (BRIEF §5, Phase 5 « L'ÉTINCELLE », incrément 2).
//!
//! L'autre facteur de l'insight, à côté de l'exposition (`crate::exposure`).
//! Le BRIEF §5.2 en fait le moteur de l'innovation : `P(insight) ∝ curiosité ×
//! compétence × exposition × pression`, avec le principe cardinal « **un clan
//! repu n'invente presque rien** ». C'est la nécessité qui pousse à chercher :
//! un clan qui gèle finit par découvrir le feu, un clan affamé la
//! conservation ou l'agriculture.
//!
//! ## Deux faces, deux échelles
//!
//! Le §5.2 décrit une tension : « trop de misère → pas de temps pour penser ;
//! trop de confort → pas de raison de penser. Les civilisations décollent dans
//! l'entre-deux. » On sépare donc deux choses, à deux échelles :
//!
//! - **La pression** (ici) — la *raison* de penser, une propriété du **clan** :
//!   ses membres ont-ils froid, faim, un voisin menaçant, trop de bouches ?
//!   Agrégée par clan, reconstruite chaque jour comme `clan_relations`.
//! - **L'oisiveté** — le *temps* de penser, une propriété de l'**agent** :
//!   est-il libre et repu à l'instant ? Ça n'a pas à être stocké ni agrégé —
//!   c'est la même garde « au confort » que `brain::decide` applique déjà à
//!   `Explore`/`Build`, évaluée au moment de l'insight (incrément 3).
//!
//! Ce module ne fait donc que **mesurer** — il ne déclenche rien encore, comme
//! l'exposition. Le moteur d'insight (incrément 3) combinera les deux.
//!
//! ## Persistance : la mémoire de la dureté récente
//!
//! La pression n'est pas seulement l'état **instantané** du clan : elle en
//! garde une **mémoire décroissante** (`max` entre la pression du jour et celle
//! de la veille atténuée de `PRESSURE_DECAY`). Sans cela, une tension
//! fondamentale bloquait la chaîne technologique en climat rude, mesurée au
//! banc `etincelle` : **le froid et le confort individuel sont anti-corrélés**.
//! Un membre qui a froid fait monter la pression de froid du clan — mais il est
//! alors trop transi pour être *oisif* (l'insight exige le confort, « le temps
//! de penser », voir `tech`) ; et quand le printemps le réchauffe, la pression
//! instantanée du clan est déjà retombée à zéro. La fenêtre « repu **et** clan
//! sous pression » était donc quasi vide. Avec la persistance, le **souvenir**
//! de l'hiver rude subsiste jusqu'au printemps, quand quelqu'un peut enfin
//! penser à s'en prémunir — c'est ce qui laisse la nécessité ressentie hier
//! nourrir l'invention d'aujourd'hui.
//!
//! ## Portée
//!
//! `measure` reconstruit `Sim::clan_pressure` en entier à partir des seuls
//! clans qui existent aujourd'hui — un clan qui s'efface emporte sa pression
//! sans code de nettoyage (même logique que `Clan::home`/`clan_relations`). La
//! mémoire d'un clan disparu s'efface donc avec lui (elle n'est pas reportée).

use std::collections::BTreeMap;

use crate::agent::Physiology;
use crate::sim::Sim;
use crate::social::{ClanId, ClanMembership};

/// Faim moyenne au-dessus de laquelle la famine commence à peser. En dessous,
/// le clan est « repu » et n'en tire aucune pression : la faim a un plancher
/// naturel non nul (elle remonte entre deux repas, cycle `HUNGER_PER_TICK`),
/// donc une moyenne modérée ne signale pas la disette — seul l'excès compte.
/// Premier ordre, à affiner sur une vraie scène (comme `SCARCITY_PER_CAPITA`).
const FED_HUNGER: f32 = 0.5;
/// Effectif d'échelle de la pression de surpopulation : la pression du nombre
/// croît avec les bouches à nourrir et sature vers cet ordre de grandeur.
/// C'est une **mesure** (un input de probabilité), pas un seuil de
/// déclenchement scripté — lire l'effectif ici est légitime (BRIEF §5.2 liste
/// « surpopulation » parmi les pressions), à la différence d'un « si taille N,
/// agir » (anti-pattern §11).
const CROWDING_SCALE: f32 = 40.0;
/// Décroissance quotidienne de la **mémoire** de pression (voir le commentaire
/// de module, « Persistance »). ~0,01/jour = demi-vie d'environ deux mois : le
/// souvenir d'un hiver rude tient jusqu'au printemps, quand un membre enfin au
/// confort peut enfin *penser* à s'en prémunir — ce qui découple le moment de
/// la pression (l'hiver, où tout le monde souffre et personne ne réfléchit) du
/// moment de l'insight (le confort revenu). Le vrai levier de calibrage de la
/// progression de la chaîne technologique en climat rude.
const PRESSURE_DECAY: f32 = 0.01;

/// La pression ressentie par un clan, chaque composante dans `[0, 1]`. Toutes
/// sont des **mesures** (jamais des décisions), reconstruites chaque jour.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ClanPressure {
    /// Faim excédentaire moyenne des membres (0 tant que le clan est repu).
    pub famine: f32,
    /// Stress thermique moyen des membres (naturellement 0 en climat clément).
    pub cold: f32,
    /// Tension maximale avec un clan voisin (réutilise `clan_relations`).
    pub threat: f32,
    /// Pression du nombre : croît avec l'effectif, sature (voir `CROWDING_SCALE`).
    pub crowding: f32,
}

impl ClanPressure {
    /// La pression totale, dans `[0, 1]` : la probabilité « au moins une de ces
    /// tensions mord », combinaison saturante qui **compose** plusieurs stress
    /// (froid *et* faim pèsent plus que l'un seul) sans jamais dépasser 1.
    /// C'est le facteur `pression` de la formule d'insight (§5.2).
    pub fn total(self) -> f32 {
        1.0 - (1.0 - self.famine)
            * (1.0 - self.cold)
            * (1.0 - self.threat)
            * (1.0 - self.crowding)
    }
}

/// Mesure la pression de chaque clan (voir le commentaire de module). Appelée
/// dans la passe quotidienne après `social::daily` — elle a besoin des clans
/// du jour, de la physiologie de leurs membres, et de la tension fraîchement
/// reconstruite (`clan_relations`).
pub(crate) fn measure(sim: &mut Sim) {
    // Faim et froid : sommes puis moyennes par clan sur la physiologie des
    // membres.
    let mut sums: BTreeMap<u64, (f32, f32, u32)> = BTreeMap::new();
    for (_, (membership, phys)) in sim.agents.query::<(&ClanMembership, &Physiology)>().iter() {
        if let Some(cid) = membership.0 {
            let e = sums.entry(cid.0).or_insert((0.0, 0.0, 0));
            e.0 += phys.hunger;
            e.1 += phys.cold;
            e.2 += 1;
        }
    }
    // Menace : la tension maximale de chaque clan avec l'un de ses voisins.
    let mut threat: BTreeMap<u64, f32> = BTreeMap::new();
    for (&(a, b), &t) in &sim.clan_relations.tension {
        let ea = threat.entry(a).or_insert(0.0);
        *ea = ea.max(t);
        let eb = threat.entry(b).or_insert(0.0);
        *eb = eb.max(t);
    }

    let mut next: BTreeMap<ClanId, ClanPressure> = BTreeMap::new();
    for clan in &sim.clans {
        let (mean_hunger, mean_cold) = sums
            .get(&clan.id.0)
            .map(|&(h, c, n)| (h / n.max(1) as f32, c / n.max(1) as f32))
            .unwrap_or((0.0, 0.0));
        // Faim excédentaire : nulle sous `FED_HUNGER`, montant à 1 à faim pleine.
        let famine = ((mean_hunger - FED_HUNGER) / (1.0 - FED_HUNGER)).clamp(0.0, 1.0);
        let crowding = (clan.members.len() as f32 / CROWDING_SCALE).min(1.0);
        let instant = ClanPressure {
            famine,
            cold: mean_cold.clamp(0.0, 1.0),
            threat: threat.get(&clan.id.0).copied().unwrap_or(0.0),
            crowding,
        };
        // Persistance : on garde le plus fort entre la pression du jour et la
        // mémoire de la veille atténuée. Un pic subsiste donc, décroissant, bien
        // après que la cause instantanée a disparu — voir `PRESSURE_DECAY`. La
        // surpopulation varie déjà lentement : pas de mémoire à tenir pour elle.
        let prev = sim.clan_pressure.get(&clan.id).copied().unwrap_or_default();
        let remember = |now: f32, before: f32| now.max(before * (1.0 - PRESSURE_DECAY));
        next.insert(
            clan.id,
            ClanPressure {
                famine: remember(instant.famine, prev.famine),
                cold: remember(instant.cold, prev.cold),
                threat: remember(instant.threat, prev.threat),
                crowding: instant.crowding,
            },
        );
    }
    sim.clan_pressure = next;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentId;
    use crate::social::Clan;
    use cairn_core::WorldSeed;

    fn clan_with(id: u64, members: usize) -> Clan {
        Clan {
            id: ClanId(id),
            founded_tick: 0,
            members: (0..members as u64).map(AgentId).collect(),
            home: (0.0, 0.0),
            stock: 0.0,
            chief: AgentId(0),
            desired: None,
            rivalry: 0.0,
        }
    }

    #[test]
    fn la_pression_totale_compose_sans_depasser_un() {
        let zero = ClanPressure::default();
        assert_eq!(zero.total(), 0.0);
        let plein = ClanPressure { cold: 1.0, ..Default::default() };
        assert_eq!(plein.total(), 1.0);
        // Deux stress modérés composent : plus que l'un seul, jamais > 1.
        let deux = ClanPressure { famine: 0.5, cold: 0.5, ..Default::default() };
        assert!(deux.total() > 0.5 && deux.total() <= 1.0);
        assert!((deux.total() - 0.75).abs() < 1e-6); // 1 - 0,5×0,5
    }

    /// Un clan de membres affamés a une forte pression de famine ; un clan
    /// repu, quasi nulle — le principe « un clan repu n'invente presque rien ».
    #[test]
    fn la_famine_ne_pese_que_sur_un_clan_affame() {
        let affame = {
            let mut sim = Sim::new(WorldSeed(1), 64);
            for _ in 0..8 {
                sim.spawn_agent(0.0, 0.0);
            }
            for (_, (m, phys)) in sim.agents.query_mut::<(&mut ClanMembership, &mut Physiology)>() {
                m.0 = Some(ClanId(1));
                phys.hunger = 0.95;
                phys.cold = 0.0;
            }
            sim.clans.push(clan_with(1, 8));
            measure(&mut sim);
            sim.clan_pressure[&ClanId(1)]
        };
        assert!(affame.famine > 0.8, "famine attendue haute ({})", affame.famine);

        let repu = {
            let mut sim = Sim::new(WorldSeed(1), 64);
            for _ in 0..8 {
                sim.spawn_agent(0.0, 0.0);
            }
            for (_, (m, phys)) in sim.agents.query_mut::<(&mut ClanMembership, &mut Physiology)>() {
                m.0 = Some(ClanId(1));
                phys.hunger = 0.3; // sous FED_HUNGER
                phys.cold = 0.0;
            }
            sim.clans.push(clan_with(1, 8));
            measure(&mut sim);
            sim.clan_pressure[&ClanId(1)]
        };
        assert_eq!(repu.famine, 0.0, "un clan repu ne subit aucune pression de famine");
    }

    /// Le froid moyen des membres et la surpopulation se mesurent ; la menace
    /// est reprise de `clan_relations`.
    #[test]
    fn froid_menace_et_surpopulation_se_mesurent() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        for _ in 0..20 {
            sim.spawn_agent(0.0, 0.0);
        }
        for (_, (m, phys)) in sim.agents.query_mut::<(&mut ClanMembership, &mut Physiology)>() {
            m.0 = Some(ClanId(1));
            phys.hunger = 0.0;
            phys.cold = 0.6;
        }
        sim.clans.push(clan_with(1, 20));
        // Une tension injectée avec un clan fantôme voisin.
        sim.clan_relations.tension.insert((1, 2), 0.4);
        measure(&mut sim);
        let p = sim.clan_pressure[&ClanId(1)];
        assert!((p.cold - 0.6).abs() < 1e-6, "froid moyen des membres");
        assert!((p.threat - 0.4).abs() < 1e-6, "menace reprise de la tension");
        assert!((p.crowding - 20.0 / CROWDING_SCALE).abs() < 1e-6, "surpopulation ∝ effectif");
        assert_eq!(p.famine, 0.0);
    }

    /// Un clan effacé n'a plus de pression : la reconstruction ne garde que les
    /// clans vivants.
    #[test]
    fn un_clan_disparu_perd_sa_pression() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        sim.clans.push(clan_with(7, 8));
        measure(&mut sim);
        assert!(sim.clan_pressure.contains_key(&ClanId(7)));
        sim.clans.clear();
        measure(&mut sim);
        assert!(sim.clan_pressure.is_empty(), "sans clan, aucune pression");
    }

    /// La pression garde une mémoire **décroissante** : un pic de froid subsiste
    /// après que la cause instantanée a disparu (voir `PRESSURE_DECAY`).
    #[test]
    fn la_pression_garde_la_memoire_de_la_durete_recente() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        for _ in 0..8 {
            sim.spawn_agent(0.0, 0.0);
        }
        for (_, (m, phys)) in sim.agents.query_mut::<(&mut ClanMembership, &mut Physiology)>() {
            m.0 = Some(ClanId(1));
            phys.cold = 0.8;
            phys.hunger = 0.0;
        }
        sim.clans.push(clan_with(1, 8));
        measure(&mut sim);
        let peak = sim.clan_pressure[&ClanId(1)].cold;
        assert!((peak - 0.8).abs() < 1e-6, "le pic instantané est mesuré");

        // L'hiver passe : plus personne n'a froid. La pression instantanée est
        // nulle, mais la mémoire subsiste, décroissante.
        for (_, phys) in sim.agents.query_mut::<&mut Physiology>() {
            phys.cold = 0.0;
        }
        measure(&mut sim);
        let after = sim.clan_pressure[&ClanId(1)].cold;
        assert!(after > 0.0 && after < peak, "la mémoire subsiste mais décroît ({after})");
        assert!(
            (after - 0.8 * (1.0 - PRESSURE_DECAY)).abs() < 1e-4,
            "décroissance d'exactement un cran de PRESSURE_DECAY"
        );
    }
}
