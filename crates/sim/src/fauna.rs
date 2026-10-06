//! La faune : troupeaux d'herbivores et meutes de prédateurs (BRIEF §2.4,
//! §3.2), en dynamique proie-prédateur **spatialisée**.
//!
//! **Choix de conception : l'entité est le troupeau, pas l'animal.** Le brief
//! demande une faune « simulée plus légèrement que les humains » (§3.2), et
//! le troupeau est de toute façon l'unité qui se déplace, se scinde et migre.
//! Un troupeau porte donc un **effectif continu** (`f32`) : on obtient la
//! dynamique de Lotka-Volterra sur cet effectif — croissance sur la pâture,
//! pertes par prédation et par chasse — sans payer 10 000 entités, et le
//! steering (§3.2 : machine à états + steering behaviors) s'applique au
//! groupe, ce qui est précisément l'échelle où « cohésion de troupeau » a un
//! sens.
//!
//! Rien n'est scripté ici. Les comportements observables tombent des règles :
//! - le troupeau broute la tuile la plus fournie à sa portée, donc il **laisse
//!   une traînée pâturée derrière lui** et avance vers l'herbe fraîche ;
//! - quand la pâture régionale s'effondre (surpâturage) ou que l'hiver arrête
//!   la croissance, sa satiété chute : il **migre vers le chaud** — la
//!   migration saisonnière n'est pas un calendrier, c'est une conséquence ;
//! - les prédateurs suivent les proies et s'effondrent quand elles manquent ;
//! - un troupeau trop nombreux **fissionne**, un troupeau décimé disparaît.

use cairn_core::{Pcg32, SimTime, WorldSeed, km_to_tiles, splitmix64};
use cairn_worldgen::Biome;

use crate::agent::Position;
use crate::climate::Climate;
use crate::salt;
use crate::world::World;

/// Identifiant stable d'une entité de faune (comme `AgentId` pour les
/// humains) : c'est lui qui nourrit les flux RNG, jamais l'`Entity` de hecs,
/// qui recycle ses slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FaunaId(pub u64);

/// Pas de temps d'un tick, en jours — les taux vitaux s'écrivent par jour.
const DT_DAYS: f32 = 1.0 / 24.0;

// — Démographie des herbivores —

/// Effectif d'un troupeau à sa création.
pub const HERD_START: f32 = 25.0;
/// Sous cet effectif, le troupeau n'est plus viable : il disparaît (les
/// survivants sont réputés dispersés).
pub const HERD_MIN: f32 = 4.0;
/// Au-dessus, le troupeau devient ingérable et **fissionne** en deux.
pub const HERD_FISSION: f32 = 90.0;
/// Natalité maximale (pâture pleine), par jour.
const HERB_BIRTH_PER_DAY: f32 = 0.020;
/// Mortalité de fond, par jour. Le rapport mortalité/natalité fixe la
/// **satiété d'équilibre** : ici 0,5 — en dessous le troupeau fond, au-dessus
/// il croît. Aucune capacité de charge n'est écrite en dur : elle émerge de
/// ce que la pâture peut soutenir.
const HERB_DEATH_PER_DAY: f32 = 0.010;
/// Biomasse broutée par tête et par tick, étalée sur la tuile et ses voisines.
const GRAZE_PER_HEAD: f32 = 1.0;
/// **Satiété d'équilibre** : en dessous le troupeau fond, au-dessus il croît.
/// Ce n'est pas un réglage indépendant, c'est exactement le rapport
/// mortalité/natalité — le nommer évite de le recalculer de tête dans les
/// bancs, et de le lire comme un seuil réglable séparément.
pub const HERB_SATIATION_EQUILIBRIUM: f32 = HERB_DEATH_PER_DAY / HERB_BIRTH_PER_DAY;

// — Démographie des prédateurs —

pub const PACK_START: f32 = 5.0;
pub const PACK_MIN: f32 = 1.0;
/// Proies tuées par prédateur et par jour, quand le gibier est à portée :
/// ~15 à 25 ongulés par loup et par an. L'ancienne valeur (0,18, soit 65 par
/// an) faisait croître une meute de ~8 % par jour — voir `PRED_CONV`.
const PRED_KILL_PER_DAY: f32 = 0.05;
/// Prédateurs entretenus par proie tuée (rendement trophique). Réglé pour que
/// la croissance **maximale** d'une meute nourrie à volonté,
/// `PRED_CONV × PRED_KILL_PER_DAY − PRED_DEATH_PER_DAY` ≈ 0,001/j, soit
/// ~0,4 par an, celle d'une population de loups réelle.
///
/// Chantier de dérive, C1 : avec 0,55 × 0,18, cette croissance valait
/// 0,079/j — huit fois celle du gibier, soixante fois celle d'un loup. Une
/// fois le gibier borné par sa pâture (étape 4), il ne distançait plus des
/// prédateurs montés à un ou deux milliers sur une centaine de km², et
/// s'éteignait sur 3 runs sur 8.
const PRED_CONV: f32 = 0.42;
/// Mortalité des prédateurs, par jour. Elle tient lieu de **famine** plus que
/// de mortalité de fond : une meute sans proie fond de moitié en ~35 jours.
/// Corollaire du calibrage : une meute ne se maintient qu'à ≥ 0,048 prise par
/// jour, soit presque toute sa capacité de chasse — un prédateur ne tient que
/// là où le gibier abonde.
const PRED_DEATH_PER_DAY: f32 = 0.020;

// — Le territoire des prédateurs (chantier de dérive, C3) —
//
// Rien ne bornait le prédateur sinon sa proie : même au rythme d'un loup (C1),
// il montait à 1,5-2 par km² — cinquante à cent fois la densité réelle — et le
// cycle proie-prédateur, ralenti mais non amorti, éteignait le gibier sur 2
// seeds sur 4 à l'an 4. Un prédateur réel défend un territoire : c'est ce qui
// borne sa densité indépendamment de sa proie, et ce qui amortit le couple.

/// Rayon d'un territoire de prédateur : ~200 km², dans la fourchette réelle
/// d'une meute de loups (100 à 1 000 km²).
pub const PREDATOR_TERRITORY_TILES: f64 = km_to_tiles(8.0);

/// Densité maximale de prédateurs, par km² : l'ordre de grandeur des plus
/// fortes densités de loups observées, là où la proie abonde.
const PREDATOR_MAX_DENSITY_KM2: f32 = 0.05;

/// Prédateurs qu'un territoire peut porter : densité maximale × aire du disque
/// (≈ 10). Aucune capacité inventée — deux grandeurs réelles.
fn predators_per_territory() -> f32 {
    let r_km = (PREDATOR_TERRITORY_TILES * cairn_core::TILE_METERS / 1000.0) as f32;
    PREDATOR_MAX_DENSITY_KM2 * std::f32::consts::PI * r_km * r_km
}

/// Ce que le territoire fait à une meute, selon les prédateurs à moins de
/// `PREDATOR_TERRITORY_TILES` (elle comprise). Renvoie (part qui mange, place
/// laissée) :
///
/// - **sous la capacité**, tous mangent, et la croissance nette d'une meute
///   bien nourrie est multipliée par la place laissée (1 − P/K) — la forme
///   logistique, qui l'amène à ce que le territoire porte ;
/// - **au-delà**, seule la part K/P a un territoire, donc accès à la proie ; le
///   surplus décline au rythme de la famine. Un prédateur sans territoire est
///   un prédateur sans proie — aucune constante de plus.
///
/// Deux formes essayées d'abord, et mesurées fausses : freiner le seul
/// **recrutement** (la marge d'un loup, 0,001/j face à une famine de 0,02/j,
/// passait sous la famine — équilibre à 0,5 prédateur par territoire au lieu
/// de 10) ; et une logistique pure au-delà de la capacité (le surplus ne
/// refluait qu'au rythme de la croissance d'un loup : ~560 jours de 80 à 20).
pub fn territory(local_predators: f32) -> (f32, f32) {
    let k = predators_per_territory();
    if local_predators <= k {
        (1.0, 1.0 - local_predators / k)
    } else {
        (k / local_predators, 0.0)
    }
}

// — Capacité de charge : le domaine vital (chantier de dérive, étape 4) —
//
// La satiété de pâture (`satiation_from_forage`) ne freine à aucune densité :
// mesuré sur 4 seeds (M1-faune), un troupeau trouve toujours une tuile pleine,
// parce qu'une tête consomme 24 unités par jour quand une tuile de forêt en
// repousse 4,2 — ce qui porte la capacité implicite à ~44 000 têtes/km², mille
// fois celle d'une forêt réelle. La natalité montait donc au plafond partout.
//
// Le frein vient d'une seconde satiété, lue à l'échelle où un troupeau vit :
// une **maille de 2 km** (domaine vital mesuré : 1,5 km net sur 15 j). Sa
// production de fourrage se lit dans le **biome** — pas dans le store, qui est
// le coût mesuré de la faune (M2-faune) — et sa demande est la somme des
// rations des têtes qui y paissent, toutes espèces confondues (elles se
// disputent le même fourrage). Aucune capacité n'est écrite : elle émerge du
// rapport entre ce que le pays produit et ce que les bêtes mangent, deux
// grandeurs réelles.

/// Côté de la maille de pâturage, en tuiles (2 km).
pub const RANGE_ZONE_TILES: f64 = km_to_tiles(2.0);

/// Aire d'une maille, en km².
const RANGE_ZONE_KM2: f32 = 4.0;

/// Échantillons par côté pour estimer la production d'une maille (4 × 4, un
/// tous les 500 m). Le biome varie à l'échelle du kilomètre ; seize lectures
/// du worldgen par maille, une seule fois (mise en cache), suffisent.
const RANGE_ZONE_SAMPLES: i64 = 4;

/// g/m²/an → kg/km²/jour (10⁶ m²/km², 10⁻³ kg/g, 365 j/an).
const G_M2_YR_TO_KG_KM2_DAY: f32 = 1000.0 / 365.0;

/// Fourrage **accessible aux herbivores**, en grammes de matière sèche par m²
/// et par an : productivité primaire nette du biome × part que les grands
/// herbivores peuvent en brouter. Ordres de grandeur de la littérature
/// (productivité : Whittaker & Likens ; part broutée : ~5 % en forêt, où
/// l'essentiel est du bois hors d'atteinte, ~20-30 % dans les milieux
/// herbacés), retenus comme point de départ, pas comme mesure.
pub fn accessible_forage_g_m2_yr(biome: Biome) -> f32 {
    use Biome::*;
    match biome {
        Ocean | Coast | Glacier => 0.0,
        HotDesert | ColdDesert => 9.0,  // ~90 × 10 %
        Tundra => 28.0,                 // ~140 × 20 %
        Taiga => 40.0,                  // ~800 × 5 %
        TemperateForest => 60.0,        // ~1 200 × 5 %
        Steppe => 100.0,                // ~350 × 30 %
        TropicalForest => 110.0,        // ~2 200 × 5 %
        Grassland => 180.0,             // ~600 × 30 %
        Savanna => 270.0,               // ~900 × 30 %
    }
}

/// La maille de pâturage qui contient `pos`.
pub fn range_zone(pos: (f64, f64)) -> (i64, i64) {
    (
        (pos.0 / RANGE_ZONE_TILES).floor() as i64,
        (pos.1 / RANGE_ZONE_TILES).floor() as i64,
    )
}

/// Satiété que la maille peut offrir : **0,5 × production / demande**. Le
/// facteur est la satiété d'équilibre, si bien que la natalité compense
/// exactement la mortalité quand la demande égale la production — l'effectif
/// d'équilibre est la capacité du pays, pas le double. Non bornée à 1 : c'est
/// la satiété de pâture qui plafonne quand le pays est vide.
pub fn zone_satiation(production_kg: f32, demand_kg: f32) -> f32 {
    if demand_kg <= 0.0 {
        return f32::MAX;
    }
    HERB_SATIATION_EQUILIBRIUM * production_kg / demand_kg
}

/// Production de fourrage des mailles de pâturage, en kg/jour. Fonction pure
/// du worldgen, donc mise en cache sans effet sur la trajectoire : le cache ne
/// fait qu'éviter de relire seize fois le biome à chaque tick.
#[derive(Default)]
pub struct Rangeland {
    production: std::collections::BTreeMap<(i64, i64), f32>,
}

impl Rangeland {
    pub fn production(&mut self, worldgen: &cairn_worldgen::WorldGen, zone: (i64, i64)) -> f32 {
        *self.production.entry(zone).or_insert_with(|| {
            let step = RANGE_ZONE_TILES / RANGE_ZONE_SAMPLES as f64;
            let mut sum = 0.0f32;
            for j in 0..RANGE_ZONE_SAMPLES {
                for i in 0..RANGE_ZONE_SAMPLES {
                    let x = zone.0 as f64 * RANGE_ZONE_TILES + (i as f64 + 0.5) * step;
                    let y = zone.1 as f64 * RANGE_ZONE_TILES + (j as f64 + 0.5) * step;
                    sum += accessible_forage_g_m2_yr(worldgen.biome(x as i64, y as i64));
                }
            }
            let mean = sum / (RANGE_ZONE_SAMPLES * RANGE_ZONE_SAMPLES) as f32;
            mean * G_M2_YR_TO_KG_KM2_DAY * RANGE_ZONE_KM2
        })
    }

    /// Mailles déjà évaluées (taille du cache).
    pub fn len(&self) -> usize {
        self.production.len()
    }

    pub fn is_empty(&self) -> bool {
        self.production.is_empty()
    }
}

// — Déplacements (tuiles par tick, soit par heure) —

/// Dérive d'un troupeau qui broute : ~80 m/h, un pâturage qui avance.
const GRAZE_STEP_TILES: f64 = 40.0;
/// Laisse d'un cheptel ancré (~600 m) : au-delà, il revient vers son foyer
/// (domestication, voir `crate::pastoral`).
const HERD_LEASH_TILES: f64 = km_to_tiles(0.6);
/// Portée d'échantillonnage de l'herbe autour du troupeau (~40 m).
///
/// — **Ramené de 80 à 20 tuiles.** `best_pasture` lit neuf tuiles en croix à
///   cette distance ; à 80, la croix couvrait 160 tuiles de large pour un chunk
///   qui en fait 64, si bien que chaque troupeau touchait jusqu'à **neuf chunks
///   distincts à chaque tick** et forçait la génération paresseuse à
///   matérialiser des tuiles dont personne d'autre n'avait besoin (mesure M1 :
///   216 tuiles générées par tuile réellement lue). La faune pèse 84,8 % du
///   temps de tick et c'est là qu'elle lit le monde.
///
///   À 20, la croix entière tient dans la largeur d'un chunk : les sondes
///   tombent sur celui où le troupeau se trouve déjà, donc résident.
///
///   Ce n'est pas qu'une optimisation : 40 m est aussi plus juste. Un troupeau
///   choisit la touffe voisine, il ne compare pas des pâturages à 160 m. Reste
///   que sa marche en dépend — le départage aléatoire des ex æquo a déjà été
///   nécessaire pour éviter une dérive balistique — donc l'effet sur la
///   dispersion se **mesure**, il ne se suppose pas. —
const GRAZE_SAMPLE_TILES: i64 = 20;
/// Fuite : ~2 km avalés d'un trait.
const FLEE_STEP_TILES: f64 = km_to_tiles(2.0);
/// Distance à laquelle un troupeau détecte une menace (~600 m).
const FLEE_RADIUS_TILES: f64 = km_to_tiles(0.6);
/// **Souffle** : après un galop, le troupeau est à bout et ne peut plus
/// détaler pendant ces quelques heures — il reste sur ses gardes, sans
/// paître.
///
/// Cette limite n'est pas un détail d'équilibrage, c'est *le* mécanisme qui
/// rend la chasse humaine possible : une bête court plus vite qu'un homme
/// mais pas plus longtemps. Le chasseur qui marche sans relâche finit par la
/// rejoindre — c'est la **chasse à l'épuisement**, la plus ancienne qu'on
/// connaisse. Sans souffle, la proie détalait à chaque tick et aucune chasse
/// n'aboutissait jamais (mesuré : 0 prise, et le couple traqueur/troupeau
/// dérivait sur des milliers de km).
const FLEE_COOLDOWN: u16 = 3;
/// Migration : dérive **lente**, ~150 m/h soit ~3,5 km/jour. Le brout suffit
/// à chercher l'herbe fraîche voisine (`best_pasture`) ; la migration n'est
/// que le lent glissement saisonnier vers le chaud. La faire rapide était une
/// erreur mesurée : à 500 tuiles/tick, une bande de troupeaux entrant en
/// hiver ensemble se ruait vers l'horizon en générant des chunks vierges par
/// centaines à chaque tick (thrash du LRU).
const MIGRATE_STEP_TILES: f64 = km_to_tiles(0.15);
/// Distance de sondage du climat pour choisir le cap (~600 m).
const MIGRATE_PROBE_TILES: i64 = 300;
/// Poursuite d'une meute vers le gibier (~1,5 km/h).
const PACK_STEP_TILES: f64 = km_to_tiles(1.5);
/// Portée à laquelle une meute chasse un troupeau (~500 m).
pub const PACK_HUNT_RADIUS_TILES: f64 = km_to_tiles(0.5);

