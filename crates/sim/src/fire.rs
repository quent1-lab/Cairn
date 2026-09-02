//! Les feux de forêt (BRIEF §5.3 « foudre → feu observé », §2.4, Phase 5
//! « L'ÉTINCELLE », incrément 5) : la **seconde voie** d'exposition au feu,
//! celle qui ne suppose pas de l'avoir déjà inventé (arbitrage « les deux »
//! acté avec l'utilisateur). Un monde qui brûle parfois tout seul, sans
//! divinité (elle n'arrive qu'en Phase 6, et ne fera qu'accélérer) — c'est ce
//! qui rend le critère « le feu découvert sans intervention » atteignable par
//! l'observation autant que par la friction.
//!
//! ## Un registre clairsemé, jamais un champ de tuile
//!
//! Comme les structures et le territoire (Phase 4), un feu est un phénomène
//! **rare et localisé** : il vit dans un `Vec<Fire>` côté `Sim`, pas dans les
//! 16 octets de chaque tuile. Ce qu'il **écrit**, en revanche — la biomasse
//! consumée — est un champ *mutable* déjà géré par le système de deltas
//! (`world::ChunkDelta`), donc parfaitement légitime à muter (contrairement au
//! territoire diffusé, qui ne pouvait pas l'être). Brûler, c'est mettre le
//! fourrage à zéro ; l'écologie le fait repousser ensuite.
//!
//! ## Le foyer minimal (choix d'implémentation acté)
//!
//! Un feu est un **disque** qui croît selon le combustible puis s'éteint en
//! quelques jours — pas une propagation cellulaire de tuile en tuile (reportée,
//! elle affinerait l'émergence mais alourdirait la gestion du front dans le
//! monde chunké). Deux rayons **découplés**, pour la même raison d'échelle qui
//! a guidé toute la Phase 4 :
//! - le rayon de **combustion** reste petit (`FIRE_MAX_RADIUS_TILES`, ~80 m) —
//!   il coûte un `tile_mut` par tuile du disque, borné à quelques milliers ;
//! - le rayon de **visibilité** est large (`FIRE_SIGHT_MARGIN_TILES`, ~1 km) —
//!   on voit un incendie (et sa fumée) de loin, et l'exposer aux agents proches
//!   ne coûte qu'un test de distance. C'est l'exposition qui importe pour
//!   l'arbre technologique ; elle est donc généreuse et bon marché, tandis que
//!   la combustion, coûteuse, reste modeste.
//!
//! ## Ambivalence
//!
//! Le feu détruit le fourrage → la faim monte → la pression d'innovation
//! monte (`pressure::ClanPressure::famine`) : « la nécessité, mère de
//! l'invention ». Il ne blesse pas encore les agents directement — le « brûler
//! vif » ambivalent viendra avec la foudre divine (Phase 6, §6.2).

use std::collections::BTreeSet;

use cairn_core::{Pcg32, TICKS_PER_DAY, km_to_tiles};

use crate::agent::Position;
use crate::chronicle::EventKind;
use crate::exposure::{Exposure, Exposures};
use crate::salt;
use crate::sim::Sim;
use crate::social::{ClanId, ClanMembership};
use crate::tile::Tile;
use crate::world::World;

/// Chance de base d'un départ de feu par jour, près de la population. C'est une
/// *base* : le tirage n'aboutit que si le site tiré est réellement inflammable
/// (sec **et** pourvu de combustible), si bien que le taux effectif s'effondre
/// en climat humide et grimpe en savane/prairie sèche — une saisonnalité
/// géographique émergente, sans règle dédiée. Calibrable.
const FIRE_IGNITION_CHANCE_PER_DAY: f64 = 0.1;
/// Un départ de feu s'allume entre 0,5 et 3 km d'un habitant pris au hasard :
/// assez près pour être vu et compter, pas sur sa tête.
const FIRE_IGNITION_MIN_TILES: f64 = km_to_tiles(0.5);
const FIRE_IGNITION_MAX_TILES: f64 = km_to_tiles(3.0);

/// Combustible minimal (biomasse 0–255) pour qu'une tuile prenne feu.
const FUEL_MIN_BIOMASS: u8 = 60;
/// Humidité maximale (0–255) : au-delà, le sol est trop humide pour brûler.
/// ~120/255 ≈ 47 % — les forêts tropicales détrempées ne s'enflamment guère,
/// les savanes et prairies sèches oui.
const DRY_MAX_HUMIDITY: u8 = 120;
/// Température moyenne minimale : rien ne brûle dans le gel permanent.
const WARM_MIN_TEMP: f32 = 5.0;

