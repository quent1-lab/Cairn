//! Couche 7 du pipeline : la géologie — type de roche et gisements.
//!
//! Indépendante du climat et de l'hydrologie (BRIEF §2.2) : elle ne dépend
//! que de la position et de l'altitude. Point à point, comme le climat.
//!
//! **Contrainte de conception majeure** (BRIEF §2.2) : cuivre et étain
//! doivent être *rarement co-localisés*, pour que l'âge du bronze force le
//! commerce longue distance. On le garantit structurellement : chaque métal a
//! sa propre *province* (bruit basse fréquence, seed indépendante), et une
//! tuile ne porte du cuivre que là où sa province cuivre **domine nettement**
//! sa province étain — avec une bande neutre entre les deux. Conséquence :
//! aucune tuile ne peut porter les deux, et leurs régions sont éloignées de
//! nature. Le bronze *oblige* la route de l'étain.

use cairn_core::WorldSeed;
use cairn_core::scale::km_to_tiles;

use crate::fbm::Fbm;

/// Salts des bruits géologiques (distincts de ceux du climat).
mod salt {
    pub const ROCK: u64 = 10;
    pub const COPPER: u64 = 11;
    pub const TIN: u64 = 12;
    pub const MINERALS: u64 = 13;
    pub const OUTCROP: u64 = 14;
}

/// Grande famille de roches. Détermine quels gisements sont possibles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RockType {
    /// Bassins sédimentaires : silex, argile.
    Sedimentary,
    /// Roches métamorphiques : or.
    Metamorphic,
    /// Roches ignées (volcanisme, intrusions) : cuivre, étain, obsidienne.
    Igneous,
}

/// Gisement présent sur une tuile. Tient dans les 4 bits `deposit` de la
/// tuile (BRIEF §2.3) : 8 valeurs sur 16 possibles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Deposit {
    None,
    Flint,
    Clay,
    Obsidian,
    Copper,
    Tin,
    Gold,
    Iron,
}

pub struct GeologyConfig {
    /// Longueur d'onde des provinces de roche, en tuiles.
    pub rock_wavelength: f64,
    /// Longueur d'onde des provinces métalliques (cuivre, étain).
    pub metal_wavelength: f64,
    /// Longueur d'onde des affleurements locaux (sparsité des gisements).
    pub outcrop_wavelength: f64,
    /// Un gisement n'apparaît que si le bruit d'affleurement dépasse ce
    /// seuil — règle la densité globale.
    pub outcrop_threshold: f64,
    /// Domination de province requise pour qu'un métal l'emporte sur l'autre.
    /// La bande neutre entre cuivre et étain fait cette largeur en valeur de
    /// bruit — c'est elle qui les sépare géographiquement.
    pub metal_margin: f64,
    /// Valeur de province minimale pour qu'un métal apparaisse : plus elle est
    /// haute, plus le métal se concentre près des pics de sa province (donc
    /// loin de l'autre métal), et plus il est rare.
    pub metal_presence: f64,
}

impl Default for GeologyConfig {
    fn default() -> Self {
        Self {
            rock_wavelength: km_to_tiles(500.0),
            metal_wavelength: km_to_tiles(450.0),
            outcrop_wavelength: km_to_tiles(6.0),
            outcrop_threshold: 0.35,
            metal_margin: 0.15,
            metal_presence: 0.35,
        }
    }
}

pub struct Geology {
    rock: Fbm,
    copper: Fbm,
    tin: Fbm,
    minerals: Fbm,
    outcrop: Fbm,
    cfg: GeologyConfig,
}

impl Geology {
    pub fn new(seed: WorldSeed) -> Self {
        Self::with_config(seed, GeologyConfig::default())
    }

    pub fn with_config(seed: WorldSeed, cfg: GeologyConfig) -> Self {
        let fbm = |s: u64, wl: f64, oct: usize| Fbm::new(seed.derive(s), oct, 1.0 / wl);
        Self {
            rock: fbm(salt::ROCK, cfg.rock_wavelength, 3),
            copper: fbm(salt::COPPER, cfg.metal_wavelength, 3),
            tin: fbm(salt::TIN, cfg.metal_wavelength, 3),
            minerals: fbm(salt::MINERALS, cfg.metal_wavelength, 3),
            outcrop: fbm(salt::OUTCROP, cfg.outcrop_wavelength, 4),
            cfg,
        }
    }

    /// Type de roche en (x, y) — défini partout, y compris sous l'océan.
    pub fn rock_type(&self, x: i64, y: i64) -> RockType {
        let r = self.rock.get(x as f64, y as f64);
        if r < -0.1 {
            RockType::Sedimentary
        } else if r < 0.35 {
            RockType::Metamorphic
        } else {
            RockType::Igneous
        }
    }