/// Une **espèce** de faune. Le comportement — machine à états, steering,
/// Lotka-Volterra — reste **commun** à toutes (la faune reste légère, §3.2) ;
/// l'espèce ne fait que le *paramétrer* : son habitat de prédilection (donc sa
/// place sur la carte), sa vigilance, et — pour les prédateurs — sa
/// **dangerosité** (le risque qu'il y aura à le chasser, quand les humains s'y
/// mettront pour protéger leur gibier : incrément « éleveur » à venir).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Species {
    // — Herbivores (proies) —
    /// Cerf — forêts, vif et farouche.
    Deer,
    /// Aurochs — prairies et steppes, massif et peu farouche (le futur bétail).
    Aurochs,
    /// Gazelle — savanes et steppes, la plus vive et la plus alerte.
    Gazelle,
    /// Renne — taïga et toundra (le futur troupeau du Nord).
    Reindeer,
    // — Prédateurs —
    /// Loup — chasse en meute, suit le gibier partout.
    Wolf,
    /// Lion des cavernes — solitaire, redoutable à affronter.
    CaveLion,
}

impl Species {
    pub fn is_predator(self) -> bool {
        matches!(self, Species::Wolf | Species::CaveLion)
    }

    /// Nom lisible (affichage client, Chronique à venir).
    pub fn label(self) -> &'static str {
        match self {
            Species::Deer => "cerf",
            Species::Aurochs => "aurochs",
            Species::Gazelle => "gazelle",
            Species::Reindeer => "renne",
            Species::Wolf => "loup",
            Species::CaveLion => "lion des cavernes",
        }
    }

    /// À quel point ce biome nourrit l'espèce (0–1). Plein dans son habitat,
    /// médiocre ailleurs : ce facteur module la satiété (donc la démographie),
    /// si bien qu'un troupeau hors de son biome **fond peu à peu et se cantonne
    /// à sa niche** — une borne d'aire de répartition *émergente*, sans qu'aucune
    /// règle ne dise « ne va pas là ». Les prédateurs suivent la proie : leur
    /// « pâture » est la viande, ce facteur ne les concerne pas (toujours 1).
    pub fn habitat_factor(self, biome: Biome) -> f32 {
        use Biome::*;
        if self.is_predator() {
            return 1.0;
        }
        let preferred = matches!(
            (self, biome),
            (Species::Deer, TemperateForest | TropicalForest | Taiga)
                | (Species::Aurochs, Grassland | Steppe)
                | (Species::Gazelle, Savanna | Steppe)
                | (Species::Reindeer, Taiga | Tundra)
        );
        if preferred {
            1.0
        } else {
            // Hors habitat : là où pousse de l'herbe c'est toléré (médiocre),
            // ailleurs (toundra sèche, désert, glace) c'est la disette.
            match biome {
                Grassland | Steppe | Savanna | TemperateForest | TropicalForest | Taiga => 0.6,
                _ => 0.35,
            }
        }
    }

    /// Ration quotidienne d'une tête, en kg de matière sèche : l'ordre de
    /// grandeur réel (~2-3 % de la masse corporelle). Cerf ~120 kg, aurochs
    /// ~700 kg, gazelle ~25 kg, renne ~100 kg. Nulle pour un prédateur : il ne
    /// broute pas.
    pub fn daily_ration_kg(self) -> f32 {
        match self {
            Species::Deer | Species::Reindeer => 3.0,
            Species::Aurochs => 12.0,
            Species::Gazelle => 1.0,
            Species::Wolf | Species::CaveLion => 0.0,
        }
    }

    /// Rayon d'alerte d'un herbivore : à quelle distance il repère une menace
    /// et détale. La gazelle voit loin, l'aurochs se laisse approcher. Sans
    /// objet pour un prédateur (il ne fuit pas dans ce modèle).
    pub fn flee_radius(self) -> f64 {
        match self {
            Species::Gazelle => km_to_tiles(1.0),
            Species::Deer => km_to_tiles(0.7),
            Species::Reindeer => km_to_tiles(0.6),
            Species::Aurochs => km_to_tiles(0.4),
            _ => FLEE_RADIUS_TILES,
        }
    }

    /// L'espèce part-elle l'hiver vers le chaud (CHA-1b) ? Le renne, oui :
    /// les caribous font jusqu'à ~1 200 km aller-retour par an, 19-55 km par
    /// jour en migration. Le cerf est un migrateur **partiel** (une partie
    /// des populations glisse de quelques dizaines de km vers un quartier
    /// d'hiver ; le reste est résident) ; le bison passe l'hiver sur place en
    /// grattant la neige, son domaine se resserre (8 km² contre 70). Faute de
    /// modéliser le quartier d'hiver, cerf, aurochs et gazelle restent.
    pub fn migrates_in_winter(self) -> bool {
        matches!(self, Species::Reindeer)
    }

    /// Rayon du domaine vital (CHA-1), d'après son aire A (r = √(A/π)) :
    /// biche de cerf 2-4 km² (Rùm, Clutton-Brock) ; bison d'Europe, proxy de
    /// l'aurochs, ~30 km² sur l'année (8 l'hiver, 70 l'été ; Białowieża) ;
    /// gazelle et renne, ordre de grandeur à sourcer (10 et 30 km² hors
    /// migration).
    pub fn home_range_radius(self) -> f64 {
        let km2: f64 = match self {
            Species::Deer => 3.0,
            Species::Aurochs => 30.0,
            Species::Gazelle => 10.0,
            Species::Reindeer => 30.0,
            _ => 3.0,
        };
        km_to_tiles((km2 / std::f64::consts::PI).sqrt())
    }

    /// Énergie comestible d'une bête, en kcal : masse vivante × part
    /// consommable (~50 % chez les grands ongulés : viande, graisse, abats,
    /// moelle ; White 1953) × ~1 200 kcal/kg (gibier cru maigre, USDA — bas de
    /// fourchette, la graisse le relèverait). Masses vivantes moyennes : cerf
    /// élaphe ~120 kg, renne ~110 kg, aurochs ~700 kg, gazelle ~25 kg. Nul
    /// pour les prédateurs, qu'on ne chasse pas pour les manger.
    pub fn edible_kcal(self) -> f32 {
        let live_kg = match self {
            Species::Deer => 120.0,
            Species::Reindeer => 110.0,
            Species::Aurochs => 700.0,
            Species::Gazelle => 25.0,
            Species::Wolf | Species::CaveLion => 0.0,
        };
        live_kg * 0.5 * 1_200.0
    }

    /// **Dangerosité** — le risque à l'affronter, dans [0, 1]. Zéro pour la
    /// plupart des proies ; élevé pour les fauves. Sert au futur incrément
    /// « éleveur » : un humain qui chasse un prédateur pour protéger son gibier
    /// pourra être blessé ou tué à proportion de ce chiffre. L'aurochs en a un
    /// peu (il encorne) — anticipation, non exploitée pour l'instant.
    pub fn danger(self) -> f32 {
        match self {
            Species::CaveLion => 0.7,
            Species::Wolf => 0.3,
            Species::Aurochs => 0.15,
            _ => 0.0,
        }
    }

    /// Cette espèce se laisse-t-elle **domestiquer** ? Anticipation de
    /// l'incrément « éleveur » (l'aurochs → bétail, le renne → troupeau du
    /// Nord). Non exploité pour l'instant.
    pub fn domesticable(self) -> bool {
        matches!(self, Species::Aurochs | Species::Reindeer)
    }

    /// Choisit une espèce d'**herbivore** adaptée à `biome` (tirage déterministe
    /// parmi les candidats du biome). C'est ce qui met « le bon animal au bon
    /// endroit » : l'aurochs en prairie, le renne en toundra. Généraliste
    /// (aurochs) là où aucun n'est clairement chez lui.
    pub fn herbivore_for_biome(biome: Biome, rng: &mut Pcg32) -> Species {
        use Biome::*;
        let candidates: &[Species] = match biome {
            Grassland => &[Species::Aurochs],
            Steppe => &[Species::Aurochs, Species::Gazelle],
            Savanna => &[Species::Gazelle],
            TemperateForest | TropicalForest => &[Species::Deer],
            Taiga => &[Species::Reindeer, Species::Deer],
            Tundra => &[Species::Reindeer],
            _ => &[Species::Aurochs], // brouteur généraliste par défaut
        };
        candidates[(rng.next_u32() as usize) % candidates.len()]
    }

    /// Choisit une espèce de **prédateur** : le loup partout, le lion des
    /// cavernes plus rare et plutôt en terrain ouvert/chaud.
    pub fn predator_for_biome(biome: Biome, rng: &mut Pcg32) -> Species {
        use Biome::*;
        let lion_country = matches!(biome, Grassland | Steppe | Savanna | HotDesert);
        if lion_country && rng.next_f32() < 0.4 {
            Species::CaveLion
        } else {
            Species::Wolf
        }
    }
}

/// Ce que fait un troupeau — la machine à états du brief (§3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HerdState {
    Grazing,
    Fleeing,
    Migrating,
}

#[derive(Debug, Clone, Copy)]
pub struct Herd {
    pub population: f32,
    pub state: HerdState,
    /// Ticks de souffle restants : tant qu'ils courent, la bête ne peut plus
    /// galoper (voir [`FLEE_COOLDOWN`]).
    pub flee_ticks: u16,
    /// Satiété du dernier tick (0–1) : lue par la démo et les overlays.
    pub satiation: f32,
    /// L'espèce (cerf, aurochs…) : paramètre l'habitat et la vigilance.
    pub species: Species,
    /// Apprivoisement, dans [0, 1] (domestication, `crate::pastoral`) : monte
    /// pour une espèce **domesticable** protégée et gardée près d'un foyer,
    /// redescend sinon (réversion férale). Reste 0 pour le gibier sauvage.
    pub tameness: f32,
    /// Foyer auquel le cheptel est **ancré** : tant qu'il est gardé, il reste à
    /// proximité (il ne migre plus, ne dérive plus). `None` = libre.
    pub anchor: Option<(f64, f64)>,
    /// Centre du **domaine vital** du troupeau sauvage (CHA-1) : là où il est
    /// né ou arrivé ; posé au premier tour. Il y revient après une fuite, et
    /// n'en sort pas pour brouter — la fidélité au site des ongulés.
    pub home: Option<(f64, f64)>,
}

