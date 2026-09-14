//! L'arbre technologique, **en données** (BRIEF §5.2 et §8.1, Phase 5
//! « L'ÉTINCELLE », incrément 3). Ce module ne fait que **charger et
//! représenter** l'arbre ; le moteur qui *découvre* les techs (l'insight) est
//! à part (incrément 3b).
//!
//! Le principe cardinal (BRIEF §5.2) : une technologie n'est pas « débloquée
//! en dépensant des points ». Elle est **découverte**, par un individu, dans
//! un contexte donné, avec une probabilité qui dépend de sa curiosité, de sa
//! compétence, de ce qu'il a **vu** (`prereq_exposure`), de ce à quoi il a
//! **accès** ici et maintenant (`prereq_environment`), et de la **pression**
//! qui pèse sur son clan. C'est ce qui rend l'arbre non linéaire et
//! contextuel : deux mondes ne le parcourent pas dans le même ordre.
//!
//! ## Pourquoi des données, pas du code
//!
//! L'arbre vit dans `assets/techs.ron` — un format lisible, commentable, qui
//! supporte enums et structs (mieux que JSON pour de la donnée écrite à la
//! main). On peut rééquilibrer un prérequis ou ajouter une branche **sans
//! toucher au moteur** (arbitrage acté avec l'utilisateur au démarrage de la
//! phase). Le RON est **embarqué à la compilation** (`include_str!`) : présent
//! partout, y compris en WASM (aucun accès fichier à l'exécution), et
//! déterministe. Un chargement depuis un fichier à l'exécution (pour itérer
//! côté serveur sans même recompiler) pourra s'ajouter plus tard en réutilisant
//! [`TechTree::from_ron`], sans rien changer d'autre.
//!
//! ## Noms puis index
//!
//! Dans le RON, une tech se nomme par une chaîne stable (`id`), et ses
//! prérequis la référencent par ce nom — lisible et robuste à la réécriture.
//! Au chargement, on compile ces noms en [`TechId`] denses (un index) : les
//! ensembles de savoirs d'un agent ou d'un clan sont alors des `BTreeSet<TechId>`
//! comparés et itérés dans un ordre déterministe, sans jamais hacher une chaîne.

use std::collections::{BTreeMap, BTreeSet};

use cairn_core::{Pcg32, km_to_tiles, splitmix64};
use serde::Deserialize;

use crate::agent::{AgentId, Physiology, Position};
use crate::chronicle::EventKind;
use crate::demography::{Demographics, Sex, Traits};
use crate::exposure::{Exposure, Exposures};
use crate::memory::{Memory, TALK_RADIUS_TILES};
use crate::pressure::ClanPressure;
use crate::salt;
use crate::sim::Sim;
use crate::skills::Skills;
use crate::social::{ClanId, ClanMembership};
use crate::tile::Tile;

/// L'arbre embarqué à la compilation. Le chemin remonte de `crates/sim/src`
/// jusqu'à la racine du workspace, où vit `assets/` (BRIEF §8.1).
const EMBEDDED_TECHS: &str = include_str!("../../../assets/techs.ron");

/// La compétence pertinente pour une découverte — le facteur
/// `compétence_pertinente` de la formule d'insight (§5.2). On réutilise pour
/// l'instant les trois compétences existantes (`skills::Skills`) comme proxys ;
/// les métiers dédiés du BRIEF (taille de pierre, agriculture…) viendront
/// affiner cela avec leurs propres systèmes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum TechSkill {
    Foraging,
    Hunting,
    Oratory,
}

impl TechSkill {
    /// La valeur de cette compétence chez un agent.
    pub fn of(self, s: &Skills) -> f32 {
        match self {
            TechSkill::Foraging => s.foraging,
            TechSkill::Hunting => s.hunting,
            TechSkill::Oratory => s.oratory,
        }
    }
}

/// La pression qui motive une découverte — le facteur `pression` (§5.2),
/// renvoyant vers une composante de [`ClanPressure`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum TechPressure {
    Famine,
    Cold,
    Threat,
    Crowding,
}

impl TechPressure {
    /// La valeur de cette pression pour un clan.
    pub fn of(self, p: &ClanPressure) -> f32 {
        match self {
            TechPressure::Famine => p.famine,
            TechPressure::Cold => p.cold,
            TechPressure::Threat => p.threat,
            TechPressure::Crowding => p.crowding,
        }
    }
}

/// L'accès environnemental requis **ici et maintenant** (BRIEF
/// `prereq_environment` : « a ACCÈS à une rivière, une forêt, un four »). À la
/// différence de l'exposition (un souvenir persistant), c'est une condition de
/// l'instant, lue sur la tuile de l'agent au moment de l'insight — jamais
/// stockée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum EnvCond {
    FreshWater,
    Forest,
}

impl EnvCond {
    /// La condition est-elle remplie sur cette tuile ?
    pub fn met(self, tile: &Tile) -> bool {
        use cairn_worldgen::Biome::*;
        match self {
            EnvCond::FreshWater => tile.has_fresh_water(),
            EnvCond::Forest => matches!(tile.biome, TemperateForest | TropicalForest | Taiga),
        }
    }
}

/// L'« âge » d'un clan (BRIEF §5.5) : une **étiquette dérivée** de l'ensemble
/// des techs qu'il possède — **jamais** une condition de déblocage (aucun code
/// ne teste l'âge pour autoriser quoi que ce soit ; il n'est qu'affiché).
/// L'ordre de déclaration est l'ordre chronologique (`Ord` dérivé) : l'âge d'un
/// clan est le plus avancé des marqueurs de ses techs. En données : chaque tech
/// porte un marqueur `age` optionnel dans le RON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, serde::Serialize)]
pub enum Age {
    Paleolithic,
    Neolithic,
    BronzeAge,
}

impl Age {
    /// Libellé lisible (affichage client).
    pub fn label(self) -> &'static str {
        match self {
            Age::Paleolithic => "Paléolithique",
            Age::Neolithic => "Néolithique",
            Age::BronzeAge => "Âge du bronze",
        }
    }
}

