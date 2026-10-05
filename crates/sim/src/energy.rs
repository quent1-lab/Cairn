//! Le bilan énergétique : ce que le corps **dépense** (chantier du bilan
//! énergétique, 2026-10-05, avant le sevrage — décision de l'utilisateur).
//!
//! Les apports sont en kcal depuis D10 (gibier, cueillette) ; la dépense, elle,
//! était la même pour tous — 2 500 kcal/j pour un chasseur comme pour un
//! nourrisson. Ici, elle découle du corps et de ce qu'il fait :
//!
//! - **masse** selon l'âge et le sexe (repères Hadza adultes : Pontzer et al.
//!   2012 ; enfants, ordre de grandeur) ;
//! - **métabolisme de base** : équations de Schofield (FAO/OMS/UNU) ;
//! - **activité** en multiples du métabolisme de base (FAO/OMS/UNU 2004,
//!   ordre de grandeur), la marche chargée au prorata de la masse portée
//!   (coût ∝ masse totale déplacée, Pandolf 1977) ;
//! - **froid** (thermogenèse), **fièvre**, **croissance**, **grossesse**.
//!
//! Le lait, lui, se paie par la mère à hauteur de ce que le nourrisson boit
//! (`demography::nurse_infants`).
//!
//! La faim reste la jauge [0, 1] que lit le cerveau, mais **à l'échelle de
//! chacun** : 1, ce sont deux jours de son besoin de référence. La nourriture
//! reste comptée en points de [`KCAL_PER_POINT`] (stock de clan, viande
//! portée, parts) ; chaque mangeur les convertit à son échelle
//! (`Physiology::scale`).

use crate::agent::Activity;
use crate::demography::Sex;

/// Énergie d'un point de nourriture — et d'une unité de faim pour l'adulte de
/// référence (deux jours à 2 500 kcal).
pub const KCAL_PER_POINT: f64 = 5_000.0;
/// Énergie d'un kilo de graisse.
pub const KCAL_PER_KG_FAT: f32 = 7_700.0;
/// Niveau d'activité de référence (dépense ÷ métabolisme de base) qui fixe
/// l'échelle de la faim : celui des femmes hadza (Pontzer et al. 2012 : 1,78).
pub const REFERENCE_PAL: f32 = 1.75;
/// Masse d'un point de viande portée : la viande crue vaut ~1 200 kcal/kg
/// (`Species::edible_kcal`).
pub const KG_PER_MEAT_POINT: f32 = (KCAL_PER_POINT / 1_200.0) as f32;

/// Masse corporelle (kg) selon l'âge et le sexe : interpolation linéaire entre
/// des repères. Adultes : Hadza (femmes 43,4 kg, hommes 50,9 kg). Enfants :
/// ordre de grandeur d'une croissance de chasseurs-cueilleurs, plus lente que
/// les standards OMS.
pub fn body_mass_kg(age_years: f64, sex: Sex) -> f32 {
    let adult = match sex {
        Sex::Female => [38.0, 43.4],
        Sex::Male => [40.0, 50.9],
    };
    let marks: [(f64, f32); 7] =
        [(0.0, 3.0), (1.0, 8.5), (2.0, 10.5), (5.0, 15.0), (10.0, 24.0), (15.0, adult[0]), (18.0, adult[1])];
    if age_years >= 18.0 {
        return adult[1];
    }
    let i = marks.iter().rposition(|(a, _)| age_years >= *a).unwrap_or(0);
    let ((a0, m0), (a1, m1)) = (marks[i], marks[i + 1]);
    m0 + (m1 - m0) * ((age_years - a0) / (a1 - a0)) as f32
}

/// Métabolisme de base (kcal/j) : équations de Schofield, par sexe et tranche
/// d'âge (FAO/OMS/UNU 1985, reprises en 2004).
pub fn bmr_kcal_day(mass_kg: f32, age_years: f64, sex: Sex) -> f32 {
    let (a, b) = match sex {
        Sex::Male => match age_years {
            a if a < 3.0 => (59.512, -30.4),
            a if a < 10.0 => (22.706, 504.3),
            a if a < 18.0 => (17.686, 658.2),
            a if a < 30.0 => (15.057, 692.2),
            a if a < 60.0 => (11.472, 873.1),
            _ => (11.711, 587.7),
        },
        Sex::Female => match age_years {
            a if a < 3.0 => (58.317, -31.1),
            a if a < 10.0 => (20.315, 485.9),
            a if a < 18.0 => (13.384, 692.6),
            a if a < 30.0 => (14.818, 486.6),
            a if a < 60.0 => (8.126, 845.6),
            _ => (9.082, 658.5),
        },
    };
    (a * mass_kg + b).max(50.0)
}

/// Énergie déposée par la croissance (kcal/j) : massive les premiers mois,
/// faible ensuite (FAO/OMS/UNU 2004, ch. 3, ordre de grandeur).
pub fn growth_kcal_day(age_years: f64) -> f32 {
    match age_years {
        a if a < 0.25 => 200.0,
        a if a < 0.5 => 80.0,
        a if a < 1.0 => 25.0,
        a if a < 18.0 => 15.0,
        _ => 0.0,
    }
}