impl Herd {
    pub fn new(population: f32, species: Species) -> Self {
        Self {
            population,
            state: HerdState::Grazing,
            flee_ticks: 0,
            satiation: 1.0,
            species,
            tameness: 0.0,
            anchor: None,
            home: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Pack {
    pub population: f32,
    /// Proies tuées au dernier tick : le signal qui nourrit sa croissance.
    pub last_kills: f32,
    /// L'espèce de prédateur (loup, lion des cavernes…) : porte sa dangerosité.
    pub species: Species,
    /// **La proie choisie**, tant qu'elle vit et reste atteignable.
    ///
    /// Sans elle, la meute reciblait le troupeau le plus proche **à chaque
    /// tick**. Or la proie fuit à 2 km/tick quand la meute poursuit à 1,5 : on
    /// ne rattrape jamais par la vitesse, seulement en épuisant le gibier
    /// (`FLEE_COOLDOWN`). Une meute qui change d'avis toutes les heures
    /// n'épuise personne — et plus il y a de troupeaux, plus elle change
    /// d'avis. L'abondance protégeait donc la proie au lieu de nourrir le
    /// prédateur.
    ///
    /// Mesuré avant ce choix (banc `derive`, scénario naturel sur 6 ans) : le
    /// rendement de chasse plafonnait à 17-35 % du potentiel, et la prédation
    /// totale restait entre 0 et 10 proies/jour pendant que le gibier passait
    /// de 59 à 4 650 têtes.
    pub quarry: Option<hecs::Entity>,
}

impl Pack {
    pub fn new(population: f32, species: Species) -> Self {
        Self { population, last_kills: 0.0, species, quarry: None }
    }
}

/// Au-delà de cette distance, la meute abandonne sa proie et en cherche une
/// autre. Assez large pour qu'une poursuite tienne malgré les zigzags de la
/// fuite (le gibier galope 2 km par tick), assez courte pour qu'une meute ne
/// s'obstine pas à traverser la carte derrière un troupeau qu'elle ne
/// rejoindra pas.
const PACK_GIVE_UP_TILES: f64 = km_to_tiles(6.0);

/// Au-delà de cet effectif, la meute se scinde — la symétrie longtemps
/// manquante de [`HERD_FISSION`].
///
/// **Une meute est un point dans l'espace.** Sa capacité à chasser ne dépend
/// pas seulement de son effectif mais de l'endroit où elle se trouve : elle ne
/// tue que ce qui entre dans son rayon de chasse. Tant que les meutes ne se
/// scindaient pas, leur **nombre** ne croissait que par immigration, à taux
/// constant — quand les troupeaux, eux, se multiplient par fission, donc
/// géométriquement.
///
/// Mesuré sur un run de 8,4 années de jeu : jamais plus de **19 meutes** sur
/// 603 relevés, pour jusqu'à **1 483 troupeaux** — soit 4,8 troupeaux par meute
/// au départ et 28,6 à l'arrivée. Chaque meute vidait son voisinage puis
/// jeûnait pendant que la proie prospérait partout ailleurs. Aucun réglage du
/// taux de prises ne pouvait corriger ça : c'était une question de couverture,
/// pas d'efficacité.
///
/// Le seuil est bas devant celui des troupeaux (90) parce qu'une meute est
/// naturellement petite : 8, la taille d'une meute de loups réelle, qui
/// essaime ses jeunes au-delà. Il doit rester **sous** ce qu'un territoire
/// porte (~10, C3) : à 24 comme avant, une meute ne l'atteignait plus jamais et
/// la fission disparaissait en silence.
pub const PACK_FISSION: f32 = 8.0;

/// Instantané d'un troupeau, pris avant les systèmes : c'est ce que lisent
/// les chasseurs humains, les meutes et les autres troupeaux. Travailler sur
/// un instantané (plutôt qu'en requêtant le monde pendant qu'on le mute)
/// garde l'ordre des lectures indépendant de l'ordre des écritures — donc le
/// déterminisme.
#[derive(Debug, Clone, Copy)]
pub struct HerdView {
    pub entity: hecs::Entity,
    pub pos: (f64, f64),
    pub population: f32,
    /// L'espèce : c'est elle qui dit ce qu'une prise rapporte.
    pub species: Species,
    /// Apprivoisement (0 = sauvage) : un troupeau bien apprivoisé est le
    /// **cheptel** qu'un éleveur va garder (`TaskKind::Herd`).
    pub tameness: f32,
}

/// Instantané d'une meute, pris avant les systèmes — ce que lisent les humains
/// qui songent à l'affronter (`crate::combat`). Porte l'**espèce**, donc sa
/// dangerosité (`Species::danger`) : c'est elle qui fait le risque du combat.
#[derive(Debug, Clone, Copy)]
pub struct PackView {
    pub entity: hecs::Entity,
    pub pos: (f64, f64),
    pub population: f32,
    pub species: Species,
}

/// Effectif retiré à un troupeau (prédation ou chasse humaine), à appliquer
/// après coup.
#[derive(Debug, Clone, Copy)]
pub struct Kill {
    pub herd: hecs::Entity,
    pub head: f32,
}

/// Une scission de troupeau à faire naître (hors itération) : position `(x, y)`,
/// effectif, et **espèce héritée de la mère**.
pub type Fission = (f64, f64, f32, Species);

// — Équations pures (testables sans monde) —

/// Satiété d'un troupeau : la part **de sa capacité** que la pâture trouvée
/// porte encore, pondérée par l'adéquation du biome à son espèce.
///
/// — **Le rapport est local, et c'est tout l'enjeu.** `found / cap`, non
///   `found / 255`. Normalisée sur la plage du `u8`, la satiété valait
///   exactement `K(biome) / 255 × habitat` : une **constante** par couple
///   (espèce, biome), que le broutage ne déplaçait jamais. Mesuré sur
///   2 234 548 herd-ticks et quatre seeds — 0,824 en forêt tempérée (210/255),
///   0,431 en prairie (110/255), 0,497 hors habitat — et **pas un seul déclin
///   causé par le broutage**. Le signe de la croissance d'un troupeau était
///   donc décidé par le biome où il était né, pour toute sa vie : 0 % de
///   déclin en forêt (croissance illimitée), 100 % en prairie (extinction
///   garantie), alors qu'une prairie pleine est une *bonne* pâture.
///
///   Avec le rapport local, pâture pleine ⇒ satiété = `habitat` ; pâture
///   broutée de moitié ⇒ 0,5, soit exactement
///   [`HERB_SATIATION_EQUILIBRIUM`]. Une capacité de charge émerge donc dans
///   **tous** les biomes, et elle s'échelonne d'elle-même : à mi-hauteur, une
///   pâture riche repousse plus vite qu'une pauvre (la logistique donne
///   `r·K/4`), donc elle nourrit plus de têtes. Rien de cela n'est écrit.
///
///   Le facteur d'habitat continue de cantonner les espèces : à 0,35 (biome
///   hostile), l'équilibre demanderait une pâture à 143 % de sa capacité —
///   inatteignable, donc le troupeau y fond quoi qu'il arrive. C'est la niche,
///   et elle survit au changement.
///
///   **Pas de borne haute ici.** Une averse peut porter la biomasse au-dessus
///   de la capacité de temps sec (voir `ecology`), et il est juste qu'une
///   pâture gorgée d'eau nourrisse mieux — surtout un troupeau hors de son
///   habitat, qui y gagne sa seule marge. `herd_population_step` écrête déjà
///   la satiété à 1 pour la natalité. —
pub fn satiation_from_forage(found: u8, cap: u8, habitat: f32) -> f32 {
    if cap == 0 {
        return 0.0; // océan, glacier : rien n'y pousse, rien n'y paît
    }
    (f32::from(found) / f32::from(cap)) * habitat
}

/// Un tick de démographie herbivore : la natalité suit la satiété, la
/// mortalité est de fond. Renvoie le nouvel effectif.
pub fn herd_population_step(population: f32, satiation: f32) -> f32 {
    let rate = HERB_BIRTH_PER_DAY * satiation.clamp(0.0, 1.0) - HERB_DEATH_PER_DAY;
    (population + population * rate * DT_DAYS).max(0.0)
}

/// Un tick de démographie de meute : elle croît de ses prises, décline sans.
/// C'est l'équation prédateur de Lotka-Volterra — un prédateur sans proie
/// s'éteint — avec le territoire (C3) : quand la meute se nourrit assez pour
/// croître, sa croissance nette est multipliée par `room`
/// ([`territory`]) ; quand elle ne se nourrit pas assez, la famine agit
/// seule.
pub fn pack_population_step(population: f32, kills: f32, room: f32) -> f32 {
    let growth = PRED_CONV * kills;
    let decay = PRED_DEATH_PER_DAY * population * DT_DAYS;
    let net = growth - decay;
    let net = if net > 0.0 { net * room } else { net };
    (population + net).max(0.0)
}

/// Proies qu'une meute de `population` prédateurs prélève en un tick, bornée
/// par le gibier réellement présent.
pub fn pack_kills(population: f32, prey_available: f32) -> f32 {
    (PRED_KILL_PER_DAY * population * DT_DAYS).min(prey_available.max(0.0))
}

/// Vecteur unitaire fuyant `threat`. Si la menace est pile dessus, on part
/// vers +x plutôt que de diviser par zéro.
fn away_from(pos: (f64, f64), threat: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (pos.0 - threat.0, pos.1 - threat.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 { (1.0, 0.0) } else { (dx / len, dy / len) }
}

/// La menace la plus proche dans `threats`, si elle est dans le `radius` de
/// détection (propre à l'espèce — la gazelle voit loin, l'aurochs non).
/// Départage déterministe par distance puis par ordre de la liste.
fn nearest_threat(pos: (f64, f64), threats: &[(f64, f64)], radius: f64) -> Option<(f64, f64)> {
    let mut best: Option<(f64, (f64, f64))> = None;
    for &t in threats {
        let d2 = (pos.0 - t.0).powi(2) + (pos.1 - t.1).powi(2);
        if d2 <= radius * radius && best.is_none_or(|(bd, _)| d2 < bd) {
            best = Some((d2, t));
        }
    }
    best.map(|(_, t)| t)
}

/// Déplace `pos` de `step` tuiles dans la direction `dir`, en refusant
/// d'entrer dans l'eau : la faune terrestre ne traverse pas l'océan. Sondé
/// dans le **baseline** (élévation), comme la marche humaine — matérialiser
/// des chunks pour ça écroulerait le LRU.
fn try_move(world: &World, pos: &mut Position, dir: (f64, f64), step: f64) {
    let next = (pos.x + dir.0 * step, pos.y + dir.1 * step);
    if world.worldgen().elevation(next.0.floor() as i64, next.1.floor() as i64) > 0.0 {
        pos.x = next.0;
        pos.y = next.1;
    }
}

/// La tuile la plus fournie en biomasse à portée de brout, et sa biomasse.
/// Échantillonnage en croix (8 directions + sur place) : 9 lectures par
/// troupeau et par tick, pas un balayage.
///
/// **Les ex æquo sont départagés au hasard** (tirage dérivé de la seed, donc
/// déterministe), et c'est essentiel : sur une prairie uniforme les huit
/// voisins portent la même herbe, et un départage par l'ordre de la liste
/// ferait gagner toujours la même direction. Mesuré : tous les troupeaux du
/// monde marchaient **plein est en ligne droite à 1,9 km/jour** — une dérive
/// balistique au lieu d'une errance. Le hasard rend la marche diffusive,
/// c'est-à-dire un vrai domaine vital.
fn best_pasture(
    world: &mut World,
    rng: &mut Pcg32,
    pos: (f64, f64),
    stats: &mut FaunaStats,
) -> ((i64, i64), u8) {
    let _ = &stats;
    let here = (pos.0.floor() as i64, pos.1.floor() as i64);
    let r = GRAZE_SAMPLE_TILES;
    let mut candidates = [(here, 0u8); 9];
    let mut n = 0;
    let mut best_biomass = 0u8;
    // Remplissage des sondes : somme, minimum, nombre. Sous feature seulement —
    // calculer la capacité de neuf tuiles par troupeau et par tick est
    // exactement le genre de coût qu'on ne met pas dans le binaire de
    // production pour une mesure.
    #[cfg(feature = "fauna-stats")]
    let mut acc: (f64, f64, u32) = (0.0, f64::INFINITY, 0);
    for (dx, dy) in [
        (0, 0),
        (r, 0), (-r, 0), (0, r), (0, -r),
        (r, r), (-r, r), (r, -r), (-r, -r),
    ] {
        let p = (here.0 + dx, here.1 + dy);
        let tile = world.tile(p.0, p.1);
        if !tile.is_walkable() {
            continue;
        }
        #[cfg(feature = "fauna-stats")]
        {
            let cap = crate::ecology::effective_capacity(&tile);
            if cap > 0 {
                let f = f64::from(tile.biomass) / f64::from(cap);
                acc.0 += f;
                acc.1 = acc.1.min(f);
                acc.2 += 1;
            }
        }
        match tile.biomass.cmp(&best_biomass) {
            std::cmp::Ordering::Greater => {
                best_biomass = tile.biomass;
                candidates[0] = (p, tile.biomass);
                n = 1;
            }
            std::cmp::Ordering::Equal => {
                candidates[n] = (p, tile.biomass);
                n += 1;
            }
            std::cmp::Ordering::Less => {}
        }
    }
    #[cfg(feature = "fauna-stats")]
    if acc.2 > 0 {
        let biome = world.tile(here.0, here.1).biome;
        stats.pasture_sample(biome, acc.0 / f64::from(acc.2), acc.1);
    }
    if n == 0 {
        return (here, 0); // cerné par l'eau : on ne bouge pas
    }
    candidates[(rng.next_u32() as usize) % n]
}

/// Cap de migration : vers le plus chaud, sondé au nord et au sud. C'est ce
/// qui produit la migration saisonnière — l'hiver rend la bande d'origine
/// stérile, le troupeau descend, et le printemps le fait remonter. Aucun
/// calendrier n'est consulté.
fn migration_heading(world: &mut World, climate: &Climate, pos: (f64, f64), time: SimTime) -> (f64, f64) {
    let here = (pos.0.floor() as i64, pos.1.floor() as i64);
    let (north, south) = (here.1 - MIGRATE_PROBE_TILES, here.1 + MIGRATE_PROBE_TILES);
    let t_north = {
        let tile = world.tile(here.0, north);
        climate.daily_mean(&tile, north, time)
    };
    let t_south = {
        let tile = world.tile(here.0, south);
        climate.daily_mean(&tile, south, time)
    };
    if t_north > t_south { (0.0, -1.0) } else { (0.0, 1.0) }
}

/// Durée pendant laquelle une piste de troupeau se lit (CHA-2) : quelques
/// heures à quelques jours selon le sol et la météo (Liebenberg, *The Art of
/// Tracking* : l'âge d'une trace se lit à sa dégradation). Ordre de grandeur,
/// choix : deux jours.
pub const TRAIL_HOURS: u64 = 48;
/// Largeur de la bande où un marcheur remarque la piste d'un troupeau en la
/// croisant (une piste de dizaines de bêtes fait une dizaine de mètres de
/// large ; on regarde le sol à quelques pas). Ordre de grandeur, choix.
pub const TRAIL_READ_TILES: f64 = km_to_tiles(0.015);

/// Les pistes des troupeaux sauvages : une position par heure, sur
/// [`TRAIL_HOURS`]. Clé : `FaunaId` (ordre stable).
pub type HerdTrails = std::collections::BTreeMap<u64, std::collections::VecDeque<((f64, f64), u64)>>;

/// Une heure de pistes : chaque troupeau sauvage laisse sa trace ; les traces
/// trop vieilles s'effacent ; les troupeaux disparus aussi.
pub fn lay_trails(fauna: &hecs::World, trails: &mut HerdTrails, tick: u64) {
    let mut alive = std::collections::BTreeSet::new();
    for (_, (id, herd, pos)) in fauna.query::<(&FaunaId, &Herd, &Position)>().iter() {
        if herd.anchor.is_some() {
            continue; // le cheptel gardé n'est pas du gibier qu'on piste
        }
        alive.insert(id.0);
        let t = trails.entry(id.0).or_default();
        t.push_back(((pos.x, pos.y), tick));
        while t.front().is_some_and(|(_, at)| tick.saturating_sub(*at) >= TRAIL_HOURS) {
            t.pop_front();
        }
    }
    trails.retain(|id, _| alive.contains(id));
}

/// La marche de `from` à `to` croise-t-elle une piste fraîche ? Renvoie le
/// point le plus récent de cette piste (où elle mène) et son heure. Le
/// pisteur suit la trace : il apprend où le troupeau est passé en dernier.
pub fn crossed_trail(trails: &HerdTrails, from: (f64, f64), to: (f64, f64), reach: f64) -> Option<((f64, f64), u64)> {
    let (mx, my) = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
    let half = ((to.0 - from.0).hypot(to.1 - from.1)) / 2.0;
    let mut best: Option<((f64, f64), u64)> = None;
    for t in trails.values() {
        let Some(&newest) = t.back() else { continue };
        // Tri grossier : une piste de deux jours s'étend sur quelques km.
        if (newest.0.0 - mx).hypot(newest.0.1 - my) > half + km_to_tiles(12.0) {
            continue;
        }
        let crosses = t.iter().zip(t.iter().skip(1)).any(|(a, b)| segment_distance(from, to, a.0, b.0) <= reach);
        if crosses && best.is_none_or(|(_, at)| newest.1 > at) {
            best = Some(newest);
        }
    }
    best
}

/// Distance entre deux segments du plan.
fn segment_distance(p1: (f64, f64), p2: (f64, f64), q1: (f64, f64), q2: (f64, f64)) -> f64 {
    let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let (d1, d2, d3, d4) = (cross(q1, q2, p1), cross(q1, q2, p2), cross(p1, p2, q1), cross(p1, p2, q2));
    if ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0)) {
        return 0.0;
    }
    let point_seg = |p: (f64, f64), a: (f64, f64), b: (f64, f64)| {
        let (abx, aby) = (b.0 - a.0, b.1 - a.1);
        let len2 = abx * abx + aby * aby;
        let t = if len2 > 0.0 { (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / len2).clamp(0.0, 1.0) } else { 0.0 };
        (p.0 - a.0 - t * abx).hypot(p.1 - a.1 - t * aby)
    };
    point_seg(p1, q1, q2).min(point_seg(p2, q1, q2)).min(point_seg(q1, p1, p2)).min(point_seg(q2, p1, p2))
}

/// Le système des troupeaux : fuite, pâture, migration, démographie.
/// `threats` = positions des humains et des meutes (tout ce qui fait fuir).
/// Renvoie les entités à retirer et les scissions à créer — les mutations
/// structurelles se font hors itération.
#[allow(clippy::too_many_arguments)]
pub fn update_herds(
    fauna: &mut hecs::World,
    world: &mut World,
    climate: &Climate,
    time: SimTime,
    seed: WorldSeed,
    threats: &[(f64, f64)],
    stats: &mut FaunaStats,
    range: &mut Rangeland,
    fence: Option<&[(f64, f64)]>,
) -> (Vec<hecs::Entity>, Vec<Fission>) {
    // Sans la feature, `stats` est un type vide : le paramètre ne coûte rien et
    // évite une seconde signature.
    let _ = &stats;
    let tick_seed = seed.derive(salt::FAUNA) ^ splitmix64(time.tick);
    let mut doomed = Vec::new();
    let mut fissions = Vec::new();
    #[cfg(feature = "profile")]
    let mut prof = [0u64; 4];

    // La demande de chaque maille, sur l'état du début du tour : toutes les
    // têtes qui y paissent, chacune à sa ration.
    #[cfg(feature = "profile")]
    let t0 = std::time::Instant::now();
    let mut demand: std::collections::BTreeMap<(i64, i64), f32> = std::collections::BTreeMap::new();
    for (_, (herd, pos)) in fauna.query::<(&Herd, &Position)>().iter() {
        *demand.entry(range_zone((pos.x, pos.y))).or_insert(0.0) +=
            herd.population * herd.species.daily_ration_kg();
    }
    #[cfg(feature = "profile")]
    {
        prof[3] += t0.elapsed().as_nanos() as u64;
    }

    for (entity, (id, herd, pos)) in fauna.query_mut::<(&FaunaId, &mut Herd, &mut Position)>() {
        // — Pas de LOD sur les troupeaux. —
        //
        //   Six tentatives mesurées, toutes fausses. Ralentir les proies seules
        //   donne +953 % de prédateurs ; ralentir les deux côtés casse la
        //   dynamique de rencontre, dont le cycle — attaquer, la proie détale,
        //   la rattraper — est plus court que la fenêtre ; exempter les meutes
        //   recrée le premier biais à +454 %.
        //
        //   **Un couple proie-prédateur ne se laisse pas grossir
        //   temporellement.** Ce qui se grossit sans dommage, c'est ce qui n'a
        //   pas de partenaire couplé : la flore (voir `ecology`), champ et non
        //   acteur, dont le rattrapage est *exact* par forme fermée.
        //
        //   **Attention en relisant ceci.** Ce commentaire s'est longtemps
        //   terminé par « la faune pèse 6,4 % du temps de tick, l'écologie
        //   58 % : il n'y a rien à gagner ici ». C'est faux depuis le
        //   balayage épars de l'écologie (92ef903) : la faune pèse désormais
        //   **84,8 %** et reste le seul effectif non borné. Ce qui reste vrai,
        //   et qui est la seule chose que ce commentaire doit interdire, c'est
        //   le **LOD temporel** — les six biais ci-dessus ne dépendent pas du
        //   profil. Optimiser la faune autrement (grille spatiale pour les
        //   requêtes de voisinage, coût par entité) reste entièrement ouvert.
        let mut rng = Pcg32::new(tick_seed, id.0);

        // Le souffle revient, qu'il y ait une menace ou non — et il revient en
        // **temps de jeu**, pas en nombre de tours. Sous LOD, un troupeau qui
        // n'agit qu'une fois toutes les 24 heures serait resté essoufflé 72
        // heures au lieu de 3 : incapable de fuir, il se faisait prendre. Le
        // biais mesuré était de +106 % sur les prédateurs, pour une confusion
        // d'unités entre « ticks » et « tours ».
        herd.flee_ticks = herd.flee_ticks.saturating_sub(1);
        // Où il était avant de bouger : le bord de la faune se juge sur le pas
        // entier du tick (fuite, pâture ou migration — un seul par tick).
        let start = (pos.x, pos.y);

        // — Fuite : elle prime sur tout le reste, y compris la faim — mais
        //   seulement si la bête a encore du souffle.
        #[cfg(feature = "profile")]
        let t0 = std::time::Instant::now();
        let threat = nearest_threat((pos.x, pos.y), threats, herd.species.flee_radius());
        #[cfg(feature = "profile")]
        {
            prof[0] += t0.elapsed().as_nanos() as u64;
        }
        let bolting = match threat {
            Some(t) if herd.flee_ticks == 0 => {
                herd.state = HerdState::Fleeing;
                herd.flee_ticks = FLEE_COOLDOWN;
                try_move(world, pos, away_from((pos.x, pos.y), t), FLEE_STEP_TILES);
                true
            }
            // Menacé mais à bout de souffle : il ne détale pas, et il ne
            // broute pas non plus — c'est là qu'il se fait prendre.
            Some(_) => {
                herd.state = HerdState::Fleeing;
                true
            }
            None => false,
        };

        if !bolting {
            // — Pâture : viser l'herbe la plus fournie à portée. C'est *elle*
            //   qui répond au manque local — un creux de pâture ne déclenche
            //   pas la migration, il pousse simplement le troupeau vers le
            //   voisin le plus vert. Une zone entièrement broutée fait donc
            //   fondre puis dériver le troupeau, sans stampede.
            #[cfg(feature = "profile")]
            let t0 = std::time::Instant::now();
            let (target, biomass) = best_pasture(world, &mut rng, (pos.x, pos.y), stats);
            // La satiété = l'herbe trouvée, PONDÉRÉE par l'adéquation du biome
            // à l'espèce (un cerf en plein désert broute mal) — c'est ce qui
            // cantonne chaque espèce à sa niche, sans règle « ne va pas là ».
            let here_tile = world.tile(pos.x.floor() as i64, pos.y.floor() as i64);
            herd.satiation = satiation_from_forage(
                biomass,
                crate::ecology::effective_capacity(&here_tile),
                herd.species.habitat_factor(here_tile.biome),
            );
            #[cfg(feature = "profile")]
            {
                prof[1] += t0.elapsed().as_nanos() as u64;
            }

            // — Mesure du fourrage. Bloc sous `cfg` et non appel no-op, parce
            //   qu'il faut relire la tuile d'où vient l'herbe pour connaître sa
            //   capacité : ce coût-là ne doit pas exister hors de la feature. —
            #[cfg(feature = "fauna-stats")]
            {
                let t = world.tile(target.0, target.1);
                stats.forage_sample(
                    here_tile.biome,
                    herd.satiation,
                    biomass,
                    crate::ecology::effective_capacity(&t),
                );
            }

            let winter = !climate.grows(&here_tile, pos.y.floor() as i64, time);
            if let Some(anchor) = herd.anchor {
                // — Cheptel ancré (domestication) : il **reste au foyer**. Trop
                //   loin → il y revient ; sinon il broute sur place. Il ne migre
                //   pas (le clan l'abrite l'hiver). C'est ce qui fait qu'un
                //   troupeau apprivoisé « suit » les gens au lieu de partir avec
                //   les saisons — sans règle « reste ici ».
                herd.state = HerdState::Grazing;
                let d = ((anchor.0 - pos.x).powi(2) + (anchor.1 - pos.y).powi(2)).sqrt();
                if d > HERD_LEASH_TILES {
                    try_move(world, pos, away_from(anchor, (pos.x, pos.y)), GRAZE_STEP_TILES);
                } else {
                    let dx = target.0 as f64 + 0.5 - pos.x;
                    let dy = target.1 as f64 + 0.5 - pos.y;
                    let len = (dx * dx + dy * dy).sqrt();
                    if len > 1e-6 {
                        try_move(world, pos, (dx / len, dy / len), GRAZE_STEP_TILES.min(len));
                    }
                }
                #[cfg(feature = "profile")]
                let t0 = std::time::Instant::now();
                graze(world, (pos.x, pos.y), herd.population);
                #[cfg(feature = "profile")]
                {
                    prof[2] += t0.elapsed().as_nanos() as u64;
                }
            } else if winter && herd.species.migrates_in_winter() {
                // Seul l'hiver — la pâture gelée sur toute la bande — met le
                // migrateur en route vers le chaud. Lentement. Le domaine suit
                // le troupeau qui migre ; les résidents restent (CHA-1b).
                herd.home = Some((pos.x, pos.y));
                herd.state = HerdState::Migrating;
                let dir = migration_heading(world, climate, (pos.x, pos.y), time);
                // Un peu de dispersion latérale : les troupeaux ne migrent
                // pas en file indienne sur le même méridien.
                let jitter = (rng.next_f64() - 0.5) * 0.6;
                let len = (1.0f64 + jitter * jitter).sqrt();
                try_move(world, pos, (jitter / len, dir.1 / len), MIGRATE_STEP_TILES);
            } else {
                herd.state = HerdState::Grazing;
                // Fidélité au site (CHA-1) : hors de son domaine — chassé par
                // une fuite, ou poussé par la pâture —, il y revient au pas
                // ; dedans, il broute où l'herbe est la meilleure. Les cerfs
                // reviennent à leur domaine en quelques heures après un
                // dérangement, même un incendie (Cervus/Odocoileus).
                let home = *herd.home.get_or_insert((pos.x, pos.y));
                let (hx, hy) = (home.0 - pos.x, home.1 - pos.y);
                let from_home = (hx * hx + hy * hy).sqrt();
                let (dx, dy) = if from_home > herd.species.home_range_radius() {
                    (hx, hy)
                } else {
                    (target.0 as f64 + 0.5 - pos.x, target.1 as f64 + 0.5 - pos.y)
                };
                let len = (dx * dx + dy * dy).sqrt();
                if len > 1e-6 {
                    try_move(world, pos, (dx / len, dy / len), GRAZE_STEP_TILES.min(len));
                }
                #[cfg(feature = "profile")]
                let t0 = std::time::Instant::now();
                graze(world, (pos.x, pos.y), herd.population);
                #[cfg(feature = "profile")]
                {
                    prof[2] += t0.elapsed().as_nanos() as u64;
                }
            }
        }

        // — Le bord de la faune (D′) : un troupeau sauvage ne franchit pas le
        //   bord vers l'extérieur. Le pas est refusé, comme devant l'eau ; le
        //   cheptel domestiqué, qui suit son clan, n'est pas concerné. —
        if let Some(humans) = fence
            && herd.anchor.is_none()
            && crosses_perimeter(start, (pos.x, pos.y), humans)
        {
            pos.x = start.0;
            pos.y = start.1;
        }

        // Domaine vital : chemin parcouru contre déplacement net, après que le
        // troupeau a bougé (fuite, pâture ou migration confondues).
        stats.herd_step(id.0, (pos.x, pos.y), time.tick / cairn_core::TICKS_PER_DAY);

        // — Le pays : la plus rare des deux satiétés commande, **que le
        //   troupeau ait brouté ou fui**. Une tuile pleine ne nourrit pas un
        //   troupeau dont la maille est surpeuplée — et fuir n'en dispense pas.
        //   Appliqué d'abord à la seule pâture, ce plafond laissait un fuyard
        //   chronique garder sa satiété de naissance (1,0) : mesuré sur seed
        //   42, 57 à 90 % des têtes des amas les plus denses en fuite depuis
        //   plus de 30 jours, satiété 0,95 contre 0,005 au calme — un million
        //   de têtes à l'an 5. La maille se lit à la position de début de
        //   tour, celle qui a servi à compter sa demande. —
        #[cfg(feature = "profile")]
        let t0 = std::time::Instant::now();
        let zone = range_zone(start);
        let zone_sat = zone_satiation(
            range.production(world.worldgen(), zone),
            demand.get(&zone).copied().unwrap_or(0.0),
        );
        herd.satiation = herd.satiation.min(zone_sat);
        #[cfg(feature = "profile")]
        {
            prof[3] += t0.elapsed().as_nanos() as u64;
        }

        // — Démographie : la satiété commande. `steps` fois quand le troupeau
        //   est simulé grossièrement — on rattrape le temps sauté, on ne le
        //   perd pas.
        herd.population = herd_population_step(herd.population, herd.satiation);
        if herd.population < HERD_MIN {
            doomed.push(entity);
        } else if herd.population > HERD_FISSION {
            // Trop nombreux : la moitié part fonder un troupeau ailleurs.
            herd.population *= 0.5;
            stats.herd_fission(id.0);
            let angle = rng.next_f64() * std::f64::consts::TAU;
            let d = km_to_tiles(1.0);
            // La fille hérite de l'espèce de la mère (un troupeau de cerfs se
            // scinde en deux troupeaux de cerfs).
            fissions.push((
                pos.x + angle.cos() * d,
                pos.y + angle.sin() * d,
                herd.population,
                herd.species,
            ));
        }
    }
    #[cfg(feature = "profile")]
    for (acc, ns) in HERD_PROF.iter().zip(prof) {
        acc.fetch_add(ns, std::sync::atomic::Ordering::Relaxed);
    }
    (doomed, fissions)
}

/// Chronométrage interne de [`update_herds`] (M2-faune), sous `profile` :
/// nanosecondes cumulées de la fuite (`nearest_threat`, balaie meutes +
/// humains), de la pâture (`best_pasture` et la lecture de la tuile) et du
/// broutage (`graze`, écritures) et de la satiété de maille (demande et
/// production, étape 4). Le reste du tour — déplacements,
/// démographie — s'obtient par différence avec la phase « 4d troupeaux ».
/// Des atomiques plutôt qu'un paramètre : `fauna` ne connaît pas le profileur
/// de `Sim`, et la simulation est mono-fil.
#[cfg(feature = "profile")]
pub static HERD_PROF: [std::sync::atomic::AtomicU64; 4] = [
    std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0),
];

/// Un troupeau tel que le voit la passe de fusion : (identifiant stable,
/// position, effectif, espèce, sauvage). « Sauvage » = ni ancré à un foyer ni
/// en cours d'apprivoisement : le cheptel appartient à un clan, et fondre un
/// troupeau qu'on apprivoise diluerait le travail de l'éleveur.
pub type MergeView = (u64, (f64, f64), f32, Species, bool);

/// **Fusion des troupeaux** (chantier de dérive, E) — le pendant de la
/// fission. Deux troupeaux sauvages de même espèce qui **se voient** (distance
/// ≤ rayon d'alerte de l'espèce) se rejoignent, si leur total reste sous
/// `HERD_FISSION` — au-delà, la fission le défairait aussitôt.
///
/// Pourquoi : sous la densité-dépendance (étape 4), un troupeau ne grossit plus
/// jusqu'à la fission, et les moitiés issues des fissions passées ne se
/// rejoignaient jamais. Les têtes plafonnaient, les **entités** montaient — et
/// le coût suit les entités (M2-faune) : 622 troupeaux de 34 têtes, 4 tps.
///
/// Appariement glouton dans l'ordre des identifiants, au plus proche (à
/// égalité, le plus petit identifiant) : déterministe. Au plus une fusion par
/// troupeau et par passe. Renvoie des couples (absorbeur, absorbé), en indices
/// de `herds` ; l'absorbeur est le plus gros (à égalité, le plus petit id).
pub fn merge_plan(herds: &[MergeView]) -> Vec<(usize, usize)> {
    let mut order: Vec<usize> = (0..herds.len()).collect();
    order.sort_by_key(|&i| herds[i].0);
    let mut taken = vec![false; herds.len()];
    let mut plan = Vec::new();
    for &i in &order {
        let (id_i, pos_i, pop_i, sp_i, wild_i) = herds[i];
        if taken[i] || !wild_i {
            continue;
        }
        let r2 = sp_i.flee_radius() * sp_i.flee_radius();
        let mut best: Option<(f64, u64, usize)> = None;
        for &j in &order {
            let (id_j, pos_j, pop_j, sp_j, wild_j) = herds[j];
            if j == i || taken[j] || !wild_j || sp_j != sp_i || pop_i + pop_j > HERD_FISSION {
                continue;
            }
            let d2 = (pos_i.0 - pos_j.0).powi(2) + (pos_i.1 - pos_j.1).powi(2);
            if d2 <= r2 && best.is_none_or(|(bd, bid, _)| d2 < bd || (d2 == bd && id_j < bid)) {
                best = Some((d2, id_j, j));
            }
        }
        if let Some((_, id_j, j)) = best {
            taken[i] = true;
            taken[j] = true;
            let pop_j = herds[j].2;
            let i_absorbs = pop_i > pop_j || (pop_i == pop_j && id_i < id_j);
            plan.push(if i_absorbs { (i, j) } else { (j, i) });
        }
    }
    plan
}

/// Le troupeau broute : la tuile sous lui et ses quatre voisines. C'est ce
/// qui laisse une traînée pâturée visible, et ce qui fait qu'un troupeau trop
/// gros épuise sa propre pâture.
fn graze(world: &mut World, pos: (f64, f64), population: f32) {
    let here = (pos.0.floor() as i64, pos.1.floor() as i64);
    let per_tile = (GRAZE_PER_HEAD * population / 5.0) as u16;
    for (dx, dy) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
        let tile = world.tile_mut(here.0 + dx, here.1 + dy);
        tile.biomass = tile.biomass.saturating_sub(per_tile.min(255) as u8);
    }
}

