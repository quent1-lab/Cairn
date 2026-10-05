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

    /// Latitude en degrés à `y` : 0 sur un équateur, ±90 sur un pôle (la même
    /// géométrie triangulaire que les saisons).
    pub fn latitude_deg(&self, y: i64) -> f64 {
        90.0 * self.latitude.signed_fraction(y)
    }

    /// Hauteur du soleil (degrés) au milieu de l'heure en cours, à `y`.
    pub fn sun_elevation_deg(&self, y: i64, time: SimTime) -> f64 {
        sun_elevation_deg(self.latitude_deg(y), time)
    }

    /// La lumière du jour, 0 (nuit noire) à 1 (plein jour), à `y` : voir
    /// [`daylight`]. Sans la lune, qui viendra plus tard.
    pub fn light(&self, y: i64, time: SimTime) -> f32 {
        daylight(self.sun_elevation_deg(y, time))
    }

    /// La végétation pousse-t-elle ce jour-là sur cette tuile ?
    pub fn grows(&self, tile: &Tile, y: i64, time: SimTime) -> bool {
        self.daily_mean(tile, y, time) > GROWTH_THRESHOLD_C
    }

    /// La saison **locale** à la latitude de `y`. `None` près de l'équateur,
    /// où l'amplitude saisonnière est trop faible pour qu'une saison veuille
    /// dire quoi que ce soit — un an y est plat, et la Chronique s'abstient
    /// alors d'en parler plutôt que d'inventer un hiver tropical.
    ///
    /// Purement descriptif : rien dans la simulation ne lit une `Season`
    /// (la physiologie et l'écologie lisent la **température**, pas une
    /// étiquette) — même discipline que `tech::Age`.
    pub fn season(&self, y: i64, time: SimTime) -> Option<Season> {
        let l = self.latitude.signed_fraction(y);
        if l.abs() < SEASONLESS_LATITUDE {
            return None;
        }
        // Phase locale : 0 = cœur de l'hiver *ici*. Les bandes australes
        // (ℓ < 0) sont en opposition de phase — même géométrie que
        // `seasonal_offset`, d'où le demi-tour d'année.
        let mut p = time.year_phase() + if l < 0.0 { 0.5 } else { 0.0 };
        p -= p.floor();
        Some(match p {
            p if p < 0.125 || p >= 0.875 => Season::Winter,
            p if p < 0.375 => Season::Spring,
            p if p < 0.625 => Season::Summer,
            _ => Season::Autumn,
        })
    }
}

/// Inclinaison de l'axe du monde : la déclinaison du soleil oscille entre
/// ±23,44° sur l'année (celle de la Terre).
pub const AXIAL_TILT_DEG: f64 = 23.44;
/// Fin du crépuscule civil : soleil à 6° sous l'horizon. Au-delà, il fait nuit
/// pour l'œil (convention astronomique).
pub const CIVIL_TWILIGHT_DEG: f64 = -6.0;

/// Déclinaison du soleil (degrés) : −23,44° au solstice d'hiver « nord »
/// (phase 0, comme les saisons), +23,44° six mois plus tard.
pub fn declination_deg(time: SimTime) -> f64 {
    -AXIAL_TILT_DEG * (std::f64::consts::TAU * time.year_phase()).cos()
}

/// Hauteur du soleil (degrés) à la latitude `lat_deg`, au milieu de l'heure en
/// cours, midi solaire à 12 h : sin h = sin φ sin δ + cos φ cos δ cos ω,
/// ω = 15° par heure depuis midi. La durée du jour en découle (cos ω₀ =
/// −tan φ tan δ) : nuits longues l'hiver et vers les pôles.
pub fn sun_elevation_deg(lat_deg: f64, time: SimTime) -> f64 {
    let (phi, delta) = (lat_deg.to_radians(), declination_deg(time).to_radians());
    let omega = (15.0 * (time.hour_of_day() as f64 + 0.5 - 12.0)).to_radians();
    (phi.sin() * delta.sin() + phi.cos() * delta.cos() * omega.cos()).asin().to_degrees()
}

/// Lumière pour l'œil selon la hauteur du soleil : nulle sous le crépuscule
/// civil (−6°), pleine à +6° (l'éclairement y dépasse déjà de loin ce qu'il
/// faut pour voir) ; linéaire entre les deux. Choix de forme.
pub fn daylight(elevation_deg: f64) -> f32 {
    ((elevation_deg - CIVIL_TWILIGHT_DEG) / (2.0 * -CIVIL_TWILIGHT_DEG)).clamp(0.0, 1.0) as f32
}

/// Latitude signée en deçà de laquelle on ne nomme pas de saison : l'amplitude
/// y vaut moins de ~3 °C sur l'année (voir [`SEASONAL_AMPLITUDE_C`]).
const SEASONLESS_LATITUDE: f64 = 0.15;

/// Les quatre saisons, pour dater un fait de la Chronique. Étiquette dérivée,
/// jamais une condition (voir [`Climate::season`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Season {
    Winter,
    Spring,
    Summer,
    Autumn,
}

