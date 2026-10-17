//! **La Foi** (BRIEF §6.1) — la ressource du joueur, et les croyants qui la
//! produisent.
//!
//! > « Générée par les croyants. **Aucun croyant au départ → la divinité est
//! > quasi impuissante au paléolithique.** Ses premiers miracles créent ses
//! > premiers fidèles, qui génèrent la Foi qui permet les miracles suivants.
//! > Boucle de rétroaction propre et auto-limitante. »
//!
//! ## La ferveur est individuelle, comme un savoir
//!
//! Arbitrage acté : [`Faith`] est un composant **par agent**, sur le patron
//! exact de `tech::Knowledge`. Les conséquences sont les mêmes, et c'est bien
//! l'intention :
//!
//! - elle **naît d'avoir été témoin** — on ne croit pas par décret, on croit
//!   parce qu'on a vu ;
//! - elle **se transmet par la parole** (`preach`, la passe de rencontre), avec
//!   une intensité moindre : croire sur parole n'est pas croire pour avoir vu ;
//! - elle **s'efface** faute d'entretien, et **meurt avec ses porteurs** — donc
//!   **une religion peut s'oublier**, exactement comme une technique.
//!
//! Un nouveau-né naît incroyant. Le monde ne se souvient pas pour lui.
//!
//! ## Ce que chacun retient
//!
//! Un croyant ne garde pas qu'une intensité : il retient **ce qu'il a vu** —
//! combien de fois le ciel l'a secouru, combien de fois il l'a frappé. Ce sont
//! deux compteurs, et rien de plus : aucun jugement moral n'est codé, on note
//! seulement ce qu'un homme a subi de son point de vue. C'est la matière brute
//! dont l'incrément suivant tirera la **théologie** d'un peuple (§6.3) — dieu
//! nourricier, dieu de colère, ou dieu absent — par simple agrégation, comme
//! `clan_corpus` agrège les savoirs.
//!
//! Conformément à l'arbitrage « mesurer d'abord, rétroagir après », ces deux
//! compteurs ne **font** rien pour l'instant. Ils comptent.

use std::collections::{BTreeMap, BTreeSet};

use cairn_core::{Pcg32, splitmix64};

use crate::agent::{AgentId, Position};
use crate::memory::TALK_RADIUS_TILES;
use crate::salt;
use crate::sim::Sim;

/// Ferveur qu'on gagne à être témoin direct d'un prodige. Élevée : voir le ciel
/// s'ouvrir ne laisse pas indifférent.
const WITNESS_FERVOR: f32 = 0.6;
/// Part de sa ferveur qu'un croyant transmet à qui l'écoute. Moins que ce qu'il
/// éprouve : croire sur parole n'est pas croire pour avoir vu.
const PREACH_TRANSFER: f32 = 0.4;
/// Chance qu'une rencontre entre un croyant et un incroyant porte, par passe.
/// Faible, comme la diffusion technique — la conviction chemine lentement.
const PREACH_RATE: f64 = 0.05;
/// Ce que la ferveur perd chaque jour sans prodige pour la raviver. Demi-vie
/// d'environ six mois : le souvenir d'un miracle traverse une saison, pas une
/// vie. C'est ce qui rend la divinité **auto-limitante** — cesser d'agir, c'est
/// être oublié.
const FERVOR_DECAY_PER_DAY: f32 = 0.004;
/// En deçà, la ferveur ne vaut plus d'être portée : on n'y croit plus.
const FERVOR_FLOOR: f32 = 0.02;
/// Foi produite par jour et par point de ferveur. Le robinet du §6.1.
const FAITH_PER_FERVOR_PER_DAY: f32 = 0.02;

/// La croyance d'un individu. Composant à part entière, comme `Knowledge`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Faith {
    /// L'intensité de la croyance, dans `[0, 1]`. Zéro = incroyant.
    pub fervor: f32,
    /// Combien de fois le ciel l'a secouru — de son point de vue.
    pub boons: u16,
    /// Combien de fois il l'a frappé.
    pub banes: u16,
}

