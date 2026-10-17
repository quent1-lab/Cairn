//! Démo de la Phase 2 : une population lâchée dans un coin tempéré du monde,
//! un an de simulation, zéro intervention.
//!
//! C'est le banc de **calibrage** du brief : si tout le monde meurt, le monde
//! est trop dur ; si personne ne meurt, il est trop mou. La sortie donne un
//! rapport mensuel (population, morts par cause, besoins moyens) puis un
//! verdict face au critère d'acceptation, et rend une carte PNG du campement.
//!
//! Usage : cargo run --release -p cairn-sim --example life_demo -- [seed] [années] [agents]

use cairn_core::{TICKS_PER_DAY, TICKS_PER_YEAR, WorldSeed, km_to_tiles, tiles_to_km};
use cairn_sim::{
    AgentId, DeathCause, Demographics, Herd, Memory, Pack, Physiology, Position, Sim, Skills,
    fauna, memory::MEMORY_CELL_TILES,
};
use cairn_worldgen::Biome;

/// Chunks résidents : ~520 Mio (dans la cible 2 Go du brief). Doit couvrir le
/// **plateau** de chunks sales de la scène — mesuré : la biomasse broutée et
/// fourragée sature vers ~8 500 chunks marqués, avec une croissance qui
/// décélère (l'empreinte spatiale de la population se stabilise). Trop juste,
/// et l'éviction bornée se met à sacrifier des chunks encore lus → thrash
/// (0,8 tick/s contre 6). Prochain incrément perf : persister les deltas des
/// chunks sales pour les évincer proprement (LOD temporel §8.2).
const CHUNK_CAPACITY: usize = 16384;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(42);
    let years: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let n_agents: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(100);

    println!(
        "— Cairn, Phases 2-3 : LA VIE & LE NOMBRE — seed {seed}, {years} an(s), {n_agents} agents\n"
    );

    let mut sim = Sim::new(WorldSeed(seed), CHUNK_CAPACITY);
    let origin = find_home(&mut sim);
    let (ox, oy) = origin;
    let tile = sim.world.tile(ox, oy);
    println!(
        "Foyer retenu : ({ox}, {oy}) — {:?}, {:.1} °C de moyenne annuelle",
        tile.biome, tile.temperature
    );

    // Semis déterministe : une grille de pas 12 tuiles autour du foyer, en
    // sautant l'eau. Pas besoin d'aléa — le monde s'en charge.
    let mut placed = 0;
    let mut ring = 0i64;
    'outer: while placed < n_agents {
        ring += 1;
        let r = ring * 12;
        for dy in (-r..=r).step_by(12) {
            for dx in (-r..=r).step_by(12) {
                if dx.abs() < r && dy.abs() < r {
                    continue; // seulement la couronne du ring courant
                }
                let (x, y) = (ox + dx, oy + dy);
                if sim.world.tile(x, y).is_walkable() {
                    sim.spawn_agent(x as f64 + 0.5, y as f64 + 0.5);
                    placed += 1;
                    if placed >= n_agents {
                        break 'outer;
                    }
                }
            }
        }
    }
    println!("{placed} agents lâchés dans un rayon de {} m", ring * 12 * 2);

    // Le gibier : semé sur une grille autour du foyer, à ~1,5 km de pas, sur
    // les terres seulement. Densité de départ, pas plafond : la suite est
    // affaire de pâture, de prédateurs et de chasseurs.
    let pas = km_to_tiles(1.5) as i64;
    let mut troupeaux = 0;
    for gy in -3..=3i64 {
        for gx in -3..=3i64 {
            let (x, y) = (ox + gx * pas, oy + gy * pas);
            if sim.world.tile(x, y).is_walkable() && sim.world.tile(x, y).biomass > 40 {
                sim.spawn_herd(x as f64, y as f64, fauna::HERD_START);
                troupeaux += 1;
            }
        }
    }
    // Quelques meutes, loin du campement : elles trouveront le gibier seules.
    let mut meutes = 0;
    for i in 0..4i64 {
        let angle = i as f64 * 1.57;
        let (x, y) = (
            ox + (angle.cos() * km_to_tiles(4.0)) as i64,
            oy + (angle.sin() * km_to_tiles(4.0)) as i64,
        );
        if sim.world.tile(x, y).is_walkable() {
            sim.spawn_pack(x as f64, y as f64, fauna::PACK_START);
            meutes += 1;
        }
    }
    let (gibier0, predateurs0, _, _) = sim.fauna_census();
    println!(
        "{troupeaux} troupeaux ({gibier0:.0} têtes) et {meutes} meutes ({predateurs0:.0} bêtes) alentour\n"
    );

    println!(
        "{:>4} {:>5} {:>5} | {:>5} {:>5} {:>5} | {:>5} {:>5} {:>5} {:>4} | {:>6} {:>6} {:>6} | {:>6}",
        "mois", "pop", "naiss", "faim", "soif", "froid", "†faim", "†soif", "†froid", "†âge",
        "gibier", "loc3km", "prises", "préda"
    );

    let start = std::time::Instant::now();
    let total_ticks = years * TICKS_PER_YEAR;
    for tick in 0..total_ticks {
        sim.step();
        if (tick + 1) % (30 * TICKS_PER_DAY) == 0 {
            report(&sim, (tick + 1) / (30 * TICKS_PER_DAY), origin);
        }
    }
    let elapsed = start.elapsed();

    let (starved, dehydrated, frozen, old_age) = death_counts(&sim);
    let alive = sim.population();
    println!(
        "\nBilan : {alive} vivants ({placed} fondateurs, {} naissances) après {years} an(s) — \
         morts : {} faim, {} soif, {} froid, {} vieillesse",
        sim.births.len(),
        starved,
        dehydrated,
        frozen,
        old_age
    );
    age_pyramid(&sim);
    knowledge_report(&sim);
    let (gibier, predateurs, troupeaux_fin, meutes_fin) = sim.fauna_census();
    let local = local_herbivores(&sim, origin, 3.0);
    println!(
        "Faune : {gibier:.0} têtes en {troupeaux_fin} troupeaux (départ {gibier0:.0}), \
         dont {local:.0} à moins de 3 km du foyer ; {predateurs:.0} prédateurs en {meutes_fin} meutes ; \
         {:.0} têtes chassées",
        sim.hunted_head
    );
    if gibier0 > 0.0 {
        let pression = local / gibier0 * 100.0;
        println!(
            "Surchasse locale : le foyer retient {pression:.0} % du gibier initial \
             (le reste est mort ou a fui hors de portée)"
        );
    }
    println!(
        "{} ticks en {:.1} s ({:.0} ticks/s) — {} chunks générés, {} évincés, {} sales",
        total_ticks,
        elapsed.as_secs_f64(),
        total_ticks as f64 / elapsed.as_secs_f64(),
        sim.world.generated,
        sim.world.evicted,
        sim.world.dirty_count(),
    );

    let dead = sim.deaths.len();
    let verdict = if alive == 0 {
        "ÉCHEC : extinction — le monde est trop dur (ou le foyer mal choisi)."
    } else if dead == 0 {
        "TROP MOU : aucune mort — la pression de sélection est nulle."
    } else {
        "OK : mortalité non nulle et non totale — critère Phase 2 rempli."
    };
    println!("\nVerdict : {verdict}");

    render_map(&mut sim, origin, seed);
}

