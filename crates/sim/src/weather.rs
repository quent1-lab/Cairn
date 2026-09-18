//! **La météo** (BRIEF §2.5) : averses et sécheresses passagères, par-dessus le
//! climat moyen que le worldgen a figé.
//!
//! ## Un phénomène, pas un effet divin
//!
//! La divinité peut déclencher une pluie (§6.2), mais **la pluie ne lui
//! appartient pas** : elle naît d'elle-même, et [`start`] est le même chemin
//! pour les deux causes — exactement comme un feu se moque de savoir si c'est
//! la friction, la foudre ou un dieu qui l'a allumé (`fire::ignite_at`). Le
//! jour où une vraie météo dirigée par les vents arrivera, elle appellera cette
//! même fonction ; rien de ce module n'aura à changer.
//!
//! ## Ce que ça change, et l'ambivalence qui en tombe
//!
//! Une cellule ne touche **jamais** `Tile::humidity` : elle vit dans un registre
//! clairsemé côté `Sim`, comme les feux et les structures. Deux raisons — le
//! chunking ne sait persister que deux champs mutables (fertilité, biomasse), et
//! surtout **une averse est temporaire** par nature. On lit donc un *décalage*
//! ([`shift_at`]) là où l'humidité compte vraiment :
//!
//! - la **capacité de charge** de la végétation (`ecology`) : il pousse plus
//!   sous la pluie, moins sous la sécheresse ;
//! - l'**inflammabilité** (`fire`) : un sol détrempé ne prend pas feu.
//!
//! De là, l'ambivalence, qu'aucune ligne n'a besoin d'écrire :
//!
//! - **la pluie** nourrit la végétation — et **éteint la voie du feu**. Un
//!   peuple sans silex ne découvre la maîtrise du feu qu'en voyant brûler
//!   (`EventKind::FireWitnessed`) ; bénir sa vallée d'une pluie généreuse, c'est
//!   le condamner à ne jamais voir d'incendie.
//! - **la sécheresse** affame — et c'est le moteur de l'invention (`pressure`),
//!   en même temps qu'elle rend la brousse inflammable. La disette qui pousse à
//!   penser et le feu qui attend d'être vu arrivent par le même geste.
//!
//! Le joueur ne choisit donc pas entre faire le bien et faire le mal : il choisit
//! ce qu'il sacrifie.

use cairn_core::{Pcg32, TICKS_PER_DAY, km_to_tiles};

use crate::agent::Position;
use crate::salt;
use crate::sim::Sim;

/// Rayon d'une cellule (~1,5 km) : une averse locale, pas un front régional.
pub(crate) const WEATHER_RADIUS_TILES: f64 = km_to_tiles(1.5);
/// Durée de vie d'une cellule, en jours.
const WEATHER_DURATION_DAYS: u64 = 5;
/// Chance qu'une cellule naisse d'elle-même un jour donné, près des habitants.
/// Comme pour les feux, on ne simule pas la météo du monde entier : seulement là
/// où quelqu'un peut la subir.
const WEATHER_CHANCE_PER_DAY: f64 = 0.06;
/// Distance à laquelle une cellule naturelle se forme d'un habitant.
const WEATHER_SPAWN_MIN_TILES: f64 = km_to_tiles(0.5);
const WEATHER_SPAWN_MAX_TILES: f64 = km_to_tiles(4.0);

/// De combien la capacité de charge végétale varie sous une cellule pleine
/// (±40 %) : une bonne pluie verdit, une sécheresse grille.
pub const CAPACITY_EFFECT: f32 = 0.4;
/// De combien l'humidité perçue varie sous une cellule pleine, en unités de
/// tuile (0–255). Assez pour faire basculer un sol de part et d'autre du seuil
/// d'inflammabilité (`fire`), ce qui est tout l'enjeu.
pub const HUMIDITY_EFFECT: f32 = 70.0;

/// Averse ou sécheresse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WeatherKind {
    Rain,
    Drought,
}

/// Une cellule météo active. `Copy` et légère, comme [`crate::fire::Fire`].
#[derive(Debug, Clone, Copy)]
pub struct WeatherCell {
    pub pos: (f64, f64),
    pub radius: f64,
    pub kind: WeatherKind,
    pub age_days: u64,
}

/// Déclenche une cellule. **Le seul chemin**, quelle que soit la cause — la
/// météo naturelle et la divinité passent tous deux par ici.
pub(crate) fn start(sim: &mut Sim, pos: (f64, f64), kind: WeatherKind) {
    sim.weather.push(WeatherCell { pos, radius: WEATHER_RADIUS_TILES, kind, age_days: 0 });
}