impl Faith {
    pub fn believes(&self) -> bool {
        self.fervor >= FERVOR_FLOOR
    }

    /// Ce que cet homme croit du caractère de sa divinité, dans `[-1, 1]` :
    /// `+1` s'il n'a connu que des bienfaits, `-1` que des malheurs, `0` s'il
    /// n'a rien vu ou a vu autant des deux. **Dérivé, jamais stocké** — même
    /// discipline que `tech::Age` ou le corpus d'un clan.
    pub fn benevolence(&self) -> f32 {
        let total = f32::from(self.boons) + f32::from(self.banes);
        if total == 0.0 {
            return 0.0;
        }
        (f32::from(self.boons) - f32::from(self.banes)) / total
    }
}

/// Fait de tous les agents à portée les **témoins** d'un prodige.
///
/// `boon` dit si le geste leur a paru favorable — vu depuis eux, pas dans
/// l'absolu : la même averse est une grâce pour qui a soif et un désastre pour
/// qui voulait du feu. C'est l'appelant (`divine`) qui tranche, parce que lui
/// seul sait ce que le geste a produit.
pub(crate) fn witness(sim: &mut Sim, center: (f64, f64), radius: f64, boon: bool) -> usize {
    let mut count = 0usize;
    for (_, (pos, faith)) in sim.agents.query_mut::<(&Position, &mut Faith)>() {
        if (pos.x - center.0).hypot(pos.y - center.1) > radius {
            continue;
        }
        faith.fervor = (faith.fervor + WITNESS_FERVOR).min(1.0);
        if boon {
            faith.boons = faith.boons.saturating_add(1);
        } else {
            faith.banes = faith.banes.saturating_add(1);
        }
        count += 1;
    }
    count
}

/// Soif au-delà de laquelle une averse cesse d'être la météo et devient une
/// réponse. En deçà, il pleut, voilà tout.
const PRAYER_THIRST: f32 = 0.45;
/// Faim au-delà de laquelle une sécheresse est vécue comme un châtiment.
const PRAYER_HUNGER: f32 = 0.45;

/// Le témoignage **de la météo**, qui ne se juge pas comme les autres.
///
/// Une averse est indiscernable du ciel ordinaire : personne ne peut savoir
/// qu'on l'a voulue. Mais ce n'est pas le phénomène qui fait le signe — c'est sa
/// **coïncidence avec le besoin**. Il pleut sur un homme qui meurt de soif, et
/// pour lui quelque chose a répondu ; il pleut sur son voisin repu, et il ne
/// s'est rien passé du tout. C'est ainsi que naissent les cultes de la pluie, et
/// c'est pourquoi le jugement est ici **individuel** : la même averse est un
/// miracle et un jour ordinaire, selon qui la reçoit.
///
/// Conséquence qu'on n'a pas eu à écrire : **un peuple qui souffre voit des
/// signes partout, un peuple comblé n'en voit aucun.**
pub(crate) fn witness_weather(sim: &mut Sim, center: (f64, f64), radius: f64, rain: bool) -> usize {
    let mut count = 0usize;
    for (_, (pos, phys, faith)) in
        sim.agents.query_mut::<(&Position, &crate::agent::Physiology, &mut Faith)>()
    {
        if (pos.x - center.0).hypot(pos.y - center.1) > radius {
            continue;
        }
        // Le besoin décide. Sans lui, ce n'est que du temps qu'il fait.
        let answered = if rain { phys.thirst >= PRAYER_THIRST } else { phys.hunger >= PRAYER_HUNGER };
        if !answered {
            continue;
        }
        faith.fervor = (faith.fervor + WITNESS_FERVOR).min(1.0);
        if rain {
            faith.boons = faith.boons.saturating_add(1);
        } else {
            faith.banes = faith.banes.saturating_add(1);
        }
        count += 1;
    }
    count
}

