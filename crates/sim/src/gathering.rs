//! La nourriture végétale des humains (défaut D10) : ce que le pays offre à
//! cueillir, maille par maille, et ce que la cueillette en retire.
//!
//! Avant ce module, un humain se nourrissait de la biomasse d'une tuile, la
//! même que broutent les troupeaux : une tuile de forêt de 4 m² en repoussait
//! assez pour nourrir un humain sur ~50 m², trois fois plus que toute la
//! matière végétale que la forêt fixe dans l'année. La part comestible pour un
//! humain est une petite fraction de cette production (fruits, noix, graines,
//! racines), et elle est **saisonnière** : elle ne naît que les jours où la
//! végétation pousse, puis se perd (elle pourrit, les bêtes la mangent).
//!
//! D'où un stock par maille de 2 km (la maille de pâturage de la faune) :
//! `dS/dt = production(jour) − S/τ − cueillette`. La production n'existe que
//! les jours de croissance ; l'hiver, le stock ne fait que décroître. C'est ce
//! creux, et non la production annuelle, que la littérature désigne comme ce
//! qui borne les chasseurs-cueilleurs (Zhu et al. 2021) : là où la saison de
//! croissance est courte, la viande prend le relais, avec la perte d'énergie
//! d'un étage trophique.
//!
//! Le stock s'avance **jour par jour** quand on le consulte, en rejouant les
//! jours écoulés (au plus un an) : une maille que personne ne visite ne coûte
//! rien, et une maille revisitée retrouve exactement l'état qu'elle aurait eu —
//! la leçon de l'éviction (D7), où une forme fermée prise au jour du
//! rechargement faussait la saison.

use std::collections::BTreeMap;

use cairn_core::{SimTime, TICKS_PER_DAY};
use cairn_worldgen::{Biome, WorldGen};

use crate::climate::{Climate, GROWTH_THRESHOLD_C};
use crate::ecology;
use crate::fauna::{RANGE_ZONE_TILES, range_zone};

/// Part de la productivité primaire nette qui est comestible **et** à portée
/// d'un humain sans outil : fruits, noix, graines, racines, moins ce que les
/// bêtes en prennent d'abord. Ordres de grandeur, pas mesures : ~1 % là où les
/// fruits et les noix abondent (forêts, savanes à tubercules), moins là où
/// l'essentiel de la production est de l'herbe ou du bois de conifère.
pub fn edible_fraction(biome: Biome) -> f32 {
    use Biome::*;
    match biome {
        Ocean | Coast | Glacier => 0.0,
        HotDesert | ColdDesert => 0.005,
        Tundra => 0.003,
        Taiga => 0.003,
        Steppe => 0.005,
        Grassland => 0.005,
        Savanna => 0.01,
        TemperateForest => 0.01,
        TropicalForest => 0.01,
    }
}

/// Énergie d'un gramme de nourriture végétale sèche : les noix vont à 6 kcal/g,
/// les graines et les racines à ~3,5, les baies séchées à ~3.
pub const EDIBLE_KCAL_PER_G: f64 = 4.0;

/// Durée de vie de la nourriture laissée sur pied, en jours : les baies
/// pourrissent en quelques semaines, les noix et les glands disparaissent
/// en quelques mois dans les réserves des rongeurs, les racines durent.
pub const PERSISTENCE_DAYS: f64 = 60.0;

/// Aire d'une maille, en m².
const ZONE_M2: f64 = 4.0e6;
/// Échantillons par côté (4 × 4, un tous les 500 m), comme la faune.
const SAMPLES: i64 = 4;

/// Ce qu'une maille produit, lu une fois dans le worldgen.
#[derive(Clone, Copy, Debug)]
struct Yield {
    /// kcal produites par jour de croissance.
    per_growing_day: f64,
    /// Température moyenne annuelle des terres de la maille.
    mean_temp: f64,
    /// Ordonnée du centre (la saison dépend de la latitude).
    y: i64,
}

#[derive(Clone, Copy, Debug)]
struct Stock {
    kcal: f64,
    /// Dernier jour déjà intégré.
    day: u64,
}

/// Les stocks de nourriture végétale des mailles visitées.
#[derive(Default)]
pub struct Gathering {
    yields: BTreeMap<(i64, i64), Yield>,
    stocks: BTreeMap<(i64, i64), Stock>,
}

