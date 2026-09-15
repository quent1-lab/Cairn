//! L'écologie végétale : croissance logistique de la biomasse
//! `dP/dt = r·P·(1 − P/K)` (BRIEF §2.4), où `K` est la capacité de charge du
//! biome (déjà dans la tuile) et `P` la biomasse courante.
//!
//! On n'intègre pas pas à pas : la logistique a une **solution en forme
//! fermée**, `P(t+Δ) = K·P / (P + (K−P)·e^(−rΔ))`, exacte pour n'importe quel
//! Δ. C'est la clé du LOD temporel du brief (§8.2) : un chunk ignoré des
//! semaines se rattrape en un calcul, au même résultat que s'il avait été
//! simulé chaque jour. La repousse tourne donc **une fois par jour**, et
//! seulement sur les chunks **sales** — les autres sont au baseline, c'est-à-
//! dire à l'équilibre : rien à faire.
//!
//! Deux subtilités dues au stockage `u8` :
//! - **arrondi stochastique** : une repousse de +0,4 unité serait tronquée à
//!   0 chaque jour et la végétation ne repousserait jamais. On arrondit vers
//!   le haut avec probabilité 0,4 — déterministe car le tirage dérive de la
//!   seed, du jour et de la tuile ; en espérance, la dynamique est exacte.
//! - **banque de graines** : la logistique ne repart jamais de P = 0. Une
//!   tuile rasée repousse depuis P = 1 (graines, rhizomes, dispersion) dès
//!   que le climat le permet.

use cairn_core::{Pcg32, SimTime, splitmix64};

use crate::CHUNK_SIZE;
use crate::climate::Climate;
use crate::salt;
use crate::world::World;

/// Taux de croissance logistique, par jour. À r = 0,08 : une tuile rasée de
/// prairie (K = 110) remonte à ~K/2 en ~2 mois, quasi pleine en 4 mois — une
/// saison de repousse, ordre de grandeur d'une strate herbacée.
pub const GROWTH_RATE_PER_DAY: f64 = 0.08;

/// Solution exacte de la logistique après `days` jours, en unités u8.
/// L'arrondi de la partie fractionnaire est tiré dans `rng` (stochastique).
pub fn logistic_step(p: u8, k: u8, days: f64, rng: &mut Pcg32) -> u8 {
    if k == 0 {
        return p; // sol stérile : rien ne pousse jamais
    }
    // Banque de graines : une tuile rasée repart d'un germe.
    let p0 = f64::from(p.max(1));
    let k_f = f64::from(k);
    let decay = (-GROWTH_RATE_PER_DAY * days).exp();
    let p1 = k_f * p0 / (p0 + (k_f - p0) * decay);
    // Arrondi stochastique : ⌊p1⌋ + 1 avec probabilité frac(p1).
    let base = p1.floor();
    let up = f64::from(rng.next_f32()) < (p1 - base);
    ((base as u8).saturating_add(up as u8)).min(k.max(p))
}

