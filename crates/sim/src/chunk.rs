//! Le chunk : un carré de 64 × 64 tuiles, unité de génération et de
//! déchargement (BRIEF §2.1).

use std::collections::BTreeSet;

use cairn_core::splitmix64;
use cairn_worldgen::{Biome, WorldGen};

use crate::salt;
use crate::tile::{Tile, TileFlags, baseline_biomass, baseline_fertility};

/// Côté d'un chunk, en tuiles.
pub const CHUNK_SIZE: i64 = 64;
/// Nombre de tuiles par chunk.
pub const CHUNK_AREA: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize;

/// Coordonnées d'un chunk (et non d'une tuile) : le chunk (cx, cy) couvre les
/// tuiles [cx·64, cx·64+64) × [cy·64, cy·64+64). En `i64` : monde infini.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkCoord {
    pub x: i64,
    pub y: i64,
}

impl ChunkCoord {
    /// Coin supérieur-gauche du chunk, en coordonnées de tuiles.
    pub fn origin(self) -> (i64, i64) {
        (self.x * CHUNK_SIZE, self.y * CHUNK_SIZE)
    }
}

pub struct Chunk {
    pub coord: ChunkCoord,
    tiles: Vec<Tile>,
    /// Un bit par tuile **déjà calculée**. 512 octets pour 64 Kio de tuiles,
    /// soit 0,8 % de surcoût mémoire.
    ///
    /// Génération paresseuse : calculer les 4 096 tuiles d'un chunk à sa
    /// naissance coûtait le bruit du worldgen sur chacune, alors que la mesure
    /// M1 (63 591 vies de chunk) en compte **18,9 réellement lues en moyenne,
    /// soit 0,46 %** — et près d'un chunk sur quatre n'en voit qu'**une seule**.
    /// On payait donc environ 216 tuiles générées par tuile utile.
    ready: Box<[u64; 64]>,
    /// Les quatre coins d'humidité, calculés une fois à la naissance du chunk.
    /// Ils sont **partagés avec les chunks voisins** (d'où l'absence de couture)
    /// et l'humidité varie sur ~256 km : les garder ici est ce qui permet de
    /// calculer une tuile isolée sans rien recalculer d'autre.
    humidity_corners: [f64; 4],
    spring_seed: u64,
    /// Positions locales des sources d'eau douce du chunk. Redondant avec les
    /// drapeaux des tuiles, mais permet à un agent de chercher « la source la
    /// plus proche » en parcourant quelques listes courtes au lieu de dizaines
    /// de milliers de tuiles.
    pub springs: Vec<(u8, u8)>,
    /// Tuiles locales déjà **écrites** : `World::tile_mut` (broutage,
    /// cueillette) et la repousse de l'écologie, qui passe par `Chunk::tile_mut`
    /// et doit donc s'inscrire elle-même (voir `ecology::regrow_tile`).
    /// Sert uniquement l'instantané d'éviction (`world::snapshot_delta`) : sans
    /// lui, retrouver les quelques tuiles modifiées obligerait à comparer les
    /// 4096 tuiles au baseline à chaque éviction — mesuré, ce scan complet
    /// coûtait plus qu'il ne faisait gagner l'éviction non protégée.
    touched: BTreeSet<(u8, u8)>,
}

impl Chunk {
    /// Génère le chunk depuis le baseline procédural. Déterministe : même
    /// seed, même chunk, bit pour bit.
    ///
    /// L'humidité est échantillonnée aux **4 coins** puis interpolée
    /// bilinéairement : elle varie sur ~256 km, un chunk fait 128 m, donc la
    /// calculer par tuile gâcherait 4096× le travail. Les coins étant
    /// **partagés** avec les chunks voisins, le champ reste continu d'un chunk
    /// à l'autre — aucune couture.
    pub fn generate(coord: ChunkCoord, world: &WorldGen) -> Self {
        let (x0, y0) = coord.origin();
        let humidity_corners = [
            world.humidity(x0, y0),
            world.humidity(x0 + CHUNK_SIZE, y0),
            world.humidity(x0, y0 + CHUNK_SIZE),
            world.humidity(x0 + CHUNK_SIZE, y0 + CHUNK_SIZE),
        ];
        // Les sources par le chemin rapide (préfiltre par hachage) plutôt qu'en
        // sous-produit de la génération complète : `springs_for` est garanti
        // bit-identique par un test, et c'est ce qui permet de ne plus calculer
        // les 4 096 tuiles ici.
        let springs = springs_for(world, coord);
        Self {
            coord,
            tiles: vec![Tile::UNCOMPUTED; CHUNK_AREA],
            ready: Box::new([0u64; 64]),
            humidity_corners,
            spring_seed: world.seed().derive(salt::SPRINGS),
            springs,
            touched: BTreeSet::new(),
        }
    }