/// L'âge dérivé d'un corpus de savoirs : le plus avancé des marqueurs d'âge de
/// ses techs, ou `Paleolithic` par défaut (un clan sans tech marquante en est
/// encore là). Fonction **pure** — l'étiquette se recalcule, ne se stocke pas.
pub fn age_of(corpus: &BTreeSet<TechId>, tree: &TechTree) -> Age {
    corpus.iter().filter_map(|&t| tree.get(t).age).max().unwrap_or(Age::Paleolithic)
}

/// Identifiant dense d'une technologie : un index dans [`TechTree`]. Stable
/// pour une version donnée du RON (l'ordre de déclaration fixe les index).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct TechId(pub u16);

/// Une technologie telle qu'**écrite** dans le RON : prérequis par nom. Forme
/// intermédiaire, compilée en [`Tech`] au chargement.
#[derive(Debug, Clone, Deserialize)]
struct TechSpec {
    id: String,
    label: String,
    /// Le groupe nominal **avec son déterminant**, tel qu'on l'écrit dans une
    /// phrase : « la maîtrise du feu », « le bronze », « l'agriculture ». La
    /// Chronique en a besoin — le français n'a pas d'article neutre, et rien
    /// dans le code ne peut deviner le genre d'un libellé. Optionnel : à défaut,
    /// `label` décapitalisé fait l'affaire (voir `Tech::narrated`).
    #[serde(default)]
    narrative: Option<String>,
    #[serde(default)]
    prereq_techs: Vec<String>,
    #[serde(default)]
    prereq_exposure: Vec<Exposure>,
    /// Alternatives d'exposition (« au moins **une** parmi »), en plus des
    /// exigences `prereq_exposure` (« **toutes** »). Sert à encoder plusieurs
    /// recettes pour une même tech — p. ex. le feu par le silex **ou** par un
    /// incendie observé (l'arbitrage « les deux » de la Phase 5).
    #[serde(default)]
    prereq_exposure_any: Vec<Exposure>,
    #[serde(default)]
    prereq_environment: Vec<EnvCond>,
    #[serde(default)]
    pressure: Vec<TechPressure>,
    #[serde(default)]
    skill: Option<TechSkill>,
    /// Marqueur d'âge (BRIEF §5.5) : l'âge qu'atteint un clan qui possède cette
    /// tech, s'il est plus avancé que le sien. Optionnel (une tech mineure n'en
    /// marque aucun).
    #[serde(default)]
    age: Option<Age>,
}

/// Une technologie **compilée** : prérequis de savoir résolus en [`TechId`].
#[derive(Debug, Clone)]
pub struct Tech {
    pub id: TechId,
    /// Identifiant textuel stable (référencé par les prérequis, la Chronique).
    pub name: String,
    /// Libellé lisible (affichage client).
    pub label: String,
    /// Forme narrative — voir `TechSpec::narrative` et [`Tech::narrated`].
    pub narrative: Option<String>,
    pub prereq_techs: Vec<TechId>,
    pub prereq_exposure: Vec<Exposure>,
    /// « Au moins une parmi » — voir `TechSpec::prereq_exposure_any`.
    pub prereq_exposure_any: Vec<Exposure>,
    pub prereq_environment: Vec<EnvCond>,
    pub pressure: Vec<TechPressure>,
    pub skill: Option<TechSkill>,
    /// Marqueur d'âge — voir `TechSpec::age` et `age_of`.
    pub age: Option<Age>,
}

impl Tech {
    /// Le nom du savoir tel qu'il s'insère dans une phrase de Chronique :
    /// « … découvre **la maîtrise du feu**. » À défaut de forme narrative dans
    /// les données, on décapitalise le libellé — imparfait (l'article manque)
    /// mais jamais fautif au point d'être illisible, et surtout : une tech
    /// ajoutée au RON sans y penser reste racontable.
    pub fn narrated(&self) -> String {
        match &self.narrative {
            Some(n) => n.clone(),
            None => {
                let mut chars = self.label.chars();
                match chars.next() {
                    Some(c) => c.to_lowercase().chain(chars).collect(),
                    None => String::new(),
                }
            }
        }
    }
}

/// L'arbre technologique chargé : la liste des techs (indexée par [`TechId`])
/// et l'index nom → id.
#[derive(Debug, Clone)]
pub struct TechTree {
    techs: Vec<Tech>,
    by_name: BTreeMap<String, TechId>,
}

impl TechTree {
    /// L'arbre embarqué dans le binaire. Panique si le RON d'`assets/` est
    /// malformé : c'est un asset de compilation, un défaut serait un bug —
    /// couvert par un test de non-régression.
    pub fn embedded() -> TechTree {
        Self::from_ron(EMBEDDED_TECHS)
            .expect("assets/techs.ron doit être un arbre valide (asset embarqué)")
    }

    /// Charge un arbre depuis une source RON. Erreur (au lieu de panique) car
    /// une source *externe* (fichier serveur, à venir) peut légitimement être
    /// invalide, et l'appelant doit pouvoir le rapporter.
    pub fn from_ron(src: &str) -> Result<TechTree, String> {
        let specs: Vec<TechSpec> =
            ron::from_str(src).map_err(|e| format!("RON de l'arbre technologique invalide : {e}"))?;

        // Index nom → id, dans l'ordre de déclaration ; rejette les doublons.
        let mut by_name: BTreeMap<String, TechId> = BTreeMap::new();
        for (i, spec) in specs.iter().enumerate() {
            if by_name.insert(spec.id.clone(), TechId(i as u16)).is_some() {
                return Err(format!("technologie déclarée deux fois : '{}'", spec.id));
            }
        }

        // Résout les prérequis (nom → id) ; rejette une référence inconnue.
        let mut techs = Vec::with_capacity(specs.len());
        for (i, spec) in specs.into_iter().enumerate() {
            let mut prereq_techs = Vec::with_capacity(spec.prereq_techs.len());
            for name in &spec.prereq_techs {
                let id = by_name.get(name).copied().ok_or_else(|| {
                    format!("'{}' requiert une technologie inconnue : '{name}'", spec.id)
                })?;
                prereq_techs.push(id);
            }
            techs.push(Tech {
                id: TechId(i as u16),
                name: spec.id,
                label: spec.label,
                narrative: spec.narrative,
                prereq_techs,
                prereq_exposure: spec.prereq_exposure,
                prereq_exposure_any: spec.prereq_exposure_any,
                prereq_environment: spec.prereq_environment,
                pressure: spec.pressure,
                skill: spec.skill,
                age: spec.age,
            });
        }
        Ok(TechTree { techs, by_name })
    }