/// Passe quotidienne de repousse sur les chunks sales. Appelée à heure fixe
/// (minuit) ; l'ordre de parcours (BTreeSet) et les flux RNG par chunk sont
/// déterministes.
/// `weather` : les cellules actives (`crate::weather`). Leur décalage est
/// évalué **une fois par chunk**, à son centre, et non par tuile : une cellule
/// météo fait ~1,5 km quand un chunk en fait 128 m, si bien que l'approximation
/// est invisible — et le coût, nul, dans une boucle qui balaie 4096 tuiles.
pub fn daily_regrowth(
    world: &mut World,
    climate: &Climate,
    time: SimTime,
    weather: &[crate::weather::WeatherCell],
) {
    let eco_seed = world.seed().derive(salt::ECOLOGY) ^ splitmix64(time.tick);
    for coord in world.dirty_coords() {
        // Un flux RNG par (jour, chunk) : l'arrondi d'une tuile ne dépend pas
        // de ce que les autres chunks ont fait.
        let stream = (coord.x as u64) ^ (coord.y as u64).rotate_left(32);
        let mut rng = Pcg32::new(eco_seed, stream);
        let (ox, oy) = coord.origin();
        // Le ciel du jour sur ce chunk : la pluie relève la capacité de charge,
        // la sécheresse l'abaisse. C'est par là que le ciel affame — ou nourrit.
        let half = crate::chunk::CHUNK_SIZE as f64 / 2.0;
        let shift = crate::weather::shift_at((ox as f64 + half, oy as f64 + half), weather);
        let weather_factor = 1.0 + crate::weather::CAPACITY_EFFECT * shift;
        // Emprunts disjoints : la repousse mute les tuiles du chunk pendant que
        // le worldgen les calcule (génération paresseuse). C'est cette passe
        // qui borne le gain de la génération paresseuse — elle balaie les 4 096
        // tuiles, donc les force toutes à exister. Elle ne porte que sur les
        // chunks **sales**, soit 24 % des chunks générés (mesure M1) ; rendre
        // ce balayage épars est un incrément à part, qui changerait un
        // comportement (les tuiles jamais touchées cesseraient de repousser).
        let Some((chunk, worldgen)) = world.chunk_mut_with_gen(coord) else { continue };
        for ly in 0..CHUNK_SIZE as usize {
            for lx in 0..CHUNK_SIZE as usize {
                let tile = chunk.tile_mut(lx, ly, worldgen);
                // Le plafond effectif suit la fertilité : un sol dégradé
                // portera moins que le K du biome (surexploitation, §2.4).
                let cap = ((f32::from(effective_capacity(tile)) * weather_factor).clamp(0.0, 255.0))
                    as u8;
                if tile.biomass >= cap {
                    continue;
                }
                if !climate.grows(tile, oy + ly as i64, time) {
                    continue;
                }
                tile.biomass = logistic_step(tile.biomass, cap, 1.0, &mut rng);
            }
        }
        world.clear_dirty_if_pristine(coord);
    }
}

/// Capacité de charge effective d'une tuile : le K du biome, réduit
/// proportionnellement si la fertilité du sol a été dégradée sous sa valeur
/// de base. Tant que rien ne dégrade les sols (plus tard en Phase 2+), c'est
/// exactement `baseline_biomass`.
pub fn effective_capacity(tile: &crate::Tile) -> u8 {
    let base_fert = crate::tile::baseline_fertility(tile.biome);
    let k = crate::tile::baseline_biomass(tile.biome);
    if base_fert == 0 {
        return k;
    }
    let ratio = f64::from(tile.soil_fertility.min(base_fert)) / f64::from(base_fert);
    (f64::from(k) * ratio) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rng() -> Pcg32 {
        Pcg32::new(1234, 0)
    }

    #[test]
    fn repousse_vers_k_sans_jamais_depasser() {
        let mut rng = rng();
        let k = 110;
        let mut p = 5u8;
        let mut jours = 0;
        while p < k && jours < 400 {
            let suivant = logistic_step(p, k, 1.0, &mut rng);
            assert!(suivant >= p, "la repousse ne régresse pas");
            assert!(suivant <= k, "jamais au-dessus de K");
            p = suivant;
            jours += 1;
        }
        assert!(p >= k - 1, "n'atteint pas K en 400 jours (p = {p})");
        assert!(jours < 200, "repousse trop lente : {jours} jours");
    }

    #[test]
    fn une_tuile_rasee_repousse_quand_meme() {
        // Sans banque de graines, P = 0 serait un point fixe : mort définitive.
        let mut rng = rng();
        let mut p = 0u8;
        for _ in 0..60 {
            p = logistic_step(p, 110, 1.0, &mut rng);
        }
        assert!(p > 3, "aucune recolonisation depuis 0 (p = {p})");
    }

    #[test]
    fn la_forme_fermee_egale_les_petits_pas() {
        // 30 jours d'un coup ≈ 30 × 1 jour : c'est le contrat du rattrapage
        // analytique (LOD temporel). En u8 avec arrondi stochastique, on
        // tolère un écart de quantification de quelques unités.
        let mut a = rng();
        let mut b = rng();
        let k = 210;
        let direct = logistic_step(20, k, 30.0, &mut a);
        let mut pas_a_pas = 20u8;
        for _ in 0..30 {
            pas_a_pas = logistic_step(pas_a_pas, k, 1.0, &mut b);
        }
        let ecart = (i16::from(direct) - i16::from(pas_a_pas)).abs();
        assert!(ecart <= 6, "forme fermée {direct} vs pas à pas {pas_a_pas}");
    }

    #[test]
    fn sol_sterile_reste_sterile() {
        let mut rng = rng();
        assert_eq!(logistic_step(0, 0, 100.0, &mut rng), 0);
    }
}