    /// Calcule la tuile locale (lx, ly) depuis le baseline — fonction **pure**
    /// de sa coordonnée, donc indépendante de l'ordre dans lequel les tuiles
    /// sont demandées. C'est cette pureté qui rend la génération paresseuse
    /// sûre vis-à-vis du déterminisme bit-à-bit.
    fn compute_tile(&self, lx: usize, ly: usize, world: &WorldGen) -> Tile {
        let (x0, y0) = self.coord.origin();
        let (x, y) = (x0 + lx as i64, y0 + ly as i64);
        let fx = lx as f64 / CHUNK_SIZE as f64;
        let fy = ly as f64 / CHUNK_SIZE as f64;
        let humidity = bilerp(self.humidity_corners, fx, fy);

        let elevation = world.elevation(x, y);
        let temperature = world.mean_temperature(x, y, elevation);
        let biome = Biome::classify(elevation, temperature, humidity);

        let mut flags = TileFlags::default();
        if elevation <= 0.0 {
            flags.set(TileFlags::WATER);
            if matches!(biome, Biome::Coast) {
                flags.set(TileFlags::COAST);
            }
            if temperature < cairn_worldgen::FREEZE_STILL_C {
                flags.set(TileFlags::FROZEN);
            }
        } else if is_spring(self.spring_seed, x, y, humidity) {
            flags.set(TileFlags::FRESH_WATER);
        }

        Tile {
            biome,
            rock: world.rock_type(x, y),
            deposit: world.deposit(x, y, elevation),
            elevation: elevation as f32,
            temperature: temperature as f32,
            humidity: (humidity.clamp(0.0, 1.0) * 255.0) as u8,
            soil_fertility: baseline_fertility(biome),
            biomass: baseline_biomass(biome),
            flags,
        }
    }

    /// Garantit que la tuile locale est calculée.
    fn ensure(&mut self, lx: usize, ly: usize, world: &WorldGen) {
        let bit = ly * CHUNK_SIZE as usize + lx;
        let (word, mask) = (bit / 64, 1u64 << (bit % 64));
        if self.ready[word] & mask != 0 {
            return;
        }
        self.tiles[bit] = self.compute_tile(lx, ly, world);
        self.ready[word] |= mask;
    }

    /// Tuile locale (lx, ly), avec 0 ≤ lx, ly < 64 — calculée à la demande.
    pub fn tile(&mut self, lx: usize, ly: usize, world: &WorldGen) -> &Tile {
        self.ensure(lx, ly, world);
        &self.tiles[ly * CHUNK_SIZE as usize + lx]
    }

    /// Tuile locale **sans la calculer** : réservé aux lectures dont on sait
    /// déjà qu'elles portent sur une tuile générée — celles de `touched`, qui
    /// n'y entre que par `tile_mut`. Évite de traîner un `&WorldGen` dans
    /// l'instantané d'éviction, qui ne fait que relire ce qu'il a lui-même
    /// modifié.
    pub(crate) fn tile_ready(&self, lx: usize, ly: usize) -> &Tile {
        debug_assert!(
            self.ready[(ly * CHUNK_SIZE as usize + lx) / 64]
                & (1u64 << ((ly * CHUNK_SIZE as usize + lx) % 64))
                != 0,
            "tile_ready sur une tuile jamais calculée"
        );
        &self.tiles[ly * CHUNK_SIZE as usize + lx]
    }

    /// Les tuiles locales déjà notées comme mutées (voir le champ `touched`).
    pub(crate) fn touched(&self) -> &BTreeSet<(u8, u8)> {
        &self.touched
    }

    /// Note qu'une tuile va être mutée — sert l'instantané d'éviction, pas la
    /// simulation elle-même (aucun ordre à respecter, un `BTreeSet` suffit).
    pub(crate) fn mark_touched(&mut self, lx: usize, ly: usize) {
        self.touched.insert((lx as u8, ly as u8));
    }

