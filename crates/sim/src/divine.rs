//! **Les interventions divines** (BRIEF §6.2) — le seul pouvoir du joueur.
//!
//! Le joueur n'a aucun contrôle direct sur un agent. Il agit sur **le monde**,
//! et observe le résultat.
//!
//! ## L'ambivalence n'est pas une option
//!
//! > « Toutes les interventions sont volontairement ambivalentes. Aucune n'est
//! > un bouton bien. »
//!
//! Ce n'est pas une contrainte esthétique, c'est le moteur du culte émergent
//! (§6.3) : un peuple ne peut se construire une théologie *cohérente avec le
//! comportement réel* de sa divinité que si ce comportement est lisible sans
//! être univoque. Une foudre qui ne ferait que du bien produirait un dieu
//! unanimement adoré — donc aucune histoire.
//!
//! L'ambivalence n'est donc **jamais** un tirage « une chance sur deux de
//! rater ». Elle est **géographique et gratuite** : c'est le lieu et le moment
//! choisis qui décident du bien et du mal, et le joueur en assume le choix.
//! Frapper une savane sèche près d'un peuple sans silex lui offre le feu ;
//! frapper vingt mètres trop près le tue. Rien dans le code ne pondère « bon »
//! contre « mauvais » : les deux tombent des mêmes règles locales.
//!
//! ## Un journal, donc un monde rejouable
//!
//! Le BRIEF §8.2 promet le « replay complet du monde depuis la seed + le
//! journal des interventions divines ». C'est la raison d'être de
//! [`Sim::miracles`] : la simulation étant déterministe, **seed + ce journal**
//! suffisent à rejouer une histoire entière. Une intervention n'est donc pas un
//! appel de fonction qu'on oublie, c'est un **fait daté** qu'on garde.
//!
//! ## Une porte unique
//!
//! [`Sim::invoke`] est le seul chemin. Le client WASM fait tourner le `Sim` en
//! local et l'appelle directement ; le jour où le serveur existera (Phase 6,
//! partie réseau), cette signature deviendra l'endpoint sans que la simulation
//! change d'une ligne.
//!
//! **La Foi ne coûte rien pour l'instant** (arbitrage acté) : elle arrive à
//! l'incrément suivant. Sans croyants, aucun miracle ne serait finançable, donc
//! aucun ne serait testable — on branche donc le coût quand il y aura de quoi
//! le payer.

use cairn_core::km_to_tiles;

use crate::agent::{DeathCause, Physiology, Position};
use crate::chronicle::EventKind;
use crate::demography::Demographics;
use crate::fire;
use crate::sim::Sim;

/// Rayon dans lequel la foudre tue net (~20 m). Court : c'est le prix d'un
/// geste mal placé, pas une arme de zone.
const LIGHTNING_LETHAL_TILES: f64 = 10.0;

/// Ce que le joueur peut demander au monde. Chaque variante porte sa cible :
/// une intervention est un geste **situé**, jamais global.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Intervention {
    /// La foudre frappe une tuile. Elle allume un feu **si le site peut
    /// brûler** — c'est le sol qui décide, pas la divinité — et tue net qui se
    /// trouvait dessous. Les deux dans le même geste : « peut *donner le feu* à
    /// un clan… ou le brûler vif ».
    Lightning { pos: (i64, i64) },
}

/// Un miracle **accompli**, daté : le journal qui, avec la seed, rejoue le
/// monde (voir l'en-tête). On garde l'issue à côté de l'intention, car c'est
/// elle qui distingue une foudre qui embrase d'une foudre qui n'a rien fait.
#[derive(Debug, Clone, Copy)]
pub struct Miracle {
    pub tick: u64,
    pub intervention: Intervention,
    pub outcome: Outcome,
}

/// Ce que le geste a **réellement** produit. Le joueur vise ; le monde répond.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    /// Un feu a-t-il pris ? (Non si le sol était détrempé, nu ou gelé.)
    pub ignited: bool,
    /// Combien d'humains le geste a-t-il tués.
    pub killed: usize,
}

/// Pourquoi une intervention n'a pas pu être tentée. Distinct d'un
/// [`Outcome`] sans effet : ici le geste n'a pas eu lieu du tout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivineError {
    /// La cible désignée n'existe pas (ou plus).
    NoSuchTarget,
}

impl Sim {
    /// **La porte unique du joueur-divinité.** Accomplit l'intervention, la
    /// journalise (rejouabilité), et en fait un fait de Chronique.
    ///
    /// Ne consomme aucune Foi pour l'instant — voir l'en-tête du module.
    pub fn invoke(&mut self, intervention: Intervention) -> Result<Outcome, DivineError> {
        let outcome = match intervention {
            Intervention::Lightning { pos } => self.strike_lightning(pos),
        };
        self.miracles.push(Miracle { tick: self.time.tick, intervention, outcome });
        let pos = match intervention {
            Intervention::Lightning { pos } => pos,
        };
        self.record(pos, EventKind::Miracle { intervention, outcome });
        Ok(outcome)
    }

