//! Aperçu natif du rendu du client : produit le même tampon que le canvas,
//! écrit en PNG. Permet de vérifier l'affichage sans navigateur.
//!
//! Usage : cargo run --release -p cairn-client --example preview -- [couche] [km/écran]
//! couche : biome | elevation | temperature | humidity | geology

use cairn_core::WorldSeed;
use cairn_core::scale::km_to_tiles;
use cairn_worldgen::{HumidityConfig, WorldGen, WorldGenConfig};

use cairn_client::palette::Layer;
use cairn_client::render::render_to_buffer;

const W: usize = 960;
const H: usize = 600;

fn main() {
    let mut args = std::env::args().skip(1);
    let layer = args
        .next()
        .and_then(|s| Layer::from_id(&s))
        .unwrap_or(Layer::Biome);
    let km_per_screen: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(80.0);

    // Même config « rapide » d'humidité que le client interactif.
    let cfg = WorldGenConfig {
        humidity: HumidityConfig {
            steps: 32,
            lateral_samples: 0,
            ..HumidityConfig::default()
        },
        ..WorldGenConfig::default()
    };
    let wg = WorldGen::with_config(WorldSeed(42), cfg);

    // scale = pixels par tuile : largeur d'écran = km_per_screen.
    let scale = W as f64 / km_to_tiles(km_per_screen);
    let (cx, cy) = (km_to_tiles(1500.0), km_to_tiles(2100.0));

    let buf = render_to_buffer(&wg, cx, cy, scale, layer, W, H);

    let img = image::RgbaImage::from_raw(W as u32, H as u32, buf).expect("buffer");
    std::fs::create_dir_all("out").expect("dossier out/");
    let path = format!("out/client_{}.png", layer.id());
    img.save(&path).expect("écriture du PNG");
    println!("{path} — {km_per_screen:.0} km de large, échelle {scale:.4} px/tuile");
}