    /// Gisement en (x, y). `elevation` sert de contexte (argile en bas-fond,
    /// obsidienne en volcan d'altitude) — fournie par l'appelant, pas
    /// recalculée. Les gisements n'existent que sur terre.
    pub fn deposit(&self, x: i64, y: i64, elevation: f64) -> Deposit {
        if elevation <= 0.0 {
            return Deposit::None;
        }
        let (xf, yf) = (x as f64, y as f64);
        // Un affleurement local est-il exposé ici ? Sinon, pas de gisement.
        if self.outcrop.get(xf, yf) < self.cfg.outcrop_threshold {
            return Deposit::None;
        }
        let rock = self.rock_type(x, y);

        // Ordre : les gisements rares et spécifiques d'abord, le premier qui
        // matche gagne.
        match rock {
            RockType::Igneous => {
                let (cu, sn) = (self.copper.get(xf, yf), self.tin.get(xf, yf));
                // Séparation cuivre/étain : chacun n'apparaît que près des
                // pics de sa province (`metal_presence`) ET là où elle domine
                // l'autre d'au moins `metal_margin`.
                let (p, m) = (self.cfg.metal_presence, self.cfg.metal_margin);
                if cu > p && cu - sn > m {
                    Deposit::Copper
                } else if sn > p && sn - cu > m {
                    Deposit::Tin
                } else if elevation > 0.55 && self.minerals.get(xf, yf) > 0.4 {
                    Deposit::Obsidian
                } else if self.minerals.get(xf, yf) > 0.45 {
                    Deposit::Iron
                } else {
                    Deposit::None
                }
            }
            RockType::Metamorphic => {
                if self.minerals.get(xf, yf) > 0.6 {
                    Deposit::Gold
                } else if self.minerals.get(xf, yf) > 0.45 {
                    Deposit::Iron
                } else {
                    Deposit::None
                }
            }
            RockType::Sedimentary => {
                if elevation < 0.1 {
                    Deposit::Clay
                } else {
                    Deposit::Flint
                }
            }
        }
    }
}

impl RockType {
    pub fn name(self) -> &'static str {
        match self {
            RockType::Sedimentary => "sédimentaire",
            RockType::Metamorphic => "métamorphique",
            RockType::Igneous => "ignée",
        }
    }
}

impl Deposit {
    pub fn name(self) -> &'static str {
        match self {
            Deposit::None => "aucun",
            Deposit::Flint => "silex",
            Deposit::Clay => "argile",
            Deposit::Obsidian => "obsidienne",
            Deposit::Copper => "cuivre",
            Deposit::Tin => "étain",
            Deposit::Gold => "or",
            Deposit::Iron => "fer",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministe() {
        let a = Geology::new(WorldSeed(42));
        let b = Geology::new(WorldSeed(42));
        for i in 0..500i64 {
            let (x, y, e) = (i * 811, i * 331 - 5000, 0.3);
            assert_eq!(a.deposit(x, y, e), b.deposit(x, y, e));
            assert_eq!(a.rock_type(x, y), b.rock_type(x, y));
        }
    }

    #[test]
    fn pas_de_gisement_en_mer() {
        let geo = Geology::new(WorldSeed(42));
        assert_eq!(geo.deposit(1000, 2000, -0.3), Deposit::None);
    }

    /// Le critère d'acceptation : cuivre et étain jamais sur la même tuile, et
    /// leurs régions séparées d'au moins ~50 km.
    #[test]
    fn cuivre_et_etain_non_co_localises() {
        let geo = Geology::new(WorldSeed(42));
        let step = km_to_tiles(5.0) as i64;
        let mut coppers = Vec::new();
        let mut tins = Vec::new();
        // Balayage d'une région ~2000 km, altitude ignée plausible.
        for i in 0..400i64 {
            for j in 0..400i64 {
                let (x, y) = (i * step - 200 * step, j * step - 200 * step);
                match geo.deposit(x, y, 0.4) {
                    Deposit::Copper => coppers.push((x, y)),
                    Deposit::Tin => tins.push((x, y)),
                    _ => {}
                }
            }
        }
        assert!(!coppers.is_empty(), "aucun cuivre dans l'échantillon");
        assert!(!tins.is_empty(), "aucun étain dans l'échantillon");

        // Distance minimale cuivre↔étain sur l'échantillon.
        let min_km = coppers
            .iter()
            .flat_map(|&(cx, cy)| {
                tins.iter().map(move |&(tx, ty)| {
                    let (dx, dy) = ((cx - tx) as f64, (cy - ty) as f64);
                    cairn_core::scale::tiles_to_km((dx * dx + dy * dy).sqrt())
                })
            })
            .fold(f64::INFINITY, f64::min);
        assert!(min_km > 50.0, "cuivre et étain trop proches : {min_km:.0} km");
    }
}
