//! Banc **nature sans humains** (chantier CAP, étape 2) : ce que les seules
//! règles de la faune produisent dans un biome, sans bande, sans immigration,
//! sans bord ni abandon (`allow_fauna_immigration = false`, aucun agent).
//!
//! Scène : un carré de 12 km (6 × 6 mailles de pâturage) choisi pour que le
//! biome visé y couvre au moins 75 % des centres de maille, le plus près du
//! point de référence ; un troupeau de `HERD_START` têtes tous les 1,5 km sur
//! toute tuile praticable portant de l'herbe (toundra comprise, contrairement à
//! `scenario::populate`) ; quatre meutes à 4 km du centre, posées une seule
//! fois. Le monde reste ouvert : les troupeaux peuvent sortir du carré.
//!
//! Deux compteurs (règle 5) : la densité dans le carré de recensement, et la
//! charge de ses mailles (demande ÷ production, 1 à l'équilibre du fourrage).
//! Plus la densité sur les mailles occupées, où qu'elles soient.
//!
//! Lecture seule : rien de ce qu'il relève ne touche la trajectoire.
//!
//! Usage : `cargo run --release -p cairn-sim --example cap_nature -- [seeds] [biomes] [années] [out.csv]`
//! (seeds et biomes séparés par des virgules ; biomes parmi
//! foret,taiga,toundra,prairie,steppe,savane).

use std::collections::BTreeMap;
use std::io::Write;

use cairn_core::{TICKS_PER_DAY, WorldSeed, km_to_tiles};
use cairn_sim::fauna::{self, Herd, Pack};
use cairn_sim::{Position, Sim, scenario};
use cairn_worldgen::Biome;

/// Mailles de pâturage par côté du carré de recensement.
const SIDE_ZONES: i64 = 6;
/// Pas d'échantillonnage, en jours : la croissance du modèle se compte en
/// semaines (doublement en 69 jours au plus).
const SAMPLE_DAYS: u64 = 5;
const START_DAY: u64 = 135;

fn biome_of(name: &str) -> Biome {
    match name {
        "foret" => Biome::TemperateForest,
        "taiga" => Biome::Taiga,
        "toundra" => Biome::Tundra,
        "prairie" => Biome::Grassland,
        "steppe" => Biome::Steppe,
        "savane" => Biome::Savanna,
        _ => panic!("biome inconnu : {name}"),
    }
}

/// Coin (en mailles) d'un carré où le biome couvre ≥ 75 % des centres de
/// maille, le plus près du point de référence (balayage par anneaux de 4 km).
fn find_square(sim: &Sim, biome: Biome) -> Option<(i64, i64)> {
    let side = fauna::RANGE_ZONE_TILES;
    let origin = scenario::reference_seed_point();
    let z0 = fauna::range_zone((origin.0 as f64, origin.1 as f64));
    for ring in 0..200i64 {
        let r = ring * 2; // 2 mailles = 4 km
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs() != r && dy.abs() != r {
                    continue;
                }
                let corner = (z0.0 + dx, z0.1 + dy);
                let mut hit = 0;
                for j in 0..SIDE_ZONES {
                    for i in 0..SIDE_ZONES {
                        let x = (corner.0 + i) as f64 * side + side / 2.0;
                        let y = (corner.1 + j) as f64 * side + side / 2.0;
                        if sim.world.worldgen().biome(x as i64, y as i64) == biome {
                            hit += 1;
                        }
                    }
                }
                if hit * 4 >= SIDE_ZONES * SIDE_ZONES * 3 {
                    return Some(corner);
                }
            }
        }
    }
    None
}

