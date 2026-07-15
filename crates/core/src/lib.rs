//! Types fondamentaux partagés par tous les crates de Cairn :
//! seed du monde, RNG déterministe, et plus tard IDs, coordonnées, config.

pub mod rng;
pub mod scale;

pub use rng::{Pcg32, splitmix64};
pub use scale::{TILE_METERS, km_to_tiles, tiles_to_km};

/// Graine globale du monde. Tout l'aléatoire de la simulation dérive de
/// cette unique valeur : c'est la garantie du critère « même seed ⇒ monde
/// strictement identique, bit pour bit ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldSeed(pub u64);

impl WorldSeed {
    /// Dérive une seed secondaire pour un sous-système.
    ///
    /// `salt` identifie le consommateur (couche de bruit, chunk, entité…) :
    /// deux salts différents donnent des seeds décorrélées, et le même salt
    /// redonne toujours la même seed.
    pub fn derive(self, salt: u64) -> u64 {
        splitmix64(self.0 ^ splitmix64(salt))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_est_stable_et_discrimine_les_salts() {
        let seed = WorldSeed(42);
        assert_eq!(seed.derive(1), seed.derive(1));
        assert_ne!(seed.derive(1), seed.derive(2));
        assert_ne!(seed.derive(1), WorldSeed(43).derive(1));
    }
}