/// Le décalage d'humidité en un point, dans `[-1, 1]` : `+1` sous le cœur d'une
/// averse, `-1` sous celui d'une sécheresse, nul au-delà des cellules.
///
/// Décroît linéairement du centre au bord — une averse n'a pas de frontière
/// nette. Les cellules qui se recouvrent s'ajoutent, puis on borne : deux pluies
/// superposées ne font pas un déluge, et pluie + sécheresse se neutralisent, ce
/// qui est physiquement raisonnable.
pub fn shift_at(point: (f64, f64), cells: &[WeatherCell]) -> f32 {
    let mut shift = 0.0f32;
    for cell in cells {
        let d = (point.0 - cell.pos.0).hypot(point.1 - cell.pos.1);
        if d >= cell.radius {
            continue;
        }
        let strength = 1.0 - (d / cell.radius) as f32;
        shift += match cell.kind {
            WeatherKind::Rain => strength,
            WeatherKind::Drought => -strength,
        };
    }
    shift.clamp(-1.0, 1.0)
}

/// La passe quotidienne : vieillir les cellules, dissiper les épuisées, puis
/// tenter une naissance naturelle. No-op complet quand le ciel est calme.
pub(crate) fn daily(sim: &mut Sim) {
    for cell in &mut sim.weather {
        cell.age_days += 1;
    }
    sim.weather.retain(|c| c.age_days < WEATHER_DURATION_DAYS);
    if sim.allow_weather {
        try_form(sim);
    }
}

/// Tente une cellule naturelle. **Le climat local décide de son signe** : un
/// pays humide reçoit des averses, un pays sec des sécheresses — la même
/// géographie qui gouverne déjà les incendies, sans règle dédiée.
fn try_form(sim: &mut Sim) {
    let day = sim.time.tick / TICKS_PER_DAY;
    let mut rng = Pcg32::new(sim.world.seed().derive(salt::WEATHER), day);
    if rng.next_f64() >= WEATHER_CHANCE_PER_DAY {
        return;
    }
    let anchors: Vec<(f64, f64)> =
        sim.agents.query::<&Position>().iter().map(|(_, p)| (p.x, p.y)).collect();
    if anchors.is_empty() {
        return;
    }
    let anchor = anchors[(rng.next_u32() as usize) % anchors.len()];
    let angle = rng.next_f64() * std::f64::consts::TAU;
    let dist = WEATHER_SPAWN_MIN_TILES
        + rng.next_f64() * (WEATHER_SPAWN_MAX_TILES - WEATHER_SPAWN_MIN_TILES);
    let pos = (anchor.0 + angle.cos() * dist, anchor.1 + angle.sin() * dist);

    // L'humidité du lieu donne la probabilité qu'il pleuve plutôt qu'il sèche.
    let tile = sim.world.tile(pos.0.floor() as i64, pos.1.floor() as i64);
    let wetness = f64::from(tile.humidity) / 255.0;
    let kind = if rng.next_f64() < wetness { WeatherKind::Rain } else { WeatherKind::Drought };
    start(sim, pos, kind);
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_core::WorldSeed;

    fn cell(kind: WeatherKind, x: f64) -> WeatherCell {
        WeatherCell { pos: (x, 0.0), radius: WEATHER_RADIUS_TILES, kind, age_days: 0 }
    }

    #[test]
    fn le_decalage_est_maximal_au_centre_et_nul_au_loin() {
        let cells = vec![cell(WeatherKind::Rain, 0.0)];
        assert!((shift_at((0.0, 0.0), &cells) - 1.0).abs() < 1e-5, "plein centre : +1");
        let bord = shift_at((WEATHER_RADIUS_TILES * 0.5, 0.0), &cells);
        assert!((bord - 0.5).abs() < 0.01, "mi-rayon : ~0,5 ({bord:.3})");
        assert_eq!(shift_at((WEATHER_RADIUS_TILES + 1.0, 0.0), &cells), 0.0, "au-delà : rien");
    }

    /// Les signes sont opposés, et deux cellules contraires se neutralisent —
    /// c'est ce qui fait que « pluie » et « sécheresse » sont bien un seul
    /// mécanisme et non deux systèmes qui pourraient diverger.
    #[test]
    fn pluie_et_secheresse_sont_de_signes_opposes_et_se_compensent() {
        assert!(shift_at((0.0, 0.0), &[cell(WeatherKind::Rain, 0.0)]) > 0.0);
        assert!(shift_at((0.0, 0.0), &[cell(WeatherKind::Drought, 0.0)]) < 0.0);
        let opposees = vec![cell(WeatherKind::Rain, 0.0), cell(WeatherKind::Drought, 0.0)];
        assert_eq!(shift_at((0.0, 0.0), &opposees), 0.0, "elles s'annulent");
    }

    /// Une cellule se dissipe d'elle-même : la météo est passagère, c'est
    /// précisément pourquoi elle ne touche jamais `Tile::humidity`.
    #[test]
    fn une_cellule_se_dissipe() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        sim.allow_weather = false; // pas de naissance parasite pendant le test
        start(&mut sim, (0.0, 0.0), WeatherKind::Rain);
        assert_eq!(sim.weather.len(), 1);
        for _ in 0..WEATHER_DURATION_DAYS {
            daily(&mut sim);
        }
        assert!(sim.weather.is_empty(), "l'averse doit finir par passer");
    }
}
