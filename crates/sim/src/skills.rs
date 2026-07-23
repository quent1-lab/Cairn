//! Les compétences acquises par la pratique (BRIEF §3.1, Phase 3).
//!
//! Une compétence ne s'achète pas : elle **se forge en faisant**. Chaque
//! heure de pratique rapproche le savoir-faire de son plafond par une courbe
//! saturante — les premiers gestes apprennent beaucoup, la maîtrise se paie
//! de plus en plus cher :
//!
//! ```text
//! compétence += taux × (plafond − compétence)
//! ```
//!
//! Le **plafond dépend des traits hérités** (BRIEF : « plafond conditionné
//! par les traits ») : la dextérité borne la cueillette, force et endurance
//! bornent la chasse. Un maladroit plafonne vite ; un doué, s'il pratique,
//! va plus loin. C'est par là que l'hérédité rencontre l'apprentissage — et
//! qu'une lignée de bons chasseurs peut émerger sans qu'aucune règle ne le
//! décrète.
//!
//! Phase 3 : cueillette et chasse. Phase 4 : oratoire (`social::encounter`
//! la pratique — chaque conversation est une occasion de rhétorique, pas
//! besoin d'une tâche dédiée). Taille de pierre, construction… viendront
//! avec leurs systèmes.

use crate::demography::Traits;

/// Gain par heure de pratique, en fraction du chemin restant vers le
/// plafond : ~63 % du plafond après 350 h de pratique, ~95 % après 1 000 h.
pub const LEARN_RATE: f32 = 0.003;
/// Une mise à mort enseigne plus qu'une heure d'affût : équivalent de 30 h.
pub const KILL_PRACTICE_BOOST: f32 = 30.0;

/// Le bloc des compétences acquises, dans [0, 1]. Les nouveau-nés partent
/// de zéro ; les fondateurs arrivent à mi-plafond (ils ont survécu jusqu'là).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Skills {
    pub foraging: f32,
    pub hunting: f32,
    /// Art de convaincre — l'un des deux facteurs du chef (BRIEF §5.1 :
    /// « chef : max(oratoire × prestige) »), voir `social::elect_chiefs`.
    pub oratory: f32,
}

impl Skills {
    /// Compétences d'un fondateur : à mi-chemin de leurs plafonds.
    pub fn founder(traits: &Traits) -> Self {
        Self {
            foraging: 0.5 * forage_cap(traits),
            hunting: 0.5 * hunt_cap(traits),
            oratory: 0.5 * oratory_cap(traits),
        }
    }
}

/// Plafond de cueillette : la dextérité fait le cueilleur.
pub fn forage_cap(traits: &Traits) -> f32 {
    0.4 + 0.6 * traits.dexterity
}

/// Plafond de chasse : la force et l'endurance font le chasseur.
pub fn hunt_cap(traits: &Traits) -> f32 {
    0.4 + 0.3 * (traits.strength + traits.endurance)
}

/// Plafond d'oratoire : la sociabilité fait l'orateur — même trait qui pousse
/// déjà `TaskKind::Socialize` (Phase 3), réutilisé plutôt qu'un nouveau trait
/// inventé pour l'occasion.
pub fn oratory_cap(traits: &Traits) -> f32 {
    0.4 + 0.6 * traits.sociability
}

/// Une heure de pratique (ou `hours` d'équivalent, pour les gestes rares).
pub fn practice(skill: &mut f32, cap: f32, hours: f32) {
    *skill += (LEARN_RATE * hours).min(1.0) * (cap - *skill).max(0.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_pratique_forge_et_sature() {
        let traits = Traits::default();
        let cap = forage_cap(&traits); // 0,7 à dextérité 0,5
        let mut skill = 0.0;
        let mut previous = 0.0;
        let mut first_gain = None;
        for hour in 0..2000 {
            practice(&mut skill, cap, 1.0);
            let gain = skill - previous;
            if hour == 0 {
                first_gain = Some(gain);
            }
            assert!(skill <= cap, "la compétence ne dépasse jamais son plafond");
            assert!(gain >= 0.0);
            previous = skill;
        }
        assert!(skill > 0.9 * cap, "2000 h de pratique doivent frôler le plafond");
        // Courbe saturante : le premier gain est le plus grand.
        assert!(previous - skill < first_gain.unwrap());
    }

    #[test]
    fn le_plafond_suit_les_traits() {
        let doue = Traits { dexterity: 1.0, ..Traits::default() };
        let maladroit = Traits { dexterity: 0.0, ..Traits::default() };
        assert!(forage_cap(&doue) > forage_cap(&maladroit));
        let costaud = Traits { strength: 1.0, endurance: 1.0, ..Traits::default() };
        assert!(hunt_cap(&costaud) > hunt_cap(&Traits::default()));
        assert!(hunt_cap(&costaud) <= 1.0);
        let sociable = Traits { sociability: 1.0, ..Traits::default() };
        let solitaire = Traits { sociability: 0.0, ..Traits::default() };
        assert!(oratory_cap(&sociable) > oratory_cap(&solitaire));
        assert!(oratory_cap(&sociable) <= 1.0);
    }

    #[test]
    fn les_fondateurs_partent_a_mi_plafond() {
        let traits = Traits::default();
        let skills = Skills::founder(&traits);
        assert!((skills.foraging - 0.5 * forage_cap(&traits)).abs() < 1e-6);
        assert!(skills.hunting < hunt_cap(&traits));
        assert!((skills.oratory - 0.5 * oratory_cap(&traits)).abs() < 1e-6);
    }
}
