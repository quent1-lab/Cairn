//! La tuile : l'unité de la grille du monde (2 m de côté).
//!
//! Elle matérialise le baseline procédural (biome, roche, gisement, altitude,
//! température) et porte les champs **mutables** que la simulation fera
//! évoluer (BRIEF §2.3). En Phase 1 ces champs mutables sont initialisés à
//! leur valeur de base et rien ne les modifie encore.

use cairn_worldgen::{Biome, Deposit, RockType};

/// Drapeaux binaires de la tuile (BRIEF §2.3). D'autres viendront : rivière
/// (attend l'hydrologie inter-chunks), brûlé, cultivé, sacré…
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TileFlags(pub u8);

impl TileFlags {
    /// Recouverte d'eau libre (océan).
    pub const WATER: u8 = 1 << 0;
    /// Eau côtière peu profonde.
    pub const COAST: u8 = 1 << 1;
    /// Surface d'eau gelée (mer prise par le froid).
    pub const FROZEN: u8 = 1 << 2;

    pub fn has(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    pub fn set(&mut self, flag: u8) {
        self.0 |= flag;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tile {
    // — Baseline (dérivé du worldgen, régénérable) —
    pub biome: Biome,
    pub rock: RockType,
    pub deposit: Deposit,
    /// Altitude normalisée : [-1, 0] eau, (0, 1] terres.
    pub elevation: f32,
    /// Température moyenne annuelle, °C.
    pub temperature: f32,
    /// Humidité de l'air, quantifiée 0–255 (depuis [0, 1]). Calculée
    /// gratuitement à la génération du chunk ; sert les biomes et l'affichage.
    pub humidity: u8,

    // — Mutable (évoluera en Phase 2 ; ici valeur de base) —
    /// Fertilité du sol, 0–255. Dégradable par la surexploitation.
    pub soil_fertility: u8,
    /// Biomasse végétale courante, 0–255. Consommée puis repousse.
    pub biomass: u8,

    pub flags: TileFlags,
}

impl Tile {
    pub fn is_water(self) -> bool {
        self.flags.has(TileFlags::WATER)
    }
}

/// Fertilité de base d'un biome (0–255), avant toute dégradation. C'est le
/// plafond que la simulation fera baisser sous la surexploitation.
pub fn baseline_fertility(biome: Biome) -> u8 {
    use Biome::*;
    match biome {
        Ocean | Coast | Glacier => 0,
        HotDesert | ColdDesert => 15,
        Tundra => 35,
        Taiga => 70,
        Steppe => 95,
        Savanna => 110,
        Grassland => 150,
        TemperateForest => 160,
        TropicalForest => 190,
    }
}

/// Capacité de charge de biomasse d'un biome (0–255) : le `K` de la croissance
/// logistique à venir (BRIEF §2.4).
pub fn baseline_biomass(biome: Biome) -> u8 {
    use Biome::*;
    match biome {
        Ocean | Coast | Glacier => 0,
        HotDesert | ColdDesert => 12,
        Tundra => 25,
        Steppe => 70,
        Savanna => 90,
        Grassland => 110,
        Taiga => 150,
        TemperateForest => 210,
        TropicalForest => 255,
    }
}