/// Rayon initial d'un foyer (~16 m).
const FIRE_START_RADIUS_TILES: f64 = 8.0;
/// Croissance du rayon par jour, tant que le feu vit.
const FIRE_GROWTH_PER_DAY_TILES: f64 = 8.0;
/// Rayon de combustion maximal (~80 m) : petit, pour borner le coût en
/// `tile_mut` (voir l'en-tête).
const FIRE_MAX_RADIUS_TILES: f64 = 40.0;
/// Durée de vie d'un feu, en jours.
const FIRE_DURATION_DAYS: u64 = 4;
/// Marge de **visibilité** au-delà du rayon de combustion (~1 km) : on voit un
/// incendie de bien plus loin qu'il ne brûle.
const FIRE_SIGHT_MARGIN_TILES: f64 = km_to_tiles(1.0);

/// Un feu actif : un disque de combustion, son rayon courant et son âge en
/// jours. `Copy` — quelques octets, comme les autres composants légers.
#[derive(Debug, Clone, Copy)]
pub struct Fire {
    pub pos: (f64, f64),
    pub radius: f64,
    pub age_days: u64,
}

/// Une tuile peut-elle s'enflammer ? Sèche, pourvue de combustible, tempérée,
/// et sur la terre ferme.
fn is_flammable(tile: &Tile) -> bool {
    tile.is_walkable()
        && tile.biomass >= FUEL_MIN_BIOMASS
        && tile.humidity <= DRY_MAX_HUMIDITY
        && tile.temperature >= WARM_MIN_TEMP
}

/// Allume un foyer en un point donné, **si le site peut brûler**. La cause
/// importe peu — friction, foudre naturelle, ou foudre divine (§6.2) : c'est le
/// site qui décide, pas le déclencheur. Renvoie `true` si le feu a pris.
///
/// `pub(crate)` : `crate::divine` en a besoin pour la foudre, et c'est
/// exactement l'ambivalence que le brief demande — frapper une savane sèche
/// offre le feu à un peuple, frapper une tourbière détrempée ne fait rien.
pub(crate) fn ignite_at(sim: &mut Sim, pos: (f64, f64)) -> bool {
    let tile = sim.world.tile(pos.0.floor() as i64, pos.1.floor() as i64);
    if !is_flammable(&tile) {
        return false;
    }
    sim.fires.push(Fire { pos, radius: FIRE_START_RADIUS_TILES, age_days: 0 });
    true
}

/// La passe quotidienne du feu : faire vivre les foyers existants (croître,
/// brûler, exposer, vieillir, s'éteindre), puis tenter un nouveau départ.
pub(crate) fn daily(sim: &mut Sim) {
    update_fires(sim);
    if sim.allow_wildfires {
        try_ignite(sim);
    }
}

/// Fait vivre les feux d'un jour. No-op s'il n'y en a aucun (coût nul quand le
/// monde ne brûle pas — le cas de très loin le plus fréquent).
fn update_fires(sim: &mut Sim) {
    if sim.fires.is_empty() {
        return;
    }
    // Croissance, puis combustion : chaque foyer met à zéro la biomasse de son
    // disque (le `tile_mut` marque le chunk sale — l'écologie le fera repousser).
    for i in 0..sim.fires.len() {
        sim.fires[i].radius = (sim.fires[i].radius + FIRE_GROWTH_PER_DAY_TILES).min(FIRE_MAX_RADIUS_TILES);
        let f = sim.fires[i];
        burn_disk(&mut sim.world, f.pos, f.radius);
    }
    // Quels peuples avaient déjà vu le feu ? Relevé **avant** d'exposer qui que
    // ce soit : c'est la comparaison avant/après qui fait la notabilité (voir
    // `EventKind::FireWitnessed`). `BTreeSet` — jamais de `HashSet` dans la
    // simulation, l'ordre d'itération doit rester déterministe.
    let mut already: BTreeSet<u64> = BTreeSet::new();
    for (_, (exposures, membership)) in sim.agents.query::<(&Exposures, &ClanMembership)>().iter() {
        if exposures.has(Exposure::Fire)
            && let Some(clan) = membership.0
        {
            already.insert(clan.0);
        }
    }

    // Exposition : tout agent à portée de vue d'un feu voit le feu.
    let fires: Vec<(f64, f64, f64)> = sim.fires.iter().map(|f| (f.pos.0, f.pos.1, f.radius)).collect();
    let mut discovering: BTreeSet<u64> = BTreeSet::new();
    for (_, (pos, exposures, membership)) in
        sim.agents.query_mut::<(&Position, &mut Exposures, &ClanMembership)>()
    {
        let seen = fires.iter().any(|&(fx, fy, r)| {
            (pos.x - fx).hypot(pos.y - fy) <= r + FIRE_SIGHT_MARGIN_TILES
        });
        if seen {
            exposures.expose(Exposure::Fire);
            if let Some(clan) = membership.0
                && !already.contains(&clan.0)
            {
                discovering.insert(clan.0);
            }
        }
    }

    // Les peuples qui découvrent le feu aujourd'hui : un fait chacun, une seule
    // fois dans leur histoire. Consigné à leur foyer (le lieu du peuple, pas
    // celui du brasier) — c'est de lui qu'on parle, et c'est sa latitude qui
    // donne la saison du récit.
    let born: Vec<(ClanId, (i64, i64))> = sim
        .clans
        .iter()
        .filter(|c| discovering.contains(&c.id.0))
        .map(|c| (c.id, (c.home.0.floor() as i64, c.home.1.floor() as i64)))
        .collect();
    for (clan, pos) in born {
        sim.record(pos, EventKind::FireWitnessed { clan });
    }
    // Vieillissement et extinction.
    for fire in &mut sim.fires {
        fire.age_days += 1;
    }
    sim.fires.retain(|f| f.age_days < FIRE_DURATION_DAYS);
}

