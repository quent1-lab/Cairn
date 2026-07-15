//! La latitude d'un monde infini.
//!
//! Le monde n'a pas de bord : la latitude est **périodique** le long de y.
//! y = 0 est un équateur, y = ±période/2 des pôles, puis les bandes
//! climatiques se répètent en miroir. Température et vent dérivent tous
//! deux de ce même paramètre — ce module est leur source commune.

use std::f64::consts::PI;

#[derive(Debug, Clone, Copy)]
pub struct Latitude {
    period: f64,
}

impl Latitude {
    /// `period` : longueur en tuiles du cycle équateur → pôle → équateur.
    pub fn new(period: f64) -> Self {
        Self { period }
    }

    /// Position dans le demi-cycle : 0.0 sur un équateur, 1.0 sur un pôle.
    pub fn fraction(&self, y: i64) -> f64 {
        // rem_euclid (et non %) : résultat toujours positif, y compris
        // pour y négatif — le monde est symétrique autour de l'équateur.
        let u = (y as f64 / (self.period / 2.0)).rem_euclid(2.0);
        1.0 - (1.0 - u).abs()
    }

    /// Direction du pôle le plus proche le long de y : +1.0 ou -1.0.
    pub fn pole_direction(&self, y: i64) -> f64 {
        let u = (y as f64 / (self.period / 2.0)).rem_euclid(2.0);
        if u < 1.0 { 1.0 } else { -1.0 }
    }

    /// Cosinus de latitude : 1.0 à l'équateur, -1.0 au pôle. Pratique pour
    /// les gradients (température).
    pub fn cosine(&self, y: i64) -> f64 {
        (PI * self.fraction(y)).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equateur_poles_et_periodicite() {
        let lat = Latitude::new(1000.0);
        assert_eq!(lat.fraction(0), 0.0);
        assert_eq!(lat.fraction(500), 1.0);
        assert_eq!(lat.fraction(-500), 1.0);
        assert_eq!(lat.fraction(1000), 0.0);
        assert_eq!(lat.fraction(250), lat.fraction(-250));
        assert_eq!(lat.fraction(250), lat.fraction(1250));
    }

    #[test]
    fn direction_du_pole() {
        let lat = Latitude::new(1000.0);
        // Entre l'équateur 0 et le pôle +500, le pôle est vers +y.
        assert_eq!(lat.pole_direction(250), 1.0);
        // De l'autre côté de l'équateur, il est vers -y.
        assert_eq!(lat.pole_direction(-250), -1.0);
        // Au-delà du pôle, on redescend vers l'équateur suivant.
        assert_eq!(lat.pole_direction(750), -1.0);
    }
}