/// Rayon auquel on mesure la densité locale de proies autour d'une meute :
/// 2 km, quatre fois le rayon de chasse. C'est l'échelle du garde-manger, pas
/// celle du coup de dent — une réponse fonctionnelle se lit contre la densité
/// *disponible*, pas contre celle qu'on est en train de mordre.
pub const PACK_SENSE_RADIUS_TILES: f64 = km_to_tiles(2.0);

/// Bornes hautes des classes de densité locale (têtes dans `PACK_SENSE_RADIUS`).
/// Échelonnées en puissances approximatives de trois : la question est la
/// *forme* de la courbe sur plusieurs ordres de grandeur, pas sa valeur en un
/// point.
#[cfg(feature = "fauna-stats")]
pub const DENSITY_EDGES: [f32; 8] =
    [1.0, 10.0, 30.0, 100.0, 300.0, 1000.0, 3000.0, f32::INFINITY];

/// Télémétrie de la faune — **mesure seulement**, aucune rétroaction sur la
/// simulation, donc aucun risque pour le déterminisme. Type vide hors de la
/// feature `fauna-stats` : les appels s'évaporent à la compilation, comme pour
/// `profile` et `chunk-stats`.
///
/// Elle répond à deux des trois questions de P1 (la troisième — réponse
/// numérique et son retard — se lit sur la série quotidienne du banc, qui n'a
/// besoin de rien ici) :
///
/// - **réponse fonctionnelle** : prises par prédateur et par jour, rangées par
///   classe de densité locale de proies. `pack_kills` ne fait pas apparaître la
///   densité, mais le rayon de chasse et l'écrêtage par le troupeau visé
///   peuvent en introduire une dépendance *de fait* — c'est précisément ce
///   qu'on ne veut pas déduire.
/// - **fraction de refuge** : part des troupeaux jamais entrés dans le rayon de
///   chasse d'une meute. Sans refuge, rien n'empêche mathématiquement la
///   prédation d'aller jusqu'à zéro proie.
#[cfg(not(feature = "fauna-stats"))]
#[derive(Default)]
pub struct FaunaStats;

