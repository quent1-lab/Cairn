//! Le gestionnaire de chunks : génère à la demande, garde en mémoire un
//! nombre borné de chunks, décharge les moins récemment utilisés (LRU).
//!
//! C'est la pièce qui rend le monde **infini de fait mais borné en mémoire**
//! (BRIEF §2.1). L'éviction est sûre tant qu'un chunk est du baseline pur :
//! décharger puis régénérer redonne l'identique. Depuis la Phase 2, une tuile
//! peut être **modifiée** (biomasse consommée…) : le chunk est alors marqué
//! **sale** et n'est plus jamais évincé — l'évincer perdrait l'état simulé.
//! Il redevient évincable si la repousse le ramène exactement au baseline.
//! La persistance des chunks sales (pour lever cette rétention) arrive en
//! Phase 6.

use std::collections::{BTreeMap, BTreeSet};

use cairn_core::WorldSeed;
use cairn_worldgen::{WorldGen, WorldGenConfig};

use crate::chunk::{CHUNK_SIZE, Chunk, ChunkCoord};
use crate::tile::{Tile, baseline_biomass, baseline_fertility};

pub struct World {
    worldgen: WorldGen,
    /// Chunks résidents. `BTreeMap` et non `HashMap` : ordre d'itération
    /// déterministe, exigence du projet.
    chunks: BTreeMap<ChunkCoord, Chunk>,
    /// Tick d'accès le plus récent par chunk, pour le LRU.
    last_access: BTreeMap<ChunkCoord, u64>,
    /// Chunks modifiés depuis leur génération : non régénérables, donc
    /// protégés de l'éviction.
    dirty: BTreeSet<ChunkCoord>,
    /// Cache des sources d'eau par chunk, calculées **sans** générer le chunk
    /// (voir `chunk::springs_for`). Jamais évincé : une entrée pèse quelques
    /// dizaines d'octets, et la requête « où est l'eau ? » porte sur le
    /// baseline pur — pas besoin de matérialiser des tuiles pour y répondre.
    springs: BTreeMap<ChunkCoord, Vec<(u8, u8)>>,
    /// Horloge logique : incrémentée à chaque accès.
    clock: u64,
    /// Nombre maximal de chunks résidents.
    capacity: usize,
    /// Statistiques cumulées (observabilité de la mémoire).
    pub generated: u64,
    pub evicted: u64,
}

impl World {
    pub fn new(seed: WorldSeed, capacity: usize) -> Self {
        Self::with_config(seed, capacity, WorldGenConfig::default())
    }

    /// Comme [`new`](Self::new), mais avec une config de worldgen — le client
    /// s'en sert pour une humidité « rapide » afin de rester interactif.
    pub fn with_config(seed: WorldSeed, capacity: usize, cfg: WorldGenConfig) -> Self {
        Self {
            worldgen: WorldGen::with_config(seed, cfg),
            chunks: BTreeMap::new(),
            last_access: BTreeMap::new(),
            dirty: BTreeSet::new(),
            springs: BTreeMap::new(),
            clock: 0,
            capacity: capacity.max(1),
            generated: 0,
            evicted: 0,
        }
    }

    /// Nombre de chunks actuellement en mémoire.
    pub fn loaded(&self) -> usize {
        self.chunks.len()
    }

    /// Nombre de chunks retenus car modifiés.
    pub fn dirty_count(&self) -> usize {
        self.dirty.len()
    }

    pub fn seed(&self) -> WorldSeed {
        self.worldgen.seed()
    }

    /// Le baseline procédural sous-jacent (lecture seule — il est pur).
    pub fn worldgen(&self) -> &WorldGen {
        &self.worldgen
    }

    /// Renvoie le chunk demandé, le générant s'il est absent. Marque l'accès
    /// (pour le LRU) et évince si la capacité est dépassée.
    pub fn chunk(&mut self, coord: ChunkCoord) -> &Chunk {
        self.clock += 1;
        if !self.chunks.contains_key(&coord) {
            let chunk = Chunk::generate(coord, &self.worldgen);
            self.chunks.insert(coord, chunk);
            self.generated += 1;
            self.evict_down_to_capacity(coord);
        }
        self.last_access.insert(coord, self.clock);
        &self.chunks[&coord]
    }

    /// Tuile à la coordonnée de tuile (x, y) — charge le chunk au besoin.
    /// Renvoie une copie : la tuile est `Copy`, pas besoin de garder le chunk
    /// emprunté.
    pub fn tile(&mut self, x: i64, y: i64) -> Tile {
        let (coord, lx, ly) = split(x, y);
        *self.chunk(coord).tile(lx, ly)
    }

    /// Accès **mutable** à une tuile : marque le chunk sale, donc protégé de
    /// l'éviction. C'est l'unique porte d'entrée de la simulation vers l'état
    /// du monde — tout ce qui évolue passe ici.
    pub fn tile_mut(&mut self, x: i64, y: i64) -> &mut Tile {
        let (coord, lx, ly) = split(x, y);
        // S'assure que le chunk est résident (et paie l'éviction éventuelle)…
        self.chunk(coord);
        self.dirty.insert(coord);
        // …puis le remprunte en mutable. `get_mut` ne peut pas échouer :
        // le chunk vient d'être chargé et `keep` interdit son éviction.
        self.chunks.get_mut(&coord).unwrap().tile_mut(lx, ly)
    }

