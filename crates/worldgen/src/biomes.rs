//! Couche 6 du pipeline : les biomes, par classification de Whittaker.
//!
//! Le biome d'une tuile est entièrement déterminé par le triplet
//! (élévation, température moyenne, humidité) — aucune couche de bruit
//! propre, aucune règle de placement. Les déserts sont là où l'air arrive
//! sec, la toundra là où il fait froid, y compris en altitude : les étages
//! alpins sur les montagnes tropicales sortent tout seuls du
//! refroidissement adiabatique.
//!
//! Diagramme simplifié (température en abscisse, humidité en ordonnée) :
//!
//! ```text
//!  h   froid ◄─────────────────────────► chaud
//! 1.0 ┌─────────┬───────┬──────────┬──────────┐
//!     │         │       │  forêt   │  forêt   │
//!     │ glacier │toundra│ tempérée │tropicale │
//! 0.6 │         │       ├──────────┤          │
//!     │         ├───────┤ prairie  ├──────────┤
//! 0.4 │         │       ├──────────┤          │
//!     │         │ taïga │  steppe  │  savane  │
//! 0.2 │         │       ├──────────┼──────────┤
//!     │         ├───────┤  désert  │  désert  │
//! 0.0 └─────────┴───────┴──froid───┴──chaud───┘
//!        -12°C    -4°C      4°C    19°C
//! ```

/// Biomes du monde. `#[repr(u8)]` : destiné à être packé dans les 5 bits
/// `terrain` de la tuile (BRIEF §2.3). Mangrove et marais viendront avec
/// l'hydrologie (ils dépendent de la proximité de l'eau, pas seulement du
/// climat).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Biome {
    Ocean,
    /// Eaux peu profondes (plateau continental).
    Coast,
    Glacier,
    Tundra,
    Taiga,
    ColdDesert,
    Steppe,
    Grassland,
    TemperateForest,
    HotDesert,
    Savanna,
    TropicalForest,
}

impl Biome {
    /// Classification de Whittaker simplifiée (voir le diagramme du module).
    pub fn classify(elevation: f64, temp_c: f64, humidity: f64) -> Biome {
        use Biome::*;

        if elevation <= 0.0 {
            return if elevation > -0.08 { Coast } else { Ocean };
        }
        match (temp_c, humidity) {
            (t, _) if t < -12.0 => Glacier,
            (t, _) if t < -4.0 => Tundra,
            (t, h) if t < 4.0 => {
                if h < 0.25 {
                    ColdDesert
                } else {
                    Taiga
                }
            }
            (t, h) if t < 19.0 => match h {
                h if h < 0.22 => ColdDesert,
                h if h < 0.42 => Steppe,
                h if h < 0.62 => Grassland,
                _ => TemperateForest,
            },
            (_, h) => match h {
                h if h < 0.25 => HotDesert,
                h if h < 0.55 => Savanna,
                _ => TropicalForest,
            },
        }
    }

    /// Nom d'affichage, pour les stats et la future UI.
    pub fn name(self) -> &'static str {
        match self {
            Biome::Ocean => "océan",
            Biome::Coast => "côte",
            Biome::Glacier => "glacier",
            Biome::Tundra => "toundra",
            Biome::Taiga => "taïga",
            Biome::ColdDesert => "désert froid",
            Biome::Steppe => "steppe",
            Biome::Grassland => "prairie",
            Biome::TemperateForest => "forêt tempérée",
            Biome::HotDesert => "désert chaud",
            Biome::Savanna => "savane",
            Biome::TropicalForest => "forêt tropicale",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cas_de_reference() {
        // (élévation, °C, humidité) → biome attendu
        let cas = [
            ((-0.5, 10.0, 0.9), Biome::Ocean),
            ((-0.02, 10.0, 0.9), Biome::Coast),
            ((0.8, -20.0, 0.5), Biome::Glacier),
            ((0.3, -8.0, 0.4), Biome::Tundra),
            ((0.2, 0.0, 0.5), Biome::Taiga),
            ((0.2, 10.0, 0.1), Biome::ColdDesert),
            ((0.2, 10.0, 0.3), Biome::Steppe),
            ((0.1, 12.0, 0.5), Biome::Grassland),
            ((0.1, 12.0, 0.8), Biome::TemperateForest),
            ((0.1, 28.0, 0.1), Biome::HotDesert),
            ((0.1, 25.0, 0.35), Biome::Savanna),
            ((0.1, 26.0, 0.8), Biome::TropicalForest),
        ];
        for ((e, t, h), attendu) in cas {
            assert_eq!(Biome::classify(e, t, h), attendu, "({e}, {t}, {h})");
        }
    }

    #[test]
    fn l_altitude_cree_des_etages_alpins() {
        // Même climat de base tropical : en montant, la température chute
        // (fournie ici déjà refroidie) et le biome doit finir en glacier.
        let en_bas = Biome::classify(0.1, 26.0, 0.7);
        let en_haut = Biome::classify(0.95, -15.0, 0.7);
        assert_eq!(en_bas, Biome::TropicalForest);
        assert_eq!(en_haut, Biome::Glacier);
    }
}
