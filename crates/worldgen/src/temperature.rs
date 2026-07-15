//! Couche 2 du pipeline : la température moyenne annuelle.
//!
//! Trois contributions (BRIEF §2.2) :
//! - la **latitude** : gradient équateur → pôles ;
//! - l'**altitude** : refroidissement adiabatique, ~6,5 °C perdus par km ;
//! - une **perturbation** basse fréquence pour que les isothermes ne soient
//!   pas des droites parfaites.
//!
//! Le monde étant infini, la latitude est **périodique** : y = 0 est un
//! équateur, y = ±période/2 des pôles, puis les bandes climatiques se
//! répètent. Le monde reste ainsi infini dans les deux axes, sans « bord ».

use cairn_core::WorldSeed;

use crate::fbm::Fbm;
use crate::latitude::Latitude;
use crate::salt;

pub struct TemperatureConfig {
    /// Température moyenne au niveau de la mer à l'équateur.
    pub equator_temp_c: f64,
    /// Température moyenne au niveau de la mer aux pôles.
    pub pole_temp_c: f64,
    /// Période du cycle équateur → pôle → équateur, en tuiles.
    pub planet_period: f64,
    /// Altitude réelle (en mètres) correspondant à une élévation de 1.0.
    pub max_elevation_m: f64,
    /// Gradient adiabatique : °C perdus par kilomètre d'altitude.
    pub lapse_rate_c_per_km: f64,
    /// Amplitude de la perturbation climatique locale.
    pub noise_amplitude_c: f64,
    pub noise_frequency: f64,
    pub noise_octaves: usize,
}

impl Default for TemperatureConfig {
    fn default() -> Self {
        Self {
            equator_temp_c: 30.0,
            pole_temp_c: -25.0,
            planet_period: 32_768.0,
            max_elevation_m: 4_000.0,
            lapse_rate_c_per_km: 6.5,
            noise_amplitude_c: 3.0,
            noise_frequency: 1.0 / 1024.0,
            noise_octaves: 2,
        }
    }
}

pub struct TemperatureField {
    variation: Fbm,
    latitude: Latitude,
    cfg: TemperatureConfig,
}

impl TemperatureField {
    pub fn new(seed: WorldSeed) -> Self {
        Self::with_config(seed, TemperatureConfig::default())
    }

    pub fn with_config(seed: WorldSeed, cfg: TemperatureConfig) -> Self {
        Self {
            variation: Fbm::new(
                seed.derive(salt::TEMPERATURE),
                cfg.noise_octaves,
                cfg.noise_frequency,
            ),
            latitude: Latitude::new(cfg.planet_period),
            cfg,
        }
    }

    /// Température moyenne annuelle en °C au point (x, y).
    ///
    /// `elevation` est l'élévation normalisée fournie par
    /// [`AltitudeField::elevation`](crate::AltitudeField::elevation) : la
    /// couche température ne recalcule pas l'altitude, elle la consomme —
    /// c'est le contrat du pipeline.
    pub fn mean_temperature(&self, x: i64, y: i64, elevation: f64) -> f64 {
        let cfg = &self.cfg;
        // Moyenne annuelle quadratique en fraction de latitude : colle aux
        // moyennes zonales terrestres (tropiques larges, chute rapide vers
        // les pôles) là où un cosinus refroidit trop les latitudes moyennes
        // (2,5 °C à 45° au lieu de ~12 °C) et couvre un tiers du monde de
        // glaciers.
        let lambda = self.latitude.fraction(y);
        let t_sea =
            cfg.equator_temp_c - (cfg.equator_temp_c - cfg.pole_temp_c) * lambda * lambda;
        // L'océan (élévation ≤ 0) reste à la température de surface.
        let alt_km = elevation.max(0.0) * cfg.max_elevation_m / 1000.0;
        let t = t_sea - alt_km * cfg.lapse_rate_c_per_km;
        t + self.variation.get(x as f64, y as f64) * cfg.noise_amplitude_c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministe() {
        let a = TemperatureField::new(WorldSeed(42));
        let b = TemperatureField::new(WorldSeed(42));
        for &(x, y, e) in &[(0, 0, 0.0), (5_000, -3_000, 0.5), (-40_000, 12_345, 1.0)] {
            assert_eq!(
                a.mean_temperature(x, y, e).to_bits(),
                b.mean_temperature(x, y, e).to_bits()
            );
        }
    }

    #[test]
    fn l_equateur_est_plus_chaud_que_les_poles() {
        let field = TemperatureField::new(WorldSeed(42));
        let pole_y = (TemperatureConfig::default().planet_period / 2.0) as i64;
        let equateur = field.mean_temperature(0, 0, 0.0);
        let pole = field.mean_temperature(0, pole_y, 0.0);
        // Écart théorique de 55 °C, le bruit (±3 °C) ne peut pas l'inverser.
        assert!(
            equateur - pole > 40.0,
            "équateur {equateur:.1} °C, pôle {pole:.1} °C"
        );
    }

    #[test]
    fn l_altitude_refroidit_au_taux_adiabatique() {
        let field = TemperatureField::new(WorldSeed(42));
        let cfg = TemperatureConfig::default();
        // Même (x, y) : la perturbation locale est identique et s'annule
        // dans la différence — il ne reste que l'effet adiabatique pur.
        let delta = field.mean_temperature(100, 200, 0.0) - field.mean_temperature(100, 200, 1.0);
        let attendu = cfg.max_elevation_m / 1000.0 * cfg.lapse_rate_c_per_km;
        assert!((delta - attendu).abs() < 1e-9, "delta {delta} ≠ {attendu}");
    }

    #[test]
    fn l_ocean_ignore_la_profondeur() {
        let field = TemperatureField::new(WorldSeed(42));
        let surface = field.mean_temperature(0, 0, 0.0);
        let abysse = field.mean_temperature(0, 0, -1.0);
        assert_eq!(surface.to_bits(), abysse.to_bits());
    }
}
