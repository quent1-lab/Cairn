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
    humans: &[(f64, f64)],
    lod_period: u64,
) {
    let eco_seed = world.seed().derive(salt::ECOLOGY) ^ splitmix64(time.tick);
    let today = time.tick / cairn_core::TICKS_PER_DAY;
    for coord in world.dirty_coords() {
        // — LOD de la flore, exactement ce que le BRIEF §8.2 décrit : « un
        //   chunk sans agent n'est pas simulé tick par tick ; sa végétation est
        //   rattrapée analytiquement au moment de l'accès ». `logistic_step`
        //   prend déjà une durée, donc rattraper N jours coûte le même calcul
        //   qu'en rattraper un — la logistique a une solution exacte.
        //
        //   Le rattrapage n'est pas une approximation de N pas quotidiens :
        //   c'est la **même courbe**, échantillonnée moins souvent. Seul
        //   l'arrondi stochastique diffère, et il diffère en mieux (moins de
        //   bruit d'arrondi accumulé). —
        let last = world.last_regrowth.get(&coord).copied().unwrap_or(today);
        let elapsed = today.saturating_sub(last).max(1);
        if lod_period > 1 && elapsed < lod_period {
            let (ox0, oy0) = coord.origin();
            let half0 = crate::chunk::CHUNK_SIZE as f64 / 2.0;
            let c = (ox0 as f64 + half0, oy0 as f64 + half0);
            let r = cairn_core::km_to_tiles(4.0);
            let vu = humans.iter().any(|h| {
                (h.0 - c.0).powi(2) + (h.1 - c.1).powi(2) <= r * r
            });
            if !vu {
                continue;
            }
        }
        world.last_regrowth.insert(coord, today);
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
        // — Balayage épars. Une tuile jamais mutée porte exactement le baseline
        //   que `compute_tile` lui a donné : `biomass == baseline_biomass(biome)`
        //   et `soil_fertility == baseline_fertility(biome)`, donc
        //   `effective_capacity(tile) == tile.biomass`. Le test `biomass >= cap`
        //   la rejette immédiatement. Le balayage complet payait donc 4 096
        //   lectures pour la poignée de tuiles broutées — et, sous génération
        //   paresseuse, **forçait le calcul** des 4 095 autres. `Chunk::touched`
        //   liste précisément celles qui peuvent avoir quelque chose à faire.
        //
        //   **La seule exception est la pluie.** `weather_factor > 1` relève le
        //   plafond *au-dessus* du baseline : une tuile intacte a alors
        //   légitimement de quoi pousser, et l'ignorer supprimerait l'effet du
        //   ciel sur les terres non broutées. Ce chunk-là reprend le balayage
        //   complet — et ce qu'il fait pousser entre dans `touched`, faute de
        //   quoi l'éviction le rendrait au baseline sans que rien ne le dise
        //   (`regrow_tile` s'en charge).
        //
        //   La sécheresse ne demande rien : `weather_factor < 1` abaisse le
        //   plafond, mais cette passe ne fait jamais décroître une biomasse. —
        //
        //   **L'ordre de visite fait partie du contrat.** `logistic_step` tire
        //   dans le flux du chunk, donc le balayage épars doit servir les
        //   mêmes tuiles *dans le même ordre* que le balayage complet — sinon
        //   les tirages restent en même nombre mais changent de destinataire.
        //   Mesuré : l'ordre naturel du `BTreeSet` (trié par lx, donc par
        //   colonnes) décalait une tuile d'une unité de biomasse au 20ᵉ jour,
        //   et la faune, exponentiellement instable, en faisait 984 troupeaux
        //   contre 421 au 600ᵉ. On trie donc par lignes, comme la double
        //   boucle d'origine.
        let Some((chunk, worldgen)) = world.chunk_mut_with_gen(coord) else { continue };
        if weather_factor > 1.0 {
            for ly in 0..CHUNK_SIZE as usize {
                for lx in 0..CHUNK_SIZE as usize {
                    regrow_tile(
                        chunk, worldgen, lx, ly, oy, climate, time, weather_factor, elapsed,
                        &mut rng,
                    );
                }
            }
        } else {
            // `touched` est emprunté en lecture, `tile_mut` en écriture : on
            // recopie les quelques dizaines de coordonnées plutôt que de
            // rescanner 4 096 tuiles pour les retrouver.
            let mut cibles: Vec<(u8, u8)> = chunk.touched().iter().copied().collect();
            cibles.sort_unstable_by_key(|&(lx, ly)| (ly, lx));
            for (lx, ly) in cibles {
                regrow_tile(
                    chunk,
                    worldgen,
                    lx as usize,
                    ly as usize,
                    oy,
                    climate,
                    time,
                    weather_factor,
                    elapsed,
                    &mut rng,
                );
            }
        }
        world.clear_dirty_if_pristine(coord);
    }
}