#[cfg(not(feature = "fauna-stats"))]
impl FaunaStats {
    #[inline(always)]
    pub fn encounter(&mut self, _pos: &Position, _herds: &[HerdView], _pred: f32, _kills: f32) {}
    #[inline(always)]
    pub fn herds_alive(&mut self, _herds: &[HerdView]) {}
    #[inline(always)]
    pub fn herd_step(&mut self, _id: u64, _pos: (f64, f64), _day: u64) {}
    #[inline(always)]
    pub fn herd_fission(&mut self, _id: u64) {}
}

/// Un troupeau au recensement quotidien : (identifiant, position, effectif,
/// satiété, en fuite).
#[cfg(feature = "fauna-stats")]
pub type CensusHerd = (u64, (f64, f64), f32, f32, bool);

/// Rayon du recensement de densité autour d'un troupeau (M1-faune) : 2 km, à
/// l'échelle du domaine vital mesuré (1,5 km net sur 15 j). C'est la zone dont
/// la pâture nourrit ce troupeau — donc celle où une densité-dépendance, si
/// elle existait, devrait se lire.
#[cfg(feature = "fauna-stats")]
pub const HERD_CROWD_RADIUS_TILES: f64 = km_to_tiles(2.0);

/// Bornes hautes des classes de densité du recensement (têtes dans
/// `HERD_CROWD_RADIUS`, le troupeau lui-même compris — d'où un plancher
/// pratique à `HERD_MIN`). Échelonnées comme `DENSITY_EDGES`, décalées vers le
/// haut : on cherche la forme sur plusieurs ordres de grandeur.
#[cfg(feature = "fauna-stats")]
pub const CROWD_EDGES: [f32; 8] =
    [30.0, 100.0, 300.0, 1000.0, 3000.0, 10000.0, 30000.0, f32::INFINITY];

/// Une classe de densité du recensement quotidien. Deux taux nets, et c'est
/// leur écart qui compte : le **potentiel** ne voit que la satiété (le
/// fourrage), le **réalisé** voit tout ce qui a retiré des têtes depuis la
/// veille (prédation, chasse humaine comprises).
#[cfg(feature = "fauna-stats")]
#[derive(Default, Clone, Copy)]
pub struct Crowd {
    pub herd_days: u64,
    pub heads_sum: f64,
    pub satiation_sum: f64,
    /// Troupeaux-jours à satiété ≥ 1 : la natalité est déjà à son plafond.
    pub satiated: u64,
    pub potential_sum: f64,
    /// Troupeaux-jours ayant un recensement la veille (le réalisé s'y calcule).
    pub realized_n: u64,
    pub realized_sum: f64,
    /// Pondéré par l'effectif de la veille : `Σ pop·r / Σ pop` est le taux de
    /// l'ensemble, confrontable à la série `tetes` du banc.
    pub realized_w: f64,
    pub realized_pop: f64,
    /// Troupeaux recensés la veille dans cette classe et disparus depuis
    /// (sous `HERD_MIN`) — le réalisé des survivants seuls serait biaisé.
    pub vanished: u64,
    /// — Les troupeaux EN FUITE au recensement. Leur satiété n'est pas
    ///   recalculée pendant la fuite : si un troupeau fuit sans cesse, il garde
    ///   celle de sa naissance (1,0) et échappe au frein de la maille. Ces
    ///   champs mesurent l'hypothèse au lieu de la déduire. —
    pub pop_sum: f64,
    pub fleeing: u64,
    pub fleeing_pop: f64,
    pub fleeing_sat_sum: f64,
}

/// Ce qui limite le fourrage d'un troupeau, par biome. **Le discriminateur du
/// régulateur par le bas** : sur la seule satiété, un troupeau qui fond parce
/// que la steppe ne donne pas davantage et un troupeau qui fond parce qu'il a
/// tout brouté se ressemblent exactement. Les distinguer demande de comparer la
/// biomasse trouvée à la **capacité de la tuile où elle a été trouvée**.
#[cfg(feature = "fauna-stats")]
#[derive(Default, Clone, Copy)]
pub struct Forage {
    /// Herd-ticks observés dans ce biome (hors fuite : un troupeau qui détale
    /// ne broute pas et n'a pas de satiété du jour).
    pub ticks: u64,
    pub satiation_sum: f64,
    /// Somme du **taux de remplissage** `biomasse trouvée / capacité de la
    /// tuile` : 1,0 = pâture pleine *pour ce biome*, donc le broutage n'y est
    /// pour rien.
    pub fill_sum: f64,
    /// Herd-ticks sous la satiété d'équilibre — le troupeau y perd des têtes.
    pub declining: u64,
    /// Parmi ceux-là, pâture **pleine** (remplissage > 0,9) : le biome ne peut
    /// pas les nourrir, et aucune repousse n'y changera rien.
    pub declining_full_pasture: u64,
    /// Parmi ceux-là, pâture **épuisée** (remplissage < 0,5) : le régulateur par
    /// le bas a effectivement agi, c'est le mécanisme qu'on espère voir.
    pub declining_grazed_out: u64,
    /// Parmi ceux-là, tuile **stérile** (capacité nulle : océan, glacier). Ni
    /// pleine ni broutée — il n'y a jamais rien eu à manger. Compté à part
    /// parce que les ranger sous « broutée à ras » faisait dire au verdict que
    /// le régulateur avait agi : 7 936 herd-ticks de côte sur seed 1337
    /// annonçaient « régulateur à 100 % » sur un rivage nu.
    pub declining_sterile: u64,
    /// Histogramme de satiété, dix classes de 0,1.
    pub sat_hist: [u64; 10],
    /// — Ce que les **neuf sondes** de `best_pasture` voient vraiment, et non
    ///   seulement la meilleure d'entre elles.
    ///
    ///   Le remplissage déjà mesuré (`fill_sum`) porte sur la tuile *retenue*,
    ///   donc sur un maximum : il vaut 1,000 partout, ce qui ne départage pas
    ///   « le pâturage n'est pas entamé » de « il l'est, et le maximum le
    ///   cache ». Un paysage sérieusement brouté présente encore une tuile
    ///   pleine tous les 160 m. La moyenne et le minimum du voisinage
    ///   tranchent : s'ils collent à 1 aussi, il n'y a rien à percevoir et la
    ///   régulation par le fourrage est hors d'atteinte à ces densités. —
    pub sample_ticks: u64,
    pub sample_fill_sum: f64,
    pub sample_min_fill_sum: f64,
}

/// Suivi du **domaine vital** d'un troupeau, sur une fenêtre glissante de la
/// durée d'une repousse. Ce qu'on cherche : le chemin parcouru contre le
/// déplacement net. Chemin ≫ net ⇒ la bête tourne déjà dans un domaine, et le
/// confiner demande de resserrer une portée existante. Chemin ≈ net ⇒ elle
/// dérive, et il faut lui donner un domaine qu'elle n'a pas.
#[cfg(feature = "fauna-stats")]
#[derive(Clone, Copy)]
pub struct Track {
    pub window_start: (f64, f64),
    pub last: (f64, f64),
    pub path: f64,
    pub start_day: u64,
}

/// Fenêtre de suivi, en jours : la durée qu'une tuile broutée met à repousser
/// (~15 j à r = 0,08). C'est l'échelle à laquelle « revenir sur sa pâture »
/// veut dire quelque chose.
#[cfg(feature = "fauna-stats")]
pub const RANGE_WINDOW_DAYS: u64 = 15;

#[cfg(feature = "fauna-stats")]
#[derive(Default)]
pub struct FaunaStats {
    /// Suivi en cours, par troupeau.
    pub tracks: std::collections::BTreeMap<u64, Track>,
    /// Fenêtres achevées : (chemin parcouru, déplacement net), en tuiles.
    pub windows: Vec<(f64, f64)>,
    /// Indexé par `Biome as u8` — `#[repr(u8)]` le garantit, et il y a douze
    /// biomes.
    pub forage: [Forage; 12],
    /// Par classe de densité : (pack-ticks, prédateurs cumulés, prises
    /// cumulées, pack-ticks ayant effectivement pris). Les prédateurs sont
    /// cumulés et non moyennés pour que « prises par prédateur » se calcule à
    /// la fin, pondéré par l'effectif — une classe visitée par une grosse meute
    /// ne doit pas compter comme une visitée par une petite.
    pub response: [(u64, f64, f64, u64); 8],
    /// Troupeaux **distincts** entrés dans le rayon de chasse au moins une fois.
    pub hunted: std::collections::BTreeSet<u64>,
    /// Troupeaux **distincts** ayant existé. Le complément donne le refuge.
    pub seen: std::collections::BTreeSet<u64>,
    /// Recensement quotidien par classe de densité (M1-faune).
    pub crowd: [Crowd; 8],
    /// Le recensement de la veille : effectif (corrigé des fissions survenues
    /// depuis) et classe, par troupeau.
    pub census_prev: std::collections::BTreeMap<u64, (f32, usize)>,
    /// Jours de fuite consécutifs au recensement, par troupeau.
    pub flee_streak: std::collections::BTreeMap<u64, u32>,
    /// Têtes des troupeaux en fuite, par durée de fuite ininterrompue :
    /// 1 jour, 2-6, 7-29, 30 et plus.
    pub streak_pop: [f64; 4],
}

#[cfg(feature = "fauna-stats")]
impl FaunaStats {
    /// Une rencontre : la densité locale vue par cette meute, appariée à ce
    /// qu'elle a prélevé ce tick.
    pub fn encounter(&mut self, pos: &Position, herds: &[HerdView], pred: f32, kills: f32) {
        let (sense2, hunt2) = (
            PACK_SENSE_RADIUS_TILES * PACK_SENSE_RADIUS_TILES,
            PACK_HUNT_RADIUS_TILES * PACK_HUNT_RADIUS_TILES,
        );
        let mut local = 0.0f32;
        for h in herds {
            let d2 = (pos.x - h.pos.0).powi(2) + (pos.y - h.pos.1).powi(2);
            if d2 <= sense2 {
                local += h.population;
            }
            if d2 <= hunt2 {
                self.hunted.insert(h.entity.to_bits().get());
            }
        }
        let i = DENSITY_EDGES.iter().position(|&e| local < e).unwrap_or(7);
        let slot = &mut self.response[i];
        slot.0 += 1;
        slot.1 += f64::from(pred);
        slot.2 += f64::from(kills);
        if kills > 0.0 {
            slot.3 += 1;
        }
    }

    /// Les troupeaux vivants de ce tick — appelé **une fois par tick**, pas par
    /// meute, sinon on paierait le même ensemble autant de fois qu'il y a de
    /// meutes pour n'y rien ajouter.
    pub fn herds_alive(&mut self, herds: &[HerdView]) {
        for h in herds {
            self.seen.insert(h.entity.to_bits().get());
        }
    }

    /// Un pas de troupeau : cumule le chemin, et clôt la fenêtre quand elle est
    /// écoulée. Appelé après le déplacement du tick.
    pub fn herd_step(&mut self, id: u64, pos: (f64, f64), day: u64) {
        let t = self.tracks.entry(id).or_insert(Track {
            window_start: pos,
            last: pos,
            path: 0.0,
            start_day: day,
        });
        t.path += (pos.0 - t.last.0).hypot(pos.1 - t.last.1);
        t.last = pos;
        if day.saturating_sub(t.start_day) >= RANGE_WINDOW_DAYS {
            let net = (pos.0 - t.window_start.0).hypot(pos.1 - t.window_start.1);
            self.windows.push((t.path, net));
            *t = Track { window_start: pos, last: pos, path: 0.0, start_day: day };
        }
    }

    /// Une fission vient de couper ce troupeau en deux : son effectif de la
    /// veille est ramené à la moitié restante, sans quoi le taux réalisé
    /// lirait la scission comme une hécatombe de −0,69/j.
    pub fn herd_fission(&mut self, id: u64) {
        if let Some(prev) = self.census_prev.get_mut(&id) {
            prev.0 *= 0.5;
        }
    }

