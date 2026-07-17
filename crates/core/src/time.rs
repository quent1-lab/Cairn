//! Le temps de la simulation : tick fixe et calendrier dérivé.
//!
//! **1 tick = 1 heure de jeu** (BRIEF §8.2). À 20 ticks/s réels : 1 jour de
//! jeu ≈ 1,2 s, 1 an ≈ 7,2 min. L'année fait 360 jours (12 mois de 30 jours) :
//! le calendrier reste simple et les phases saisonnières tombent juste.
//!
//! Un tick est un **budget de temps** d'une heure, pas un « pas » : un agent
//! y avance le long de sa route (jusqu'à ~2 km) puis agit. Les actions plus
//! courtes qu'une heure (boire, manger) se résolvent à l'intérieur du tick.
//! C'est l'arbitrage du §10.1 du brief : la cadence est celle des besoins
//! physiologiques et du climat, la sous-résolution appartient aux agents.
//!
//! Convention de phase : l'année commence au **solstice d'hiver des bandes
//! « nord »** (celles où la latitude signée est positive) — voir
//! `signed_fraction` côté worldgen.

pub const TICKS_PER_DAY: u64 = 24;
pub const DAYS_PER_YEAR: u64 = 360;
pub const TICKS_PER_YEAR: u64 = TICKS_PER_DAY * DAYS_PER_YEAR;

/// Horloge de simulation : un simple compteur de ticks, dont tout le
/// calendrier se dérive. `Copy` : on la passe par valeur partout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SimTime {
    pub tick: u64,
}

impl SimTime {
    /// Heure du jour, 0–23.
    pub fn hour_of_day(self) -> u64 {
        self.tick % TICKS_PER_DAY
    }

    /// Jour de l'année, 0–359.
    pub fn day_of_year(self) -> u64 {
        (self.tick / TICKS_PER_DAY) % DAYS_PER_YEAR
    }

    /// Année écoulée depuis le début du monde.
    pub fn year(self) -> u64 {
        self.tick / TICKS_PER_YEAR
    }

    /// Phase de l'année dans [0, 1) : 0 = solstice d'hiver « nord ».
    pub fn year_phase(self) -> f64 {
        (self.tick % TICKS_PER_YEAR) as f64 / TICKS_PER_YEAR as f64
    }

    /// Nuit : de 20 h à 6 h. Sert au rythme veille/sommeil des agents.
    pub fn is_night(self) -> bool {
        let h = self.hour_of_day();
        !(6..20).contains(&h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendrier_coherent() {
        let t = SimTime { tick: TICKS_PER_YEAR + 24 * 45 + 13 };
        assert_eq!(t.year(), 1);
        assert_eq!(t.day_of_year(), 45);
        assert_eq!(t.hour_of_day(), 13);
        assert!(!t.is_night());
        assert!(SimTime { tick: 3 }.is_night());
        assert!(SimTime { tick: 21 }.is_night());
    }

    #[test]
    fn phase_annuelle_bornee() {
        for tick in [0, 1, TICKS_PER_YEAR - 1, TICKS_PER_YEAR, 7 * TICKS_PER_YEAR + 1234] {
            let p = SimTime { tick }.year_phase();
            assert!((0.0..1.0).contains(&p));
        }
        assert_eq!(SimTime { tick: TICKS_PER_YEAR / 2 }.year_phase(), 0.5);
    }
}
