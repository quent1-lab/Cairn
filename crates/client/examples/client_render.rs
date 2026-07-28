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
use cairn_sim::{
    Activity, AgentId, Behavior, Demographics, Expedition, Fire, Herd, Pack, Position, Sim, Species,
};
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
    // 5ᵉ arg `overlays` : injecte un feu et une expédition **synthétiques** pour
    // vérifier la géométrie des overlays Phase 5 dans le PNG — ces événements
    // sont rares, voire absents, de la scène tempérée par défaut.
    let overlays_demo = args.next().is_some_and(|s| s == "overlays");

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
        println!("jour   humains  gibier  troupeaux  prédateurs  meutes  chassé(cumul)  clans  struct");
    }
    for _ in 0..ticks {
        sim.step();
        if log_every_days > 0 && sim.time.tick.is_multiple_of(cairn_core::TICKS_PER_DAY) {
            let day = sim.time.tick / cairn_core::TICKS_PER_DAY;
            if day.is_multiple_of(log_every_days) {
                let (g, p, nh, np) = sim.fauna_census();
                println!(
                    "{day:>4}   {:>7}  {g:>6.0}  {nh:>9}  {p:>10.0}  {np:>6}  {:>13.0}  {:>5}  {:>6}",
                    sim.population(),
                    sim.hunted_head,
                    sim.clans.len(),
                    sim.structures.len(),
                );
            }
        }
    }

    // Overlays de démonstration : un feu et une expédition synthétiques, pour
    // que le PNG en montre la géométrie (voir `overlays_demo`).
    if overlays_demo {
        sim.fires.push(Fire {
            pos: (home.0 as f64 + 80.0, home.1 as f64 - 40.0),
            radius: km_to_tiles(0.1),
            age_days: 2,
        });
        if let Some(first) = sim.agents.query::<&AgentId>().iter().next().map(|(_, id)| id.0) {
            sim.expeditions.insert(
                first,
                Expedition {
                    tin: (home.0 + km_to_tiles(2.0) as i64, home.1 + km_to_tiles(1.2) as i64),
                    home: (home.0 as f64, home.1 as f64),
                    returning: false,
                },
            );
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
        fill_square(&mut buf, sx, sy, s, species_rgb(herd.species));
    }
    for (_, (pack, pos)) in sim.fauna.query::<(&Pack, &Position)>().iter() {
        let (sx, sy) = project(pos.x, pos.y);
        let s = ((scale * 1.5) * (1.0 + f64::from(pack.population) / 8.0)).clamp(3.0, 12.0);
        fill_square(&mut buf, sx, sy, s, species_rgb(pack.species));
    }
    let tick = sim.time.tick;
    let s = scale.clamp(2.5, 8.0);
    for (_, (pos, beh, demo)) in sim.agents.query::<(&Position, &Behavior, &Demographics)>().iter() {
        let (sx, sy) = project(pos.x, pos.y);
        let s = if demo.is_adult(tick) { s } else { (s * 0.55).max(2.0) };
        fill_square(&mut buf, sx, sy, s, activity_rgb(beh.activity));
    }

    // — La couche « clan » (Phase 4), même contenu que `App::draw_entities`
    //   du vrai client, transposé sur le buffer RGBA brut pour vérifier la
    //   géométrie hors navigateur (les couleurs HSL par clan, elles, ne
    //   s'exercent qu'au canvas). Territoire, tension, foyer, structures.
    let terr_r = cairn_sim::social::RESIDENCE_RADIUS_TILES * scale;
    for clan in &sim.clans {
        let (sx, sy) = project(clan.home.0, clan.home.1);
        stroke_circle(&mut buf, sx, sy, terr_r, [110, 140, 130]); // territoire
        stroke_square(&mut buf, sx, sy, (scale * 12.0).clamp(12.0, 80.0), [255, 255, 255]); // foyer
    }
    for i in 0..sim.clans.len() {
        for j in (i + 1)..sim.clans.len() {
            let t = sim.clan_relations.tension_between(sim.clans[i].id, sim.clans[j].id);
            if t < 0.05 {
                continue;
            }
            let (ax, ay) = project(sim.clans[i].home.0, sim.clans[i].home.1);
            let (bx, by) = project(sim.clans[j].home.0, sim.clans[j].home.1);
            draw_line(&mut buf, ax, ay, bx, by, [224, 80, 58]); // tension
        }
    }
    for st in &sim.structures {
        let (bx, by) = project(st.pos.0, st.pos.1);
        let (rgb, off) = match st.kind {
            cairn_sim::StructureKind::Hut => ([169, 115, 62], (-8.0, -6.0)),
            cairn_sim::StructureKind::Granary => ([216, 178, 74], (8.0, -6.0)),
            cairn_sim::StructureKind::Palisade => ([154, 160, 166], (0.0, 9.0)),
            cairn_sim::StructureKind::ChiefHut => ([194, 91, 58], (0.0, -10.0)),
        };
        fill_square(&mut buf, bx + off.0, by + off.1, (scale * 4.0).clamp(5.0, 12.0), rgb);
    }
    // Feux (Phase 5, incrément 5) : disque orange au foyer du feu — miroir de
    // `App::draw_entities`.
    for fire in &sim.fires {
        let (fx, fy) = project(fire.pos.0, fire.pos.1);
        fill_circle(&mut buf, fx, fy, (fire.radius * scale).max(2.0), [255, 106, 42]);
    }
    // Routes d'expédition (Phase 5, incrément 6) : trait envoyé→étape (l'étain à
    // l'aller, le foyer au retour) + repère à l'étape.
    for (&aid, exp) in &sim.expeditions {
        let envoy = sim
            .agents
            .query::<(&AgentId, &Position)>()
            .iter()
            .find(|(_, (id, _))| id.0 == aid)
            .map(|(_, (_, p))| (p.x, p.y));
        if let Some((ex, ey)) = envoy {
            let wp = if exp.returning { exp.home } else { (exp.tin.0 as f64, exp.tin.1 as f64) };
            let (sx, sy) = project(ex, ey);
            let (tx, ty) = project(wp.0, wp.1);
            draw_line(&mut buf, sx, sy, tx, ty, [56, 214, 192]);
            fill_square(&mut buf, tx, ty, 6.0, [56, 214, 192]);
        }
    }

    println!("clans : {} actif(s)", sim.clans.len());
    {
        use cairn_sim::StructureKind::*;
        let (mut huts, mut greniers, mut palissades, mut chef) = (0, 0, 0, 0);
        for s in &sim.structures {
            match s.kind {
                Hut => huts += 1,
                Granary => greniers += 1,
                Palisade => palissades += 1,
                ChiefHut => chef += 1,
            }
        }
        println!(
            "structures : {} au total ({huts} huttes, {greniers} greniers, {palissades} palissades, {chef} huttes du chef)",
            sim.structures.len(),
        );
    }

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

/// Peint le contour d'un cercle centré sur (cx, cy) par pas angulaires — de
/// quoi vérifier l'étendue d'un territoire dans le PNG (le buffer brut n'a
/// pas de primitive `arc`, contrairement au canvas du vrai client).
fn stroke_circle(buf: &mut [u8], cx: f64, cy: f64, r: f64, rgb: [u8; 3]) {
    if r < 1.0 {
        return;
    }
    let steps = (r * 6.5).clamp(64.0, 4096.0) as usize;
    for k in 0..steps {
        let a = k as f64 / steps as f64 * std::f64::consts::TAU;
        let (x, y) = ((cx + a.cos() * r) as i64, (cy + a.sin() * r) as i64);
        if x < 0 || y < 0 || x >= W as i64 || y >= H as i64 {
            continue;
        }
        let o = (y as usize * W + x as usize) * 4;
        buf[o..o + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
}

/// Peint un disque plein centré sur (cx, cy) — pour le foyer d'un feu (le
/// canvas du vrai client le fait par `arc`+`fill`).
fn fill_circle(buf: &mut [u8], cx: f64, cy: f64, r: f64, rgb: [u8; 3]) {
    let ri = r.max(1.0) as i64;
    let (cxi, cyi) = (cx as i64, cy as i64);
    for dy in -ri..=ri {
        for dx in -ri..=ri {
            if dx * dx + dy * dy > ri * ri {
                continue;
            }
            let (x, y) = (cxi + dx, cyi + dy);
            if x < 0 || y < 0 || x >= W as i64 || y >= H as i64 {
                continue;
            }
            let o = (y as usize * W + x as usize) * 4;
            buf[o..o + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
}

/// Trace un segment (cx0,cy0)→(cx1,cy1) — Bresenham simplifié par pas.
fn draw_line(buf: &mut [u8], x0: f64, y0: f64, x1: f64, y1: f64, rgb: [u8; 3]) {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let steps = dx.hypot(dy).ceil().max(1.0) as usize;
    for k in 0..=steps {
        let t = k as f64 / steps as f64;
        let (x, y) = ((x0 + dx * t) as i64, (y0 + dy * t) as i64);
        if x < 0 || y < 0 || x >= W as i64 || y >= H as i64 {
            continue;
        }
        let o = (y as usize * W + x as usize) * 4;
        buf[o..o + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
}

/// Couleur RGB d'une espèce (miroir de `species_color` du client web).
fn species_rgb(s: Species) -> [u8; 3] {
    match s {
        Species::Deer => [181, 121, 74],
        Species::Aurochs => [138, 106, 68],
        Species::Gazelle => [216, 178, 106],
        Species::Reindeer => [195, 192, 170],
        Species::Wolf => [139, 63, 176],
        Species::CaveLion => [192, 86, 46],
    }
}

fn activity_rgb(a: Activity) -> [u8; 3] {
    match a {
        Activity::Idle | Activity::Walking => [232, 80, 58],
        Activity::Eating | Activity::Hunting => [255, 154, 60],
        Activity::Drinking => [70, 180, 255],
        Activity::Sleeping => [106, 106, 208],
        Activity::Sheltering => [176, 112, 200],
        Activity::Farming => [123, 200, 108],
    }
}