    /// Les chunks actuellement sales, dans l'ordre déterministe du BTreeSet.
    /// C'est le domaine de travail de l'écologie : seuls eux dévient du
    /// baseline, donc seuls eux ont quelque chose à faire repousser.
    pub fn dirty_coords(&self) -> Vec<ChunkCoord> {
        self.dirty.iter().copied().collect()
    }

    /// Accès mutable direct à un chunk **déjà résident** (les chunks sales le
    /// sont toujours). Réservé aux systèmes du crate ; ne marque pas sale.
    pub(crate) fn chunk_mut(&mut self, coord: ChunkCoord) -> Option<&mut Chunk> {
        self.chunks.get_mut(&coord)
    }

    /// Si le chunk est revenu exactement au baseline (repousse complète),
    /// lève la protection : il redevient régénérable donc évincable.
    pub(crate) fn clear_dirty_if_pristine(&mut self, coord: ChunkCoord) {
        let Some(chunk) = self.chunks.get(&coord) else { return };
        let pristine = (0..CHUNK_SIZE as usize).all(|ly| {
            (0..CHUNK_SIZE as usize).all(|lx| {
                let t = chunk.tile(lx, ly);
                t.biomass == baseline_biomass(t.biome)
                    && t.soil_fertility == baseline_fertility(t.biome)
            })
        });
        if pristine {
            self.dirty.remove(&coord);
        }
    }

    /// Évince les chunks les moins récemment accédés jusqu'à revenir sous la
    /// capacité. `keep` (celui qu'on vient de charger) n'est jamais évincé.
    ///
    /// **Politique en deux temps.** On sacrifie d'abord les chunks **propres**
    /// (régénérables sans perte) ; ce n'est que si tous les résidents restants
    /// sont sales qu'on évince un chunk sale, en LRU.
    ///
    /// Évincer un chunk sale « perd » sa biomasse broutée — mais c'est le
    /// chunk le **moins récemment touché**, donc sans troupeau ni agent depuis
    /// longtemps : sur ce laps, la repousse l'aurait de toute façon ramené
    /// près du baseline. Le régénérer, c'est le **rattrapage analytique** du
    /// LOD temporel (§8.2), en instantané plutôt que graduel. Ce qui compte
    /// pour la surchasse — l'effectif des troupeaux — vit dans des entités,
    /// jamais évincées. La mémoire reste ainsi **bornée** quoi qu'il arrive,
    /// au lieu de déborder puis de thrasher (mesuré : 0,6 tick/s au débordement
    /// contre 15 sous la capacité).
    fn evict_down_to_capacity(&mut self, keep: ChunkCoord) {
        // 1er temps : les propres.
        self.evict_pass(keep, false);
        // 2e temps, si toujours au-dessus : les sales, à contrecœur.
        self.evict_pass(keep, true);
    }

    fn evict_pass(&mut self, keep: ChunkCoord, allow_dirty: bool) {
        while self.chunks.len() > self.capacity {
            // Victime = plus petit tick d'accès. Les ex æquo sont départagés
            // par l'ordre du BTreeMap → déterministe.
            let victim = self
                .last_access
                .iter()
                .filter(|(c, _)| **c != keep && (allow_dirty || !self.dirty.contains(c)))
                .min_by_key(|(_, t)| **t)
                .map(|(c, _)| *c);
            match victim {
                Some(c) => {
                    self.chunks.remove(&c);
                    self.last_access.remove(&c);
                    self.dirty.remove(&c);
                    self.evicted += 1;
                }
                None => break,
            }
        }
    }

    /// Les sources d'eau du chunk, via le cache — sans générer le chunk.
    fn springs_of(&mut self, coord: ChunkCoord) -> &[(u8, u8)] {
        if !self.springs.contains_key(&coord) {
            // Si le chunk est résident, sa liste fait foi (identique par
            // construction — un test le garantit) ; sinon calcul rapide.
            let list = match self.chunks.get(&coord) {
                Some(chunk) => chunk.springs.clone(),
                None => crate::chunk::springs_for(&self.worldgen, coord),
            };
            self.springs.insert(coord, list);
        }
        &self.springs[&coord]
    }

    /// La source d'eau douce la plus proche de `from`, cherchée dans un carré
    /// de `radius_chunks` chunks autour — quelques listes courtes en cache,
    /// ni tuiles matérialisées ni pression sur le LRU. Départage
    /// déterministe : distance, puis (x, y).
    pub fn nearest_spring(
        &mut self,
        from: (i64, i64),
        radius_chunks: i64,
    ) -> Option<(i64, i64)> {
        let (center, _, _) = split(from.0, from.1);
        let mut best: Option<(i64, (i64, i64))> = None;
        for cy in (center.y - radius_chunks)..=(center.y + radius_chunks) {
            for cx in (center.x - radius_chunks)..=(center.x + radius_chunks) {
                let coord = ChunkCoord { x: cx, y: cy };
                let (ox, oy) = coord.origin();
                for &(lx, ly) in self.springs_of(coord) {
                    let p = (ox + lx as i64, oy + ly as i64);
                    let d2 = (p.0 - from.0).pow(2) + (p.1 - from.1).pow(2);
                    let candidate = (d2, p);
                    if best.is_none_or(|b| candidate < b) {
                        best = Some(candidate);
                    }
                }
            }
        }
        best.map(|(_, p)| p)
    }
}