/// Coût d'une activité en multiples du métabolisme de base (FAO/OMS/UNU 2004,
/// annexe, ordre de grandeur). Marcher : 4 km/h en terrain sauvage.
pub fn activity_factor(activity: Activity) -> f32 {
    match activity {
        Activity::Sleeping => 1.0,
        Activity::Sheltering => 1.2,
        Activity::Idle | Activity::Drinking => 1.4,
        Activity::Eating => 2.5, // cueillir : chercher, creuser, ramasser
        Activity::Walking => 3.0,
        Activity::Hunting | Activity::Farming => 3.5,
        Activity::Fighting => 6.0,
    }
}

/// Thermogenèse (multiples du métabolisme de base en plus) sous 10 °C
/// ressentis : croît avec le déficit, plafonnée à 2 en plein frisson. Choix de
/// forme : les sources chiffrent mal la relation, pas son plafond.
pub fn cold_factor(felt_c: f64) -> f32 {
    ((10.0 - felt_c).max(0.0) * 0.04).min(2.0) as f32
}

/// Fièvre : +0,2 métabolisme de base à gravité pleine (~+10-13 % par °C).
pub const FEVER_FACTOR: f32 = 0.2;

/// Surcoût de la grossesse (kcal/j) selon le trimestre (FAO/OMS/UNU 2004).
pub fn pregnancy_kcal_day(days_pregnant: f64) -> f32 {
    match days_pregnant {
        d if d < 90.0 => 85.0,
        d if d < 180.0 => 360.0,
        _ => 475.0,
    }
}

/// Le corps d'un humain à un âge donné : de quoi chiffrer sa dépense et son
/// échelle de faim.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    pub mass_kg: f32,
    pub bmr_day: f32,
    pub growth_day: f32,
}

impl Body {
    pub fn of(age_years: f64, sex: Sex) -> Self {
        let mass_kg = body_mass_kg(age_years, sex);
        Self { mass_kg, bmr_day: bmr_kcal_day(mass_kg, age_years, sex), growth_day: growth_kcal_day(age_years) }
    }

    /// Besoin de référence (kcal/j) : celui qui fixe l'échelle de la faim.
    pub fn reference_day(&self) -> f32 {
        self.bmr_day * REFERENCE_PAL + self.growth_day
    }

    /// Points de nourriture par unité de faim : deux jours de son besoin.
    pub fn scale(&self) -> f32 {
        (2.0 * f64::from(self.reference_day()) / KCAL_PER_POINT) as f32
    }

    /// Dépense d'une heure (kcal). `load_kg` : ce qu'on porte (nourrisson,
    /// viande), qui alourdit la marche ; `fever` : gravité de l'infection la
    /// plus grave.
    pub fn hourly_kcal(&self, activity: Activity, felt_c: f64, load_kg: f32, fever: f32) -> f32 {
        let mut factor = activity_factor(activity);
        if activity == Activity::Walking {
            factor *= 1.0 + load_kg / self.mass_kg;
        }
        factor += cold_factor(felt_c) + FEVER_FACTOR * fever;
        (self.bmr_day * factor + self.growth_day) / 24.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Repère de Pontzer et al. 2012 : une journée hadza type (8 h de sommeil,
    /// ~1,5 h de marche pour une femme, ~3 h pour un homme, le reste entre
    /// cueillette ou chasse et repos) doit tomber dans l'ordre de grandeur de
    /// la dépense mesurée (femmes 1 877, hommes 2 649 kcal/j).
    #[test]
    fn une_journee_hadza_coute_ce_que_mesure_pontzer() {
        let day = |sex, walk: f32, work: f32, work_activity| {
            let b = Body::of(30.0, sex);
            let rest = 24.0 - 8.0 - walk - work;
            8.0 * b.hourly_kcal(Activity::Sleeping, 20.0, 0.0, 0.0)
                + walk * b.hourly_kcal(Activity::Walking, 20.0, 0.0, 0.0)
                + work * b.hourly_kcal(work_activity, 20.0, 0.0, 0.0)
                + rest * b.hourly_kcal(Activity::Idle, 20.0, 0.0, 0.0)
        };
        let woman = day(Sex::Female, 1.5, 3.0, Activity::Eating);
        let man = day(Sex::Male, 3.0, 3.0, Activity::Hunting);
        assert!((1_500.0..2_300.0).contains(&woman), "femme : {woman:.0} kcal/j");
        assert!((2_100.0..3_300.0).contains(&man), "homme : {man:.0} kcal/j");
    }

    #[test]
    fn un_enfant_coute_bien_moins_qu_un_adulte() {
        let toddler = Body::of(1.5, Sex::Female).reference_day();
        let adult = Body::of(30.0, Sex::Male).reference_day();
        assert!((600.0..1_100.0).contains(&toddler), "1,5 an : {toddler:.0} kcal/j");
        assert!(adult > 2.0 * toddler);
    }

    #[test]
    fn le_froid_la_charge_et_la_fievre_coutent() {
        let b = Body::of(30.0, Sex::Female);
        let base = b.hourly_kcal(Activity::Walking, 20.0, 0.0, 0.0);
        assert!(b.hourly_kcal(Activity::Walking, -20.0, 0.0, 0.0) > base);
        assert!(b.hourly_kcal(Activity::Walking, 20.0, 9.0, 0.0) > base);
        assert!(b.hourly_kcal(Activity::Walking, 20.0, 0.0, 1.0) > base);
    }
}