impl Gathering {
    fn yield_of(&mut self, worldgen: &WorldGen, climate: &Climate, zone: (i64, i64)) -> Yield {
        *self.yields.entry(zone).or_insert_with(|| {
            let step = RANGE_ZONE_TILES / SAMPLES as f64;
            let (mut kcal_m2_yr, mut temp, mut land) = (0.0, 0.0, 0);
            for j in 0..SAMPLES {
                for i in 0..SAMPLES {
                    let x = (zone.0 as f64 * RANGE_ZONE_TILES + (i as f64 + 0.5) * step) as i64;
                    let y = (zone.1 as f64 * RANGE_ZONE_TILES + (j as f64 + 0.5) * step) as i64;
                    let biome = worldgen.biome(x, y);
                    kcal_m2_yr += f64::from(ecology::npp_g_m2_yr(biome) * edible_fraction(biome))
                        * EDIBLE_KCAL_PER_G;
                    if edible_fraction(biome) > 0.0 {
                        temp += worldgen.mean_temperature(x, y, worldgen.elevation(x, y));
                        land += 1;
                    }
                }
            }
            let n = (SAMPLES * SAMPLES) as f64;
            let y = ((zone.1 as f64 + 0.5) * RANGE_ZONE_TILES) as i64;
            let mean_temp = if land > 0 { temp / f64::from(land) } else { 0.0 };
            let growing = (0..cairn_core::DAYS_PER_YEAR)
                .filter(|&d| grows(climate, mean_temp, y, d))
                .count()
                .max(1) as f64;
            Yield { per_growing_day: kcal_m2_yr / n * ZONE_M2 / growing, mean_temp, y }
        })
    }

    /// Avance le stock de `zone` jusqu'au jour `today` inclus et le renvoie.
    fn advance(
        &mut self,
        worldgen: &WorldGen,
        climate: &Climate,
        zone: (i64, i64),
        today: u64,
    ) -> &mut Stock {
        let y = self.yield_of(worldgen, climate, zone);
        let stock = self.stocks.entry(zone).or_insert(Stock { kcal: 0.0, day: today.saturating_sub(361) });
        // Au-delà d'un an sans visite, le stock a oublié son passé (τ = 60 j) :
        // on ne rejoue que la dernière année, depuis zéro.
        if today.saturating_sub(stock.day) > 360 {
            *stock = Stock { kcal: 0.0, day: today - 360 };
        }
        let keep = (-1.0 / PERSISTENCE_DAYS).exp();
        while stock.day < today {
            stock.day += 1;
            stock.kcal *= keep;
            if grows(climate, y.mean_temp, y.y, stock.day) {
                stock.kcal += y.per_growing_day;
            }
        }
        stock
    }

    /// Comme [`Self::available`], mais **sans rien écrire** : l'état de la
    /// maille est avancé sur une copie. Pour les instruments de mesure, qui
    /// ne doivent pas créer d'état qu'une simulation sans eux n'aurait pas.
    pub fn peek(
        &mut self,
        worldgen: &WorldGen,
        climate: &Climate,
        pos: (f64, f64),
        time: SimTime,
    ) -> f64 {
        let zone = range_zone(pos);
        let saved = self.stocks.get(&zone).copied();
        let kcal = self.advance(worldgen, climate, zone, time.tick / TICKS_PER_DAY).kcal;
        match saved {
            Some(stock) => {
                self.stocks.insert(zone, stock);
            }
            None => {
                self.stocks.remove(&zone);
            }
        }
        kcal
    }

    /// Ce que la maille de `pos` offre aujourd'hui, en kcal.
    pub fn available(
        &mut self,
        worldgen: &WorldGen,
        climate: &Climate,
        pos: (f64, f64),
        time: SimTime,
    ) -> f64 {
        self.advance(worldgen, climate, range_zone(pos), time.tick / TICKS_PER_DAY).kcal
    }

    /// Cueille jusqu'à `want` kcal dans la maille de `pos` ; renvoie ce qui a
    /// été trouvé.
    pub fn take(
        &mut self,
        worldgen: &WorldGen,
        climate: &Climate,
        pos: (f64, f64),
        time: SimTime,
        want: f64,
    ) -> f64 {
        let stock = self.advance(worldgen, climate, range_zone(pos), time.tick / TICKS_PER_DAY);
        let got = want.clamp(0.0, stock.kcal);
        stock.kcal -= got;
        got
    }

    /// Mailles suivies (taille de l'état).
    pub fn len(&self) -> usize {
        self.stocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stocks.is_empty()
    }
}

/// La végétation pousse-t-elle ce jour-là, pour une maille de température
/// moyenne `mean_temp` ? Même règle que l'écologie (`Climate::grows`).
fn grows(climate: &Climate, mean_temp: f64, y: i64, day: u64) -> bool {
    let time = SimTime { tick: day * TICKS_PER_DAY + TICKS_PER_DAY / 2 };
    mean_temp + climate.seasonal_offset(y, time) > GROWTH_THRESHOLD_C
}