/// Repousse d'une tuile, **et l'inscrit dans `Chunk::touched` si elle a
/// effectivement poussé**. Ce marquage vivait avant chez l'appelant, sous
/// forme d'un `bool` de retour que les deux appelants ignoraient : le
/// balayage épars n'en avait pas besoin (ses tuiles sont déjà marquées) et le
/// balayage complet de la pluie l'oubliait. Une tuile intacte poussée par
/// l'averse n'entrait donc dans aucun registre — ni l'instantané d'éviction
/// (`world::snapshot_delta`), ni le test de retour au baseline — et repartait
/// silencieusement au baseline à la première éviction. Le marquage appartient
/// à l'écriture, pas à ses appelants : c'est la seule place où on ne peut pas
/// l'oublier.
/// `oy` est l'ordonnée absolue de l'origine du chunk (la latitude compte : le
/// climat ne fait pas pousser partout à la même saison).
#[allow(clippy::too_many_arguments)]
fn regrow_tile(
    chunk: &mut crate::chunk::Chunk,
    worldgen: &cairn_worldgen::WorldGen,
    lx: usize,
    ly: usize,
    oy: i64,
    climate: &Climate,
    time: SimTime,
    weather_factor: f32,
    elapsed: u64,
    rng: &mut Pcg32,
) {
    let tile = chunk.tile_mut(lx, ly, worldgen);
    // Le plafond effectif suit la fertilité : un sol dégradé portera moins
    // que le K du biome (surexploitation, §2.4).
    let cap = ((f32::from(effective_capacity(tile)) * weather_factor).clamp(0.0, 255.0)) as u8;
    if tile.biomass >= cap {
        return;
    }
    if !climate.grows(tile, oy + ly as i64, time) {
        return;
    }
    tile.biomass = logistic_step(tile.biomass, cap, elapsed as f64, rng);
    // L'emprunt mutable de `tile` s'arrête à la ligne au-dessus : le
    // compilateur laisse donc reprendre `chunk` ici (NLL).
    chunk.mark_touched(lx, ly);
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

    use crate::world::World;
    use cairn_core::WorldSeed;

    /// Une tuile qui pousse vraiment : biome fertile, et le climat du jour
    /// l'autorise. Sans ce filtrage, un test posé au hasard tomberait sur de
    /// l'océan ou sur un hiver, et passerait pour de mauvaises raisons.
    fn tuile_qui_pousse(world: &mut World, time: SimTime, climate: &Climate) -> (i64, i64) {
        for i in 0..4000i64 {
            let (x, y) = (700_000 + i * 37, 1_050_000 + i * 53);
            let tile = world.tile(x, y);
            if crate::tile::baseline_biomass(tile.biome) > 20 && climate.grows(&tile, y, time) {
                return (x, y);
            }
        }
        panic!("aucune tuile fertile trouvée pour le test");
    }

    fn scene() -> (World, Climate, SimTime) {
        let world = World::new(WorldSeed(42), 64);
        let climate = Climate::new(world.worldgen().temperature.latitude());
        // Plein été de l'hémisphère nord : on ne veut pas mesurer un hiver.
        let time = SimTime { tick: 180 * cairn_core::TICKS_PER_DAY };
        (world, climate, time)
    }

    #[test]
    fn une_tuile_broutee_repousse_sans_pluie() {
        // Le balayage épars doit rester équivalent au balayage complet sur ce
        // qui compte : les tuiles effectivement modifiées.
        let (mut world, climate, time) = scene();
        let (x, y) = tuile_qui_pousse(&mut world, time, &climate);
        let cap = crate::tile::baseline_biomass(world.tile(x, y).biome);
        world.tile_mut(x, y).biomass = 1;

        // Une journée depuis P = 1 donne P₁ ≈ 1,08 : l'arrondi stochastique la
        // laisse à 1 neuf fois sur dix. Ce qu'on vérifie, c'est la repousse
        // d'une saison — le régime dans lequel la passe est réellement utile.
        let mut t = time;
        for _ in 0..60 {
            daily_regrowth(&mut world, &climate, t, &[], &[], 1);
            t.tick += cairn_core::TICKS_PER_DAY;
        }

        let apres = world.tile(x, y).biomass;
        assert!(apres > 20, "la tuile broutée n'a pas repoussé en 60 jours (à {apres})");
        assert!(apres <= cap, "repousse au-delà de la capacité : {apres} > {cap}");
    }

    #[test]
    fn sous_la_pluie_une_tuile_intacte_depasse_son_baseline() {
        // C'est l'exception que le balayage épars doit préserver. Une tuile
        // jamais touchée est à sa capacité *de temps sec* ; l'averse relève
        // cette capacité de 40 %, et elle a donc légitimement de quoi pousser.
        // Un balayage épars sans exception météo ne la visiterait jamais : ce
        // test échoue alors, et c'est exactement ce qu'il est là pour attraper.
        let (mut world, climate, time) = scene();
        let (x, y) = tuile_qui_pousse(&mut world, time, &climate);
        let baseline = world.tile(x, y).biomass;
        assert_eq!(
            baseline,
            crate::tile::baseline_biomass(world.tile(x, y).biome),
            "la tuile témoin doit être intacte"
        );

        // Le chunk doit être sale pour que la passe le visite : on salit une
        // *autre* tuile du même chunk, jamais celle qu'on observe.
        let (vx, vy) = (x ^ 1, y ^ 1);
        assert_ne!((vx, vy), (x, y));
        world.tile_mut(vx, vy).biomass = 1;

        let averse = [crate::weather::WeatherCell {
            pos: (x as f64, y as f64),
            radius: crate::weather::WEATHER_RADIUS_TILES,
            kind: crate::weather::WeatherKind::Rain,
            age_days: 0,
        }];
        daily_regrowth(&mut world, &climate, time, &averse, &[], 1);

        let apres = world.tile(x, y).biomass;
        assert!(
            apres > baseline,
            "l'averse n'a pas fait pousser la tuile intacte ({baseline} → {apres})"
        );
    }

    #[test]
    fn la_pousse_sous_la_pluie_survit_a_une_eviction() {
        // Règle 5, deux compteurs qui doivent concorder : ce que la tuile vaut
        // juste après l'averse, et ce qu'elle vaut après un aller-retour par
        // l'éviction. L'instantané d'éviction ne parcourt que `Chunk::touched`,
        // et la passe de repousse écrit par `Chunk::tile_mut`, qui n'y inscrit
        // rien : une tuile intacte poussée par la pluie n'existe donc que dans
        // le chunk résident, et repart au baseline sans que personne ne le dise.
        //
        // Capacité 8 : le chunk observé est forcément évincé dès qu'on regarde
        // ailleurs, comme dans les tests d'éviction de `world`.
        let mut world = World::new(WorldSeed(42), 8);
        let climate = Climate::new(world.worldgen().temperature.latitude());
        let time = SimTime { tick: 180 * cairn_core::TICKS_PER_DAY };
        let (x, y) = tuile_qui_pousse(&mut world, time, &climate);
        let baseline = crate::tile::baseline_biomass(world.tile(x, y).biome);
        assert_eq!(world.tile(x, y).biomass, baseline, "la tuile témoin doit être intacte");

        // Salir une *autre* tuile du chunk : c'est ce qui rend le chunk sale,
        // donc visible de la passe, sans toucher celle qu'on observe.
        world.tile_mut(x ^ 1, y ^ 1).biomass = 1;

        let averse = [crate::weather::WeatherCell {
            pos: (x as f64, y as f64),
            radius: crate::weather::WEATHER_RADIUS_TILES,
            kind: crate::weather::WeatherKind::Rain,
            age_days: 0,
        }];
        // Plusieurs jours d'averse : une seule journée depuis le baseline ne
        // gagne parfois qu'une fraction d'unité, que l'arrondi stochastique
        // peut laisser à zéro. Le test ne doit pas dépendre d'un tirage.
        let mut t = time;
        for _ in 0..5 {
            daily_regrowth(&mut world, &climate, t, &averse, &[], 1);
            t.tick += cairn_core::TICKS_PER_DAY;
        }
        let pousse = world.tile(x, y).biomass;
        assert!(pousse > baseline, "l'averse n'a rien fait pousser : le test ne prouverait rien");

        // On s'éloigne : le chunk est évincé, et seul son instantané le suit.
        for cx in 50..70 {
            world.chunk(crate::chunk::ChunkCoord { x: cx, y: cx });
        }
        assert!(world.evicted > 0, "le chunk observé aurait dû être évincé");

        assert_eq!(
            world.tile(x, y).biomass,
            pousse,
            "la pousse due à la pluie a été perdue à l'éviction : \
             la tuile n'était pas dans l'instantané"
        );
    }

}