/// La passe quotidienne : la ferveur s'émousse, et ce qu'il en reste produit la
/// Foi du joueur.
///
/// L'ordre compte — on décroît **avant** de récolter, sinon un miracle
/// rapporterait sa pleine mesure le jour même où il est déjà oublié.
pub(crate) fn daily(sim: &mut Sim) {
    let mut produced = 0.0f32;
    for (_, faith) in sim.agents.query_mut::<&mut Faith>() {
        if faith.fervor <= 0.0 {
            continue;
        }
        faith.fervor = (faith.fervor - FERVOR_DECAY_PER_DAY).max(0.0);
        if faith.fervor < FERVOR_FLOOR {
            faith.fervor = 0.0; // on n'y croit plus ; les souvenirs, eux, restent
            continue;
        }
        produced += faith.fervor;
    }
    sim.faith += produced * FAITH_PER_FERVOR_PER_DAY;
}

/// La transmission par la parole, dans la passe de rencontre — même cadence et
/// même esprit que `tech::diffuse` : on ne convertit pas une foule, on convainc
/// un voisin, parfois.
///
/// Deux passes lire-puis-écrire (jamais deux emprunts de l'ECS à la fois), donc
/// **indépendant de l'ordre** ; un flux RNG par auditeur, donc déterministe.
pub(crate) fn preach(sim: &mut Sim) {
    // 1. Qui croit, où, et avec quelle intensité.
    let believers: Vec<(AgentId, (f64, f64), f32)> = sim
        .agents
        .query::<(&AgentId, &Position, &Faith)>()
        .iter()
        .filter(|(_, (_, _, f))| f.believes())
        .map(|(_, (id, pos, f))| (*id, (pos.x, pos.y), f.fervor))
        .collect();
    if believers.is_empty() {
        return;
    }

    // 2. Ce que chaque incroyant entend du plus fervent de ses voisins.
    let seed = sim.world.seed().derive(salt::FAITH) ^ splitmix64(sim.time.tick);
    let mut converted: BTreeMap<u64, f32> = BTreeMap::new();
    let known: BTreeSet<u64> = believers.iter().map(|(id, _, _)| id.0).collect();
    for (_, (id, pos, faith)) in sim.agents.query::<(&AgentId, &Position, &Faith)>().iter() {
        if known.contains(&id.0) {
            continue; // il croit déjà ; on ne prêche pas un converti
        }
        let best = believers
            .iter()
            .filter(|(_, p, _)| (pos.x - p.0).hypot(pos.y - p.1) <= TALK_RADIUS_TILES)
            .map(|(_, _, f)| *f)
            .fold(0.0f32, f32::max);
        if best <= 0.0 {
            continue;
        }
        let mut rng = Pcg32::new(seed, id.0);
        if f64::from(rng.next_f32()) < PREACH_RATE {
            converted.insert(id.0, (faith.fervor + best * PREACH_TRANSFER).min(1.0));
        }
    }
    if converted.is_empty() {
        return;
    }

    // 3. Écriture.
    for (_, (id, faith)) in sim.agents.query_mut::<(&AgentId, &mut Faith)>() {
        if let Some(&fervor) = converted.get(&id.0) {
            faith.fervor = fervor;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_core::WorldSeed;

    fn faith_of(sim: &Sim, who: AgentId) -> Faith {
        sim.agents
            .query::<(&AgentId, &Faith)>()
            .iter()
            .find(|(_, (id, _))| **id == who)
            .map(|(_, (_, f))| *f)
            .unwrap()
    }

    /// On ne croit que pour avoir vu — et seulement si on était là.
    #[test]
    fn la_ferveur_naît_du_temoignage_et_de_lui_seul() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let present = sim.spawn_agent(0.0, 0.0);
        let absent = sim.spawn_agent(500.0, 0.0);
        let vus = witness(&mut sim, (0.0, 0.0), 100.0, true);

        assert_eq!(vus, 1, "un seul témoin");
        assert!(faith_of(&sim, present).believes(), "celui qui a vu croit");
        assert!(!faith_of(&sim, absent).believes(), "celui qui n'était pas là, non");
        assert_eq!(faith_of(&sim, present).boons, 1, "et il se souvient de quoi");
    }

    /// **La boucle du §6.1** : pas de croyants, pas de Foi. C'est ce qui rend la
    /// divinité impuissante au départ, et auto-limitante ensuite.
    #[test]
    fn sans_croyants_aucune_foi_n_est_produite() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        sim.spawn_agent(0.0, 0.0);
        daily(&mut sim);
        assert_eq!(sim.faith, 0.0, "un monde incroyant ne produit rien");

        witness(&mut sim, (0.0, 0.0), 100.0, true);
        daily(&mut sim);
        assert!(sim.faith > 0.0, "un fidèle, et la Foi commence à couler");
    }

    /// Cesser d'agir, c'est être oublié : sans nouveau prodige, la ferveur
    /// s'éteint — et le robinet se ferme avec elle.
    #[test]
    fn une_divinite_qui_se_tait_finit_par_ne_plus_rien_recevoir() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let croyant = sim.spawn_agent(0.0, 0.0);
        witness(&mut sim, (0.0, 0.0), 100.0, true);

        // Assez de temps pour que le souvenir s'efface (demi-vie ~6 mois).
        for _ in 0..400 {
            daily(&mut sim);
        }
        assert!(!faith_of(&sim, croyant).believes(), "le souvenir s'est éteint");

        let acquise = sim.faith;
        daily(&mut sim);
        assert_eq!(sim.faith, acquise, "et plus rien n'arrive");
        // Mais ce qu'il a vécu, lui, ne s'efface pas : c'est son histoire.
        assert_eq!(faith_of(&sim, croyant).boons, 1);
    }

    /// La parole convertit — moins fort que le témoignage, et jamais au-delà de
    /// la portée de voix.
    #[test]
    fn la_parole_convertit_moins_fort_que_la_vue() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let temoin = sim.spawn_agent(0.0, 0.0);
        let voisin = sim.spawn_agent(TALK_RADIUS_TILES * 0.5, 0.0);
        let lointain = sim.spawn_agent(10_000.0, 0.0);
        witness(&mut sim, (0.0, 0.0), 10.0, true); // seul le témoin voit

        // Assez de passes pour que le tirage finisse par tomber. **Le temps doit
        // avancer** : le tirage dérive du tick, si bien qu'appeler `preach` en
        // boucle sur le même instant rejouerait deux cents fois la même seconde
        // — et le même refus. Dans la simulation, la passe tombe toutes les 4 h.
        for _ in 0..200 {
            sim.time.tick += 4;
            preach(&mut sim);
        }
        let (t, v, l) = (faith_of(&sim, temoin), faith_of(&sim, voisin), faith_of(&sim, lointain));
        assert!(v.believes(), "le voisin a fini par être convaincu");
        assert!(v.fervor < t.fervor, "mais il y croit moins fort ({} < {})", v.fervor, t.fervor);
        assert!(!l.believes(), "on ne prêche pas à dix kilomètres");
        assert_eq!(v.boons, 0, "il n'a rien vu lui-même : il n'a pas de souvenir");
    }

    /// Ce qu'on croit du caractère d'un dieu se **dérive** de ce qu'on a subi.
    #[test]
    fn le_caractere_percu_se_derive_de_ce_qu_on_a_subi() {
        let nourricier = Faith { fervor: 0.5, boons: 3, banes: 0 };
        let colerique = Faith { fervor: 0.5, boons: 0, banes: 3 };
        let ambigu = Faith { fervor: 0.5, boons: 2, banes: 2 };
        assert_eq!(nourricier.benevolence(), 1.0);
        assert_eq!(colerique.benevolence(), -1.0);
        assert_eq!(ambigu.benevolence(), 0.0);
        assert_eq!(Faith::default().benevolence(), 0.0, "qui n'a rien vu ne croit rien");
    }
}