    /// Retire une tuile des tuiles notées mutées. Réservé à une tuile revenue
    /// **exactement** à son baseline : elle n'a plus rien à dire à la
    /// repousse, et une éviction la régénérerait à l'identique (voir
    /// `ecology::daily_regrowth`).
    pub(crate) fn unmark_touched(&mut self, lx: usize, ly: usize) {
        self.touched.remove(&(lx as u8, ly as u8));
    }

    /// Accès mutable à la tuile locale. Réservé au [`World`](crate::World),
    /// qui doit marquer le chunk sale — passer par `World::tile_mut`.
    pub(crate) fn tile_mut(&mut self, lx: usize, ly: usize, world: &WorldGen) -> &mut Tile {
        self.ensure(lx, ly, world);
        &mut self.tiles[ly * CHUNK_SIZE as usize + lx]
    }
}

/// Densité maximale de sources (en air saturé), en sources par tuile.
const SPRING_DENSITY_MAX: f64 = 1.6e-5;

/// Hachage par tuile : deux passes de SplitMix64 chaînées pour que x et y
/// soient mélangés sans symétrie (x, y) ↔ (y, x).
fn spring_hash(spring_seed: u64, x: i64, y: i64) -> u64 {
    splitmix64(splitmix64(spring_seed ^ x as u64) ^ y as u64)
}

/// Une source d'eau douce jaillit-elle en (x, y) ?
///
/// **Proxy d'hydrologie, assumé et provisoire** : les rivières inter-chunks ne
/// sont pas encore matérialisées (amélioration en attente de Phase 1), or la
/// Phase 2 a besoin d'eau buvable pour que la soif soit un vrai problème
/// spatial. Des sources ponctuelles, pure fonction de la seed — donc sans
/// couture et compatibles avec l'éviction —, dont la densité suit l'humidité :
/// l'eau reste rare là où la géographie la rend rare (désert ≈ 1 source pour
/// plusieurs km), abondante en zone humide (≈ 1 pour quelques centaines de m).
fn is_spring(spring_seed: u64, x: i64, y: i64, humidity: f64) -> bool {
    // Densité en sources/tuile : quadratique en humidité, pour creuser
    // l'écart humide/aride. h=0,5 → distance moyenne ~500 m ; h=0,15 → ~1,7 km.
    let density = SPRING_DENSITY_MAX * humidity.clamp(0.0, 1.0).powi(2);
    (spring_hash(spring_seed, x, y) as f64) < density * u64::MAX as f64
}

/// Les sources d'un chunk, calculées **sans générer le chunk** — c'est la
/// requête des agents assoiffés, qui scrute des dizaines de chunks alentour :
/// matérialiser 4 096 tuiles pour trouver 0,07 source en moyenne ruinerait le
/// LRU (mesuré : ~300 générations/tick, la simulation s'effondrait).
///
/// Deux étages :
/// 1. **préfiltre** par hachage à la densité maximale — pur calcul entier,
///    ~93 % des chunks s'arrêtent là (aucun candidat) ;
/// 2. pour les rares candidats, le vrai test avec l'humidité bilerpée **selon
///    exactement la même formule que `generate`** — les deux chemins doivent
///    rester bit-identiques (un test le garantit).
pub(crate) fn springs_for(world: &WorldGen, coord: ChunkCoord) -> Vec<(u8, u8)> {
    let spring_seed = world.seed().derive(salt::SPRINGS);
    let (x0, y0) = coord.origin();
    let threshold = SPRING_DENSITY_MAX * u64::MAX as f64;
    let mut candidates = Vec::new();
    for ly in 0..CHUNK_SIZE {
        for lx in 0..CHUNK_SIZE {
            if (spring_hash(spring_seed, x0 + lx, y0 + ly) as f64) < threshold {
                candidates.push((lx as u8, ly as u8));
            }
        }
    }
    if candidates.is_empty() {
        return candidates;
    }
    let h = [
        world.humidity(x0, y0),
        world.humidity(x0 + CHUNK_SIZE, y0),
        world.humidity(x0, y0 + CHUNK_SIZE),
        world.humidity(x0 + CHUNK_SIZE, y0 + CHUNK_SIZE),
    ];
    candidates.retain(|&(lx, ly)| {
        let (x, y) = (x0 + lx as i64, y0 + ly as i64);
        let fx = lx as f64 / CHUNK_SIZE as f64;
        let fy = ly as f64 / CHUNK_SIZE as f64;
        world.elevation(x, y) > 0.0 && is_spring(spring_seed, x, y, bilerp(h, fx, fy))
    });
    candidates
}