    /// La technologie d'identifiant `id`.
    pub fn get(&self, id: TechId) -> &Tech {
        &self.techs[id.0 as usize]
    }

    /// Le nom d'une tech tel qu'on l'insère dans une phrase (voir
    /// [`Tech::narrated`]) — le raccourci dont se sert la Chronique.
    pub fn narrated(&self, id: TechId) -> String {
        self.get(id).narrated()
    }

    /// L'identifiant de la technologie de nom `name`, si elle existe.
    pub fn id_of(&self, name: &str) -> Option<TechId> {
        self.by_name.get(name).copied()
    }

    /// Toutes les technologies, dans l'ordre de déclaration (donc de `TechId`).
    pub fn iter(&self) -> impl Iterator<Item = &Tech> {
        self.techs.iter()
    }

    pub fn len(&self) -> usize {
        self.techs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.techs.is_empty()
    }
}

// — Le moteur d'insight (incrément 3b) —
//
// La formule du BRIEF §5.2, `P(insight) ∝ curiosité × compétence × exposition
// × pression`, rendue littérale. L'exposition et l'environnement agissent en
// **portail** binaire (réunis ou non) ; parmi les techs ainsi ouvertes,
// curiosité × compétence × pression fixent la chance quotidienne. Deux gardes
// encadrent le tirage, une par face de la tension du §5.2 :
//   - l'agent doit avoir du **temps de cerveau** (besoins couverts) — sinon la
//     misère l'accapare ;
//   - son clan doit subir une **pression** présente parmi celles de la tech —
//     sinon « un clan repu n'invente presque rien ».

/// Taux de base d'un insight, par jour et par tech ouverte, avant pondération
/// par curiosité × compétence × pression. **Le** bouton de calibrage de la
/// phase : réglé pour que « le feu découvert par ≥ 1 clan sur 3 en 500 ans »
/// tienne — validé hors ligne sur un banc long (500 ans ≈ dizaines d'heures de
/// calcul, hors d'un test, comme le critère démographique 50→300 en 100 ans).
/// Volontairement bas : une découverte est rare.
const BASE_INSIGHT_PER_DAY: f32 = 0.004;
/// Compétence prêtée à une tech sans compétence pertinente déclarée : ni
/// avantage ni handicap. (Toutes les techs de l'amorce en déclarent une.)
const NEUTRAL_COMPETENCE: f32 = 0.5;
/// Probabilité qu'une tech se transmette d'un agent à un voisin par rencontre
/// (passe toutes les 4 h). `p < 1` (BRIEF §5.4) : la transmission est
/// incertaine, un savoir ne se copie pas instantanément. Sur plusieurs
/// rencontres par jour, une tech gagne le clan en quelques semaines — mais
/// peut se perdre plus vite qu'elle ne se répand si ses porteurs meurent.
/// Calibrable.
const DIFFUSION_RATE: f32 = 0.05;
/// Probabilité qu'une matière troquable change de mains par rencontre. Plus
/// haute que la diffusion d'un savoir : un objet concret se montre et se cède
/// plus sûrement qu'une idée ne se transmet. Calibrable.
const TRADE_RATE: f32 = 0.1;
// Le « temps de cerveau » : besoins couverts (mêmes seuils que la garde « au
// confort » de `brain::decide` pour Explore/Build).
const INSIGHT_MAX_HUNGER: f32 = 0.6;
const INSIGHT_MAX_THIRST: f32 = 0.6;
const INSIGHT_MAX_COLD: f32 = 0.4;
const INSIGHT_MAX_FATIGUE: f32 = 0.7;
/// Rayon d'« accès » à une ressource environnementale (~2 km). Le BRIEF parle
/// d'ACCÈS (« a accès à une rivière »), pas d'être planté dessus : on l'entend
/// donc « à portée ». Pour l'eau douce, une source **connue** (mémoire) proche
/// suffit — sans quoi l'insight ne pourrait tomber que le rare tick où l'agent
/// se tient exactement sur une source, et la poterie serait quasi inatteignable.
const ENV_ACCESS_TILES: f64 = km_to_tiles(2.0);

/// L'agent a-t-il **accès** à la ressource `cond` ici et maintenant ? Eau douce
/// = sur place, ou une source connue à moins de `ENV_ACCESS_TILES` ; forêt = la
/// tuile courante est boisée.
fn env_access(cond: EnvCond, tile: &Tile, mem: &Memory, pos: (f64, f64)) -> bool {
    match cond {
        EnvCond::FreshWater => {
            tile.has_fresh_water()
                || mem.nearest_known_spring(pos).is_some_and(|s| {
                    let dx = s.0 as f64 + 0.5 - pos.0;
                    let dy = s.1 as f64 + 0.5 - pos.1;
                    dx * dx + dy * dy <= ENV_ACCESS_TILES * ENV_ACCESS_TILES
                })
        }
        EnvCond::Forest => cond.met(tile),
    }
}

/// Ce qu'un individu sait faire : le sous-ensemble des techs qu'il maîtrise
/// **personnellement** (BRIEF §3.1). Composant à allocation (comme `Memory`),
/// pas `Copy`. Un nouveau-né et un fondateur partent vides : tout se découvre
/// ou s'apprend — c'est ce qui rend l'oubli d'un savoir possible. Le corpus
/// d'un clan sera l'**union** de ces ensembles (incrément 4).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Knowledge {
    techs: BTreeSet<TechId>,
}

