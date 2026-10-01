//! Le gestionnaire de chunks : génère à la demande, garde en mémoire un
//! nombre borné de chunks, décharge les moins récemment utilisés (LRU).
//!
//! C'est la pièce qui rend le monde **infini de fait mais borné en mémoire**
//! (BRIEF §2.1). L'éviction est sûre tant qu'un chunk est du baseline pur :
//! décharger puis régénérer redonne l'identique. Depuis la Phase 2, une tuile
//! peut être **modifiée** (biomasse consommée…).
//!
//! **Deltas persistés (perf, Phase 3)** : un chunk sale n'est plus
//! spécialement protégé — il est évincé en LRU pur, comme n'importe quel
//! autre. Ce qui rendait ça dangereux (perdre la consommation simulée) est
//! résolu par un **instantané léger** : à l'éviction, les tuiles qui
//! diffèrent du baseline (quelques octets chacune, pas les 16 o × 4096 du
//! chunk entier — voir `Chunk::touched`, qui les repère sans scanner le
//! chunk) sont copiées dans [`deltas`](World::deltas) ; à la prochaine
//! résidence, elles sont réappliquées sur le baseline frais. La repousse **se
//! met en pause** pendant l'éviction plutôt que de reprendre au baseline —
//! plus fidèle que l'ancien comportement.
//!
//! **Ce que la mesure a vraiment montré** (3 000 ticks, 100 agents dispersés,
//! capacité 16 384 — `dirty_count`/`delta_count` distinguent résident et en
//! pause) :
//! - **Référence** (protection dirty à l'ancienne) : 5,5 tps.
//! - **Plafond sans éviction** (capacité 65 536, 0 évincé) : 9,3 tps — mais
//!   ~4 Go à cette capacité, hors budget serveur (§8.3). Sert de repère : la
//!   marge de manœuvre réelle plafonne là, pas plus haut.
//! - **Écologie désactivée** (pour vérifier si la repousse quotidienne
//!   coûtait cher) : 1,6 tps, bien **pire** — c'est `daily_regrowth` qui
//!   nettoie les chunks sales (`clear_dirty_if_pristine`) ; sans elle, plus
//!   rien ne redevient jamais propre. Hypothèse écartée par la mesure.
//! - **1ʳᵉ tentative de déltas** : dirty ne distinguait pas résident/en
//!   pause → `dirty_coords()` (parcourue chaque jour par l'écologie)
//!   grossissait sans borne avec *tous* les chunks jamais touchés. **6,2 tps**
//!   : le gain de l'éviction non protégée était mangé par ce scan quotidien
//!   qui grossissait sans cesse.
//! - **2ᵉ tentative, borne corrigée** (`dirty` résident seulement, `deltas`
//!   séparé) mais instantané par scan complet des 4096 tuiles à chaque
//!   éviction : **4,8 tps**, encore **pire que la référence** — le scan
//!   coûtait plus qu'il ne faisait gagner l'éviction non protégée.
//! - **Version finale** (`Chunk::touched`, instantané en O(tuiles vraiment
//!   modifiées)) : **6,3 tps**. Gain modeste (+15 %) mais net, plus un vrai
//!   gain de correction indépendant du chiffre : l'ancienne éviction
//!   remettait un chunk sale évincé pile au baseline (perte silencieuse de
//!   la consommation simulée) ; ici, rien n'est perdu.
//!
//! Le reste de l'écart avec le plafond de 9,3 tps est structurel, pas un bug
//! de politique d'éviction : la Phase 3 (curiosité, errance, sociabilité)
//! fait visiter à la population bien plus de chunks **distincts** qu'avant,
//! et régénérer un chunk (bruit du worldgen) coûte ce qu'il coûte, deltas ou
//! pas. Ceci est un cache **en mémoire seulement** ; la persistance
//! **durable** (disque, redémarrage) reste pour la Phase 6.

use std::collections::{BTreeMap, BTreeSet};

use cairn_core::WorldSeed;
use cairn_worldgen::{WorldGen, WorldGenConfig};

use crate::chunk::{CHUNK_SIZE, Chunk, ChunkCoord};
use crate::tile::{Tile, baseline_biomass, baseline_fertility};

/// Instantané léger d'un chunk sale évincé : seulement les tuiles qui
/// diffèrent du baseline. `(lx, ly, biomass, soil_fertility)` — les deux
/// seuls champs mutables (`tile.rs`).
struct ChunkDelta {
    tiles: Vec<(u8, u8, u8, u8)>,
}