/// Met à zéro la biomasse des tuiles de terre dans le disque `(centre, rayon)`.
fn burn_disk(world: &mut World, center: (f64, f64), radius: f64) {
    let (cx, cy) = center;
    let x0 = (cx - radius).floor() as i64;
    let x1 = (cx + radius).ceil() as i64;
    let y0 = (cy - radius).floor() as i64;
    let y1 = (cy + radius).ceil() as i64;
    let r2 = radius * radius;
    for ty in y0..=y1 {
        for tx in x0..=x1 {
            let dx = tx as f64 + 0.5 - cx;
            let dy = ty as f64 + 0.5 - cy;
            if dx * dx + dy * dy <= r2 {
                let tile = world.tile_mut(tx, ty);
                if tile.is_walkable() {
                    tile.biomass = 0;
                }
            }
        }
    }
}

/// Tente un départ de feu : un tirage quotidien, un site près d'un habitant, un
/// allumage seulement si ce site est réellement inflammable.
fn try_ignite(sim: &mut Sim) {
    let day = sim.time.tick / TICKS_PER_DAY;
    let mut rng = Pcg32::new(sim.world.seed().derive(salt::FIRE), day);
    if rng.next_f64() >= FIRE_IGNITION_CHANCE_PER_DAY {
        return;
    }
    let anchors: Vec<(f64, f64)> =
        sim.agents.query::<&Position>().iter().map(|(_, p)| (p.x, p.y)).collect();
    if anchors.is_empty() {
        return;
    }
    let anchor = anchors[(rng.next_u32() as usize) % anchors.len()];
    let angle = rng.next_f64() * std::f64::consts::TAU;
    let dist = FIRE_IGNITION_MIN_TILES + rng.next_f64() * (FIRE_IGNITION_MAX_TILES - FIRE_IGNITION_MIN_TILES);
    let pos = (anchor.0 + angle.cos() * dist, anchor.1 + angle.sin() * dist);
    // Rien dans la Chronique ici : un départ de feu n'est pas un fait. Il en
    // devient un le jour où un peuple le voit pour la première fois — c'est là
    // que se joue la seconde voie d'accès au feu (§5.3), celle qui n'a besoin
    // d'aucun silex. Voir `update_fires`.
    ignite_at(sim, pos);
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_core::WorldSeed;

    /// Cherche une tuile inflammable près de l'origine (via le worldgen, sans
    /// générer de chunks pour rien) — pour ancrer un test déterministe sur une
    /// vraie tuile qui brûle.
    fn find_flammable(sim: &mut Sim) -> Option<(i64, i64)> {
        let step = km_to_tiles(8.0) as i64;
        for r in 0..300i64 {
            let d = r * step;
            for &(x, y) in &[(d, 0), (-d, 0), (0, d), (0, -d), (d, d), (-d, -d)] {
                if is_flammable(&sim.world.tile(x, y)) {
                    return Some((x, y));
                }
            }
        }
        None
    }

    #[test]
    fn un_feu_brule_le_fourrage_et_expose_les_agents_proches() {
        let mut sim = Sim::new(WorldSeed(42), 256);
        let Some((fx, fy)) = find_flammable(&mut sim) else {
            return; // seed sans zone sèche proche : test sans objet (rare)
        };
        // Un agent tout près du futur foyer, un autre très loin.
        let near = sim.spawn_agent(fx as f64 + 0.5, fy as f64 + 0.5);
        let far = sim.spawn_agent(fx as f64 + 100_000.0, fy as f64 + 0.5);
        // On allume à la main (l'ignition aléatoire est testée à part).
        sim.fires.push(Fire { pos: (fx as f64 + 0.5, fy as f64 + 0.5), radius: FIRE_START_RADIUS_TILES, age_days: 0 });
        // Quelques jours de feu.
        for _ in 0..FIRE_DURATION_DAYS {
            sim.time.tick += TICKS_PER_DAY;
            update_fires(&mut sim);
        }
        // La biomasse du foyer est consumée.
        assert_eq!(sim.world.tile(fx, fy).biomass, 0, "le fourrage doit avoir brûlé");
        // Le feu s'est éteint après sa durée de vie.
        assert!(sim.fires.is_empty(), "le feu doit finir par s'éteindre");
        // L'agent proche a vu le feu ; le lointain non.
        let exposed = |id| {
            sim.agents
                .query::<(&crate::agent::AgentId, &Exposures)>()
                .iter()
                .find(|(_, (a, _))| **a == id)
                .map(|(_, (_, e))| e.has(Exposure::Fire))
                .unwrap()
        };
        assert!(exposed(near), "l'agent proche doit être exposé au feu");
        assert!(!exposed(far), "l'agent lointain ne voit rien");
    }

    /// Notabilité : un peuple n'entre dans la Chronique qu'à **son premier**
    /// feu. Dans une savane sèche il brûle plusieurs fois par an — journaliser
    /// chaque départ noyait le récit sous la météo (mesuré : 80 faits sur 93 en
    /// 11 ans). Ce qui fait date, c'est le jour où l'on voit le feu, et il n'y
    /// en a qu'un.
    #[test]
    fn un_peuple_n_entre_dans_la_chronique_qu_a_son_premier_feu() {
        use crate::social::{Clan, ClanId, ClanMembership};

        let mut sim = Sim::new(WorldSeed(1), 64);
        let member = sim.spawn_agent(0.0, 0.0);
        for (_, membership) in sim.agents.query_mut::<&mut ClanMembership>() {
            membership.0 = Some(ClanId(1));
        }
        sim.clans.push(Clan {
            id: ClanId(1),
            founded_tick: 0,
            members: [member].into_iter().collect(),
            home: (0.0, 0.0),
            stock: 0.0,
            chief: member,
            desired: None,
            rivalry: 0.0,
        });

        // Premier incendie sous leurs yeux : cela fait date.
        sim.fires.push(Fire { pos: (0.0, 0.0), radius: FIRE_START_RADIUS_TILES, age_days: 0 });
        update_fires(&mut sim);
        assert_eq!(sim.chronicle.len(), 1, "le premier feu vu entre dans la Chronique");
        assert!(matches!(
            sim.chronicle[0].kind,
            crate::chronicle::EventKind::FireWitnessed { clan } if clan == ClanId(1)
        ));

        // Les suivants sont du climat, plus de l'histoire.
        for _ in 0..5 {
            sim.fires.push(Fire { pos: (0.0, 0.0), radius: FIRE_START_RADIUS_TILES, age_days: 0 });
            update_fires(&mut sim);
        }
        assert_eq!(sim.chronicle.len(), 1, "les incendies suivants ne se racontent plus");
    }

    #[test]
    fn une_tuile_humide_ou_nue_ne_prend_pas_feu() {
        let dry_fuel = Tile {
            biome: cairn_worldgen::Biome::Savanna,
            rock: cairn_worldgen::RockType::Sedimentary,
            deposit: cairn_worldgen::Deposit::None,
            elevation: 0.5,
            temperature: 25.0,
            humidity: 80,
            soil_fertility: 100,
            biomass: 120,
            flags: crate::tile::TileFlags::default(),
        };
        assert!(is_flammable(&dry_fuel), "savane sèche et fournie : inflammable");
        assert!(!is_flammable(&Tile { humidity: 200, ..dry_fuel }), "trop humide");
        assert!(!is_flammable(&Tile { biomass: 10, ..dry_fuel }), "pas assez de combustible");
        assert!(!is_flammable(&Tile { temperature: -5.0, ..dry_fuel }), "trop froid");
    }
}