impl Knowledge {
    pub fn has(&self, id: TechId) -> bool {
        self.techs.contains(&id)
    }
    /// Acquiert une tech ; renvoie `true` si elle était nouvelle.
    pub fn insert(&mut self, id: TechId) -> bool {
        self.techs.insert(id)
    }
    pub fn len(&self) -> usize {
        self.techs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.techs.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = TechId> + '_ {
        self.techs.iter().copied()
    }
}

/// Ce qui peut arriver à une technologie dans l'Histoire du monde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TechEventKind {
    /// Découverte par un individu (l'insight).
    Discovered,
    /// Disparue du monde : son dernier porteur s'est éteint sans l'avoir
    /// transmise (« l'humanité peut régresser », BRIEF §5.4).
    Forgotten,
}

/// Un événement technologique : découverte ou oubli. L'embryon de la Chronique
/// (BRIEF §6.4 ; la vraie viendra en Phase 6) et l'observable des tests.
#[derive(Debug, Clone, Copy)]
pub struct TechEvent {
    pub tick: u64,
    pub kind: TechEventKind,
    pub tech: TechId,
    /// Le découvreur et son clan — pour une découverte. `None` pour un oubli :
    /// le dernier savoir a disparu, il n'y a plus personne à nommer.
    pub agent: Option<AgentId>,
    pub clan: Option<ClanId>,
}

/// La passe d'insight, appelée une fois par jour (voir le bloc de commentaires
/// ci-dessus). Déterministe : le tirage dérive de (seed, tick, id agent).
/// **La révélation** (BRIEF §6.2) : souffler un insight à un agent précis.
///
/// Renvoie la technologie comprise, ou `None` si rien ne l'était — et c'est là
/// tout l'intérêt du mécanisme.
///
/// ## Ce que la révélation ne fait pas
///
/// Elle **n'offre pas un savoir**, elle fait comprendre ce qu'on avait déjà sous
/// les yeux. Tous les prérequis restent exigés : les savoirs préalables, les
/// matières **vues** (`Exposures`), l'accès environnemental. Un homme qui n'a
/// jamais croisé d'argile n'inventera pas la poterie, fût-il inspiré par un
/// dieu — et aucune inspiration ne saute un maillon de la chaîne.
///
/// Ce qu'elle lève, ce sont les deux conditions *humaines* de l'insight
/// ordinaire : le confort (« du temps pour penser ») et la pression (« une
/// raison de penser »). On peut donc révéler à un affamé au milieu d'un hiver,
/// ce que le hasard n'aurait jamais permis. C'est puissant, et strictement borné
/// par ce que cet homme-là a vécu.
///
/// La limite est aussi le revers : le savoir naît chez **un seul** porteur. S'il
/// est vieux, isolé, ou meurt avant d'avoir parlé, la révélation se perd avec
/// lui (`forget`). Choisir à qui l'on parle est tout le geste.
/// Ce qu'il faut avoir relevé sur l'inspiré **avant** de lâcher l'emprunt de
/// l'ECS pour écrire dans la Chronique : la tech comprise, et les attributs
/// que le journal fige au moment des faits (§6.4).
type Revealed = (TechId, (i64, i64), Sex, Option<ClanId>);

pub(crate) fn reveal(sim: &mut Sim, agent: AgentId) -> Option<TechId> {
    if sim.tech_tree.is_empty() {
        return None;
    }
    let tick = sim.time.tick;
    // Relevé d'abord : l'écriture dans la Chronique demande `&mut sim` entier,
    // or la requête emprunte l'ECS (même patron que partout ailleurs).
    let mut found: Option<Revealed> = None;

    for (_, (id, pos, demo, membership, exposures, mem, knowledge)) in sim
        .agents
        .query_mut::<(
            &AgentId,
            &Position,
            &Demographics,
            &ClanMembership,
            &Exposures,
            &Memory,
            &mut Knowledge,
        )>()
    {
        if *id != agent {
            continue;
        }
        let (tx, ty) = pos.tile();
        let tile = sim.world.tile(tx, ty);
        for tech in sim.tech_tree.iter() {
            // Exactement les portails de l'insight ordinaire — ni plus, ni
            // moins. Seuls le confort et la pression sont levés.
            if knowledge.has(tech.id)
                || !tech.prereq_techs.iter().all(|&t| knowledge.has(t))
                || !tech.prereq_exposure.iter().all(|&e| exposures.has(e))
                || (!tech.prereq_exposure_any.is_empty()
                    && !tech.prereq_exposure_any.iter().any(|&e| exposures.has(e)))
                || !tech.prereq_environment.iter().all(|c| env_access(*c, &tile, mem, (pos.x, pos.y)))
            {
                continue;
            }
            knowledge.insert(tech.id);
            found = Some((tech.id, (tx, ty), demo.sex, membership.0));
            break; // une compréhension à la fois : le reste viendra
        }
        break;
    }

    let (tech, pos, sex, clan) = found?;
    // Une révélation **est** une découverte : elle entre au journal des techs
    // comme n'importe quelle autre, et la Chronique la raconte de la même façon.
    // Le geste divin, lui, est consigné à part par `divine` — deux faits, parce
    // que ce sont deux choses : quelqu'un a compris, et le ciel s'en est mêlé.
    sim.tech_events.push(TechEvent {
        tick,
        kind: TechEventKind::Discovered,
        tech,
        agent: Some(agent),
        clan,
    });
    sim.record(pos, EventKind::TechDiscovered { tech, agent, sex, clan });
    Some(tech)
}