/// Cherche un foyer tempéré : prairie ou forêt tempérée, douceur annuelle,
/// et une source d'eau à portée. Balayage déterministe en anneaux de 20 km.
fn find_home(sim: &mut Sim) -> (i64, i64) {
    let step = km_to_tiles(20.0) as i64;
    for ring in 0..120i64 {
        let r = ring * step;
        let mut candidates = Vec::new();
        if ring == 0 {
            candidates.push((0, 0));
        } else {
            for i in (-ring..=ring).map(|i| i * step) {
                candidates.push((i, -r));
                candidates.push((i, r));
                candidates.push((-r, i));
                candidates.push((r, i));
            }
        }
        for (x, y) in candidates {
            let wg = sim.world.worldgen();
            let e = wg.elevation(x, y);
            if e <= 0.0 {
                continue;
            }
            let t = wg.mean_temperature(x, y, e);
            if !(6.0..=18.0).contains(&t) {
                continue;
            }
            if !matches!(wg.biome(x, y), Biome::Grassland | Biome::TemperateForest) {
                continue;
            }
            if sim.world.nearest_spring((x, y), 4).is_some() {
                return (x, y);
            }
        }
    }
    panic!("aucun foyer habitable trouvé avec cette seed — essayer une autre");
}

fn death_counts(sim: &Sim) -> (usize, usize, usize, usize) {
    let mut c = (0, 0, 0, 0);
    for d in &sim.deaths {
        match d.cause {
            DeathCause::Starvation => c.0 += 1,
            DeathCause::Dehydration => c.1 += 1,
            DeathCause::Hypothermia => c.2 += 1,
            DeathCause::OldAge => c.3 += 1,
            DeathCause::Predation
            | DeathCause::Violence
            | DeathCause::Disease
            | DeathCause::Lightning => {} // non ventilées (démo Phase 2)
        }
    }
    c
}

