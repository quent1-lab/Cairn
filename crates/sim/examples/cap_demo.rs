//! Banc **démographique réduit** (chantier CAP, étape 3) : un troupeau agrégé
//! dans une maille de pâturage, sans espace, sans prédateur, sans humain. Il
//! appelle les mêmes fonctions pures que `fauna::update_herds` — pas une copie
//! du modèle — et fait tenir des décennies de jeu en quelques secondes.
//!
//! Ce qu'il ne voit pas : la pâture de la tuile, les déplacements, la fuite, la
//! prédation, la migration. Il dit ce que la démographie fait d'une offre de
//! fourrage donnée.
//!
//! Usage : `cargo run --release -p cairn-sim --example cap_demo -- [années] [densité de départ /km²]`

use cairn_core::TICKS_PER_DAY;
use cairn_sim::fauna::{self, Species};
use cairn_worldgen::Biome;

const ZONE_KM2: f32 = 4.0;

fn main() {
    let mut args = std::env::args().skip(1);
    let years: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(50);
    let start: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(11.0);
    let cases = [
        ("forêt", Biome::TemperateForest, Species::Deer),
        ("taïga", Biome::Taiga, Species::Reindeer),
        ("toundra", Biome::Tundra, Species::Reindeer),
        ("prairie", Biome::Grassland, Species::Aurochs),
        ("steppe", Biome::Steppe, Species::Gazelle),
        ("savane", Biome::Savanna, Species::Gazelle),
    ];
    println!("# cap_demo — {years} ans, départ {start} têtes/km², une maille de {ZONE_KM2} km², sans prédateur");
    for (name, biome, species) in cases {
        // Production de la maille, kg MS/jour : la même conversion que `Rangeland`.
        let production = fauna::accessible_forage_g_m2_yr(biome) * 1000.0 / 365.0 * ZONE_KM2;
        let ration = species.daily_ration_kg();
        let capacity = production / ration / ZONE_KM2;
        let mut n = start * ZONE_KM2;
        let mut out = String::new();
        for year in 1..=years {
            let mut d: Vec<f32> = Vec::with_capacity(360);
            for _day in 0..360 {
                for _ in 0..TICKS_PER_DAY {
                    let sat = fauna::zone_satiation(production, n * ration);
                    n = fauna::herd_population_step(n, sat);
                }
                d.push(n / ZONE_KM2);
            }
            if year == 1 || year % 10 == 0 {
                d.sort_by(f32::total_cmp);
                out += &format!(" · an {year} : {:.1}/{:.1}/{:.1}", d[0], d[180], d[359]);
            }
        }
        println!("{name} ({}, capacité {capacity:.1} /km²) — densité min/méd/max{out}", species.label());
    }
}