    /// Le recensement quotidien de M1-faune : pour chaque troupeau, les têtes à
    /// moins de `HERD_CROWD_RADIUS` (lui compris), sa satiété et ses deux taux
    /// nets. Quadratique en troupeaux, mais une fois par jour et sous feature.
    /// `herds` = (id, position, effectif, satiété).
    pub fn crowd_census(&mut self, herds: &[CensusHerd]) {
        let r2 = HERD_CROWD_RADIUS_TILES * HERD_CROWD_RADIUS_TILES;
        let mut next = std::collections::BTreeMap::new();
        let mut streaks = std::collections::BTreeMap::new();
        for &(id, pos, pop, sat, fleeing) in herds {
            let heads: f32 = herds
                .iter()
                .filter(|h| (h.1.0 - pos.0).powi(2) + (h.1.1 - pos.1).powi(2) <= r2)
                .map(|h| h.2)
                .sum();
            let i = CROWD_EDGES.iter().position(|&e| heads < e).unwrap_or(7);
            let c = &mut self.crowd[i];
            c.herd_days += 1;
            c.heads_sum += f64::from(heads);
            c.satiation_sum += f64::from(sat);
            if sat >= 1.0 {
                c.satiated += 1;
            }
            c.potential_sum +=
                f64::from(HERB_BIRTH_PER_DAY * sat.clamp(0.0, 1.0) - HERB_DEATH_PER_DAY);
            c.pop_sum += f64::from(pop);
            if fleeing {
                c.fleeing += 1;
                c.fleeing_pop += f64::from(pop);
                c.fleeing_sat_sum += f64::from(sat);
                let streak = self.flee_streak.get(&id).copied().unwrap_or(0) + 1;
                streaks.insert(id, streak);
                let k = match streak {
                    1 => 0,
                    2..=6 => 1,
                    7..=29 => 2,
                    _ => 3,
                };
                self.streak_pop[k] += f64::from(pop);
            }
            if let Some(&(prev, _)) = self.census_prev.get(&id)
                && prev > 0.0
            {
                let r = f64::from(pop / prev).ln();
                c.realized_n += 1;
                c.realized_sum += r;
                c.realized_w += f64::from(prev) * r;
                c.realized_pop += f64::from(prev);
            }
            next.insert(id, (pop, i));
        }
        for (id, &(_, i)) in &self.census_prev {
            if !next.contains_key(id) {
                self.crowd[i].vanished += 1;
            }
        }
        self.census_prev = next;
        self.flee_streak = streaks;
    }

    /// Le voisinage sondé par `best_pasture`, en moyenne et au pire.
    pub fn pasture_sample(&mut self, biome: cairn_worldgen::Biome, mean: f64, min: f64) {
        let f = &mut self.forage[biome as usize];
        f.sample_ticks += 1;
        f.sample_fill_sum += mean;
        f.sample_min_fill_sum += min;
    }

    /// Un herd-tick de pâture : le biome où il broute, la satiété qui en
    /// résulte, la biomasse trouvée et la capacité de la tuile qui la portait.
    pub fn forage_sample(
        &mut self,
        biome: cairn_worldgen::Biome,
        satiation: f32,
        found: u8,
        cap: u8,
    ) {
        let f = &mut self.forage[biome as usize];
        f.ticks += 1;
        f.satiation_sum += f64::from(satiation);
        // Capacité nulle (océan, glacier) : pas de remplissage définissable.
        let fill = if cap == 0 { 0.0 } else { f64::from(found) / f64::from(cap) };
        f.fill_sum += fill;
        let bin = ((satiation.clamp(0.0, 0.999) * 10.0) as usize).min(9);
        f.sat_hist[bin] += 1;
        if satiation < HERB_SATIATION_EQUILIBRIUM {
            f.declining += 1;
            // Les trois classes sont exclusives et couvrent tout : sans la
            // garde sur `cap`, une tuile stérile tombait dans « broutée à ras »
            // par le seul fait que son remplissage est nul.
            if cap == 0 {
                f.declining_sterile += 1;
            } else if fill > 0.9 {
                f.declining_full_pasture += 1;
            } else if fill < 0.5 {
                f.declining_grazed_out += 1;
            }
        }
    }
}

/// Le système des meutes : poursuite du gibier, prises, démographie.
/// Renvoie les prises à appliquer aux troupeaux et les meutes à retirer.
pub fn update_packs(
    fauna: &mut hecs::World,
    world: &World,
    herds: &[HerdView],
    stats: &mut FaunaStats,
) -> (Vec<Kill>, Vec<hecs::Entity>, Vec<PackFission>) {
    let mut kills = Vec::new();
    let mut doomed = Vec::new();
    let mut fissions = Vec::new();
    stats.herds_alive(herds);

    // Les meutes telles qu'au début du tour : chacune juge son territoire sur
    // la même photo, quel que soit l'ordre d'itération.
    let territory2 = PREDATOR_TERRITORY_TILES * PREDATOR_TERRITORY_TILES;
    let packs_now: Vec<((f64, f64), f32)> = fauna
        .query::<(&Pack, &Position)>()
        .iter()
        .map(|(_, (p, pos))| ((pos.x, pos.y), p.population))
        .collect();

    for (entity, (pack, pos)) in fauna.query_mut::<(&mut Pack, &mut Position)>() {
        // — Les meutes restent en simulation FINE, toujours.
        //
        //   Une interaction fondée sur des rencontres ne survit pas au
        //   grossissement temporel : le cycle d'une chasse — attaquer, la proie
        //   détale, la rattraper, retuer — est plus court que la fenêtre
        //   grossière, et aucun facteur multiplicatif ne le reproduit. Mesuré
        //   sur cinq tentatives : à pas 4 les prédateurs finissaient +59 %,
        //   à pas 24 ils s'effondraient de −75 %, sans réglage intermédiaire
        //   qui tienne.
        //
        //   Et le jeu n'en vaut pas la chandelle : une scène compte ~10 meutes
        //   pour ~150 troupeaux. Les grossir ne rapporte presque rien et casse
        //   toute l'écologie.
        //
        //   D'où la règle : **grossir ce qui est nombreux et faiblement couplé
        //   (troupeaux, flore) ; garder fin ce qui est rare et fortement couplé
        //   (prédateurs).** Un prédateur est rare par nature — c'est le sommet
        //   de la pyramide.

        // Rien de tué tant qu'on n'a pas frappé ce tick. Sans cette remise à
        // zéro, une meute hors de portée conserve son dernier bilan et le
        // reconvertit en croissance à chaque tick : mesuré à 622 meutes et
        // 10 927 prédateurs en 90 jours, contre 13 et 109 attendus.
        pack.last_kills = 0.0;

        // — La proie choisie d'abord : on la garde tant qu'elle vit et qu'elle
        //   reste à portée d'obstination. C'est ce qui permet d'épuiser un
        //   gibier plus rapide que soi. —
        let held = pack.quarry.and_then(|q| {
            herds.iter().find(|h| h.entity == q).and_then(|h| {
                let d2 = (pos.x - h.pos.0).powi(2) + (pos.y - h.pos.1).powi(2);
                (d2 <= PACK_GIVE_UP_TILES * PACK_GIVE_UP_TILES).then_some((d2, h))
            })
        });

        // Sinon seulement, le gibier le plus proche : départage par distance
        // puis ordre de la liste (construite dans l'ordre d'itération) →
        // déterministe.
        let nearest = held.or_else(|| {
            let mut best: Option<(f64, &HerdView)> = None;
            for h in herds {
                let d2 = (pos.x - h.pos.0).powi(2) + (pos.y - h.pos.1).powi(2);
                if best.is_none_or(|(bd, _)| d2 < bd) {
                    best = Some((d2, h));
                }
            }
            best
        });
        pack.quarry = nearest.map(|(_, h)| h.entity);
        if let Some((d2, herd)) = nearest {
            let dist = d2.sqrt();
            if dist <= PACK_HUNT_RADIUS_TILES {
                // — Combien de temps le contact dure-t-il vraiment ?
                //
                //   Multiplier les prises par `steps` supposerait la meute
                //   collée à sa proie toute la fenêtre. C'est faux : dès la
                //   première attaque le troupeau détale de 2 km, hors du rayon
                //   de chasse (500 m). Une rencontre ne vaut donc qu'**un
                //   tick** — sauf si le troupeau est à bout de souffle, et
                //   c'est là, et seulement là, qu'il se fait prendre
                //   plusieurs fois de suite.
                //
                //   Mesuré sans cette borne : les meutes en LOD tuaient 30 à
                //   38 % de plus par prédateur qu'en simulation fine.
                let taken = pack_kills(pack.population, herd.population);
                pack.last_kills = taken;
                kills.push(Kill { herd: herd.entity, head: taken });
            } else {
                // Poursuite : la meute suit les troupeaux, donc elle suit
                // aussi leurs migrations — sans qu'on l'ait écrit.
                let dir = ((herd.pos.0 - pos.x) / dist, (herd.pos.1 - pos.y) / dist);
                try_move(world, pos, dir, PACK_STEP_TILES.min(dist));
            }
        }

        // La rencontre, telle qu'elle a eu lieu : densité locale de proies
        // appariée aux prises du tick. Placé **après** le calcul des prises et
        // **avant** la démographie, seul instant où les deux sont vrais
        // ensemble. Sans la feature, cette ligne n'existe pas.
        stats.encounter(pos, herds, pack.population, pack.last_kills);

        // Le territoire (C3) : les prédateurs à moins de 8 km, elle comprise,
        // sur la photo du début du tour.
        let local: f32 = packs_now
            .iter()
            .filter(|(p, _)| (p.0 - pos.x).powi(2) + (p.1 - pos.y).powi(2) <= territory2)
            .map(|(_, n)| n)
            .sum();
        let (share, room) = territory(local);
        pack.population = pack_population_step(pack.population, pack.last_kills * share, room);
        if pack.population < PACK_MIN {
            doomed.push(entity);
        } else if pack.population > PACK_FISSION {
            // Trop nombreuse pour un seul territoire : la moitié part chasser
            // ailleurs. C'est le pendant exact de la fission des troupeaux, et
            // ce qui permet enfin au **nombre** de meutes de suivre la
            // population de prédateurs — donc à la prédation de couvrir un
            // territoire qui s'étend.
            pack.population *= 0.5;
            // Direction dérivée de la position, sans RNG : `update_packs` n'en
            // tire aucun, et en introduire un ici décalerait tous les flux
            // aléatoires en aval (le déterminisme bit-à-bit est une garde du
            // projet). L'angle varie assez d'une meute à l'autre pour que les
            // filles ne partent pas toutes dans le même sens.
            let angle = (pos.x * 0.7 + pos.y * 1.3).rem_euclid(std::f64::consts::TAU);
            let d = km_to_tiles(2.0);
            fissions.push(PackFission {
                pos: (pos.x + angle.cos() * d, pos.y + angle.sin() * d),
                population: pack.population,
                species: pack.species,
            });
        }
    }
    (kills, doomed, fissions)
}

/// Une meute fille, à faire naître par l'appelant — `fauna` ne connaît pas
/// `Sim::spawn_pack`, comme le reste du module.
pub struct PackFission {
    pub pos: (f64, f64),
    pub population: f32,
    pub species: Species,
}

/// Applique les prises (prédation + chasse humaine) aux troupeaux. L'ordre du
/// vecteur est déterministe, et un troupeau ramené sous le seuil sera retiré
/// au tick suivant par [`update_herds`].
pub fn apply_kills(fauna: &mut hecs::World, kills: &[Kill]) {
    for kill in kills {
        if let Ok(mut herd) = fauna.get::<&mut Herd>(kill.herd) {
            herd.population = (herd.population - kill.head).max(0.0);
        }
    }
}

// — Immigration —
//
// Un troupeau qui tombe sous `HERD_MIN` disparaît, et rien d'autre ne le
// remplace : contrairement à la végétation, qui repart toujours d'une
// banque de graines (`ecology::logistic_step`), un troupeau à 0 individu
// n'a pas de descendance possible. Une zone surchassée au point de perdre
// tous ses troupeaux restait donc vide *pour toujours* — observé et déjà
// noté comme limite en Phase 2 (« la coexistence durable suppose... un
// afflux de gibier »). Ce module ajoute cet afflux : chaque jour, une petite
// chance qu'un troupeau apparaisse près de la population, sur une tuile
// giboyeuse loin de tout troupeau existant. **`Sim::step` (pas ce module)
// garde l'appel derrière `Sim::allow_fauna_immigration`** (vrai par défaut) :
// les scènes de test qui veulent isoler une mécanique de toute interférence
// de faune (`herd_grid=0`, tests de cohésion sociale) le désactivent
// explicitement — voir le commentaire du champ. Une première version
// gardait l'appel derrière `hunted_head > 0` (« ça n'a jamais tué de gibier
// depuis le néant si personne n'a jamais chassé ») : heuristique séduisante
// mais fausse deux fois — une zone où *seuls les prédateurs* ont vidé le
// gibier ne voit jamais `hunted_head` monter, et une scène qui démarre à
// densité de gibier nulle (`herd_grid=0`, y compris côté client) n'a
// simplement personne à qui donner une chance de chasser. Le drapeau
// explicite règle les deux cas sans deviner l'intention depuis l'état.

/// **Bord de la faune** (chantier de dérive, D′). Le gibier est simulé autour
/// des humains — l'immigration le fait naître à 1-4 km d'eux — et ce bord
/// l'y **retient** : au-delà de cette distance de tout humain, un troupeau
/// sauvage ne peut plus faire un pas qui l'éloigne davantage.
///
/// C'est un bord à **flux nul**, la condition qu'on pose en physique pour une
/// région plongée dans un milieu homogène : elle revient à supposer qu'au-delà
/// vit la même population à la même densité, qui renvoie autant de bêtes
/// qu'elle en reçoit. Le bord **absorbant** essayé avant (D, annulé) vidait ce
/// qu'il enclôt : les troupeaux fuient les humains, la fuite les poussait au
/// bord, le bord les retirait (~5 par jour contre une immigration tous les six
/// ou sept jours) — gibier éteint sur 3 seeds sur 4.
///
/// Sans aucun bord, une densité bornée (étape 4) n'empêchait pas le gibier de
/// s'étendre : jusqu'à 430 mailles de 2 km en 540 jours, débit 2,5 à 5 tps.
///
/// **Une règle de périmètre, pas un grossissement temporel** (ce dernier est
/// écarté pour la faune : six biais mesurés). 8 km : deux fois le rayon
/// maximal d'immigration, quatre fois la portée de vue d'un chasseur.
pub const FAUNA_PERIMETER_TILES: f64 = km_to_tiles(8.0);

/// Au-delà de cette distance de tout humain, un troupeau sauvage est
/// **abandonné** : les humains sont partis, le bord avec eux, et le troupeau
/// quitte la simulation. Deux fois le bord, pour que seul un déplacement des
/// humains puisse y laisser un troupeau — le bord interdit d'y aller seul.
pub const FAUNA_ABANDON_TILES: f64 = 2.0 * FAUNA_PERIMETER_TILES;

/// Carré de la distance de `pos` au plus proche humain (∞ sans humain).
pub fn nearest_human_d2(pos: (f64, f64), humans: &[(f64, f64)]) -> f64 {
    humans
        .iter()
        .map(|h| (h.0 - pos.0).powi(2) + (h.1 - pos.1).powi(2))
        .fold(f64::INFINITY, f64::min)
}

/// Le pas de `from` à `to` franchit-il le bord vers l'extérieur ? Refusé s'il
/// mène au-delà de `FAUNA_PERIMETER_TILES` **et** éloigne des humains ; un pas
/// qui rentre, ou qui reste dedans, passe toujours.
pub fn crosses_perimeter(from: (f64, f64), to: (f64, f64), humans: &[(f64, f64)]) -> bool {
    let d2_to = nearest_human_d2(to, humans);
    d2_to > FAUNA_PERIMETER_TILES * FAUNA_PERIMETER_TILES && d2_to > nearest_human_d2(from, humans)
}

