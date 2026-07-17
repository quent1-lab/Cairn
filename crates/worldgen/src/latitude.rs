//! La latitude d'un monde infini.
//!
//! Le monde n'a pas de bord : la latitude est **périodique** le long de y.
//! y = 0 est un équateur, y = ±période/2 des pôles, puis les bandes
//! climatiques se répètent en miroir. Température et vent dérivent tous
//! deux de ce même paramètre — ce module est leur source commune.

#[derive(Debug, Clone, Copy)]
pub struct Latitude {
    period: f64,
}

impl Latitude {
    /// `period` : longueur en tuiles du cycle équateur → pôle → équateur.
    pub fn new(period: f64) -> Self {
        Self { period }
    }

    /// Position du demi-cycle, dans [0, 2) : 0 = équateur montant, 1 = pôle,
    /// 2⁻ = équateur suivant.
    ///
    /// La réduction modulo est faite en **entier i64** quand la période est
    /// entière (le cas réel) : `y as f64` perdrait des bits au-delà de 2⁵³
    /// et ferait dériver le climat aux très grandes coordonnées. Le reste du
    /// worldgen échantillonne le bruit en `x as f64` et partage donc cette
    /// borne de ~2⁵³ tuiles ; au moins la latitude, elle, reste exacte sur
    /// tout l'axe i64.
    fn half_cycle(&self, y: i64) -> f64 {
        let half = self.period / 2.0;
        let reduced = if self.period.fract() == 0.0 && self.period >= 1.0 {
            (y.rem_euclid(self.period as i64)) as f64
        } else {
            // rem_euclid (et non %) : toujours positif, monde symétrique.
            (y as f64).rem_euclid(self.period)
        };
        reduced / half
    }

    /// Position dans le demi-cycle : 0.0 sur un équateur, 1.0 sur un pôle.
    pub fn fraction(&self, y: i64) -> f64 {
        let u = self.half_cycle(y);
        1.0 - (1.0 - u).abs()
    }

    /// Latitude **signée** dans [-1, 1] : onde triangulaire de période
    /// **double** de celle de [`fraction`](Self::fraction), qui alterne
    /// bandes « nord » (pôle à +1) et « sud » (pôle à -1).
    ///
    /// C'est la donnée dont les **saisons** ont besoin : le décalage
    /// saisonnier vaut `-ℓ·A·cos(2π·phase)`, donc deux bandes voisines (de
    /// part et d'autre d'un équateur) ont des saisons **opposées** — comme
    /// les hémisphères terrestres — tandis que la fonction reste continue en
    /// franchissant un pôle (pas de saut d'hiver à été au point le plus
    /// froid du monde). Invariant : `|signed_fraction(y)| == fraction(y)`.
    pub fn signed_fraction(&self, y: i64) -> f64 {
        let full = self.period * 2.0;
        // Même précaution que half_cycle : réduction modulo en i64 quand la
        // période est entière, pour rester exact sur tout l'axe.
        let reduced = if self.period.fract() == 0.0 && self.period >= 1.0 {
            (y.rem_euclid(2 * self.period as i64)) as f64
        } else {
            (y as f64).rem_euclid(full)
        };
        let t = reduced / full; // [0, 1) sur le cycle complet à deux bandes
        if t < 0.25 {
            4.0 * t
        } else if t < 0.75 {
            2.0 - 4.0 * t
        } else {
            4.0 * t - 4.0
        }
    }

    /// Direction du pôle le plus proche le long de y : +1.0 ou -1.0.
    pub fn pole_direction(&self, y: i64) -> f64 {
        if self.half_cycle(y) < 1.0 { 1.0 } else { -1.0 }
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

    #[test]
    fn latitude_signee_alterne_les_bandes_et_reste_continue() {
        let lat = Latitude::new(1000.0);
        // Pôle de la bande « nord » : +1 ; pôle de la bande suivante : -1.
        assert_eq!(lat.signed_fraction(500), 1.0);
        assert_eq!(lat.signed_fraction(1500), -1.0);
        // Nulle sur tous les équateurs.
        assert_eq!(lat.signed_fraction(0), 0.0);
        assert_eq!(lat.signed_fraction(1000), 0.0);
        assert_eq!(lat.signed_fraction(2000), 0.0);
        // |signée| == fraction, partout.
        for y in [-1500, -250, 0, 137, 500, 999, 1001, 1750, 3250] {
            assert!(
                (lat.signed_fraction(y).abs() - lat.fraction(y)).abs() < 1e-12,
                "désaccord en y={y}"
            );
        }
        // Continue en franchissant un pôle : pas de saut de signe.
        assert!((lat.signed_fraction(499) - lat.signed_fraction(501)).abs() < 0.01);
        assert!((lat.signed_fraction(1499) - lat.signed_fraction(1501)).abs() < 0.01);
    }

    #[test]
    fn exact_au_dela_de_deux_puissance_53() {
        // Au-delà de 2^53, (y as f64) arrondirait et casserait la
        // périodicité ; la réduction en i64 la préserve exactement.
        let lat = Latitude::new(1000.0);
        let enorme = 1_000 * 10_000_000_000_000 + 251; // 10^16 + 251, > 2^53
        assert_eq!(lat.fraction(enorme), lat.fraction(251));
        assert_eq!(lat.fraction(enorme), lat.fraction(1251));
    }
}