/// Plafond du cache d'humidité, en coins (~50 octets chacun avec la clé et le
/// nœud de l'arbre, soit ~50 Mo au plus). Un coin sert quatre chunks : c'est
/// l'équivalent de ~1 million de chunks déjà vus, soixante fois la capacité
/// résidente de `chronicle`.
const HUMIDITY_CACHE_MAX: usize = 1_000_000;

/// Recensement des tuiles marquées (voir [`World::touched_census`]).
#[derive(Debug, Default, Clone, Copy)]
pub struct TouchedCensus {
    /// Tuiles marquées des chunks sales résidents.
    pub marked: usize,
    /// Au-dessus de leur capacité de temps sec (surplus d'une averse).
    pub above_cap: usize,
    /// Exactement à leur capacité : la repousse n'a rien à y faire.
    pub at_cap: usize,
    /// Parmi elles, revenues au baseline exact (biomasse et fertilité).
    pub at_baseline: usize,
}

pub struct World {
    worldgen: WorldGen,
    /// Chunks résidents. `BTreeMap` et non `HashMap` : ordre d'itération
    /// déterministe, exigence du projet.
    chunks: BTreeMap<ChunkCoord, Chunk>,
    /// Tick d'accès le plus récent par chunk, pour le LRU.
    last_access: BTreeMap<ChunkCoord, u64>,
    /// Chunks modifiés depuis leur génération : **résidents ou non**. Un
    /// chunk sale évincé y reste — sa repousse est en pause, pas guérie —
    /// tant que [`deltas`](Self::deltas) porte son instantané.
    dirty: BTreeSet<ChunkCoord>,
    /// Instantanés des chunks sales évincés (voir le commentaire de module).
    /// Une entrée pèse quelques dizaines d'octets, pas 64 Kio.
    deltas: BTreeMap<ChunkCoord, ChunkDelta>,
    /// Cache des sources d'eau par chunk, calculées **sans** générer le chunk
    /// (voir `chunk::springs_for`). Jamais évincé : une entrée pèse quelques
    /// dizaines d'octets, et la requête « où est l'eau ? » porte sur le
    /// baseline pur — pas besoin de matérialiser des tuiles pour y répondre.
    springs: BTreeMap<ChunkCoord, Vec<(u8, u8)>>,
    /// Jour de la dernière repousse par chunk (LOD de la flore). Quelques
    /// octets par chunk sale, sans commune mesure avec un chunk résident.
    pub last_regrowth: BTreeMap<ChunkCoord, u64>,
    /// Coins d'humidité réellement calculés (advection sur ~256 km) depuis la
    /// création du monde — ce que coûte la génération des chunks (M4 : 66 à
    /// 73 % de chaque chunk généré).
    pub humidity_computed: u64,
    /// Recherches de sources réellement calculées (`chunk::springs_for`)
    /// depuis la création du monde.
    pub springs_computed: u64,
    humidity_cache: BTreeMap<(i64, i64), f64>,
    /// Nourriture végétale des humains, maille par maille (défaut D10) : un
    /// état de la terre, à une échelle plus grossière que la tuile.
    gathering: crate::gathering::Gathering,
    /// Horloge logique : incrémentée à chaque accès.
    clock: u64,
    /// Nombre maximal de chunks résidents.
    capacity: usize,
    /// Statistiques cumulées (observabilité de la mémoire).
    pub generated: u64,
    pub evicted: u64,
    /// Instrumentation du **taux d'utilisation** des chunks (mesure M1), sous
    /// la feature `chunk-stats` — absente du binaire autrement.
    #[cfg(feature = "chunk-stats")]
    stats: ChunkStats,
}

