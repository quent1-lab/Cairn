//! Vérification native du **rendu vivant** du client, sans navigateur.
//!
//! Reproduit exactement le chemin du client web : même scène (foyer tempéré,
//! population + gibier), même `render::render_to_buffer` pour le terrain, même
//! projection monde→écran pour poser les entités. La seule différence est le
//! support (PNG au lieu d'un canvas) — la boucle rAF et le DOM sont du
//! plombage testé séparément.
//!
//! Usage : cargo run --release -p cairn-client --example client_render -- [seed] [ticks] [scale] [log_every_days]
//!
//! `log_every_days` (0 = silencieux, défaut) : imprime une ligne d'état par
//! jour de jeu simulé — population, gibier/troupeaux, prédateurs/meutes,
//! `hunted_head` cumulé, clans actifs. Sert à diagnostiquer visuellement une
//! trajectoire (ex. l'extinction du gibier) sans rejouer toute la sim pour
//! chaque hypothèse.

use cairn_client::palette::Layer;
use cairn_client::render;
use cairn_core::{WorldSeed, km_to_tiles};
use cairn_sim::{Activity, Behavior, Demographics, Herd, Pack, Position, Sim};
use cairn_worldgen::{HumidityConfig, WorldGenConfig};

const W: usize = 1100;
const H: usize = 800;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(42);
    let ticks: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    // Échelle en px/tuile : ~0,6 cadre les ~3 km où vivent agents et gibier.
    let scale: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0.6);
    let log_every_days: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0);

    // — Même construction que le client (humidité rapide). —
    let cfg = WorldGenConfig {
        humidity: HumidityConfig { steps: 32, lateral_samples: 0, ..HumidityConfig::default() },
        ..WorldGenConfig::default()
    };
    let mut sim = Sim::with_config(WorldSeed(seed), 2048, cfg);
    let seed_point = (km_to_tiles(1500.0) as i64, km_to_tiles(2100.0) as i64);
    let home = cairn_sim::scenario::find_home(&mut sim, seed_point);
    let placed = cairn_sim::scenario::populate(&mut sim, home, 40, 2);
    let (g0, p0, nh0, np0) = sim.fauna_census();
    println!("foyer {home:?}, {placed} humains, {g0:.0} gibier ({nh0} troupeaux), {p0:.0} prédateurs ({np0} meutes)");

    if log_every_days > 0 {
        println!("jour   humains  gibier  troupeaux  prédateurs  meutes  chassé(cumul)  clans");
    }
    for _ in 0..ticks {
        sim.step();
        if log_every_days > 0 && sim.time.tick.is_multiple_of(cairn_core::TICKS_PER_DAY) {
            let day = sim.time.tick / cairn_core::TICKS_PER_DAY;
            if day.is_multiple_of(log_every_days) {
                let (g, p, nh, np) = sim.fauna_census();
                println!(
                    "{day:>4}   {:>7}  {g:>6.0}  {nh:>9}  {p:>10.0}  {np:>6}  {:>13.0}  {:>5}",
                    sim.population(),
                    sim.hunted_head,
                    sim.clans.len(),
                );
            }
        }
    }

    // — Caméra cadrant toute la population (comme le mode « Suivre »). —
    let mut bounds: Option<(f64, f64, f64, f64)> = None;
    for (_, pos) in sim.agents.query::<&Position>().iter() {
        bounds = Some(match bounds {
            None => (pos.x, pos.y, pos.x, pos.y),
            Some((x0, y0, x1, y1)) => (x0.min(pos.x), y0.min(pos.y), x1.max(pos.x), y1.max(pos.y)),
        });
    }
    let (cx, cy, scale) = match bounds {
        Some((x0, y0, x1, y1)) => {
            let pad = km_to_tiles(0.4);
            let s = (W as f64 / (x1 - x0 + 2.0 * pad).max(1.0))
                .min(H as f64 / (y1 - y0 + 2.0 * pad).max(1.0))
                .clamp(0.02, 3.0);
            ((x0 + x1) / 2.0, (y0 + y1) / 2.0, s)
        }
        None => (home.0 as f64, home.1 as f64, scale),
    };
    let mut buf = render::render_to_buffer(sim.world.worldgen(), cx, cy, scale, Layer::Biome, W, H);

    // — Entités par-dessus, même projection que le client. —
    let project = |wx: f64, wy: f64| ((wx - cx) * scale + W as f64 / 2.0, (wy - cy) * scale + H as f64 / 2.0);
    for (_, (herd, pos)) in sim.fauna.query::<(&Herd, &Position)>().iter() {
        let (sx, sy) = project(pos.x, pos.y);
        let s = ((scale * 1.5) * (1.0 + f64::from(herd.population) / 60.0)).clamp(3.0, 16.0);
        fill_square(&mut buf, sx, sy, s, [200, 162, 74]);
    }
    for (_, (pack, pos)) in sim.fauna.query::<(&Pack, &Position)>().iter() {
        let (sx, sy) = project(pos.x, pos.y);
        let s = ((scale * 1.5) * (1.0 + f64::from(pack.population) / 8.0)).clamp(3.0, 12.0);
        fill_square(&mut buf, sx, sy, s, [139, 63, 176]);
    }
    let tick = sim.time.tick;
    let s = scale.clamp(2.5, 8.0);
    for (_, (pos, beh, demo)) in sim.agents.query::<(&Position, &Behavior, &Demographics)>().iter() {
        let (sx, sy) = project(pos.x, pos.y);
        let s = if demo.is_adult(tick) { s } else { (s * 0.55).max(2.0) };
        fill_square(&mut buf, sx, sy, s, activity_rgb(beh.activity));
    }

    // Foyers de clan (Phase 4) : un contour carré (le buffer RGBA brut ne
    // prête pas à un cercle sans plus d'outillage) à la position de
    // `Clan::home` — vérifie que la géométrie et la présence sont bonnes ;
    // la couleur par clan, elle, ne s'exerce que dans le vrai rendu canvas
    // (`App::draw_entities`), vérifiée en navigateur.
    for clan in &sim.clans {
        let (sx, sy) = project(clan.home.0, clan.home.1);
        stroke_square(&mut buf, sx, sy, (scale * 12.0).clamp(12.0, 80.0), [255, 255, 255]);
    }
    println!("clans : {} actif(s)", sim.clans.len());

    let (g1, p1, _, _) = sim.fauna_census();
    println!(
        "après {ticks} ticks (an {}, jour {}) : {} humains, {g1:.0} gibier, {p1:.0} prédateurs",
        sim.time.year(), sim.time.day_of_year(), sim.population(),
    );
    println!(
        "perf : {} chunks générés, {} appels A* ({:.1}/tick)",
        sim.world.generated, sim.path_calls, sim.path_calls as f64 / ticks.max(1) as f64,
    );

    std::fs::create_dir_all("out").ok();
    let img = image::RgbaImage::from_raw(W as u32, H as u32, buf).expect("buffer");
    let path = format!("out/client_{seed}_{ticks}.png");
    img.save(&path).expect("png");
    println!("→ {path}");
}

