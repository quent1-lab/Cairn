//! Télémétrie de l'alimentation humaine (feature `food-stats`, défaut D10) :
//! d'où vient chaque point de faim retiré, et ce que la cueillette prélève sur
//! la flore. Hors de la feature, les fonctions sont vides et s'évaporent.
//!
//! Les compteurs sont globaux au processus (un banc = une simulation) et lus
//! puis remis à zéro par le banc (`take`). Ils n'influencent rien : aucune
//! lecture depuis la simulation, donc aucun effet sur le déterminisme.

/// Les voies par lesquelles la faim d'un humain baisse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Cueillette : biomasse prélevée sur une tuile.
    Forage = 0,
    /// Chasse : la part mangée sur place par le chasseur.
    HuntEaten = 1,
    /// Chasse : le surplus porté vers le stock du clan.
    HuntCarried = 2,
    /// Chasse : le surplus perdu (chasseur sans clan, ou déjà repu).
    HuntWasted = 3,
    /// Puisage dans le stock du clan.
    Stock = 4,
    /// Prélèvement sur le cheptel, versé au stock.
    Herd = 5,
    /// Lait maternel (transfert mère → nourrisson).
    Milk = 6,
    /// Part de viande reçue d'un chasseur, hors clan ou non.
    Shared = 7,
    /// Viande portée, mangée plus tard par celui qui la porte.
    Carried = 8,
}

/// Compteurs d'événements (et non de faim).
#[derive(Clone, Copy, Debug)]
pub enum Event {
    /// Unités de biomasse retirées des tuiles par la cueillette.
    BiomassTaken = 0,
    /// Heures de cueillette.
    ForageHours = 1,
    /// Heures de cueillette qui n'ont pas trouvé ce qu'elles voulaient.
    ForageShort = 2,
    /// Bêtes tuées par des humains.
    Kills = 3,
}

#[cfg(feature = "food-stats")]
mod imp {
    use std::sync::atomic::{AtomicU64, Ordering};
    /// Faim en millionièmes de point.
    pub static HUNGER: [AtomicU64; 9] = [const { AtomicU64::new(0) }; 9];
    pub static EVENTS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];

    pub fn fed(source: super::Source, hunger: f32) {
        let v = (f64::from(hunger.max(0.0)) * 1e6) as u64;
        HUNGER[source as usize].fetch_add(v, Ordering::Relaxed);
    }
    pub fn event(e: super::Event, n: u64) {
        EVENTS[e as usize].fetch_add(n, Ordering::Relaxed);
    }
    pub fn take() -> ([f64; 9], [u64; 4]) {
        let mut h = [0.0; 9];
        for (i, a) in HUNGER.iter().enumerate() {
            h[i] = a.swap(0, Ordering::Relaxed) as f64 / 1e6;
        }
        let mut e = [0; 4];
        for (i, a) in EVENTS.iter().enumerate() {
            e[i] = a.swap(0, Ordering::Relaxed);
        }
        (h, e)
    }
}

/// Enregistre `hunger` points de faim retirés par la voie `source`.
#[inline]
pub fn fed(_source: Source, _hunger: f32) {
    #[cfg(feature = "food-stats")]
    imp::fed(_source, _hunger);
}

/// Enregistre `n` occurrences de `event`.
#[inline]
pub fn event(_event: Event, _n: u64) {
    #[cfg(feature = "food-stats")]
    imp::event(_event, _n);
}

/// Lit et remet à zéro : (faim retirée par source, événements).
#[cfg(feature = "food-stats")]
pub fn take() -> ([f64; 9], [u64; 4]) {
    imp::take()
}