/// Décompose une coordonnée de tuile en (chunk, position locale). `div_euclid`
/// et `rem_euclid` donnent le bon résultat pour x, y négatifs — sinon la tuile
/// (-1, -1) tomberait dans le mauvais chunk.
fn split(x: i64, y: i64) -> (ChunkCoord, usize, usize) {
    let coord = ChunkCoord {
        x: x.div_euclid(CHUNK_SIZE),
        y: y.div_euclid(CHUNK_SIZE),
    };
    let lx = x.rem_euclid(CHUNK_SIZE) as usize;
    let ly = y.rem_euclid(CHUNK_SIZE) as usize;
    (coord, lx, ly)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoupage_des_coordonnees_negatives() {
        assert_eq!(split(0, 0), (ChunkCoord { x: 0, y: 0 }, 0, 0));
        assert_eq!(split(63, 63), (ChunkCoord { x: 0, y: 0 }, 63, 63));
        assert_eq!(split(64, 64), (ChunkCoord { x: 1, y: 1 }, 0, 0));
        assert_eq!(split(-1, -1), (ChunkCoord { x: -1, y: -1 }, 63, 63));
        assert_eq!(split(-64, -65), (ChunkCoord { x: -1, y: -2 }, 0, 63));
    }

    #[test]
    fn la_memoire_reste_bornee() {
        let mut world = World::new(WorldSeed(42), 16);
        // On balaie bien plus de chunks que la capacité.
        for cy in 0..12 {
            for cx in 0..12 {
                world.chunk(ChunkCoord { x: cx, y: cy });
                assert!(world.loaded() <= 16, "capacité dépassée : {}", world.loaded());
            }
        }
        assert_eq!(world.generated, 144);
        assert!(world.evicted >= 144 - 16);
    }

    #[test]
    fn eviction_puis_regeneration_est_identique() {
        // Lire une tuile, la faire évincer en chargeant beaucoup d'autres
        // chunks, puis la relire : le monde est régénérable, donc identique.
        let mut world = World::new(WorldSeed(42), 4);
        let avant = world.tile(100, 200);
        for cx in 50..80 {
            world.chunk(ChunkCoord { x: cx, y: cx });
        }
        let apres = world.tile(100, 200);
        assert_eq!(avant, apres);
    }

    #[test]
    fn un_chunk_sale_est_prefere_aux_propres_a_l_eviction() {
        // Capacité 8 : un chunk sale et assez de propres pour absorber la
        // pression. Le sale doit survivre — on sacrifie les propres d'abord.
        let mut world = World::new(WorldSeed(42), 8);
        world.tile_mut(100, 200).biomass = 7;
        for cx in 50..70 {
            world.chunk(ChunkCoord { x: cx, y: cx });
        }
        assert_eq!(world.tile(100, 200).biomass, 7, "le sale a été régénéré à tort");
        assert_eq!(world.dirty_count(), 1);
        assert!(world.loaded() <= 8, "mémoire non bornée : {}", world.loaded());
    }

    #[test]
    fn sous_pression_maximale_la_memoire_reste_bornee() {
        // Plus de chunks sales que la capacité : on ne peut pas tous les
        // retenir sans déborder. La mémoire prime — on évince des sales.
        let mut world = World::new(WorldSeed(42), 4);
        for cx in 0..20 {
            // Chaque tile_mut salit un chunk distinct.
            world.tile_mut(cx * 64, 0).biomass = 1;
        }
        assert!(world.loaded() <= 4, "capacité dépassée : {}", world.loaded());
        assert!(world.evicted >= 16, "des sales auraient dû être évincés");
    }

    #[test]
    fn un_chunk_revenu_au_baseline_redevient_evincable() {
        let mut world = World::new(WorldSeed(42), 4);
        let baseline = world.tile(100, 200).biomass;
        let (coord, _, _) = split(100, 200);
        world.tile_mut(100, 200).biomass = 7;
        world.clear_dirty_if_pristine(coord);
        assert_eq!(world.dirty_count(), 1, "encore modifié : doit rester sale");
        world.tile_mut(100, 200).biomass = baseline;
        world.clear_dirty_if_pristine(coord);
        assert_eq!(world.dirty_count(), 0, "revenu au baseline : plus sale");
    }

    #[test]
    fn tuile_du_monde_colle_au_worldgen() {
        let mut world = World::new(WorldSeed(42), 64);
        let reference = WorldGen::new(WorldSeed(42));
        for &(x, y) in &[(0, 0), (-1, -1), (1000, -500), (63, 64)] {
            assert_eq!(world.tile(x, y).elevation, reference.elevation(x, y) as f32);
        }
    }
}