/// Chance qu'un site candidat soit tenté par jour (indépendante du succès :
/// la plupart des tentatives échouent simplement le test de distance dans
/// une zone déjà giboyeuse — voir plus bas pourquoi c'est voulu).
const IMMIGRATION_CHANCE_PER_DAY: f32 = 0.15;
/// Le site candidat est tiré autour d'un humain vivant pris au hasard, entre
/// ces deux rayons : assez loin pour ne pas apparaître sous les pieds de
/// quelqu'un, assez proche pour rester dans la zone chargée (chunks déjà
/// résidents pour la plupart — pas de génération forcée au loin).
///
/// **L'ancrage sur un humain est une garde de performance, pas un choix de
/// modélisation — ne pas le remplacer par « là où il y a de l'herbe ».** Ce
/// serait plus réaliste et ce serait ruineux : le monde est infini et l'herbe y
/// est partout, donc le gibier peuplerait des régions que personne ne regarde,
/// chacune coûtant des chunks résidents et des herd-ticks à chaque tour. On ne
/// simule la faune que là où quelqu'un peut la voir ; c'est le même principe
/// que pour les feux et la météo, qui ne naissent eux aussi qu'à proximité des
/// habitants. Corollaire à garder en tête en lisant la **fraction de refuge** :
/// le gibier réapparaît autour des humains alors que les prédateurs
/// réapparaissent autour du gibier, donc le refuge ne peut que décroître — et
/// cette asymétrie-là est assumée, pas un oubli.
const IMMIGRATION_MIN_RADIUS_TILES: f64 = km_to_tiles(1.0);
const IMMIGRATION_MAX_RADIUS_TILES: f64 = km_to_tiles(4.0);
/// Aucun troupeau ne doit déjà se trouver à moins de cette distance du site
/// candidat. **C'est ce qui rend le mécanisme auto-limitant** : une zone
/// déjà giboyeuse n'a simplement jamais de site candidat valide — pas besoin
/// d'écrire « seulement si le gibier local est rare », l'effet en découle.
const IMMIGRATION_MIN_HERD_DISTANCE_TILES: f64 = km_to_tiles(3.0);
/// Biomasse minimale du site pour valoir la peine (même seuil que le
/// placement initial des troupeaux, `scenario::populate`).
const IMMIGRATION_MIN_BIOMASS: u8 = 40;

/// Le site `candidate` convainc-il ? Fonction pure (pas de RNG) : testable
/// sans tirage, séparée de [`daily_immigration`] qui, elle, choisit le site.
fn is_valid_immigration_site(
    candidate: (f64, f64),
    tile: &crate::Tile,
    herds: &[(f64, f64)],
) -> bool {
    if !tile.is_walkable() || tile.biomass < IMMIGRATION_MIN_BIOMASS {
        return false;
    }
    !herds.iter().any(|&(hx, hy)| {
        let d2 = (hx - candidate.0).powi(2) + (hy - candidate.1).powi(2);
        d2 < IMMIGRATION_MIN_HERD_DISTANCE_TILES * IMMIGRATION_MIN_HERD_DISTANCE_TILES
    })
}

/// Passe quotidienne : tire un site candidat près d'un humain au hasard, le
/// valide, et renvoie sa position si un troupeau doit y apparaître (au
/// crate appelant de le faire naître — ce module ne connaît pas
/// `Sim::spawn_herd`, comme le reste de `fauna` ne connaît pas `Sim`).
pub fn daily_immigration(
    humans: &[(f64, f64)],
    herds: &[(f64, f64)],
    world: &mut World,
    seed: WorldSeed,
    time: SimTime,
) -> Option<(f64, f64)> {
    let mut rng = Pcg32::new(seed.derive(salt::IMMIGRATION) ^ splitmix64(time.tick), 0);
    if humans.is_empty() || rng.next_f32() >= IMMIGRATION_CHANCE_PER_DAY {
        return None;
    }
    let anchor = humans[(rng.next_u32() as usize) % humans.len()];
    let angle = rng.next_f64() * std::f64::consts::TAU;
    let r = IMMIGRATION_MIN_RADIUS_TILES
        + rng.next_f64() * (IMMIGRATION_MAX_RADIUS_TILES - IMMIGRATION_MIN_RADIUS_TILES);
    let candidate = (anchor.0 + angle.cos() * r, anchor.1 + angle.sin() * r);
    let tile = world.tile(candidate.0.floor() as i64, candidate.1.floor() as i64);
    is_valid_immigration_site(candidate, &tile, herds).then_some(candidate)
}

// — Immigration de prédateurs —
//
// Le pendant, longtemps manquant, de l'immigration de gibier — sans lui, un
// simple boom-bust de Lotka-Volterra qui éteint les meutes (normal, et
// récurrent) était **définitif** : rien ne faisait revenir un prédateur
// depuis zéro, alors que le gibier, lui, réapparaissait. Mesuré sur un run
// long en tempéré (voir la Chronique) : prédateurs éteints au 6ᵉ mois, puis
// gibier explosant de 495 à 66 800 têtes faute de tout frein. La symétrie
// règle ça : les prédateurs suivent leur proie, donc ils réapparaissent
// **près d'un troupeau** (leur garde-manger) plutôt que près d'un humain —
// et loin des meutes déjà là, exactement comme le gibier apparaît loin des
// troupeaux déjà là (auto-limitation : une zone déjà tenue par des meutes
// n'a jamais de site valide, pas besoin d'écrire « seulement si rare »).

/// Chance qu'un site de meute candidat soit tenté par jour. Plus basse que
/// pour le gibier (0,15) : les prédateurs sont rares par nature, et on ne
/// veut pas qu'ils sur-répriment la proie ; assez haute tout de même pour
/// qu'une extinction se répare en quelques semaines, avant que le gibier
/// n'ait le temps d'exploser.
const PREDATOR_IMMIGRATION_CHANCE_PER_DAY: f32 = 0.06;
/// Le site est tiré autour d'un troupeau (la proie) pris au hasard, entre ces
/// deux rayons : près de son garde-manger, mais pas dessus.
const PREDATOR_IMMIGRATION_MIN_RADIUS_TILES: f64 = km_to_tiles(0.5);
const PREDATOR_IMMIGRATION_MAX_RADIUS_TILES: f64 = km_to_tiles(2.0);
/// Aucune meute ne doit déjà se trouver à moins de cette distance du site —
/// c'est ce qui rend le mécanisme auto-limitant (même rôle que la distance
/// aux troupeaux pour le gibier).
const PREDATOR_IMMIGRATION_MIN_PACK_DISTANCE_TILES: f64 = km_to_tiles(3.0);

/// Le site `candidate` convient-il à une meute ? Fonction pure : praticable,
/// et loin de toute meute existante. Pas de test de biomasse — un prédateur
/// ne broute pas ; la présence de proie est garantie par l'ancrage sur un
/// troupeau dans [`daily_predator_immigration`].
fn is_valid_pack_site(candidate: (f64, f64), tile: &crate::Tile, packs: &[(f64, f64)]) -> bool {
    if !tile.is_walkable() {
        return false;
    }
    !packs.iter().any(|&(px, py)| {
        let d2 = (px - candidate.0).powi(2) + (py - candidate.1).powi(2);
        d2 < PREDATOR_IMMIGRATION_MIN_PACK_DISTANCE_TILES * PREDATOR_IMMIGRATION_MIN_PACK_DISTANCE_TILES
    })
}

