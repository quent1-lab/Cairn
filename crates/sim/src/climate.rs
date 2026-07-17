//! La température **instantanée** : la moyenne annuelle du worldgen, modulée
//! par la saison et l'heure (BRIEF §2.5). C'est la couche temporelle du
//! climat — pure fonction de (tuile, y, tick), déterministe.
//!
//! - **Saison** : décalage `-ℓ · A · cos(2π·phase)` où ℓ est la latitude
//!   signée. À phase 0 (solstice d'hiver « nord »), les bandes ℓ > 0 sont en
//!   hiver pendant que les bandes ℓ < 0 sont en été — hémisphères opposés,
//!   continuité assurée aux pôles comme aux équateurs.
//! - **Jour/nuit** : sinusoïde de pic à 15 h, creux à 3 h, dont l'amplitude
//!   croît quand l'air est sec — la vapeur d'eau amortit le refroidissement
//!   nocturne, d'où les nuits glaciales du désert.
//!
//! La pression climatique est le principal moteur exogène du jeu : c'est
//! cette température-là (pas la moyenne annuelle) que la physiologie subit
//! et qui gèle la croissance végétale l'hiver.

use cairn_core::SimTime;
use cairn_worldgen::latitude::Latitude;

use crate::tile::Tile;

/// Amplitude saisonnière au pôle (± °C autour de la moyenne annuelle).
/// L'amplitude réelle croît linéairement avec |latitude| : nulle à
/// l'équateur (pas de saisons), maximale au pôle.
pub const SEASONAL_AMPLITUDE_C: f64 = 18.0;

/// Amplitude jour/nuit en air saturé (± °C)…
pub const DIURNAL_HUMID_C: f64 = 2.0;
/// …et supplément en air parfaitement sec (les déserts oscillent à ±10 °C).
pub const DIURNAL_DRY_BONUS_C: f64 = 8.0;

/// Seuil de croissance végétale : en dessous de la température **du jour**
/// (sans cycle diurne), la biomasse ne pousse pas — l'hiver suspend la
/// repousse, et c'est lui qui rend l'automne nourricier et février cruel.
pub const GROWTH_THRESHOLD_C: f64 = 5.0;

#[derive(Clone, Copy)]
pub struct Climate {
    latitude: Latitude,
}

impl Climate {
    /// Construit le climat depuis la latitude **du worldgen** — la même
    /// géométrie que la couche de température moyenne, pas une copie locale
    /// qui pourrait diverger.
    pub fn new(latitude: Latitude) -> Self {
        Self { latitude }
    }

    /// Décalage saisonnier en °C au jour donné, à la latitude de `y`.
    pub fn seasonal_offset(&self, y: i64, time: SimTime) -> f64 {
        let l = self.latitude.signed_fraction(y);
        -l * SEASONAL_AMPLITUDE_C * (std::f64::consts::TAU * time.year_phase()).cos()
    }

    /// Température moyenne **du jour** (saison comprise, cycle diurne exclu).
    /// C'est elle que consulte l'écologie.
    pub fn daily_mean(&self, tile: &Tile, y: i64, time: SimTime) -> f64 {
        tile.temperature as f64 + self.seasonal_offset(y, time)
    }

    /// Température de l'air ressentie sur la tuile à cette heure précise.
    /// C'est elle que subit la physiologie.
    pub fn instant(&self, tile: &Tile, y: i64, time: SimTime) -> f64 {
        let dryness = 1.0 - tile.humidity as f64 / 255.0;
        let amplitude = DIURNAL_HUMID_C + DIURNAL_DRY_BONUS_C * dryness;
        let hour = time.hour_of_day() as f64;
        let diurnal = amplitude * (std::f64::consts::TAU * (hour - 15.0) / 24.0).cos();
        self.daily_mean(tile, y, time) + diurnal
    }

    /// La végétation pousse-t-elle ce jour-là sur cette tuile ?
    pub fn grows(&self, tile: &Tile, y: i64, time: SimTime) -> bool {
        self.daily_mean(tile, y, time) > GROWTH_THRESHOLD_C
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_core::{TICKS_PER_YEAR, WorldSeed};
    use cairn_worldgen::WorldGen;

    fn setup() -> (Climate, Tile) {
        let wg = WorldGen::new(WorldSeed(42));
        let climate = Climate::new(wg.temperature.latitude());
        let chunk = crate::Chunk::generate(crate::ChunkCoord { x: 0, y: 0 }, &wg);
        (climate, *chunk.tile(0, 0))
    }

    #[test]
    fn ete_plus_chaud_que_l_hiver_en_bande_nord() {
        let (climate, tile) = setup();
        // Mi-latitude de la bande « nord » : y = période/4.
        let y = (cairn_worldgen::DEFAULT_PLANET_PERIOD / 4.0) as i64;
        let hiver = climate.daily_mean(&tile, y, SimTime { tick: 0 });
        let ete = climate.daily_mean(&tile, y, SimTime { tick: TICKS_PER_YEAR / 2 });
        assert!(
            ete > hiver + 10.0,
            "été {ete:.1} °C vs hiver {hiver:.1} °C : saisons trop plates"
        );
    }

    #[test]
    fn les_bandes_voisines_ont_des_saisons_opposees() {
        let (climate, _) = setup();
        let period = cairn_worldgen::DEFAULT_PLANET_PERIOD as i64;
        let t = SimTime { tick: 0 };
        let nord = climate.seasonal_offset(period / 4, t);
        let sud = climate.seasonal_offset(-period / 4, t);
        assert!(nord < -5.0, "bande nord en hiver à phase 0 (offset {nord:.1})");
        assert!(sud > 5.0, "bande sud en été à phase 0 (offset {sud:.1})");
    }

    #[test]
    fn l_apres_midi_est_plus_chaud_que_l_aube() {
        let (climate, tile) = setup();
        let aube = climate.instant(&tile, 1000, SimTime { tick: 3 });
        let apres_midi = climate.instant(&tile, 1000, SimTime { tick: 15 });
        assert!(apres_midi > aube + 2.0);
    }

    #[test]
    fn pas_d_ecart_saisonnier_a_l_equateur() {
        let (climate, _) = setup();
        assert_eq!(climate.seasonal_offset(0, SimTime { tick: 0 }), 0.0);
    }
}
