//! Ce que rapporte chaque façon de se nourrir (MAR-6) : le registre, par
//! humain, des heures passées à chaque activité de subsistance et des kcal
//! qu'elle a acquises.
//!
//! Une activité se compte **trajets compris** : l'heure de marche vers la
//! tuile à cueillir ou vers le troupeau fait partie de ce que coûte la
//! cueillette ou la chasse. La chasse réunit la quête, la piste et
//! l'approche : une prise est le fruit de toute la chaîne. Les kcal sont
//! celles **acquises** (la bête entière, partagée ou non ; la cueillette
//! mangée), pas celles mangées par l'acquéreur.
//!
//! MAR-6a0 : simple observation, rien ne la lit dans la simulation.

use crate::agent::TaskKind;

/// Les activités de subsistance dont on compte le rendement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pursuit {
    Gather = 0,
    Hunt = 1,
}

pub const PURSUITS: usize = 2;

impl Pursuit {
    /// L'activité de subsistance que sert une tâche, s'il y en a une.
    pub fn of(kind: TaskKind) -> Option<Self> {
        match kind {
            TaskKind::Forage => Some(Self::Gather),
            TaskKind::SeekGame | TaskKind::Track | TaskKind::Hunt => Some(Self::Hunt),
            _ => None,
        }
    }
}

/// Le registre d'un humain : heures passées, kcal acquises et kcal dépensées
/// au-delà du repos, par activité, depuis sa naissance (ou son arrivée pour un
/// fondateur).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Yields {
    pub hours: [f64; PURSUITS],
    pub kcal: [f64; PURSUITS],
    pub cost: [f64; PURSUITS],
}

impl Yields {
    /// Une heure passée à cette activité, qui a coûté `extra_kcal` de plus que
    /// le repos.
    pub fn spend_hour(&mut self, p: Pursuit, extra_kcal: f64) {
        self.hours[p as usize] += 1.0;
        self.cost[p as usize] += extra_kcal;
    }

    /// Des kcal acquises par cette activité.
    pub fn gain(&mut self, p: Pursuit, kcal: f64) {
        self.kcal[p as usize] += kcal.max(0.0);
    }
}