pub(crate) fn insight(sim: &mut Sim) {
    if sim.tech_tree.is_empty() {
        return;
    }
    let seed = sim.world.seed();
    let tick = sim.time.tick;
    let mut discoveries: Vec<TechEvent> = Vec::new();
    // Les mêmes découvertes, rédigeables : la Chronique veut le **sexe** du
    // découvreur (son nom en dépend) et le **lieu** de l'insight, deux choses
    // qu'on ne pourra plus retrouver le jour où on lira le journal.
    let mut told: Vec<((i64, i64), EventKind)> = Vec::new();

    for (_, (id, pos, phys, traits, demo, membership, exposures, skills, mem, knowledge)) in sim
        .agents
        .query_mut::<(
            &AgentId,
            &Position,
            &Physiology,
            &Traits,
            &Demographics,
            &ClanMembership,
            &Exposures,
            &Skills,
            &Memory,
            &mut Knowledge,
        )>()
    {
        if !demo.is_adult(tick) {
            continue;
        }
        // Face « temps de penser » : besoins couverts.
        if phys.hunger >= INSIGHT_MAX_HUNGER
            || phys.thirst >= INSIGHT_MAX_THIRST
            || phys.cold >= INSIGHT_MAX_COLD
            || phys.fatigue >= INSIGHT_MAX_FATIGUE
        {
            continue;
        }
        // Face « raison de penser » : la pression, agrégée à son clan.
        let Some(clan_id) = membership.0 else { continue };
        let Some(pressure) = sim.clan_pressure.get(&clan_id) else { continue };

        // La tuile courante, pour les prérequis d'environnement. Déjà résidente
        // (la physiologie l'a lue ce tick) — pas de matérialisation en plus.
        let (tx, ty) = pos.tile();
        let tile = sim.world.tile(tx, ty);
        let mut rng = Pcg32::new(seed.derive(salt::INSIGHT) ^ splitmix64(tick), id.0);

        for tech in sim.tech_tree.iter() {
            // Portail : déjà su, ou un prérequis manque (savoir, exposition,
            // accès) → cette tech reste fermée, on ne tire même pas.
            if knowledge.has(tech.id)
                || !tech.prereq_techs.iter().all(|&t| knowledge.has(t))
                || !tech.prereq_exposure.iter().all(|&e| exposures.has(e))
                || (!tech.prereq_exposure_any.is_empty()
                    && !tech.prereq_exposure_any.iter().any(|&e| exposures.has(e)))
                || !tech.prereq_environment.iter().all(|c| env_access(*c, &tile, mem, (pos.x, pos.y)))
            {
                continue;
            }
            // Pression pertinente : la plus forte des pressions listées. Nulle
            // → pas d'insight (le facteur essentiel du §5.2).
            let pf = tech.pressure.iter().map(|p| p.of(pressure)).fold(0.0_f32, f32::max);
            if pf <= 0.0 {
                continue;
            }
            let competence = tech.skill.map(|s| s.of(skills)).unwrap_or(NEUTRAL_COMPETENCE);
            let p = BASE_INSIGHT_PER_DAY * traits.curiosity * competence * pf;
            if rng.next_f64() < f64::from(p) {
                knowledge.insert(tech.id);
                discoveries.push(TechEvent {
                    tick,
                    kind: TechEventKind::Discovered,
                    tech: tech.id,
                    agent: Some(*id),
                    clan: Some(clan_id),
                });
                told.push((
                    (tx, ty),
                    EventKind::TechDiscovered {
                        tech: tech.id,
                        agent: *id,
                        sex: demo.sex,
                        clan: Some(clan_id),
                    },
                ));
                break; // une découverte par jour suffit — on redécouvrira demain
            }
        }
    }
    sim.tech_events.extend(discoveries);
    for (pos, kind) in told {
        sim.record(pos, kind);
    }
}

/// L'échange par contact (BRIEF §5.4, incrément 4 ; §5.3 commerce, incrément
/// 6) : ce qui passe d'un agent à un voisin à portée de conversation, quel que
/// soit son clan (c'est là toute la diffusion **inter-clans**). Deux choses
/// circulent, à la même cadence que `social::encounter` (toutes les 4 h) :
///
/// - **Les savoir-faire** (`Knowledge`) : une tech s'apprend d'un voisin qui la
///   maîtrise, à condition de tenir déjà ses prérequis de savoir (on n'apprend
///   pas la poterie sans connaître le feu), avec `p < 1` — la transmission
///   d'une idée est incertaine.
/// - **Les matières troquables** (`Exposure::TRADEABLE`) : le troc fait *voir*
///   à l'un ce que l'autre porte (un lingot, un minerai). C'est le maillon
///   court du commerce : combiné aux expéditions (incrément 6b), il fait
///   voyager l'étain de clan en clan jusqu'à un clan du cuivre, seule façon
///   d'y déclencher le bronze.
///
/// Deux passes, comme `memory::exchange_knowledge` : on lit tout l'état, on
/// calcule ce que chacun gagne, puis on l'applique — le résultat ne dépend donc
/// pas de l'ordre de traitement, et on n'emprunte jamais deux composants à la
/// fois. Un flux RNG par récepteur (stream = son id) → déterministe.
pub(crate) fn diffuse(sim: &mut Sim) {
    struct View {
        entity: hecs::Entity,
        id: AgentId,
        pos: (f64, f64),
        knows: BTreeSet<TechId>,
        seen: Exposures,
    }
    let mut views: Vec<View> = sim
        .agents
        .query::<(&AgentId, &Position, &Knowledge, &Exposures)>()
        .iter()
        .map(|(entity, (id, pos, k, e))| View {
            entity,
            id: *id,
            pos: (pos.x, pos.y),
            knows: k.techs.clone(),
            seen: *e,
        })
        .collect();
    views.sort_unstable_by_key(|v| v.id.0);

    let seed = sim.world.seed();
    let tick = sim.time.tick;
    let mut gained_techs: Vec<Vec<TechId>> = vec![Vec::new(); views.len()];
    let mut gained_exposures: Vec<Vec<Exposure>> = vec![Vec::new(); views.len()];

    for i in 0..views.len() {
        // Union de ce que les voisins à portée savent faire et portent sur eux.
        let mut techs_here: BTreeSet<TechId> = BTreeSet::new();
        let mut goods_here = Exposures::default();
        let mut has_neighbor = false;
        for j in 0..views.len() {
            if i == j {
                continue;
            }
            let dx = views[i].pos.0 - views[j].pos.0;
            let dy = views[i].pos.1 - views[j].pos.1;
            if dx * dx + dy * dy <= TALK_RADIUS_TILES * TALK_RADIUS_TILES {
                has_neighbor = true;
                techs_here.extend(&views[j].knows);
                goods_here.0 |= views[j].seen.0;
            }
        }
        if !has_neighbor {
            continue;
        }
        let mut rng = Pcg32::new(seed.derive(salt::DIFFUSION) ^ splitmix64(tick), views[i].id.0);
        // Savoir-faire (ordre trié du BTreeSet → suite de tirages reproductible).
        for &t in &techs_here {
            if views[i].knows.contains(&t)
                || !sim.tech_tree.get(t).prereq_techs.iter().all(|p| views[i].knows.contains(p))
            {
                continue;
            }
            if rng.next_f64() < f64::from(DIFFUSION_RATE) {
                gained_techs[i].push(t);
            }
        }
        // Matières troquables (ordre fixe de `TRADEABLE`).
        for &e in &Exposure::TRADEABLE {
            if views[i].seen.has(e) || !goods_here.has(e) {
                continue;
            }
            if rng.next_f64() < f64::from(TRADE_RATE) {
                gained_exposures[i].push(e);
            }
        }
    }

    for i in 0..views.len() {
        if gained_techs[i].is_empty() && gained_exposures[i].is_empty() {
            continue;
        }
        if let Ok((knowledge, exposures)) =
            sim.agents.query_one_mut::<(&mut Knowledge, &mut Exposures)>(views[i].entity)
        {
            for t in &gained_techs[i] {
                knowledge.insert(*t);
            }
            for &e in &gained_exposures[i] {
                exposures.expose(e);
            }
        }
    }
}