/// Interpolation bilinéaire dans le carré unité entre 4 coins
/// `[haut-gauche, haut-droit, bas-gauche, bas-droit]`.
fn bilerp(c: [f64; 4], fx: f64, fy: f64) -> f64 {
    let top = c[0] + (c[1] - c[0]) * fx;
    let bottom = c[2] + (c[3] - c[2]) * fx;
    top + (bottom - top) * fy
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_core::WorldSeed;

    #[test]
    fn chunk_deterministe() {
        let wg = WorldGen::new(WorldSeed(42));
        let coord = ChunkCoord { x: 3, y: -7 };
        let a = Chunk::generate(coord, &wg);
        let b = Chunk::generate(coord, &wg);
        for i in 0..CHUNK_AREA {
            assert_eq!(a.tiles[i], b.tiles[i]);
        }
    }

    #[test]
    fn les_sources_suivent_l_humidite() {
        // Sur un million de tuiles fictives, l'aride doit porter nettement
        // moins de sources que l'humide — c'est le contrat du proxy.
        let seed = 0xC0FFEE;
        let count = |humidity: f64| {
            (0..1_000_000i64)
                .filter(|&i| is_spring(seed, i % 1000, i / 1000, humidity))
                .count()
        };
        let humide = count(0.8);
        let aride = count(0.15);
        // Densités attendues : 1,02e-5 vs 3,6e-7 par tuile.
        assert!(humide > 5 * aride.max(1), "humide {humide}, aride {aride}");
        assert!(humide > 0, "aucune source même en zone humide");
    }

    #[test]
    fn le_calcul_rapide_des_sources_egale_la_generation() {
        // Le chemin « sans générer » (préfiltre + confirmation) doit donner
        // bit à bit la même liste que la génération complète du chunk.
        let wg = WorldGen::new(WorldSeed(42));
        for cy in -6..6 {
            for cx in -6..6 {
                let coord = ChunkCoord { x: cx * 17, y: cy * 23 };
                let chunk = Chunk::generate(coord, &wg);
                assert_eq!(chunk.springs, springs_for(&wg, coord), "chunk {coord:?}");
            }
        }
    }

    #[test]
    fn la_liste_des_sources_colle_aux_drapeaux() {
        let wg = WorldGen::new(WorldSeed(42));
        // Balaye quelques chunks : chaque entrée de `springs` doit pointer une
        // tuile FRESH_WATER, et le compte total doit correspondre.
        for cy in -2..2 {
            for cx in -2..2 {
                let mut chunk = Chunk::generate(ChunkCoord { x: cx, y: cy }, &wg);
                let mut par_drapeau = 0;
                for ly in 0..CHUNK_SIZE as usize {
                    for lx in 0..CHUNK_SIZE as usize {
                        if chunk.tile(lx, ly, &wg).has_fresh_water() {
                            par_drapeau += 1;
                        }
                    }
                }
                assert_eq!(par_drapeau, chunk.springs.len());
                let sources = chunk.springs.clone();
                for (lx, ly) in sources {
                    assert!(chunk.tile(lx as usize, ly as usize, &wg).has_fresh_water());
                }
            }
        }
    }

    #[test]
    fn tuile_du_chunk_colle_au_worldgen() {
        // La matérialisation ne doit rien inventer : l'altitude d'une tuile
        // vaut exactement celle du worldgen au même endroit (garantie
        // « aucune couture »).
        let wg = WorldGen::new(WorldSeed(42));
        let coord = ChunkCoord { x: -2, y: 5 };
        let mut chunk = Chunk::generate(coord, &wg);
        let (x0, y0) = coord.origin();
        for &(lx, ly) in &[(0usize, 0usize), (63, 63), (17, 40)] {
            let (x, y) = (x0 + lx as i64, y0 + ly as i64);
            assert_eq!(chunk.tile(lx, ly, &wg).elevation, wg.elevation(x, y) as f32);
        }
    }
}