/// Passe quotidienne : tire un site près d'un troupeau au hasard, le valide,
/// et renvoie sa position si une meute doit y apparaître (au crate appelant
/// de la faire naître). Renvoie `None` s'il n'y a aucun gibier — sans proie,
/// pas de prédateur, ce qui évite au passage de peupler un monde vide.
pub fn daily_predator_immigration(
    herds: &[(f64, f64)],
    packs: &[(f64, f64)],
    world: &mut World,
    seed: WorldSeed,
    time: SimTime,
) -> Option<(f64, f64)> {
    let mut rng = Pcg32::new(seed.derive(salt::PREDATOR_IMMIGRATION) ^ splitmix64(time.tick), 0);
    if herds.is_empty() || rng.next_f32() >= PREDATOR_IMMIGRATION_CHANCE_PER_DAY {
        return None;
    }
    let anchor = herds[(rng.next_u32() as usize) % herds.len()];
    let angle = rng.next_f64() * std::f64::consts::TAU;
    let r = PREDATOR_IMMIGRATION_MIN_RADIUS_TILES
        + rng.next_f64() * (PREDATOR_IMMIGRATION_MAX_RADIUS_TILES - PREDATOR_IMMIGRATION_MIN_RADIUS_TILES);
    let candidate = (anchor.0 + angle.cos() * r, anchor.1 + angle.sin() * r);
    let tile = world.tile(candidate.0.floor() as i64, candidate.1.floor() as i64);
    is_valid_pack_site(candidate, &tile, packs).then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sim;

    /// CHA-2 : une marche qui coupe la piste fraîche d'un troupeau la remarque
    /// et apprend où elle mène ; une marche parallèle à 100 m, non ; une piste
    /// de plus de deux jours s'est effacée.
    #[test]
    fn on_remarque_la_piste_qu_on_croise() {
        let mut trails: HerdTrails = std::collections::BTreeMap::new();
        let trail: std::collections::VecDeque<_> = (0..10u64).map(|h| (((h * 40) as f64, 0.0), 100 + h)).collect();
        trails.insert(1, trail);
        let found = crossed_trail(&trails, (200.0, -500.0), (200.0, 500.0), TRAIL_READ_TILES);
        assert_eq!(found, Some(((360.0, 0.0), 109)), "la piste mène à son point le plus récent");
        assert_eq!(crossed_trail(&trails, (0.0, 50.0), (400.0, 50.0), TRAIL_READ_TILES), None);
        let mut fauna = hecs::World::new();
        lay_trails(&fauna, &mut trails, 109 + TRAIL_HOURS);
        assert!(trails.is_empty(), "un troupeau disparu n'a plus de piste");
        fauna.spawn((FaunaId(2), Position { x: 0.0, y: 0.0 }, Herd::new(30.0, Species::Deer)));
        for h in 0..(TRAIL_HOURS + 5) {
            lay_trails(&fauna, &mut trails, h);
        }
        assert_eq!(trails[&2].len() as u64, TRAIL_HOURS, "les traces de plus de deux jours s'effacent");
    }

    #[test]
    fn un_troupeau_bien_nourri_croit_et_un_troupeau_affame_fond() {
        // Satiété au-dessus du point d'équilibre (0,5) : croissance.
        let mut repu = 30.0;
        let mut affame = 30.0;
        for _ in 0..24 * 30 {
            repu = herd_population_step(repu, 1.0);
            affame = herd_population_step(affame, 0.0);
        }
        assert!(repu > 33.0, "un mois de pâture pleine doit faire croître : {repu:.1}");
        assert!(affame < 25.0, "un mois sans herbe doit faire fondre : {affame:.1}");
    }

    #[test]
    fn la_satiete_d_equilibre_stabilise_l_effectif() {
        // À satiété = mortalité/natalité = 0,5, le troupeau ne bouge pas.
        let mut pop = 40.0;
        for _ in 0..24 * 90 {
            pop = herd_population_step(pop, 0.5);
        }
        assert!((pop - 40.0).abs() < 0.5, "doit rester ~40 têtes : {pop:.2}");
    }

    #[test]
    fn une_pature_pleine_rassasie_dans_tous_les_biomes() {
        // Un troupeau dans sa niche, sur une pâture **intacte**, doit pouvoir
        // croître — donc dépasser la satiété d'équilibre. Avec une satiété
        // normalisée sur la plage du u8, il ne le peut qu'au-dessus de K = 128 :
        // une prairie pleine (K = 110) condamne l'aurochs à fondre sur de
        // l'herbe vierge, et la steppe encore plus. Mesuré sur 2 234 548
        // herd-ticks : 100 % de déclin en prairie, 0 % en forêt, et pas un seul
        // déclin causé par le broutage. La satiété ne mesurait pas la pâture,
        // elle mesurait le biome.
        for (nom, k) in
            [("steppe", 70u8), ("savane", 90), ("prairie", 110), ("forêt tempérée", 210)]
        {
            let s = satiation_from_forage(k, k, 1.0);
            assert!(
                s > HERB_SATIATION_EQUILIBRIUM,
                "{nom} pleine (K = {k}) ne rassasie pas : satiété {s:.3} ≤ équilibre {:.3}",
                HERB_SATIATION_EQUILIBRIUM
            );
        }
    }

    #[test]
    fn une_pature_pleine_vaut_autant_partout() {
        // L'invariant qui fait de la satiété une mesure de la **pâture** et non
        // du biome : plein, c'est plein, que le biome porte 70 ou 210. Sans lui,
        // le troupeau ne peut pas distinguer « j'ai tout mangé » de « je suis
        // dans un pays pauvre » — une prairie pleine donnait 0,431 et une forêt
        // broutée de moitié 0,412, deux situations écologiquement opposées.
        let steppe = satiation_from_forage(70, 70, 1.0);
        let foret = satiation_from_forage(210, 210, 1.0);
        assert_eq!(
            steppe, foret,
            "pâture pleine : la satiété dépend encore du biome ({steppe:.3} vs {foret:.3})"
        );
    }

    #[test]
    fn la_satiete_suit_ce_qui_reste_a_brouter() {
        // Et elle doit **descendre** quand la pâture baisse, sinon aucune
        // capacité de charge ne peut émerger : c'est le mécanisme même du
        // régulateur par le bas. À moitié broutée, on est pile à l'équilibre —
        // c'est donc là que l'effectif se stabilise, dans n'importe quel biome.
        let k = 210u8;
        assert!(satiation_from_forage(k, k, 1.0) > HERB_SATIATION_EQUILIBRIUM);
        assert!((satiation_from_forage(k / 2, k, 1.0) - HERB_SATIATION_EQUILIBRIUM).abs() < 0.01);
        assert!(satiation_from_forage(k / 4, k, 1.0) < HERB_SATIATION_EQUILIBRIUM);
    }

    #[test]
    fn un_sol_sterile_ne_nourrit_personne() {
        // Capacité nulle (océan, glacier) : pas de division par zéro, et pas de
        // satiété non plus.
        assert_eq!(satiation_from_forage(0, 0, 1.0), 0.0);
    }

    #[test]
    fn les_sondes_de_pature_tiennent_dans_un_chunk() {
        // `best_pasture` lit neuf tuiles en croix, espacées de
        // `GRAZE_SAMPLE_TILES`. À 80 tuiles, la croix couvre 160 tuiles de large
        // pour un chunk qui en fait 64 : chaque troupeau touche donc jusqu'à
        // neuf **chunks distincts** à chaque tick, et force la génération
        // paresseuse à matérialiser des tuiles dont personne d'autre n'a besoin
        // (M1 : 216 tuiles générées par tuile utile). C'est le poste le plus
        // lourd de la faune, qui pèse 84,8 % du temps de tick.
        //
        // L'invariant : la croix entière doit tenir dans la largeur d'un chunk,
        // pour que les sondes tombent sur celui où le troupeau se trouve déjà.
        assert!(
            GRAZE_SAMPLE_TILES * 2 <= crate::CHUNK_SIZE,
            "sondage sur {} tuiles de large pour un chunk de {} : chaque troupeau \
             traverse plusieurs chunks à chaque tick",
            GRAZE_SAMPLE_TILES * 2,
            crate::CHUNK_SIZE
        );
    }

    #[test]
    fn une_meute_sans_proie_s_eteint() {
        let mut pop = 8.0;
        let mut jours = 0;
        while pop >= PACK_MIN && jours < 24 * 365 {
            pop = pack_population_step(pop, 0.0, 1.0);
            jours += 1;
        }
        assert!(pop < PACK_MIN, "une meute sans gibier doit disparaître");
    }

    #[test]
    fn une_meute_qui_prospere_finit_par_se_scinder() {
        // La garde du correctif de couverture : sans fission, le NOMBRE de
        // meutes ne croissait que par immigration — jamais plus de 19 sur 8,4
        // années de jeu, face à 1 483 troupeaux. Une meute est un point dans
        // l'espace ; sa taille ne remplace pas sa position.
        let mut sim = Sim::new(WorldSeed(42), 256);
        sim.allow_fauna_immigration = false; // sinon on ne saurait pas d'où vient la 2ᵉ
        let home = (0.0, 0.0);
        sim.spawn_pack(home.0, home.1, PACK_FISSION - 0.03);
        // Du gibier sous son nez, pour qu'elle franchisse le seuil : nourrie à
        // sa faim, seule sur son territoire, la meute croît de ~0,1 %/jour
        // (C1) freiné par la place qui reste (C3) — ~0,002 prédateur par jour
        // à 7,97, soit le seuil en une quinzaine de jours.
        sim.spawn_herd(home.0 + 1.0, home.1, 400.0);
        assert_eq!(sim.fauna.query::<&Pack>().iter().count(), 1);

        for _ in 0..24 * 30 {
            sim.step();
        }
        let meutes = sim.fauna.query::<&Pack>().iter().count();
        assert!(meutes >= 2, "une meute nourrie doit se scinder, on en compte {meutes}");
    }

    #[test]
    fn une_meute_bien_nourrie_croit() {
        let mut pop = 5.0;
        for _ in 0..24 * 60 {
            let kills = pack_kills(pop, 1000.0); // gibier abondant
            pop = pack_population_step(pop, kills, 1.0);
        }
        assert!(pop > 5.0, "avec du gibier à volonté, la meute croît : {pop:.1}");
    }

    /// Chantier de dérive, C1 : **une meute ne croît pas plus vite qu'une
    /// population de loups**. Nourrie à volonté pendant un an, une vraie
    /// population croît d'environ 0,3 à 0,5 par an (×1,35 à ×1,65) ; un plafond
    /// à ×2 laisse de la marge sans admettre l'ancien calibrage, qui permettait
    /// 0,079 par jour — ×10¹² en un an, soit un à deux milliers de loups sur une
    /// centaine de km² mesurés à l'étape 4, contre 1 à 5 dans la nature.
    #[test]
    fn une_meute_ne_croit_pas_plus_vite_qu_un_loup() {
        let mut pop = 5.0;
        for _ in 0..24 * 360 {
            let kills = pack_kills(pop, 1.0e6); // gibier à volonté
            pop = pack_population_step(pop, kills, 1.0);
        }
        let facteur = pop / 5.0;
        assert!(
            (1.2..=2.0).contains(&facteur),
            "nourrie à volonté un an, la meute a été multipliée par {facteur:.3e} \
             (réel : ×1,35 à ×1,65)"
        );
    }

    /// C3, l'autre face du territoire : **une meute seule sur son territoire,
    /// bien nourrie, croît jusqu'à ce qu'il porte** — pas au-delà, mais pas en
    /// deçà non plus. Le territoire freine la multiplication ; il ne doit pas
    /// faire passer la marge d'un loup (0,001/j) sous sa famine.
    #[test]
    fn une_meute_seule_croit_jusqu_a_ce_que_porte_son_territoire() {
        let mut pop = 3.0;
        for _ in 0..24 * 360 * 8 {
            let kills = pack_kills(pop, 1.0e6);
            let (share, room) = territory(pop);
            pop = pack_population_step(pop, kills * share, room);
        }
        let k = predators_per_territory();
        assert!(
            (0.7 * k..=1.05 * k).contains(&pop),
            "seule et nourrie à volonté huit ans, la meute compte {pop:.2} prédateurs \
             pour un territoire qui en porte {k:.1}"
        );
    }

    #[test]
    fn les_prises_sont_bornees_par_le_gibier_present() {
        // On ne tue pas plus de bêtes qu'il n'y en a : sinon l'effectif
        // passerait négatif et la meute se nourrirait de fantômes.
        // 100 prédateurs peuvent prendre 0,21 bête par tick (C1) : il n'y en a
        // que 0,1.
        assert_eq!(pack_kills(100.0, 0.1), 0.1);
        assert!(pack_kills(2.0, 1000.0) < 1.0);
    }

    #[test]
    fn on_fuit_a_l_oppose_de_la_menace() {
        let dir = away_from((100.0, 100.0), (90.0, 100.0));
        assert!((dir.0 - 1.0).abs() < 1e-9 && dir.1.abs() < 1e-9);
        // Menace pile dessus : pas de division par zéro.
        let dir = away_from((0.0, 0.0), (0.0, 0.0));
        assert!(dir.0.is_finite() && dir.1.is_finite());
    }

    #[test]
    fn la_menace_hors_de_portee_n_alarme_pas() {
        let loin = FLEE_RADIUS_TILES + 10.0;
        assert!(nearest_threat((0.0, 0.0), &[(loin, 0.0)], FLEE_RADIUS_TILES).is_none());
        assert!(nearest_threat((0.0, 0.0), &[(10.0, 0.0)], FLEE_RADIUS_TILES).is_some());
        // La plus proche gagne.
        let t = nearest_threat((0.0, 0.0), &[(200.0, 0.0), (10.0, 0.0)], FLEE_RADIUS_TILES).unwrap();
        assert_eq!(t, (10.0, 0.0));
    }

    // — Diversité d'espèces —

    #[test]
    fn l_espece_choisie_colle_au_biome() {
        // Biomes à candidat unique : le tirage est déterministe quel que soit
        // le RNG — c'est « le bon animal au bon endroit ».
        let mut rng = Pcg32::new(1, 0);
        assert_eq!(Species::herbivore_for_biome(Biome::Grassland, &mut rng), Species::Aurochs);
        assert_eq!(Species::herbivore_for_biome(Biome::Savanna, &mut rng), Species::Gazelle);
        assert_eq!(Species::herbivore_for_biome(Biome::Tundra, &mut rng), Species::Reindeer);
        assert_eq!(Species::herbivore_for_biome(Biome::TemperateForest, &mut rng), Species::Deer);
    }

    #[test]
    fn l_habitat_borne_la_niche_et_les_predateurs_l_ignorent() {
        // Un herbivore est au mieux dans son habitat, médiocre ailleurs : c'est
        // ce qui le cantonne à sa niche (aire de répartition émergente).
        assert_eq!(Species::Aurochs.habitat_factor(Biome::Grassland), 1.0);
        assert!(Species::Aurochs.habitat_factor(Biome::HotDesert) < 1.0);
        assert_eq!(Species::Reindeer.habitat_factor(Biome::Tundra), 1.0);
        assert!(Species::Reindeer.habitat_factor(Biome::Savanna) < 1.0);
        // Un prédateur suit la proie : son habitat ne le borne pas.
        assert_eq!(Species::Wolf.habitat_factor(Biome::HotDesert), 1.0);
        assert_eq!(Species::CaveLion.habitat_factor(Biome::Tundra), 1.0);
    }

    #[test]
    fn dangerosite_et_domestication_distinguent_les_especes() {
        // Anticipation « éleveur » : les fauves sont dangereux à chasser, pas
        // les proies (sauf l'aurochs, qui encorne un peu).
        assert!(Species::CaveLion.danger() > Species::Wolf.danger());
        assert_eq!(Species::Gazelle.danger(), 0.0);
        assert!(Species::Aurochs.danger() > 0.0);
        // Domesticables : le bétail (aurochs) et le renne, pas le cerf ni les
        // fauves.
        assert!(Species::Aurochs.domesticable() && Species::Reindeer.domesticable());
        assert!(!Species::Deer.domesticable() && !Species::Wolf.domesticable());
    }

    /// Cherche une tuile praticable et giboyeuse près de `from`, pour les
    /// tests — même idiome que `sim::tests::find_land` (sondage par pas de
    /// 16 km dans 8 directions, jusqu'à 6 400 km — les océans entre
    /// continents font des milliers de km), avec la biomasse en plus de la
    /// terre : `find_land` seul retomberait parfois sur une plage ou un
    /// désert, pas assez giboyeux pour ces tests.
    fn find_lush(world: &mut World, from: (i64, i64)) -> (i64, i64) {
        let step = cairn_core::km_to_tiles(16.0) as i64;
        for r in 0..400i64 {
            let d = r * step;
            for &(x, y) in &[
                (d, 0), (-d, 0), (0, d), (0, -d),
                (d, d), (-d, d), (d, -d), (-d, -d),
            ] {
                let p = (from.0 + x, from.1 + y);
                let tile = world.tile(p.0, p.1);
                if tile.is_walkable() && tile.biomass >= IMMIGRATION_MIN_BIOMASS {
                    return p;
                }
            }
        }
        panic!("aucune tuile giboyeuse trouvée près de {from:?}");
    }

    #[test]
    fn un_site_giboyeux_sans_troupeau_voisin_est_valide() {
        let mut world = World::new(WorldSeed(42), 64);
        let lush = find_lush(&mut world, (0, 0));
        let tile = world.tile(lush.0, lush.1);
        let candidate = (lush.0 as f64 + 0.5, lush.1 as f64 + 0.5);
        assert!(
            is_valid_immigration_site(candidate, &tile, &[]),
            "une tuile giboyeuse sans troupeau à proximité doit être un site valide"
        );
    }

    #[test]
    fn un_troupeau_proche_invalide_le_site_mais_pas_un_troupeau_lointain() {
        let mut world = World::new(WorldSeed(42), 64);
        let lush = find_lush(&mut world, (0, 0));
        let tile = world.tile(lush.0, lush.1);
        let candidate = (lush.0 as f64 + 0.5, lush.1 as f64 + 0.5);

        let tout_pres = [(candidate.0 + 10.0, candidate.1)];
        assert!(
            !is_valid_immigration_site(candidate, &tile, &tout_pres),
            "un troupeau à 10 tuiles doit invalider le site (bien sous IMMIGRATION_MIN_HERD_DISTANCE_TILES)"
        );

        let tres_loin = [(candidate.0 + km_to_tiles(50.0), candidate.1)];
        assert!(
            is_valid_immigration_site(candidate, &tile, &tres_loin),
            "un troupeau à 50 km ne doit pas gêner un site par ailleurs valide"
        );
    }

    /// LE critère de ce mécanisme : une zone vidée de tout troupeau finit par
    /// être réensemencée. Beaucoup de jours simulés (le tirage n'a que 15 %
    /// de chances par jour) pour une confiance statistique, comme les autres
    /// tests stochastiques de ce projet (émergence de clan, exploration…).
    #[test]
    fn une_zone_giboyeuse_sans_troupeau_finit_par_etre_reensemencee() {
        let mut world = World::new(WorldSeed(42), 64);
        let lush = find_lush(&mut world, (0, 0));
        let anchor = (lush.0 as f64 + 0.5, lush.1 as f64 + 0.5);
        let humans = [anchor];

        let mut found = false;
        for day in 0..2000u64 {
            let time = SimTime { tick: day * 24 };
            if daily_immigration(&humans, &[], &mut world, WorldSeed(42), time).is_some() {
                found = true;
                break;
            }
        }
        assert!(found, "aucune immigration en ~5,5 ans simulés sur une zone giboyeuse vide");
    }

    /// Le pendant du critère précédent : une zone déjà dense en troupeaux
    /// (tous les candidats retombent à moins d'`IMMIGRATION_MIN_HERD_DISTANCE_TILES`)
    /// ne doit **jamais** recevoir d'immigrant — le mécanisme est
    /// auto-limitant sans qu'aucune règle ne dise « pas si déjà peuplé ».
    #[test]
    fn une_zone_deja_dense_en_troupeaux_ne_recoit_jamais_d_immigrant() {
        let mut world = World::new(WorldSeed(42), 64);
        let lush = find_lush(&mut world, (0, 0));
        let anchor = (lush.0 as f64 + 0.5, lush.1 as f64 + 0.5);
        let humans = [anchor];
        // Un quadrillage serré de troupeaux couvrant largement le rayon
        // d'apparition possible (jusqu'à IMMIGRATION_MAX_RADIUS_TILES).
        let step = km_to_tiles(2.0);
        let mut herds = Vec::new();
        for gy in -3..=3 {
            for gx in -3..=3 {
                herds.push((anchor.0 + gx as f64 * step, anchor.1 + gy as f64 * step));
            }
        }
        for day in 0..2000u64 {
            let time = SimTime { tick: day * 24 };
            assert!(
                daily_immigration(&humans, &herds, &mut world, WorldSeed(42), time).is_none(),
                "jour {day} : une zone déjà dense en troupeaux ne doit jamais recevoir d'immigrant"
            );
        }
    }

    #[test]
    fn sans_humain_aucune_immigration() {
        let mut world = World::new(WorldSeed(42), 16);
        for day in 0..500u64 {
            let time = SimTime { tick: day * 24 };
            assert!(daily_immigration(&[], &[], &mut world, WorldSeed(42), time).is_none());
        }
    }

    // — Immigration de prédateurs (symétrie avec le gibier) —

    #[test]
    fn un_site_de_meute_sans_meute_voisine_est_valide() {
        let mut world = World::new(WorldSeed(42), 64);
        let land = find_lush(&mut world, (0, 0));
        let tile = world.tile(land.0, land.1);
        let candidate = (land.0 as f64 + 0.5, land.1 as f64 + 0.5);
        assert!(is_valid_pack_site(candidate, &tile, &[]), "terre sans meute voisine : site valide");
    }

    #[test]
    fn une_meute_proche_invalide_le_site_pas_une_lointaine() {
        let mut world = World::new(WorldSeed(42), 64);
        let land = find_lush(&mut world, (0, 0));
        let tile = world.tile(land.0, land.1);
        let candidate = (land.0 as f64 + 0.5, land.1 as f64 + 0.5);
        assert!(
            !is_valid_pack_site(candidate, &tile, &[(candidate.0 + 10.0, candidate.1)]),
            "une meute à 10 tuiles doit invalider le site"
        );
        assert!(
            is_valid_pack_site(candidate, &tile, &[(candidate.0 + km_to_tiles(50.0), candidate.1)]),
            "une meute à 50 km ne doit pas gêner"
        );
    }

    /// LE critère du correctif : une zone giboyeuse mais **sans prédateur**
    /// (extinction) finit par en recevoir un — c'est ce qui, longtemps
    /// manquant, laissait le gibier exploser sans frein.
    #[test]
    fn une_zone_giboyeuse_sans_meute_finit_par_recevoir_un_predateur() {
        let mut world = World::new(WorldSeed(42), 64);
        let lush = find_lush(&mut world, (0, 0));
        let herds = [(lush.0 as f64 + 0.5, lush.1 as f64 + 0.5)];
        let mut found = false;
        for day in 0..2000u64 {
            let time = SimTime { tick: day * 24 };
            if daily_predator_immigration(&herds, &[], &mut world, WorldSeed(42), time).is_some() {
                found = true;
                break;
            }
        }
        assert!(found, "un prédateur doit finir par réapparaître près d'un troupeau isolé");
    }

    /// Le pendant : sans le moindre gibier, aucun prédateur n'immigre — un
    /// prédateur suit sa proie, on ne peuple pas un monde vide de carnivores.
    #[test]
    fn sans_gibier_aucune_immigration_de_predateur() {
        let mut world = World::new(WorldSeed(42), 16);
        for day in 0..500u64 {
            let time = SimTime { tick: day * 24 };
            assert!(daily_predator_immigration(&[], &[], &mut world, WorldSeed(42), time).is_none());
        }
    }
}
