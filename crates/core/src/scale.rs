//! Échelle physique du monde.
//!
//! Une tuile mesure [`TILE_METERS`]. Tout le reste du code raisonne en
//! **tuiles** ; ce module est l'unique pont vers les mètres et les
//! kilomètres. Conséquence : les tailles de features (continents, reliefs,
//! portée d'advection…) s'écrivent en km lisibles, et **changer d'échelle
//! ne touche qu'une constante**.

/// Côté physique d'une tuile, en mètres.
///
/// Choix de conception : ~2 m = échelle humaine. Une hutte occupe plusieurs
/// tuiles, un agent se tient sur une, un village se parcourt tuile à tuile.
/// Le monde est donc énorme en tuiles (continents ≈ centaines de milliers de
/// tuiles) — sans coût, car procédural et chunké.
pub const TILE_METERS: f64 = 2.0;

/// Kilomètres → nombre de tuiles. `const fn` : utilisable dans les constantes
/// et les valeurs par défaut de config.
pub const fn km_to_tiles(km: f64) -> f64 {
    km * 1000.0 / TILE_METERS
}

/// Nombre de tuiles → kilomètres.
pub const fn tiles_to_km(tiles: f64) -> f64 {
    tiles * TILE_METERS / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_reciproques() {
        assert_eq!(km_to_tiles(1.0), 500.0);
        assert_eq!(tiles_to_km(500.0), 1.0);
        assert_eq!(tiles_to_km(km_to_tiles(300.0)), 300.0);
    }
}
