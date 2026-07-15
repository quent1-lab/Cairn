//! Démonstration du monde chunké (Phase 1) :
//! 1. rend une région depuis les chunks → PNG, pour vérifier « aucune couture
//!    entre chunks » ;
//! 2. simule un balayage de caméra qui déborde largement la capacité →
//!    montre que la mémoire reste bornée (chunks générés ≫ chunks résidents) ;
//! 3. relit une tuile après l'avoir fait évincer → confirme la régénération
//!    à l'identique.
//!
//! Usage : cargo run --release -p cairn-sim --example chunk_demo -- [seed]

use std::mem::size_of;

use cairn_core::WorldSeed;
use cairn_core::scale::km_to_tiles;
use cairn_sim::{CHUNK_AREA, CHUNK_SIZE, ChunkCoord, Tile, World};

/// Côté de l'image, en pixels.
const VIEW: i64 = 768;

fn main() {
    let seed: u64 = arg(1).unwrap_or(42.0) as u64;
    // Échelle locale (2 tuiles/px ⇒ fenêtre ~3 km) : le store chunké sert le
    // local ; la vue monde lit le worldgen grossièrement (mode carte). À cette
    // échelle la fenêtre traverse ~24 chunks — une couture s'y verrait.
    let tiles_per_px = (arg(2).unwrap_or(2.0) as i64).max(1);
    let center_x = km_to_tiles(arg(3).unwrap_or(1160.0)) as i64;
    let center_y = km_to_tiles(arg(4).unwrap_or(2200.0)) as i64;

    render_region(seed, tiles_per_px, center_x, center_y);
    memory_is_bounded(seed);
    eviction_is_reversible(seed);
}

fn arg(n: usize) -> Option<f64> {
    std::env::args().nth(n).and_then(|s| s.parse().ok())
}

/// Étape 1 — rend une fenêtre depuis le monde chunké. Comme la tuile
/// matérialise exactement le worldgen, l'image ne peut pas montrer de couture à
/// la frontière des chunks (grille de 64 tuiles) — la fenêtre en traverse des
/// dizaines.
fn render_region(seed: u64, tiles_per_px: i64, center_x: i64, center_y: i64) {
    let span = VIEW * tiles_per_px;
    // Capacité large : pas d'éviction pendant le rendu.
    let chunks_needed = (span / CHUNK_SIZE + 2).pow(2) as usize;
    let mut world = World::new(WorldSeed(seed), chunks_needed);

    let half = span / 2;
    let mut img = image::RgbImage::new(VIEW as u32, VIEW as u32);
    for py in 0..VIEW {
        for px in 0..VIEW {
            let x = center_x + px * tiles_per_px - half;
            let y = center_y + py * tiles_per_px - half;
            let tile = world.tile(x, y);
            img.put_pixel(px as u32, py as u32, image::Rgb(tile_color(&tile)));
        }
    }
    std::fs::create_dir_all("out").expect("dossier out/");
    let path = format!("out/chunks_{seed}.png");
    img.save(&path).expect("écriture du PNG");
    println!(
        "1. rendu depuis les chunks → {path} — fenêtre {span}×{span} tuiles ({} chunks générés)",
        world.generated,
    );
}

/// Étape 2 — balaie une grande zone avec une petite capacité. La mémoire
/// résidente ne dépasse jamais la capacité, alors qu'on génère bien plus de
/// chunks au total.
fn memory_is_bounded(seed: u64) {
    const CAPACITY: usize = 64;
    let mut world = World::new(WorldSeed(seed), CAPACITY);
    let mut peak = 0usize;

    // 40 × 40 chunks = 1600 chunks parcourus, capacité 64.
    for cy in 0..40 {
        for cx in 0..40 {
            world.chunk(ChunkCoord { x: cx, y: cy });
            peak = peak.max(world.loaded());
        }
    }

    let tile_bytes = size_of::<Tile>();
    let chunk_kb = tile_bytes * CHUNK_AREA / 1024;
    println!(
        "2. mémoire bornée : {} chunks générés, {} évincés, pic résident {peak}/{CAPACITY}",
        world.generated, world.evicted,
    );
    println!(
        "   tuile = {tile_bytes} octets ⇒ chunk = {chunk_kb} Ko ⇒ empreinte max ≈ {} Mo",
        chunk_kb * CAPACITY / 1024,
    );
}

/// Étape 3 — lit une tuile, la fait évincer en chargeant beaucoup d'autres
/// chunks, puis la relit : identique bit pour bit.
fn eviction_is_reversible(seed: u64) {
    let mut world = World::new(WorldSeed(seed), 8);
    let before = world.tile(1234, -5678);
    for c in 0..100 {
        world.chunk(ChunkCoord { x: c, y: -c });
    }
    let after = world.tile(1234, -5678);
    let ok = before == after;
    println!("3. éviction réversible : tuile relue identique = {ok}");
    assert!(ok, "la régénération après éviction diffère !");
}

/// Couleur d'une tuile par altitude (teintes hypsométriques). L'altitude varie
/// à toutes les échelles : une couture entre chunks apparaîtrait comme une
/// discontinuité alignée sur la grille de 64 tuiles. Ici, aucune — la tuile
/// vaut exactement le worldgen.
fn tile_color(tile: &Tile) -> [u8; 3] {
    let e = tile.elevation;
    if e <= 0.0 {
        // Océan : du bleu profond au bleu côtier.
        let t = ((e + 1.0) / 1.0).clamp(0.0, 1.0);
        return [
            (16.0 + 54.0 * t) as u8,
            (40.0 + 98.0 * t) as u8,
            (96.0 + 104.0 * t) as u8,
        ];
    }
    // Terres : vert → brun → blanc selon l'altitude.
    const STOPS: &[(f32, [u8; 3])] = &[
        (0.0, [62, 126, 71]),
        (0.25, [134, 158, 88]),
        (0.5, [168, 136, 88]),
        (0.75, [136, 108, 96]),
        (1.0, [245, 245, 245]),
    ];
    for pair in STOPS.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if e <= b.0 {
            let f = (e - a.0) / (b.0 - a.0);
            return [
                (a.1[0] as f32 + (b.1[0] as f32 - a.1[0] as f32) * f) as u8,
                (a.1[1] as f32 + (b.1[1] as f32 - a.1[1] as f32) * f) as u8,
                (a.1[2] as f32 + (b.1[2] as f32 - a.1[2] as f32) * f) as u8,
            ];
        }
    }
    STOPS[STOPS.len() - 1].1
}