/// L'oubli (BRIEF §5.4, incrément 4) : « une tech portée par trop peu
/// d'individus peut disparaître avec eux ». Rien à défaire — une tech ne vit
/// que dans le `Knowledge` d'agents vivants ; quand son dernier porteur meurt
/// sans l'avoir transmise, elle s'évapore d'elle-même. Cette passe ne fait que
/// le **constater** pour le rendre visible : elle compare l'ensemble des techs
/// encore portées aujourd'hui à celui d'hier (`Sim::known_techs`) et journalise
/// un `Forgotten` pour chacune qui a disparu — le « un clan qui s'effondre perd
/// des technologies » du brief, quand ce clan en était le seul dépositaire.
/// Appelée une fois par jour, après les morts du jour et l'insight.
pub(crate) fn forget(sim: &mut Sim) {
    let tick = sim.time.tick;
    let mut current: BTreeSet<TechId> = BTreeSet::new();
    for (_, k) in sim.agents.query::<&Knowledge>().iter() {
        current.extend(k.iter());
    }
    // On relève d'abord les disparues (l'emprunt de `sim.known_techs` doit être
    // rendu avant d'écrire dans `sim`), puis on les journalise.
    let lost: Vec<TechId> =
        sim.known_techs.iter().copied().filter(|t| !current.contains(t)).collect();
    for t in lost {
        sim.tech_events.push(TechEvent {
            tick,
            kind: TechEventKind::Forgotten,
            tech: t,
            agent: None,
            clan: None,
        });
        // Un savoir perdu fait date au même titre qu'un savoir trouvé — c'est
        // ce que le critère §9 appelle « visible dans la Chronique ». Sans
        // lieu : il ne disparaît pas quelque part, il disparaît partout.
        sim.record((0, 0), EventKind::TechForgotten { tech: t });
    }
    sim.known_techs = current;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::Clan;
    use cairn_core::{TICKS_PER_DAY, WorldSeed};

    /// Prépare une scène minimale : un fondateur exposé au silex + bois, au
    /// confort, seul membre d'un clan dont on fixe la pression. Renvoie la sim,
    /// l'agent, et l'id de `fire_mastery`. `expose` permet de couper
    /// l'exposition pour la contre-épreuve.
    fn scene(cold: f32, exposures: &[Exposure]) -> (Sim, AgentId, TechId) {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let a = sim.spawn_agent(0.0, 0.0);
        let fire = sim.tech_tree.id_of("fire_mastery").unwrap();
        for (_, (traits, skills, exp, phys, m)) in sim.agents.query_mut::<(
            &mut Traits,
            &mut Skills,
            &mut Exposures,
            &mut Physiology,
            &mut ClanMembership,
        )>() {
            traits.curiosity = 1.0;
            skills.foraging = 1.0;
            for &e in exposures {
                exp.expose(e);
            }
            *phys = Physiology {
                hunger: 0.1,
                thirst: 0.1,
                fatigue: 0.1,
                cold: 0.0,
                health: 1.0,
                last_damage: None,
            };
            m.0 = Some(ClanId(1));
        }
        sim.clans.push(Clan {
            id: ClanId(1),
            founded_tick: 0,
            members: [a].into_iter().collect(),
            home: (0.0, 0.0),
            stock: 0.0,
            chief: a,
            desired: None,
            rivalry: 0.0,
        });
        sim.clan_pressure.insert(ClanId(1), ClanPressure { cold, ..Default::default() });
        (sim, a, fire)
    }

    /// Fait passer `days` journées d'insight (en avançant l'horloge, sans quoi
    /// le tirage déterministe serait identique chaque jour). Renvoie si le feu
    /// a été découvert.
    fn run_days(sim: &mut Sim, fire: TechId, days: u64) -> bool {
        for _ in 0..days {
            sim.time.tick += TICKS_PER_DAY;
            insight(sim);
            if sim.agents.query::<&Knowledge>().iter().any(|(_, k)| k.has(fire)) {
                return true;
            }
        }
        false
    }

    #[test]
    fn le_feu_se_decouvre_sous_froid_exposition_et_oisivete() {
        // Voie de la friction : bois + silex.
        let (mut sim, _, fire) = scene(1.0, &[Exposure::Flint, Exposure::Wood]);
        assert!(run_days(&mut sim, fire, 8000), "le feu doit finir par être découvert");
        assert!(
            sim.tech_events.iter().any(|e| e.tech == fire),
            "la découverte doit être journalisée"
        );
    }

    #[test]
    fn le_feu_se_decouvre_aussi_en_voyant_un_incendie() {
        // Seconde voie (« les deux ») : bois + feu observé, sans le moindre
        // silex. Un clan qui n'a jamais croisé de silex peut quand même
        // maîtriser le feu après avoir vu un incendie.
        let (mut sim, _, fire) = scene(1.0, &[Exposure::Wood, Exposure::Fire]);
        assert!(run_days(&mut sim, fire, 8000), "voir un incendie ouvre aussi la voie du feu");
    }

    #[test]
    fn le_bois_seul_ne_suffit_pas() {
        // Ni silex ni feu vu : l'alternative « au moins un parmi » n'est pas
        // remplie → le feu reste hors de portée malgré le bois et le froid.
        let (mut sim, _, fire) = scene(1.0, &[Exposure::Wood]);
        assert!(!run_days(&mut sim, fire, 8000), "le bois seul n'allume rien");
    }

    #[test]
    fn sans_exposition_pas_de_feu() {
        // Mêmes conditions favorables, mais l'agent n'a rien vu du tout.
        let (mut sim, _, fire) = scene(1.0, &[]);
        assert!(!run_days(&mut sim, fire, 8000), "sans exposition, le feu reste hors de portée");
        assert!(sim.tech_events.is_empty(), "aucune découverte journalisée");
    }

    #[test]
    fn un_clan_repu_n_invente_rien() {
        // Exposé et au confort, mais aucune pression (froid nul) : la face
        // « raison de penser » manque → pas d'insight.
        let (mut sim, _, fire) = scene(0.0, &[Exposure::Flint, Exposure::Wood]);
        assert!(!run_days(&mut sim, fire, 8000), "sans pression, un clan repu n'invente rien");
    }

    /// L'entité `hecs` d'un agent, par son identifiant stable (pour les tests
    /// d'oubli, qui despawnent des agents précis).
    fn entity_of(sim: &Sim, id: AgentId) -> hecs::Entity {
        sim.agents
            .query::<&AgentId>()
            .iter()
            .find(|(_, a)| **a == id)
            .map(|(e, _)| e)
            .expect("agent présent")
    }

    /// Un savoir connu par un porte-savoir se transmet à un voisin à portée.
    #[test]
    fn une_tech_se_transmet_a_un_voisin() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let fire = sim.tech_tree.id_of("fire_mastery").unwrap();
        let teacher = sim.spawn_agent(0.0, 0.0);
        sim.spawn_agent(10.0, 0.0); // ~20 m : à portée de conversation
        for (_, (id, k)) in sim.agents.query_mut::<(&AgentId, &mut Knowledge)>() {
            if *id == teacher {
                k.insert(fire);
            }
        }
        let mut learned = false;
        for _ in 0..2000 {
            sim.time.tick += 4;
            diffuse(&mut sim);
            learned = sim
                .agents
                .query::<(&AgentId, &Knowledge)>()
                .iter()
                .any(|(_, (id, k))| *id != teacher && k.has(fire));
            if learned {
                break;
            }
        }
        assert!(learned, "le feu doit se transmettre au voisin à portée");
    }

    /// Hors de portée, aucune transmission (la diffusion est spatiale).
    #[test]
    fn hors_de_portee_rien_ne_se_transmet() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let fire = sim.tech_tree.id_of("fire_mastery").unwrap();
        let teacher = sim.spawn_agent(0.0, 0.0);
        sim.spawn_agent(10_000.0, 0.0); // 20 km : bien au-delà de la portée
        for (_, (id, k)) in sim.agents.query_mut::<(&AgentId, &mut Knowledge)>() {
            if *id == teacher {
                k.insert(fire);
            }
        }
        for _ in 0..2000 {
            sim.time.tick += 4;
            diffuse(&mut sim);
        }
        let spread = sim
            .agents
            .query::<(&AgentId, &Knowledge)>()
            .iter()
            .any(|(_, (id, k))| *id != teacher && k.has(fire));
        assert!(!spread, "hors de portée, aucune transmission");
    }

    /// On n'apprend pas une tech dont on ne tient pas les prérequis : ici le
    /// maître ne connaît QUE la poterie (état forcé, sans le feu), personne
    /// n'enseigne le feu → l'apprenti ne peut jamais apprendre la poterie.
    #[test]
    fn on_n_apprend_pas_sans_les_prerequis() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let pottery = sim.tech_tree.id_of("pottery").unwrap();
        let teacher = sim.spawn_agent(0.0, 0.0);
        sim.spawn_agent(10.0, 0.0);
        for (_, (id, k)) in sim.agents.query_mut::<(&AgentId, &mut Knowledge)>() {
            if *id == teacher {
                k.insert(pottery);
            }
        }
        for _ in 0..2000 {
            sim.time.tick += 4;
            diffuse(&mut sim);
        }
        let learned = sim
            .agents
            .query::<(&AgentId, &Knowledge)>()
            .iter()
            .any(|(_, (id, k))| *id != teacher && k.has(pottery));
        assert!(!learned, "sans le feu (prérequis), la poterie ne s'apprend pas");
    }

    /// L'oubli : une tech disparaît du monde quand son **dernier** porteur
    /// s'éteint — pas avant. Journalisé (visible dans la Chronique).
    #[test]
    fn une_tech_s_oublie_quand_son_dernier_porteur_meurt() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let fire = sim.tech_tree.id_of("fire_mastery").unwrap();
        let a = sim.spawn_agent(0.0, 0.0);
        let b = sim.spawn_agent(1000.0, 0.0);
        for (_, k) in sim.agents.query_mut::<&mut Knowledge>() {
            k.insert(fire); // les deux connaissent le feu
        }
        forget(&mut sim); // établit known_techs = {fire}, aucun oubli
        assert!(sim.known_techs.contains(&fire));
        assert!(sim.tech_events.is_empty());

        // Un porteur meurt : il en reste un → pas d'oubli.
        let _ = sim.agents.despawn(entity_of(&sim, a));
        forget(&mut sim);
        assert!(sim.known_techs.contains(&fire), "un porteur survit : pas d'oubli");
        assert!(sim.tech_events.is_empty());

        // Le dernier porteur meurt : le feu disparaît, journalisé.
        let _ = sim.agents.despawn(entity_of(&sim, b));
        forget(&mut sim);
        assert!(!sim.known_techs.contains(&fire), "plus aucun porteur : le feu est oublié");
        assert_eq!(sim.tech_events.len(), 1);
        assert_eq!(sim.tech_events[0].kind, TechEventKind::Forgotten);
        assert_eq!(sim.tech_events[0].tech, fire);
    }

    /// Le troc (incrément 6) : une matière portable passe à un voisin, mais pas
    /// une matière qui ne se colporte pas (le bois).
    #[test]
    fn une_matiere_se_troque_mais_pas_une_foret() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let trader = sim.spawn_agent(0.0, 0.0);
        sim.spawn_agent(10.0, 0.0); // à portée de conversation
        for (_, (id, exp)) in sim.agents.query_mut::<(&AgentId, &mut Exposures)>() {
            if *id == trader {
                exp.expose(Exposure::Copper); // troquable
                exp.expose(Exposure::Wood); // non troquable
            }
        }
        let mut got_copper = false;
        for _ in 0..2000 {
            sim.time.tick += 4;
            diffuse(&mut sim);
            got_copper = sim
                .agents
                .query::<(&AgentId, &Exposures)>()
                .iter()
                .any(|(_, (id, e))| *id != trader && e.has(Exposure::Copper));
            if got_copper {
                break;
            }
        }
        assert!(got_copper, "le cuivre doit se troquer au voisin");
        let got_wood = sim
            .agents
            .query::<(&AgentId, &Exposures)>()
            .iter()
            .any(|(_, (id, e))| *id != trader && e.has(Exposure::Wood));
        assert!(!got_wood, "le bois ne se troque pas (on ne colporte pas une forêt)");
    }

    /// La chaîne du bronze est bien en place, avec son verrou : le bronze exige
    /// la métallurgie du cuivre **et** l'exposition à l'étain.
    #[test]
    fn la_chaine_du_bronze_est_en_place() {
        let tree = TechTree::embedded();
        let copper = tree.id_of("copper_metallurgy").expect("métallurgie du cuivre");
        let bronze = tree.id_of("bronze").expect("bronze");
        assert_eq!(tree.get(bronze).prereq_techs, vec![copper]);
        assert!(
            tree.get(bronze).prereq_exposure.contains(&Exposure::Tin),
            "le bronze est verrouillé par l'étain — le nœud de la route commerciale"
        );
        assert!(tree.get(copper).prereq_exposure.contains(&Exposure::Copper));
    }

    /// L'Âge (incrément 7) : une étiquette dérivée du corpus, le plus avancé des
    /// marqueurs, `Paleolithic` par défaut.
    #[test]
    fn l_age_derive_du_corpus() {
        let tree = TechTree::embedded();
        let id = |name| tree.id_of(name).unwrap();
        assert_eq!(age_of(&BTreeSet::new(), &tree), Age::Paleolithic);
        let paleo: BTreeSet<TechId> = [id("fire_mastery")].into_iter().collect();
        assert_eq!(age_of(&paleo, &tree), Age::Paleolithic);
        let neo: BTreeSet<TechId> = [id("fire_mastery"), id("pottery")].into_iter().collect();
        assert_eq!(age_of(&neo, &tree), Age::Neolithic);
        // Le bronze l'emporte, même en gardant les techs antérieures (le max).
        let bronze: BTreeSet<TechId> =
            [id("fire_mastery"), id("pottery"), id("bronze")].into_iter().collect();
        assert_eq!(age_of(&bronze, &tree), Age::BronzeAge);
    }

    #[test]
    fn l_arbre_embarque_se_charge_et_resout_ses_prerequis() {
        let tree = TechTree::embedded();
        assert!(tree.len() >= 3, "l'amorce a au moins feu/cuisson/poterie");

        let fire = tree.id_of("fire_mastery").expect("le feu doit exister");
        assert!(tree.get(fire).prereq_techs.is_empty(), "le feu n'a pas de savoir préalable");
        // Recette à deux voies : bois obligatoire, puis silex OU feu vu.
        assert!(tree.get(fire).prereq_exposure.contains(&Exposure::Wood));
        assert!(tree.get(fire).prereq_exposure_any.contains(&Exposure::Flint));
        assert!(tree.get(fire).prereq_exposure_any.contains(&Exposure::Fire));
        assert!(tree.get(fire).pressure.contains(&TechPressure::Cold));

        // La cuisson requiert le feu : le nom a bien été résolu en son TechId.
        let cooking = tree.id_of("cooking").expect("la cuisson doit exister");
        assert_eq!(tree.get(cooking).prereq_techs, vec![fire]);

        // La poterie porte ses trois sortes de prérequis.
        let pottery = tree.id_of("pottery").expect("la poterie doit exister");
        let p = tree.get(pottery);
        assert_eq!(p.prereq_techs, vec![fire]);
        assert!(p.prereq_exposure.contains(&Exposure::Clay));
        assert!(p.prereq_environment.contains(&EnvCond::FreshWater));
    }

    #[test]
    fn un_prerequis_inconnu_est_rejete() {
        let src = r#"[
            ( id: "b", label: "B", prereq_techs: ["a"] ),
        ]"#;
        let err = TechTree::from_ron(src).unwrap_err();
        assert!(err.contains("inconnue"), "message inattendu : {err}");
    }

    #[test]
    fn une_tech_en_double_est_rejetee() {
        let src = r#"[
            ( id: "a", label: "A" ),
            ( id: "a", label: "A bis" ),
        ]"#;
        let err = TechTree::from_ron(src).unwrap_err();
        assert!(err.contains("deux fois"), "message inattendu : {err}");
    }

    #[test]
    fn les_mappages_lisent_le_bon_facteur() {
        let p = ClanPressure { famine: 0.1, cold: 0.7, threat: 0.3, crowding: 0.2 };
        assert_eq!(TechPressure::Cold.of(&p), 0.7);
        assert_eq!(TechPressure::Threat.of(&p), 0.3);

        let s = Skills { foraging: 0.4, hunting: 0.6, oratory: 0.2, combat: 0.0 };
        assert_eq!(TechSkill::Foraging.of(&s), 0.4);
        assert_eq!(TechSkill::Hunting.of(&s), 0.6);
    }
}