/// Combien de tuiles d'un chunk sont **réellement lues ou écrites** entre sa
/// génération et son éviction ?
///
/// La question qui arbitre tout le chantier de performance. Générer un chunk
/// coûte le bruit du worldgen sur ses 4 096 tuiles ; si l'accès n'en touche
/// qu'une poignée, le coût n'est pas dans la génération mais dans sa
/// **granularité**, et le correctif est de générer à la tuile plutôt que de
/// générer moins cher.
///
/// Mesuré et non supposé, parce que les deux correctifs sont incompatibles :
/// rendre la génération moins chère ne sert à rien si l'on génère 4 096 fois
/// trop, et générer paresseusement ne sert à rien si l'on lit tout le chunk.
#[cfg(feature = "chunk-stats")]
#[derive(Default)]
struct ChunkStats {
    /// Un bit par tuile touchée, pour chaque chunk résident.
    masks: BTreeMap<ChunkCoord, Box<[u64; 64]>>,
    /// Chunks ayant subi au moins une **écriture** pendant leur vie, et qui
    /// sont donc balayés en entier par l'écologie chaque jour. La génération
    /// paresseuse ne peut rien pour eux : c'est la fraction qui décide si C1
    /// suffit ou s'il faut aussi rendre l'écologie éparse.
    written: BTreeSet<ChunkCoord>,
    dirty_lives: u64,
    /// Vies de chunk achevées (générations suivies d'une éviction).
    lives: u64,
    /// Somme des tuiles distinctes touchées sur ces vies.
    touched_total: u64,
    /// Répartition par puissance de deux : `hist[k]` compte les vies dont le
    /// nombre de tuiles touchées tombe dans `[2^k, 2^(k+1))`. La moyenne seule
    /// mentirait — une poignée de chunks intensément lus la tirerait vers le
    /// haut en masquant la masse des chunks à peine effleurés.
    hist: [u64; 14],
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
            deltas: BTreeMap::new(),
            springs: BTreeMap::new(),
            last_regrowth: BTreeMap::new(),
            humidity_computed: 0,
            springs_computed: 0,
            gathering: Default::default(),
            humidity_cache: BTreeMap::new(),
            clock: 0,
            capacity: capacity.max(1),
            generated: 0,
            evicted: 0,
            #[cfg(feature = "chunk-stats")]
            stats: ChunkStats::default(),
        }
    }

    /// Note qu'une tuile de ce chunk a été touchée (mesure M1).
    #[cfg(feature = "chunk-stats")]
    fn note_access(&mut self, coord: ChunkCoord, lx: usize, ly: usize, write: bool) {
        let bit = ly * crate::chunk::CHUNK_SIZE as usize + lx;
        let mask = self.stats.masks.entry(coord).or_insert_with(|| Box::new([0u64; 64]));
        mask[bit / 64] |= 1 << (bit % 64);
        if write {
            self.stats.written.insert(coord);
        }
    }

    #[cfg(not(feature = "chunk-stats"))]
    #[inline(always)]
    fn note_access(&mut self, _coord: ChunkCoord, _lx: usize, _ly: usize, _write: bool) {}

    /// Clôt la vie d'un chunk : range son nombre de tuiles touchées dans la
    /// répartition (mesure M1).
    #[cfg(feature = "chunk-stats")]
    fn close_life(&mut self, coord: ChunkCoord) {
        let Some(mask) = self.stats.masks.remove(&coord) else { return };
        let touched: u32 = mask.iter().map(|w| w.count_ones()).sum();
        if self.stats.written.remove(&coord) {
            self.stats.dirty_lives += 1;
        }
        self.stats.lives += 1;
        self.stats.touched_total += touched as u64;
        let bucket = (u32::BITS - touched.leading_zeros()) as usize;
        self.stats.hist[bucket.min(13)] += 1;
    }

    #[cfg(not(feature = "chunk-stats"))]
    #[inline(always)]
    fn close_life(&mut self, _coord: ChunkCoord) {}

    /// Le rapport d'utilisation (mesure M1) : `(vies, tuiles touchées en
    /// moyenne, répartition par puissance de deux)`. Les chunks encore
    /// résidents n'y sont pas — leur vie n'est pas finie.
    #[cfg(feature = "chunk-stats")]
    pub fn utilization(&self) -> (u64, f64, [u64; 14], u64) {
        let mean = if self.stats.lives == 0 {
            0.0
        } else {
            self.stats.touched_total as f64 / self.stats.lives as f64
        };
        (self.stats.lives, mean, self.stats.hist, self.stats.dirty_lives)
    }

    /// Nombre de chunks actuellement en mémoire.
    pub fn loaded(&self) -> usize {
        self.chunks.len()
    }

    /// Nombre de chunks **résidents** modifiés depuis le baseline — borné par
    /// la capacité, comme `loaded()`. Les chunks sales évincés (en pause)
    /// n'y comptent pas : voir [`delta_count`](Self::delta_count).
    pub fn dirty_count(&self) -> usize {
        self.dirty.len()
    }

    /// Nombre de chunks sales **en pause** (évincés, instantané léger dans
    /// `deltas`) : leur repousse est suspendue jusqu'à la prochaine
    /// résidence. Pas de plafond dédié — chaque entrée ne coûte que
    /// quelques octets, sans commune mesure avec un chunk résident (64 Kio).
    pub fn delta_count(&self) -> usize {
        self.deltas.len()
    }

    pub fn seed(&self) -> WorldSeed {
        self.worldgen.seed()
    }

    /// Le baseline procédural sous-jacent (lecture seule — il est pur).
    pub fn worldgen(&self) -> &WorldGen {
        &self.worldgen
    }

    /// Nourriture végétale offerte aujourd'hui par la maille de `pos`, en kcal.
    pub fn edible_kcal(&mut self, pos: (f64, f64), tick: u64) -> f64 {
        let climate = crate::climate::Climate::new(self.worldgen.temperature.latitude());
        self.gathering.available(&self.worldgen, &climate, pos, cairn_core::SimTime { tick })
    }

    /// Cueille jusqu'à `want` kcal dans la maille de `pos` ; renvoie ce qui a
    /// été trouvé.
    pub fn gather(&mut self, pos: (f64, f64), tick: u64, want: f64) -> f64 {
        let climate = crate::climate::Climate::new(self.worldgen.temperature.latitude());
        self.gathering.take(&self.worldgen, &climate, pos, cairn_core::SimTime { tick }, want)
    }

    /// Mailles dont on suit la nourriture végétale.
    pub fn gathering_zones(&self) -> usize {
        self.gathering.len()
    }

    /// Renvoie le chunk demandé, le générant s'il est absent. Marque l'accès
    /// (pour le LRU) et évince si la capacité est dépassée.
    ///
    /// Si un instantané sale existait (le chunk avait été évincé sans être
    /// revenu au baseline), il est réappliqué sur le baseline frais : la
    /// consommation simulée reprend exactement où elle en était.
    pub fn chunk(&mut self, coord: ChunkCoord) -> &Chunk {
        self.ensure_resident(coord);
        &self.chunks[&coord]
    }

    /// L'humidité d'un coin de la grille des chunks — en cache.
    ///
    /// C'est une fonction pure de la coordonnée : la garder après l'éviction
    /// du chunk ne change rien à la simulation (même valeur, au bit près),
    /// seulement le temps. Mesuré (M4) : ces quatre coins faisaient 66 à 73 %
    /// du coût d'un chunk généré, et les troupeaux font régénérer des chunks
    /// par milliers. Un coin est partagé par quatre chunks voisins, d'où la clé
    /// par coin plutôt que par chunk.
    ///
    /// Borné à `HUMIDITY_CACHE_MAX` coins : au-delà, on le vide. Brutal, mais
    /// sans effet sur la trajectoire — un coin oublié se recalcule à
    /// l'identique — et suffisant tant que l'ensemble de travail d'un monde
    /// tient largement sous le plafond.
    fn corner_humidity(&mut self, x: i64, y: i64) -> f64 {
        if let Some(&h) = self.humidity_cache.get(&(x, y)) {
            return h;
        }
        if self.humidity_cache.len() >= HUMIDITY_CACHE_MAX {
            self.humidity_cache.clear();
        }
        self.humidity_computed += 1;
        #[cfg(feature = "profile")]
        let t0 = std::time::Instant::now();
        let h = self.worldgen.humidity(x, y);
        #[cfg(feature = "profile")]
        crate::chunk::CHUNK_PROF[0]
            .fetch_add(t0.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
        self.humidity_cache.insert((x, y), h);
        h
    }

    /// Coins d'humidité actuellement en cache.
    pub fn humidity_cache_len(&self) -> usize {
        self.humidity_cache.len()
    }

    /// Charge le chunk s'il est absent, note l'accès pour le LRU, évince au
    /// besoin. Séparé de [`chunk`](Self::chunk) parce que la génération
    /// paresseuse oblige les lectures de tuile à emprunter `chunks` en mutable :
    /// elles ne peuvent pas passer par une méthode qui rend déjà un `&Chunk`.
    fn ensure_resident(&mut self, coord: ChunkCoord) {
        self.clock += 1;
        if !self.chunks.contains_key(&coord) {
            let corners = Chunk::corner_coords(coord).map(|(x, y)| self.corner_humidity(x, y));
            // Les sources, par le cache que `nearest_spring` tient déjà : une
            // fonction pure de la coordonnée, que la génération recalculait à
            // chaque régénération (~120 µs, M4 après I).
            let springs = self.springs_of(coord).to_vec();
            let mut chunk = Chunk::from_parts(coord, &self.worldgen, corners, springs);
            if let Some(delta) = self.deltas.remove(&coord) {
                for (lx, ly, biomass, soil_fertility) in delta.tiles {
                    // Réappliquer un delta **calcule** ces tuiles-là : il n'y
                    // en a que quelques dizaines, et ce sont exactement celles
                    // que la simulation avait déjà touchées.
                    let t = chunk.tile_mut(lx as usize, ly as usize, &self.worldgen);
                    t.biomass = biomass;
                    t.soil_fertility = soil_fertility;
                    // Sans ce marquage, `touched` repartirait vide alors que
                    // le chunk est de nouveau sale : une seconde éviction sans
                    // écriture entre-temps produirait un instantané vide et
                    // rendrait la tuile au baseline. Une tuile restaurée est
                    // aussi modifiée qu'une tuile broutée — c'est le même fait.
                    chunk.mark_touched(lx as usize, ly as usize);
                }
                // De nouveau résident : redevient sale « pour de vrai »,
                // l'écologie peut reprendre sa repousse là où elle était.
                self.dirty.insert(coord);
            }
            self.chunks.insert(coord, chunk);
            self.generated += 1;
            self.evict_down_to_capacity(coord);
        }
        self.last_access.insert(coord, self.clock);
    }

    /// Tuile à la coordonnée de tuile (x, y) — charge le chunk au besoin.
    /// Renvoie une copie : la tuile est `Copy`, pas besoin de garder le chunk
    /// emprunté.
    pub fn tile(&mut self, x: i64, y: i64) -> Tile {
        let (coord, lx, ly) = split(x, y);
        self.ensure_resident(coord);
        // Emprunts disjoints : le chunk est muté (génération paresseuse de la
        // tuile) pendant que le worldgen est lu. Passer par `self` entier
        // heurterait le borrow checker ; déstructurer nomme les deux champs
        // séparément et lui montre qu'ils ne se recouvrent pas.
        let Self { chunks, worldgen, .. } = self;
        let t = *chunks.get_mut(&coord).expect("résident").tile(lx, ly, worldgen);
        self.note_access(coord, lx, ly, false);
        t
    }

    /// Accès **mutable** à une tuile : marque le chunk sale (une éviction
    /// ultérieure en sauvera un instantané plutôt que de le perdre, voir le
    /// commentaire de module). C'est l'unique porte d'entrée de la
    /// simulation vers l'état du monde — tout ce qui évolue passe ici.
    pub fn tile_mut(&mut self, x: i64, y: i64) -> &mut Tile {
        let (coord, lx, ly) = split(x, y);
        // S'assure que le chunk est résident (et paie l'éviction éventuelle)…
        self.ensure_resident(coord);
        self.dirty.insert(coord);
        // …puis le remprunte en mutable. `get_mut` ne peut pas échouer :
        // le chunk vient d'être chargé et `keep` interdit son éviction.
        self.note_access(coord, lx, ly, true);
        let Self { chunks, worldgen, .. } = self;
        let chunk = chunks.get_mut(&coord).expect("résident");
        chunk.mark_touched(lx, ly);
        chunk.tile_mut(lx, ly, worldgen)
    }

    /// Les chunks **résidents** actuellement sales, dans l'ordre déterministe
    /// du BTreeSet — donc bornés par la capacité, comme `dirty_count()`.
    /// C'est le domaine de travail de l'écologie quotidienne : seuls des
    /// chunks résidents ont des tuiles à faire repousser. Les chunks sales en
    /// pause (évincés) n'y figurent pas — inutile de les parcourir chaque
    /// jour pour ne rien y trouver de résident ; leur repousse reprendra
    /// d'elle-même à la prochaine résidence (voir `deltas` / `delta_count`).
    pub fn dirty_coords(&self) -> Vec<ChunkCoord> {
        self.dirty.iter().copied().collect()
    }

    /// Accès mutable direct à un chunk **déjà résident** (`None` sinon).
    /// Réservé aux systèmes du crate ; ne marque pas sale. En pratique,
    /// toujours `Some` pour un coord venu de `dirty_coords()` — celle-ci ne
    /// renvoie que des résidents.
    /// Le chunk **et** le baseline, empruntés séparément : depuis la génération
    /// paresseuse, muter une tuile exige de pouvoir la calculer, donc de tenir
    /// les deux à la fois. Déstructurer `self` est ce qui prouve au compilateur
    /// que les deux champs ne se recouvrent pas.
    pub(crate) fn chunk_mut_with_gen(
        &mut self,
        coord: ChunkCoord,
    ) -> Option<(&mut Chunk, &WorldGen)> {
        let Self { chunks, worldgen, .. } = self;
        chunks.get_mut(&coord).map(|c| (c, &*worldgen))
    }

    /// Si le chunk est revenu exactement au baseline (repousse complète), il
    /// n'est plus sale. Ne vérifie que les tuiles **touchées** : une tuile
    /// jamais mutée est par construction déjà au baseline — inutile de
    /// rescanner les 4096 tuiles du chunk chaque jour pour ne trouver que la
    /// poignée qui a vraiment bougé.
    pub(crate) fn clear_dirty_if_pristine(&mut self, coord: ChunkCoord) {
        let Some(chunk) = self.chunks.get(&coord) else { return };
        let pristine = chunk.touched().iter().all(|&(lx, ly)| {
            let t = chunk.tile_ready(lx as usize, ly as usize);
            t.biomass == baseline_biomass(t.biome) && t.soil_fertility == baseline_fertility(t.biome)
        });
        if pristine {
            self.dirty.remove(&coord);
        }
    }

    /// Recensement de la biomasse écrite, à l'usage des bancs : parmi les
    /// tuiles **marquées** des chunks sales résidents, combien il y en a, et
    /// combien portent une biomasse **au-dessus de leur capacité de temps
    /// sec**.
    ///
    /// Le second compteur est le seul qui rende la météo lisible sur la durée
    /// d'un run. Une averse relève le plafond de croissance et laisse derrière
    /// elle des tuiles au-dessus de leur capacité : si ce nombre **croît sans
    /// fin**, le surplus ne redescend jamais ; s'il **oscille avec le ciel**,
    /// il redescend. Le premier compteur répond à la question voisine — est-ce
    /// que `touched` enfle, c'est-à-dire est-ce que le balayage épars se
    /// dégrade en balayage complet ?
    ///
    /// Domaine : les chunks **sales et résidents**, comme `dirty_coords()`.
    /// Un chunk propre a par définition toutes ses tuiles au baseline, et un
    /// chunk en pause n'a rien à dire d'aujourd'hui. Parcours en O(tuiles
    /// marquées) et déterministe (`BTreeSet`) — à appeler au relevé, pas au
    /// tick.
    pub fn biomass_census(&self) -> (usize, usize) {
        let c = self.touched_census();
        (c.marked, c.above_cap)
    }

    /// Ce que valent les tuiles **marquées** des chunks sales résidents — celles
    /// que la repousse quotidienne visite une à une. Mesure du chantier de
    /// l'écologie : une tuile revenue exactement à son baseline (biomasse et
    /// fertilité d'origine) n'a plus rien à faire, et une éviction la
    /// régénérerait à l'identique ; si elle reste marquée, la repousse la
    /// visite chaque jour pour rien.
    pub fn touched_census(&self) -> TouchedCensus {
        let mut c = TouchedCensus::default();
        for coord in &self.dirty {
            let Some(chunk) = self.chunks.get(coord) else { continue };
            for &(lx, ly) in chunk.touched() {
                c.marked += 1;
                let t = chunk.tile_ready(lx as usize, ly as usize);
                let cap = crate::ecology::effective_capacity(t);
                if t.biomass > cap {
                    c.above_cap += 1;
                } else if t.biomass == cap {
                    c.at_cap += 1;
                    if t.biomass == crate::tile::baseline_biomass(t.biome)
                        && t.soil_fertility == crate::tile::baseline_fertility(t.biome)
                    {
                        c.at_baseline += 1;
                    }
                }
            }
        }
        c
    }

    /// Évince les chunks les moins récemment accédés jusqu'à revenir sous la
    /// capacité. `keep` (celui qu'on vient de charger) n'est jamais évincé.
    ///
    /// **LRU pur, sans traitement spécial pour les chunks sales** : depuis
    /// que l'éviction en prend un instantané (voir le commentaire de module),
    /// évincer un sale ne perd plus rien — inutile de le protéger au prix de
    /// geler la capacité résidente autour de vieux chunks froids pendant
    /// qu'une population dispersée en salit des milliers.
    fn evict_down_to_capacity(&mut self, keep: ChunkCoord) {
        while self.chunks.len() > self.capacity {
            // Victime = plus petit tick d'accès. Les ex æquo sont départagés
            // par l'ordre du BTreeMap → déterministe.
            let victim = self
                .last_access
                .iter()
                .filter(|(c, _)| **c != keep)
                .min_by_key(|(_, t)| **t)
                .map(|(c, _)| *c);
            match victim {
                Some(c) => self.evict_one(c),
                None => break,
            }
        }
    }

    /// Décharge un chunk résident. S'il est sale, instantané léger d'abord
    /// (voir le commentaire de module) — sauf s'il est en fait revenu pile au
    /// baseline entre-temps, auquel cas il n'y a rien à sauver.
    ///
    /// `dirty` ne garde que les chunks **résidents** modifiés : un chunk en
    /// pause quitte `dirty` et entre dans `deltas`. Sans cette distinction,
    /// `dirty_coords()` (parcourue chaque jour par l'écologie) grossirait
    /// avec **tous** les chunks jamais touchés, résidents ou non — régression
    /// mesurée à l'implémentation : ça a nivelé le gain de l'éviction non
    /// protégée (5,5 → 6,2 tps au lieu du potentiel ~9 tps).
    fn evict_one(&mut self, coord: ChunkCoord) {
        if self.dirty.remove(&coord) {
            let delta = snapshot_delta(&self.chunks[&coord]);
            if !delta.tiles.is_empty() {
                self.deltas.insert(coord, delta);
            }
        }
        self.chunks.remove(&coord);
        self.last_access.remove(&coord);
        self.evicted += 1;
        self.close_life(coord);
    }

    /// Les sources d'eau du chunk, via le cache — sans générer le chunk.
    fn springs_of(&mut self, coord: ChunkCoord) -> &[(u8, u8)] {
        if !self.springs.contains_key(&coord) {
            // Si le chunk est résident, sa liste fait foi (identique par
            // construction — un test le garantit) ; sinon calcul rapide.
            let list = match self.chunks.get(&coord) {
                Some(chunk) => chunk.springs.clone(),
                None => {
                    self.springs_computed += 1;
                    #[cfg(feature = "profile")]
                    let t0 = std::time::Instant::now();
                    let list = crate::chunk::springs_for(&self.worldgen, coord);
                    #[cfg(feature = "profile")]
                    crate::chunk::CHUNK_PROF[1]
                        .fetch_add(t0.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
                    list
                }
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

/// Instantané léger d'un chunk : parmi les tuiles **déjà notées touchées**
/// (`Chunk::touched` — quelques-unes, jamais les 4096), celles dont
/// `biomass` ou `soil_fertility` diffère encore du baseline de leur biome.
/// Ne scanne **pas** le chunk entier : mesuré, ce scan complet à chaque
/// éviction coûtait plus qu'il ne faisait gagner l'éviction non protégée
/// (4,8 tps, pire que sans instantané du tout).
fn snapshot_delta(chunk: &Chunk) -> ChunkDelta {
    let mut tiles = Vec::new();
    for &(lx, ly) in chunk.touched() {
        let t = chunk.tile_ready(lx as usize, ly as usize);
        let (base_biomass, base_fertility) =
            (baseline_biomass(t.biome), baseline_fertility(t.biome));
        if t.biomass != base_biomass || t.soil_fertility != base_fertility {
            tiles.push((lx, ly, t.biomass, t.soil_fertility));
        }
    }
    ChunkDelta { tiles }
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

    /// Chantier du coût d'un troupeau, I : **un chunk régénéré ne recalcule
    /// pas son humidité**. Mesuré (M4) : les quatre coins d'humidité font 66 à
    /// 73 % du coût d'un chunk généré, ~1,2 ms sur ~1,8 — et les troupeaux font
    /// régénérer des chunks par milliers. L'humidité d'un coin est une fonction
    /// pure de sa coordonnée : la recalculer ne sert à rien.
    #[test]
    fn un_chunk_regenere_ne_recalcule_pas_son_humidite() {
        let mut world = World::new(WorldSeed(42), 4);
        let a = world.tile(100, 200);
        for cx in 50..80 {
            world.chunk(ChunkCoord { x: cx, y: cx });
        }
        assert!(world.evicted > 0, "le chunk observé aurait dû être évincé");
        let avant = world.humidity_computed;
        let b = world.tile(100, 200);
        assert_eq!(a, b, "régénéré, le chunk doit être identique");
        assert_eq!(
            world.humidity_computed, avant,
            "régénérer un chunk déjà vu a recalculé {} coins d'humidité",
            world.humidity_computed - avant
        );
    }

    /// Chantier du coût d'un troupeau, J : **un chunk régénéré ne recalcule
    /// pas ses sources**. Mesuré après I (M4) : la recherche des sources fait
    /// ~120 µs sur les ~650 d'un chunk généré. C'est une fonction pure de la
    /// coordonnée du chunk, et le monde en tient déjà un cache pour
    /// `nearest_spring` : la génération doit le lire.
    #[test]
    fn un_chunk_regenere_ne_recalcule_pas_ses_sources() {
        let mut world = World::new(WorldSeed(42), 4);
        let _ = world.tile(100, 200);
        for cx in 50..80 {
            world.chunk(ChunkCoord { x: cx, y: cx });
        }
        assert!(world.evicted > 0, "le chunk observé aurait dû être évincé");
        let avant = world.springs_computed;
        let _ = world.tile(100, 200);
        assert_eq!(
            world.springs_computed, avant,
            "régénérer un chunk déjà vu a recalculé ses sources"
        );
    }

    #[test]
    fn un_chunk_sale_evince_garde_son_etat_via_l_instantane() {
        // Capacité 8, largement dépassée : le chunk salit tôt est forcément
        // évincé (LRU pur, plus de protection spéciale). Sa modification doit
        // pourtant survivre — c'est tout l'intérêt de l'instantané léger.
        let mut world = World::new(WorldSeed(42), 8);
        world.tile_mut(100, 200).biomass = 7;
        for cx in 50..70 {
            world.chunk(ChunkCoord { x: cx, y: cx });
        }
        assert!(world.evicted > 0, "le chunk sale aurait dû être évincé (LRU pur)");
        assert_eq!(
            world.tile(100, 200).biomass,
            7,
            "l'instantané aurait dû restaurer la modification à la réaccession"
        );
        assert_eq!(world.dirty_count(), 1);
        assert!(world.loaded() <= 8, "mémoire non bornée : {}", world.loaded());
    }

    #[test]
    fn un_instantane_survit_a_une_seconde_eviction_sans_ecriture() {
        // Le test voisin ne fait qu'**un** cycle d'éviction. Celui-ci en fait
        // deux, et c'est le second qui compte : entre les deux, le chunk n'est
        // rechargé que par une **lecture**.
        //
        // Réappliquer un instantané remet le chunk dans `dirty` mais laissait
        // `touched` vide — or c'est `touched` que parcourt `snapshot_delta`.
        // Une seconde éviction sans écriture entre-temps produisait donc un
        // instantané vide, aussitôt jeté, et la biomasse consommée revenait
        // silencieusement au baseline : exactement la perte d'état que le
        // mécanisme de deltas existe pour empêcher.
        let mut world = World::new(WorldSeed(42), 8);
        world.tile_mut(100, 200).biomass = 7;
        let baseline = baseline_biomass(world.tile(100, 200).biome);
        assert_ne!(baseline, 7, "le test ne prouverait rien si 7 était le baseline");

        let eloigne = |world: &mut World| {
            for cx in 50..70 {
                world.chunk(ChunkCoord { x: cx, y: cx });
            }
        };

        eloigne(&mut world);
        // Rechargement par une **lecture seule** : rien ne salit le chunk.
        assert_eq!(world.tile(100, 200).biomass, 7, "premier retour");

        eloigne(&mut world);
        assert_eq!(
            world.tile(100, 200).biomass,
            7,
            "l'état a été perdu à la seconde éviction : l'instantané n'avait rien à sauver"
        );
    }

    #[test]
    fn sous_pression_maximale_la_memoire_reste_bornee() {
        // Beaucoup de chunks sales, capacité minuscule : sans protection
        // spéciale, l'éviction LRU les traite comme n'importe quel chunk.
        let mut world = World::new(WorldSeed(42), 4);
        for cx in 0..20 {
            // Chaque tile_mut salit un chunk distinct.
            world.tile_mut(cx * 64, 0).biomass = 1;
        }
        assert!(world.loaded() <= 4, "capacité dépassée : {}", world.loaded());
        assert!(world.evicted >= 16, "des sales auraient dû être évincés");
    }

    /// Passage à l'échelle du test précédent : beaucoup de chunks sales
    /// distincts, tous évincés (capacité minuscule), chacun avec **sa propre**
    /// tuile modifiée. Vérifie que la table des instantanés ne mélange pas
    /// les entrées entre chunks voisins.
    #[test]
    fn plusieurs_instantanes_distincts_ne_se_melangent_pas() {
        let mut world = World::new(WorldSeed(42), 2);
        for cx in 0..50 {
            // Une valeur différente par chunk : une éventuelle confusion
            // d'entrée (mauvaise clé, mauvais index local) se verrait.
            world.tile_mut(cx * 64, 0).biomass = (cx % 250 + 1) as u8;
        }
        assert!(world.evicted >= 40, "la plupart doivent avoir été évincés");
        for cx in 0..50 {
            assert_eq!(
                world.tile(cx * 64, 0).biomass,
                (cx % 250 + 1) as u8,
                "chunk {cx} : état perdu ou mélangé avec un voisin"
            );
        }
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