/// Ce que la population **sait** (Phase 3) : le territoire vécu (union des
/// cartes mentales), les sources d'eau connues, le savoir-faire moyen.
fn knowledge_report(sim: &Sim) {
    let mut all_cells = std::collections::BTreeSet::new();
    let (mut springs, mut foraging, mut hunting, mut n) = (0usize, 0.0f32, 0.0f32, 0usize);
    for (_, (mem, skills)) in sim.agents.query::<(&Memory, &Skills)>().iter() {
        all_cells.extend(mem.known.iter().copied());
        springs += mem.springs.len();
        foraging += skills.foraging;
        hunting += skills.hunting;
        n += 1;
    }
    if n == 0 {
        return;
    }
    let cell_km = tiles_to_km(MEMORY_CELL_TILES as f64);
    println!(
        "\nSavoirs : territoire vécu {:.0} km² ({} cellules), {:.1} sources connues/tête, \
         cueillette {:.2} / chasse {:.2} en moyenne",
        all_cells.len() as f64 * cell_km * cell_km,
        all_cells.len(),
        springs as f64 / n as f64,
        foraging / n as f32,
        hunting / n as f32,
    );
}

/// Pyramide des âges en ASCII : tranches de 5 ans, un « █ » par individu.
/// C'est l'observable démographique de la Phase 3 — une population qui
/// persiste a une base (des enfants) et un sommet (des vieux).
fn age_pyramid(sim: &Sim) {
    let tick = sim.time.tick;
    let mut buckets = [0usize; 17]; // 0-4, 5-9, …, 80+
    for (_, demo) in sim.agents.query::<&Demographics>().iter() {
        let age = demo.age_years(tick).max(0.0);
        buckets[((age / 5.0) as usize).min(16)] += 1;
    }
    println!("\nPyramide des âges :");
    for (i, &n) in buckets.iter().enumerate().rev() {
        if n == 0 {
            continue;
        }
        let label = if i == 16 { "80+ ".to_string() } else { format!("{:>2}-{:<2}", i * 5, i * 5 + 4) };
        println!("  {label} | {} {n}", "█".repeat(n.min(60)));
    }
}

fn report(sim: &Sim, month: u64, origin: (i64, i64)) {
    let mut n = 0usize;
    let (mut hunger, mut thirst, mut cold) = (0.0f32, 0.0, 0.0);
    for (_, phys) in sim.agents.query::<&Physiology>().iter() {
        hunger += phys.hunger;
        thirst += phys.thirst;
        cold += phys.cold;
        n += 1;
    }
    let mean = |v: f32| if n > 0 { v / n as f32 } else { 0.0 };
    let (starved, dehydrated, frozen, old_age) = death_counts(sim);
    let (gibier, predateurs, _, _) = sim.fauna_census();
    println!(
        "{:>4} {:>5} {:>5} | {:>5.2} {:>5.2} {:>5.2} | {:>5} {:>5} {:>5} {:>4} | {:>6.0} {:>6.0} {:>6.0} | {:>6.0}",
        month,
        n,
        sim.births.len(),
        mean(hunger),
        mean(thirst),
        mean(cold),
        starved,
        dehydrated,
        frozen,
        old_age,
        gibier,
        local_herbivores(sim, origin, 3.0),
        sim.hunted_head,
        predateurs,
    );
}

/// Cheptel dans un rayon de `radius_km` autour d'un point — l'observable de
/// la surchasse locale.
fn local_herbivores(sim: &Sim, center: (i64, i64), radius_km: f64) -> f32 {
    let r = km_to_tiles(radius_km);
    sim.fauna
        .query::<(&Herd, &Position)>()
        .iter()
        .filter(|(_, (_, p))| (p.x - center.0 as f64).hypot(p.y - center.1 as f64) < r)
        .map(|(_, (h, _))| h.population)
        .sum()
}

