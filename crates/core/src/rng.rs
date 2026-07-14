//! RNG déterministe maison.
//!
//! Pourquoi pas le crate `rand` ? Le déterminisme bit-à-bit inter-exécutions
//! (et à terme inter-plateformes) est un critère d'acceptation du projet, et
//! PCG32 tient en ~30 lignes tout en étant statistiquement solide. On garde
//! ainsi le contrôle total du flux aléatoire. `rand` restera envisageable
//! plus tard pour ses distributions, sur besoin avéré.

/// Mélangeur SplitMix64 : disperse un `u64` quelconque (même 0, 1, 2…) en
/// valeur bien répartie sur les 64 bits. Sert à dériver des seeds, jamais
/// comme générateur de flux.
pub fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// PCG32 (O'Neill, 2014) : état 64 bits, sortie 32 bits, période 2^64.
#[derive(Debug, Clone)]
pub struct Pcg32 {
    state: u64,
    /// Sélecteur de flux (toujours impair) : deux flux distincts sur la même
    /// seed produisent des séquences indépendantes.
    inc: u64,
}

impl Pcg32 {
    const MULTIPLIER: u64 = 6_364_136_223_846_793_005;

    /// Crée un générateur pour le couple (seed, flux). Même couple ⇒ même
    /// séquence, toujours.
    pub fn new(seed: u64, stream: u64) -> Self {
        let mut rng = Self {
            state: 0,
            inc: (stream << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(Self::MULTIPLIER).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    pub fn next_u64(&mut self) -> u64 {
        (u64::from(self.next_u32()) << 32) | u64::from(self.next_u32())
    }

    /// Flottant uniforme dans [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        // Une mantisse f32 porte 24 bits : on n'utilise que 24 bits d'aléa
        // pour une répartition exactement uniforme.
        (self.next_u32() >> 8) as f32 * (1.0 / (1u32 << 24) as f32)
    }

    /// Flottant uniforme dans [0, 1), en double précision (53 bits d'aléa).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meme_seed_meme_sequence() {
        let mut a = Pcg32::new(123, 0);
        let mut b = Pcg32::new(123, 0);
        for _ in 0..1000 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn flux_differents_sequences_differentes() {
        let mut a = Pcg32::new(123, 0);
        let mut b = Pcg32::new(123, 1);
        let identiques = (0..100).all(|_| a.next_u32() == b.next_u32());
        assert!(!identiques);
    }

    #[test]
    fn flottants_dans_l_intervalle() {
        let mut rng = Pcg32::new(7, 0);
        for _ in 0..10_000 {
            let f = rng.next_f32();
            assert!((0.0..1.0).contains(&f));
            let d = rng.next_f64();
            assert!((0.0..1.0).contains(&d));
        }
    }
}