    /// La foudre : un feu si le sol s'y prête, des morts si quelqu'un était là.
    /// Les deux sont indépendants — on peut tout à fait embraser une lande
    /// déserte, ou tuer un homme sans que rien ne brûle.
    fn strike_lightning(&mut self, pos: (i64, i64)) -> Outcome {
        let point = (pos.0 as f64 + 0.5, pos.1 as f64 + 0.5);
        let ignited = fire::ignite_at(self, point);

        // Qui se tenait sous le coup. On relève d'abord (l'itération emprunte
        // l'ECS), on retire ensuite — même patron que les morts ordinaires.
        let mut killed = 0usize;
        for (_, (agent_pos, phys, demo)) in
            self.agents.query_mut::<(&Position, &mut Physiology, &Demographics)>()
        {
            let d = (agent_pos.x - point.0).hypot(agent_pos.y - point.1);
            if d <= LIGHTNING_LETHAL_TILES && !phys.is_dead() {
                // On passe par le pipeline de mort ordinaire : santé à zéro et
                // cause inscrite. La passe de physiologie fera le reste, et la
                // Chronique nommera le mort si c'était un chef.
                phys.health = 0.0;
                phys.last_damage = Some(DeathCause::Lightning);
                let _ = demo;
                killed += 1;
            }
        }
        Outcome { ignited, killed }
    }

    /// Distance (en tuiles) à laquelle la foudre est mortelle — le client s'en
    /// sert pour montrer au joueur ce qu'il risque de faire.
    pub fn lightning_lethal_radius() -> f64 {
        LIGHTNING_LETHAL_TILES
    }

    /// Rayon indicatif d'un embrasement, pour le même usage.
    pub fn lightning_sight_radius() -> f64 {
        km_to_tiles(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentId;
    use crate::tile::Tile;
    use cairn_core::WorldSeed;

    /// Cherche une tuile réellement inflammable (même méthode que les tests de
    /// `fire`) : on veut éprouver la foudre sur un vrai sol, pas sur un décor.
    fn find_flammable(sim: &mut Sim) -> Option<(i64, i64)> {
        let step = km_to_tiles(8.0) as i64;
        for r in 0..300i64 {
            let d = r * step;
            for &(x, y) in &[(d, 0), (-d, 0), (0, d), (0, -d), (d, d), (-d, -d)] {
                let t: Tile = sim.world.tile(x, y);
                if t.is_walkable() && t.biomass >= 60 && t.humidity <= 120 && t.temperature >= 5.0 {
                    return Some((x, y));
                }
            }
        }
        None
    }

    /// Le cœur du §6.2 : **le même geste** donne le feu et tue. Ce qui décide,
    /// c'est où l'on frappe — pas un tirage « bon ou mauvais ».
    #[test]
    fn la_foudre_donne_le_feu_et_tue_dans_le_meme_geste() {
        let mut sim = Sim::new(WorldSeed(42), 256);
        let Some((fx, fy)) = find_flammable(&mut sim) else {
            return; // seed sans zone sèche proche : test sans objet (rare)
        };
        // Un malheureux sous le coup, un autre à bonne distance.
        let dessous = sim.spawn_agent(fx as f64 + 0.5, fy as f64 + 0.5);
        let a_l_ecart = sim.spawn_agent(fx as f64 + 500.0, fy as f64 + 0.5);

        let outcome = sim.invoke(Intervention::Lightning { pos: (fx, fy) }).unwrap();

        assert!(outcome.ignited, "une savane sèche doit s'embraser");
        assert_eq!(outcome.killed, 1, "seul celui qui était dessous doit mourir");
        assert!(!sim.fires.is_empty(), "le feu doit exister dans le monde");

        let dead = |id| {
            sim.agents
                .query::<(&AgentId, &Physiology)>()
                .iter()
                .find(|(_, (a, _))| **a == id)
                .map(|(_, (_, p))| p.is_dead())
                .unwrap()
        };
        assert!(dead(dessous), "l'agent sous la foudre est tué");
        assert!(!dead(a_l_ecart), "celui qui était à l'écart est indemne");
    }

    /// L'ambivalence n'est pas un tirage : sur un sol qui ne peut pas brûler,
    /// la foudre ne donne rien — mais elle tue quand même.
    #[test]
    fn sur_un_sol_qui_ne_brule_pas_la_foudre_ne_donne_rien() {
        let mut sim = Sim::new(WorldSeed(42), 256);
        // Une tuile d'eau : rien n'y prend feu, par construction.
        let mut ocean = None;
        for r in 0..400i64 {
            let p = (r * 200, 0);
            if !sim.world.tile(p.0, p.1).is_walkable() {
                ocean = Some(p);
                break;
            }
        }
        let Some(pos) = ocean else { return };
        let outcome = sim.invoke(Intervention::Lightning { pos }).unwrap();
        assert!(!outcome.ignited, "l'eau ne prend pas feu");
        assert_eq!(outcome.killed, 0, "personne n'était là");
    }

    /// Le journal est ce qui rend le monde rejouable (§8.2) : chaque geste y
    /// laisse sa date, son intention **et** son issue.
    #[test]
    fn chaque_miracle_entre_au_journal_et_dans_la_chronique() {
        let mut sim = Sim::new(WorldSeed(42), 256);
        sim.time.tick = 1234;
        let pos = (0, 0);
        sim.invoke(Intervention::Lightning { pos }).unwrap();

        assert_eq!(sim.miracles.len(), 1);
        assert_eq!(sim.miracles[0].tick, 1234, "le miracle est daté");
        assert_eq!(sim.miracles[0].intervention, Intervention::Lightning { pos });
        assert!(
            sim.chronicle.iter().any(|e| matches!(e.kind, EventKind::Miracle { .. })),
            "un miracle fait toujours date"
        );
    }
}