/// Carte PNG du campement : biomes assombris par la biomasse manquante,
/// sources en bleu, agents en rouge, lieux de mort en noir. Cadrée sur la
/// **médiane des survivants** : après un an d'errances, la population peut
/// s'être déplacée à des kilomètres du point de largage.
fn render_map(sim: &mut Sim, origin: (i64, i64), seed: u64) {
    const HALF: i64 = 512; // 1024×1024 tuiles ≈ 2 km de côté
    let mut xs: Vec<i64> = Vec::new();
    let mut ys: Vec<i64> = Vec::new();
    for (_, pos) in sim.agents.query::<&Position>().iter() {
        let (x, y) = pos.tile();
        xs.push(x);
        ys.push(y);
    }
    let origin = if xs.is_empty() {
        origin
    } else {
        xs.sort_unstable();
        ys.sort_unstable();
        (xs[xs.len() / 2], ys[ys.len() / 2])
    };
    let mut img = image::RgbImage::new((2 * HALF) as u32, (2 * HALF) as u32);
    for py in 0..2 * HALF {
        for px in 0..2 * HALF {
            let (x, y) = (origin.0 - HALF + px, origin.1 - HALF + py);
            let tile = sim.world.tile(x, y);
            let mut rgb = biome_color(tile.biome);
            // La consommation se voit : moins de biomasse = plus terne.
            let k = cairn_sim::tile::baseline_biomass(tile.biome).max(1);
            let ratio = 0.45 + 0.55 * f32::from(tile.biomass) / f32::from(k);
            for c in &mut rgb {
                *c = (f32::from(*c) * ratio.min(1.0)) as u8;
            }
            img.put_pixel(px as u32, py as u32, image::Rgb(rgb));
        }
    }
    let mut mark = |x: i64, y: i64, color: [u8; 3], size: i64| {
        for dy in -size..=size {
            for dx in -size..=size {
                let (px, py) = (x - origin.0 + HALF + dx, y - origin.1 + HALF + dy);
                if (0..2 * HALF).contains(&px) && (0..2 * HALF).contains(&py) {
                    img.put_pixel(px as u32, py as u32, image::Rgb(color));
                }
            }
        }
    };
    // Les sources d'un pixel seraient invisibles : marquées en croix bleues.
    for cy in (origin.1 - HALF) / 64 - 1..=(origin.1 + HALF) / 64 + 1 {
        for cx in (origin.0 - HALF) / 64 - 1..=(origin.0 + HALF) / 64 + 1 {
            let coord = cairn_sim::ChunkCoord { x: cx, y: cy };
            let (ox, oy) = coord.origin();
            let springs = sim.world.chunk(coord).springs.clone();
            for (lx, ly) in springs {
                mark(ox + lx as i64, oy + ly as i64, [40, 120, 255], 3);
            }
        }
    }
    for d in &sim.deaths {
        mark(d.pos.0, d.pos.1, [10, 10, 10], 1);
    }
    // Le gibier en fauve, les prédateurs en violet : la taille du carré suit
    // l'effectif du groupe.
    for (_, (herd, pos)) in sim.fauna.query::<(&Herd, &Position)>().iter() {
        let (x, y) = pos.tile();
        let size = 2 + (herd.population / 25.0) as i64;
        mark(x, y, [200, 150, 60], size.min(5));
    }
    for (_, (pack, pos)) in sim.fauna.query::<(&Pack, &Position)>().iter() {
        let (x, y) = pos.tile();
        let size = 2 + (pack.population / 5.0) as i64;
        mark(x, y, [170, 70, 200], size.min(5));
    }
    let agents: Vec<(AgentId, Position)> = sim
        .agents
        .query::<(&AgentId, &Position)>()
        .iter()
        .map(|(_, (id, p))| (*id, *p))
        .collect();
    for (_, p) in &agents {
        let (x, y) = p.tile();
        mark(x, y, [230, 40, 40], 2);
    }
    if !agents.is_empty() {
        let xs: Vec<i64> = agents.iter().map(|(_, p)| p.tile().0).collect();
        let ys: Vec<i64> = agents.iter().map(|(_, p)| p.tile().1).collect();
        let span_x = xs.iter().max().unwrap() - xs.iter().min().unwrap();
        let span_y = ys.iter().max().unwrap() - ys.iter().min().unwrap();
        println!(
            "Étendue de la population : {:.1} × {:.1} km (dispersion émergente autour des sources)",
            span_x as f64 * 2.0 / 1000.0,
            span_y as f64 * 2.0 / 1000.0,
        );
    }
    std::fs::create_dir_all("out").ok();
    let path = format!("out/life_{seed}.png");
    img.save(&path).expect("écriture PNG");
    println!("Carte du campement : {path} (agents en rouge, morts en noir, sources en bleu)");
}

fn biome_color(biome: Biome) -> [u8; 3] {
    match biome {
        Biome::Ocean => [12, 44, 96],
        Biome::Coast => [24, 74, 140],
        Biome::Glacier => [225, 235, 245],
        Biome::Tundra => [150, 160, 145],
        Biome::Taiga => [55, 95, 75],
        Biome::ColdDesert => [170, 160, 130],
        Biome::HotDesert => [210, 185, 120],
        Biome::Steppe => [165, 160, 95],
        Biome::Savanna => [180, 165, 80],
        Biome::Grassland => [110, 150, 70],
        Biome::TemperateForest => [60, 110, 55],
        Biome::TropicalForest => [30, 90, 45],
    }
}