/// Peint un carré plein centré sur (cx, cy) dans le tampon RGBA.
fn fill_square(buf: &mut [u8], cx: f64, cy: f64, size: f64, rgb: [u8; 3]) {
    let half = (size / 2.0).max(1.0) as i64;
    let (cxi, cyi) = (cx as i64, cy as i64);
    for dy in -half..=half {
        for dx in -half..=half {
            let (x, y) = (cxi + dx, cyi + dy);
            if x < 0 || y < 0 || x >= W as i64 || y >= H as i64 {
                continue;
            }
            let o = (y as usize * W + x as usize) * 4;
            buf[o] = rgb[0];
            buf[o + 1] = rgb[1];
            buf[o + 2] = rgb[2];
            buf[o + 3] = 255;
        }
    }
}

/// Peint le contour d'un carré (pas le remplissage) centré sur (cx, cy).
fn stroke_square(buf: &mut [u8], cx: f64, cy: f64, size: f64, rgb: [u8; 3]) {
    let half = (size / 2.0).max(1.0) as i64;
    let (cxi, cyi) = (cx as i64, cy as i64);
    let mut put = |x: i64, y: i64| {
        if x < 0 || y < 0 || x >= W as i64 || y >= H as i64 {
            return;
        }
        let o = (y as usize * W + x as usize) * 4;
        buf[o] = rgb[0];
        buf[o + 1] = rgb[1];
        buf[o + 2] = rgb[2];
        buf[o + 3] = 255;
    };
    for d in -half..=half {
        put(cxi + d, cyi - half);
        put(cxi + d, cyi + half);
        put(cxi - half, cyi + d);
        put(cxi + half, cyi + d);
    }
}

fn activity_rgb(a: Activity) -> [u8; 3] {
    match a {
        Activity::Idle | Activity::Walking => [232, 80, 58],
        Activity::Eating | Activity::Hunting => [255, 154, 60],
        Activity::Drinking => [70, 180, 255],
        Activity::Sleeping => [106, 106, 208],
        Activity::Sheltering => [176, 112, 200],
    }
}