impl Season {
    pub fn label(self) -> &'static str {
        match self {
            Season::Winter => "hiver",
            Season::Spring => "printemps",
            Season::Summer => "été",
            Season::Autumn => "automne",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_core::{TICKS_PER_YEAR, WorldSeed};
    use cairn_worldgen::WorldGen;

    /// Heures de jour (soleil au-dessus de l'horizon) d'un jour donné.
    fn day_hours(lat_deg: f64, day: u64) -> usize {
        (0..24).filter(|h| sun_elevation_deg(lat_deg, SimTime { tick: day * cairn_core::TICKS_PER_DAY + h }) > 0.0).count()
    }

    /// La durée du jour suit la latitude et la saison, comme sur Terre : 12 h
    /// toute l'année à l'équateur ; à 60°, ~6 h au solstice d'hiver et ~18 h
    /// au solstice d'été ; nuit polaire et soleil de minuit au-delà du cercle.
    #[test]
    fn la_duree_du_jour_suit_latitude_et_saison() {
        assert_eq!(day_hours(0.0, 0), 12);
        assert_eq!(day_hours(0.0, 180), 12);
        let (hiver, ete) = (day_hours(60.0, 0), day_hours(60.0, 180));
        assert!((5..=7).contains(&hiver), "60° en hiver : {hiver} h");
        assert!((17..=19).contains(&ete), "60° en été : {ete} h");
        assert_eq!(day_hours(80.0, 0), 0, "nuit polaire");
        assert_eq!(day_hours(80.0, 180), 24, "soleil de minuit");
        // Hémisphère opposé : saisons inversées.
        assert!(day_hours(-60.0, 0) > 15);
        assert_eq!(daylight(-10.0), 0.0);
        assert_eq!(daylight(10.0), 1.0);
    }

    fn setup() -> (Climate, Tile) {
        let wg = WorldGen::new(WorldSeed(42));
        let climate = Climate::new(wg.temperature.latitude());
        let mut chunk = crate::Chunk::generate(crate::ChunkCoord { x: 0, y: 0 }, &wg);
        (climate, *chunk.tile(0, 0, &wg))
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

    /// La saison nommée suit la température : quand `seasonal_offset` est au
    /// plus bas, c'est l'hiver — et les deux hémisphères restent opposés.
    #[test]
    fn la_saison_nommee_suit_la_temperature() {
        let (climate, _) = setup();
        let period = cairn_worldgen::DEFAULT_PLANET_PERIOD as i64;
        let (nord, sud) = (period / 4, -period / 4);
        let t0 = SimTime { tick: 0 };
        assert_eq!(climate.season(nord, t0), Some(Season::Winter));
        assert_eq!(climate.season(sud, t0), Some(Season::Summer));
        let t_mi = SimTime { tick: TICKS_PER_YEAR / 2 };
        assert_eq!(climate.season(nord, t_mi), Some(Season::Summer));
        assert_eq!(climate.season(sud, t_mi), Some(Season::Winter));
        // Le quart d'année après le solstice d'hiver : le printemps du nord.
        assert_eq!(climate.season(nord, SimTime { tick: TICKS_PER_YEAR / 4 }), Some(Season::Spring));
        // À l'équateur, on ne nomme rien.
        assert_eq!(climate.season(0, t0), None);
    }

    /// **Le banc `etincelle` doit connaître des jours sous zéro.** Il choisit
    /// son foyer entre 1 et 5 °C de moyenne annuelle « parce que c'est le froid
    /// qui pousse au feu » — mais le froid ne s'accumule qu'en dessous de
    /// `COLD_THRESHOLD_C = 0 °C` (`agent::Physiology::tick`). Si le ressenti ne
    /// croise jamais zéro à ce foyer, la prémisse du banc ne se déclenche
    /// jamais et sa validation ne prouve rien.
    ///
    /// Mesuré sur la run longue : le foyer *tempéré* de `chronicle` ne descend
    /// pas sous 2,96 °C en 7 616 jours, et le froid y vaut exactement 0,00 à
    /// tous les percentiles. D'où ce test sur le foyer *frais*.
    #[test]
    fn le_foyer_frais_d_etincelle_connait_le_gel() {
        use crate::scenario;
        use cairn_worldgen::Biome;
        let mut sim = crate::Sim::new(cairn_core::WorldSeed(42), 4096);
        let seed_point = (
            cairn_core::km_to_tiles(1500.0) as i64,
            cairn_core::km_to_tiles(2100.0) as i64,
        );
        let home = scenario::find_home_where(
            &mut sim,
            seed_point,
            1.0..=5.0,
            &[Biome::TemperateForest, Biome::Taiga, Biome::Grassland],
        )
        .expect("le banc etincelle doit trouver son foyer frais");
        let tile = sim.world.tile(home.0, home.1);
        let mut min = f64::INFINITY;
        let mut sous_zero = 0;
        for jour in 0..360u64 {
            let t = SimTime { tick: jour * cairn_core::TICKS_PER_DAY };
            let felt = sim.climate.instant(&tile, home.1, t);
            min = min.min(felt);
            if felt < crate::agent::COLD_THRESHOLD_C {
                sous_zero += 1;
            }
        }
        println!(
            "foyer etincelle ({}, {}) — ressenti minimal {min:.2} °C, {sous_zero} jours sous {} °C sur 360",
            home.0, home.1, crate::agent::COLD_THRESHOLD_C
        );
        assert!(
            sous_zero > 0,
            "le foyer « frais » ne gèle jamais (minimum {min:.2} °C) : le froid ne \
             s'accumulant qu'en dessous de {} °C, la prémisse du banc etincelle — \
             « c'est le froid qui pousse au feu » — ne se déclenche jamais",
            crate::agent::COLD_THRESHOLD_C
        );
    }
}
