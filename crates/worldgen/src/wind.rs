//! Couche 3 du pipeline : le vent dominant.
//!
//! Circulation planétaire simplifiée, fonction de la seule latitude λ
//! (0 = équateur, 1 = pôle). L'enveloppe unique `-sin(3πλ)` reproduit les
//! trois régimes terrestres et leurs zones de calme :
//!
//! - λ ≈ 1/6 : **alizés** — vers l'ouest, convergeant vers l'équateur ;
//! - λ ≈ 1/2 : **vents d'ouest** — vers l'est, divergeant vers le pôle ;
//! - λ ≈ 5/6 : **vents polaires** — vers l'ouest, vers l'équateur ;
//! - λ = 0, 1/3, 2/3, 1 : **calmes** (pot au noir, latitudes des chevaux).
//!
//! Les calmes ne sont pas décoratifs : là où l'air ne circule pas,
//! l'humidité océanique n'est pas advectée vers les terres — les ceintures
//! désertiques subtropicales en découleront toutes seules.

use std::f64::consts::PI;

use crate::latitude::Latitude;

pub struct WindConfig {
    /// Période de latitude. Doit coïncider avec
    /// `TemperatureConfig::planet_period` — les deux valent
    /// [`crate::DEFAULT_PLANET_PERIOD`] par défaut.
    pub planet_period: f64,
    /// Amplitude de la composante est-ouest (magnitude max ≈ 1).
    pub zonal_strength: f64,
    /// Amplitude de la composante nord-sud.
    pub meridional_strength: f64,
}

impl Default for WindConfig {
    fn default() -> Self {
        Self {
            planet_period: crate::DEFAULT_PLANET_PERIOD,
            zonal_strength: 1.0,
            meridional_strength: 0.5,
        }
    }
}

pub struct WindField {
    latitude: Latitude,
    cfg: WindConfig,
}

impl WindField {
    pub fn new(cfg: WindConfig) -> Self {
        Self {
            latitude: Latitude::new(cfg.planet_period),
            cfg,
        }
    }

    /// Vent dominant (vx, vy) en tuiles par pas d'advection, magnitude ≲ 1.
    ///
    /// `x` est ignoré pour l'instant : la circulation est purement zonale.
    /// Il fait partie de la signature parce que les raffinements prévus
    /// (déviation par le relief, moussons) dépendront de la position.
    pub fn wind(&self, x: i64, y: i64) -> (f64, f64) {
        let _ = x;
        let lambda = self.latitude.fraction(y);
        let envelope = -(3.0 * PI * lambda).sin();
        // Même enveloppe pour les deux composantes : dans chaque régime,
        // flux zonal et flux méridien s'inversent ensemble.
        let vx = envelope * self.cfg.zonal_strength;
        let vy = envelope * self.cfg.meridional_strength * self.latitude.pole_direction(y);
        (vx, vy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field() -> WindField {
        WindField::new(WindConfig::default())
    }

    /// y correspondant à une fraction de latitude donnée (pôle vers +y).
    fn y_at(lambda: f64) -> i64 {
        (lambda * WindConfig::default().planet_period / 2.0) as i64
    }

    #[test]
    fn alizes_vers_l_ouest_et_l_equateur() {
        let (vx, vy) = field().wind(0, y_at(1.0 / 6.0));
        assert!(vx < -0.9, "vx = {vx}");
        assert!(vy < 0.0, "vy = {vy}"); // l'équateur est vers -y ici
        // Hémisphère miroir : la composante méridienne s'inverse.
        let (vx_s, vy_s) = field().wind(0, -y_at(1.0 / 6.0));
        assert!(vx_s < -0.9);
        assert!(vy_s > 0.0);
    }

    #[test]
    fn vents_d_ouest_vers_l_est_et_le_pole() {
        let (vx, vy) = field().wind(0, y_at(0.5));
        assert!(vx > 0.9, "vx = {vx}");
        assert!(vy > 0.0, "vy = {vy}");
    }

    #[test]
    fn calmes_a_l_equateur_et_aux_latitudes_des_chevaux() {
        for lambda in [0.0, 1.0 / 3.0, 2.0 / 3.0] {
            let (vx, vy) = field().wind(0, y_at(lambda));
            let magnitude = (vx * vx + vy * vy).sqrt();
            assert!(magnitude < 0.05, "λ = {lambda} : magnitude {magnitude}");
        }
    }
}