fn main() {
    let mut args = std::env::args().skip(1);
    let seeds: Vec<u64> = args
        .next()
        .unwrap_or_else(|| "42".into())
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    let biomes: Vec<String> = args
        .next()
        .unwrap_or_else(|| "foret,taiga,toundra,prairie,steppe,savane".into())
        .split(',')
        .map(String::from)
        .collect();
    let years: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    let out = args.next().unwrap_or_else(|| "out/cap/cap_nature.csv".into());
    if let Some(dir) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    let mut csv = std::fs::File::create(&out).unwrap();
    writeln!(
        csv,
        "seed,biome,day,season,heads,herds,predators,packs,sq_heads,sq_density,sq_load,sq_satiation,\
         occ_zones,occ_density,growth,predation,dissolved,pack_condition,pack_meat_days,pack_young,pack_share,pack_local"
    )
    .unwrap();

    for &seed in &seeds {
        for name in &biomes {
            let biome = biome_of(name);
            let mut sim = Sim::new(WorldSeed(seed), scenario::REFERENCE_CAPACITY);
            scenario::start_on_day(&mut sim, START_DAY);
            sim.allow_fauna_immigration = false;
            let Some(corner) = find_square(&sim, biome) else {
                println!("seed {seed} {name} : aucun carré trouvé");
                continue;
            };
            let side = fauna::RANGE_ZONE_TILES;
            let x0 = corner.0 as f64 * side;
            let y0 = corner.1 as f64 * side;
            let span = SIDE_ZONES as f64 * side;
            let center = (x0 + span / 2.0, y0 + span / 2.0);

            // Placement : un troupeau tous les 1,5 km.
            let pas = km_to_tiles(1.5);
            let mut gy = y0 + pas / 2.0;
            while gy < y0 + span {
                let mut gx = x0 + pas / 2.0;
                while gx < x0 + span {
                    let tile = sim.world.tile(gx as i64, gy as i64);
                    if tile.is_walkable() && tile.biomass > 0 {
                        sim.spawn_herd(gx, gy, fauna::HERD_START);
                    }
                    gx += pas;
                }
                gy += pas;
            }
            for i in 0..4i64 {
                let angle = i as f64 * 1.57;
                let (x, y) = (center.0 + angle.cos() * km_to_tiles(4.0), center.1 + angle.sin() * km_to_tiles(4.0));
                if sim.world.tile(x as i64, y as i64).is_walkable() {
                    sim.spawn_pack(x, y, fauna::PACK_START);
                }
            }
            // Production des mailles du carré (kg/jour) : pure du worldgen.
            let zones: Vec<(i64, i64)> = (0..SIDE_ZONES)
                .flat_map(|j| (0..SIDE_ZONES).map(move |i| (corner.0 + i, corner.1 + j)))
                .collect();
            let production: f32 = zones.iter().map(|&z| sim.rangeland.production(sim.world.worldgen(), z)).sum();
            let in_square = |p: (f64, f64)| p.0 >= x0 && p.0 < x0 + span && p.1 >= y0 && p.1 < y0 + span;
            let area_km2 = (SIDE_ZONES * SIDE_ZONES * 4) as f32;
            let started = std::time::Instant::now();
            let mut year_rows: Vec<(f32, f32, f32)> = Vec::new();
            sim.fauna_flows = Default::default();

            for day in 0..years * 360 {
                for _ in 0..TICKS_PER_DAY {
                    sim.step();
                }
                if (day + 1) % SAMPLE_DAYS != 0 {
                    continue;
                }
                let mut heads = 0.0f32;
                let mut herds = 0usize;
                let mut sq_heads = 0.0f32;
                let mut sq_demand = 0.0f32;
                let mut sq_sat = 0.0f32;
                let mut occ: BTreeMap<(i64, i64), f32> = BTreeMap::new();
                for (_, (h, p)) in sim.fauna.query::<(&Herd, &Position)>().iter() {
                    heads += h.population;
                    herds += 1;
                    *occ.entry(fauna::range_zone((p.x, p.y))).or_insert(0.0) += h.population;
                    if in_square((p.x, p.y)) {
                        sq_heads += h.population;
                        sq_demand += h.population * h.species.daily_ration_kg();
                        sq_sat += h.population * h.satiation.min(1.0);
                    }
                }
                let (mut predators, mut packs) = (0.0f32, 0usize);
                let (mut p_cond, mut p_meat, mut p_young) = (0.0f32, 0.0f32, 0.0f32);
                for (_, p) in sim.fauna.query::<&Pack>().iter() {
                    predators += p.population;
                    packs += 1;
                    p_cond += p.condition * p.population;
                    p_meat += p.meat;
                    p_young += p.young;
                }
                // Part des loups qui mangent (territoire, C3) et loups à 8 km, vus
                // de chaque meute, moyennés à proportion des loups (contre-lecture 3.3).
                let packs_pos: Vec<((f64, f64), f32)> =
                    sim.fauna.query::<(&Pack, &Position)>().iter().map(|(_, (p, q))| ((q.x, q.y), p.population)).collect();
                let r2 = fauna::PREDATOR_TERRITORY_TILES * fauna::PREDATOR_TERRITORY_TILES;
                let (mut share_w, mut local_w) = (0.0f32, 0.0f32);
                for &(p0, n0) in &packs_pos {
                    let local: f32 = packs_pos
                        .iter()
                        .filter(|(p, _)| (p.0 - p0.0).powi(2) + (p.1 - p0.1).powi(2) <= r2)
                        .map(|(_, n)| n)
                        .sum();
                    share_w += n0;
                    local_w += local * n0;
                }
                let pack_share = if predators > 0.0 { share_w / predators } else { 0.0 };
                let pack_local = if predators > 0.0 { local_w / predators } else { 0.0 };
                // Réserves moyennes des loups, viande en jours de ration, louveteaux (CAP 3.3).
                let pack_condition = if predators > 0.0 { p_cond / predators } else { 0.0 };
                let pack_meat_days = if predators > 0.0 { p_meat / (predators * fauna::WOLF_RATION_KG_DAY) } else { 0.0 };
                let season = sim
                    .climate
                    .season(center.1 as i64, sim.time)
                    .map_or("-".to_string(), |s| format!("{s:?}"));
                let sq_density = sq_heads / area_km2;
                let sq_load = sq_demand / production.max(1e-6);
                let sq_satiation = if sq_heads > 0.0 { sq_sat / sq_heads } else { 0.0 };
                let occ_density = if occ.is_empty() { 0.0 } else { heads / (occ.len() as f32 * 4.0) };
                let f = sim.fauna_flows;
                writeln!(
                    csv,
                    "{seed},{name},{},{season},{heads:.0},{herds},{predators:.1},{packs},{sq_heads:.0},{sq_density:.2},\
                     {sq_load:.3},{sq_satiation:.3},{},{occ_density:.2},{:.1},{:.1},{:.1},{pack_condition:.3},{pack_meat_days:.2},{p_young:.2},{pack_share:.3},{pack_local:.1}",
                    day + 1,
                    occ.len(),
                    f.growth,
                    f.predation,
                    f.dissolved
                )
                .unwrap();
                csv.flush().unwrap();
                sim.fauna_flows = Default::default();
                year_rows.push((sq_density, sq_load, predators));
                if (day + 1) % 360 == 0 {
                    let mut d: Vec<f32> = year_rows.iter().map(|r| r.0).collect();
                    d.sort_by(f32::total_cmp);
                    let load_med = {
                        let mut l: Vec<f32> = year_rows.iter().map(|r| r.1).collect();
                        l.sort_by(f32::total_cmp);
                        l[l.len() / 2]
                    };
                    println!(
                        "seed {seed} {name} an {} : carré min {:.1} méd {:.1} têtes/km², charge méd {load_med:.2}, \
                         occupé {occ_density:.1} /km² sur {} mailles, prédateurs {predators:.0}, {herds} troupeaux, {:.0} s",
                        (day + 1) / 360,
                        d[0],
                        d[d.len() / 2],
                        occ.len(),
                        started.elapsed().as_secs_f64()
                    );
                    year_rows.clear();
                }
            }
        }
    }
}
