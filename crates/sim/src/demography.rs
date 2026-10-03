//! La démographie : traits hérités, reproduction, enfance, sénescence
//! (BRIEF §9, Phase 3 « LE NOMBRE », incrément 1).
//!
//! Le principe : passer d'une population qui *survit* à une population qui
//! *persiste*. Rien n'est scripté — le taux de croissance **émerge** de
//! quelques règles locales :
//!
//! - **Conception** : une femme féconde conçoit avec une petite probabilité
//!   quotidienne quand un homme est à portée. Les gardes de condition (santé,
//!   faim) font que la fertilité chute d'elle-même en période de disette —
//!   c'est le frein malthusien, personne n'écrit « limiter la population ».
//! - **Aménorrhée de lactation** : la mère d'un nourrisson vivant ne conçoit
//!   pas. L'espacement des naissances (~4 ans) en découle ; la mort d'un
//!   nourrisson le raccourcit — émergent et réaliste.
//! - **Hérédité** : chaque trait de l'enfant = moyenne parentale + mutation
//!   gaussienne (BRIEF §3.1). La sélection fait le reste : si les curieux
//!   meurent jeunes, la curiosité recule.
//! - **Enfance** : un nourrisson est **porté et allaité** (il coûte à sa
//!   mère, meurt sans elle) ; un enfant suit ses parents et fourrage mal.
//!   « Les enfants sont improductifs et coûteux — c'est ce qui rend le
//!   surplus nécessaire » (BRIEF §9).
//! - **Sénescence** : tirage quotidien sur une loi de Gompertz — négligeable
//!   à 30 ans, écrasante à 80. Les autres causes de mort (faim, soif, froid)
//!   restent l'affaire de la physiologie.

use std::collections::{BTreeMap, BTreeSet};

use cairn_core::{DAYS_PER_YEAR, Pcg32, TICKS_PER_DAY, TICKS_PER_YEAR, WorldSeed, km_to_tiles, splitmix64};

use crate::agent::{
    Activity, AgentId, Behavior, DeathCause, HUNGER_PER_TICK, Physiology, Position,
    THIRST_PER_TICK,
};
use crate::salt;
use crate::sim::{BirthRecord, DeathRecord, Sim};

// — Stades de vie —

/// Durée de gestation : 9 mois de 30 jours.
pub const GESTATION_DAYS: u64 = 270;
/// Avant cet âge, l'enfant est un **nourrisson** : porté par sa mère,
/// allaité, aucune autonomie.
pub const NURSING_AGE_YEARS: f64 = 3.0;
/// L'âge adulte : pleine capacité de travail, chasse, reproduction.
pub const ADULT_AGE_YEARS: f64 = 14.0;

// — Fécondité —

/// Fenêtre féconde des femmes.
pub const FEMALE_FERTILE_YEARS: std::ops::RangeInclusive<f64> = 15.0..=45.0;
/// Fenêtre féconde des hommes.
pub const MALE_FERTILE_YEARS: std::ops::RangeInclusive<f64> = 15.0..=60.0;
/// Portée de rencontre pour une conception (~400 m) : il faut se croiser.
pub const MATE_RADIUS_TILES: f64 = km_to_tiles(0.4);
/// Probabilité de concevoir par jour quand toutes les conditions sont là
/// (~30 % par mois : la fécondabilité humaine naturelle).
pub const CONCEPTION_DAILY_P: f64 = 0.012;

// — Coûts de l'enfance —

/// Ce que l'allaitement retire par tick aux besoins du nourrisson (couvre
/// faim et soif : le lait est les deux).
pub const NURSE_RELIEF: f32 = 0.1;
/// Au-delà de cette faim, la mère n'a plus de lait à donner.
pub const STARVING_MOTHER_HUNGER: f32 = 0.95;
/// Surcoût de faim de la mère qui allaite (+30 % du métabolisme de base).
pub const NURSING_HUNGER_PER_TICK: f32 = HUNGER_PER_TICK * 0.3;
/// Le lait coûte aussi de l'eau.
pub const NURSING_THIRST_PER_TICK: f32 = THIRST_PER_TICK * 0.3;
/// Surcoût de faim d'une grossesse (+20 %).
pub const PREGNANCY_HUNGER_PER_TICK: f32 = HUNGER_PER_TICK * 0.2;

/// Écart-type de la mutation gaussienne à l'hérédité d'un trait.
pub const MUTATION_SIGMA: f64 = 0.06;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Sex {
    Female,
    Male,
}

/// Les six traits hérités (BRIEF §3.1), chacun dans [0, 1]. Ils ne font
/// rien « en bloc » : chaque système en lit un — l'endurance ralentit la
/// fatigue, la dextérité améliore la cueillette, la curiosité allonge les
/// jambes d'errance, la sociabilité rapproche des autres. Force et
/// agressivité attendent leurs systèmes (combat, Phase 4+).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Traits {
    pub strength: f32,
    pub endurance: f32,
    pub dexterity: f32,
    pub curiosity: f32,
    pub sociability: f32,
    pub aggression: f32,
}

impl Default for Traits {
    fn default() -> Self {
        Self {
            strength: 0.5,
            endurance: 0.5,
            dexterity: 0.5,
            curiosity: 0.5,
            sociability: 0.5,
            aggression: 0.5,
        }
    }
}

impl Traits {
    fn map2(a: &Self, b: &Self, mut f: impl FnMut(f32, f32) -> f32) -> Self {
        Self {
            strength: f(a.strength, b.strength),
            endurance: f(a.endurance, b.endurance),
            dexterity: f(a.dexterity, b.dexterity),
            curiosity: f(a.curiosity, b.curiosity),
            sociability: f(a.sociability, b.sociability),
            aggression: f(a.aggression, b.aggression),
        }
    }

    /// Traits d'un fondateur : centrés, dispersion naturelle, sans extrêmes.
    pub fn sample(rng: &mut Pcg32) -> Self {
        let mut draw = |_: f32, _: f32| {
            (0.5 + 0.15 * rng.next_normal() as f32).clamp(0.05, 0.95)
        };
        Self::map2(&Self::default(), &Self::default(), &mut draw)
    }

    /// Hérédité : moyenne parentale + mutation gaussienne, trait par trait.
    pub fn inherit(mother: &Self, father: &Self, rng: &mut Pcg32) -> Self {
        Self::map2(mother, father, |m, f| {
            ((m + f) / 2.0 + (MUTATION_SIGMA * rng.next_normal()) as f32).clamp(0.0, 1.0)
        })
    }
}

/// Une grossesse en cours. Les traits du père sont **copiés** à la
/// conception : l'hérédité ne dépend pas de sa survie jusqu'au terme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pregnancy {
    pub due_tick: u64,
    pub father: AgentId,
    pub father_traits: Traits,
}

/// L'état démographique d'un agent : sexe, date de naissance, grossesse.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Demographics {
    pub sex: Sex,
    /// Tick de naissance — **négatif** pour la population initiale, née
    /// avant le début du monde.
    pub born_tick: i64,
    pub pregnancy: Option<Pregnancy>,
}

impl Demographics {
    pub fn age_years(&self, tick: u64) -> f64 {
        (tick as i64 - self.born_tick) as f64 / TICKS_PER_YEAR as f64
    }

    pub fn is_infant(&self, tick: u64) -> bool {
        self.age_years(tick) < NURSING_AGE_YEARS
    }

    pub fn is_adult(&self, tick: u64) -> bool {
        self.age_years(tick) >= ADULT_AGE_YEARS
    }
}

/// La filiation. `None` : fondateur (ou parent inconnu). C'est l'embryon de
/// l'arbre généalogique du panneau d'inspection (Phase 5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Kinship {
    pub mother: Option<AgentId>,
    pub father: Option<AgentId>,
}

/// Instantané d'un humain au début du tick — l'équivalent de `HerdView` pour
/// la coordination entre agents (suivre un parent, rejoindre les autres,
/// trouver un partenaire). Trié par identifiant dans le vecteur d'instantanés.
#[derive(Debug, Clone, Copy)]
pub struct HumanView {
    pub id: AgentId,
    pub pos: (f64, f64),
    pub sex: Sex,
    pub adult: bool,
    /// Clan d'appartenance : c'est lui qui distingue un compagnon d'un **rival**
    /// (conflit inter-clans, `crate::combat`). `None` = sans clan.
    pub clan: Option<crate::social::ClanId>,
}

/// Recherche binaire dans un instantané trié par identifiant.
pub fn find_human(humans: &[HumanView], id: AgentId) -> Option<&HumanView> {
    humans
        .binary_search_by_key(&id.0, |h| h.id.0)
        .ok()
        .map(|i| &humans[i])
}

/// Mortalité annuelle de sénescence (loi de Gompertz) : ~0,1 % à 30 ans,
/// ~0,6 % à 40, ~5 % à 60, ~16 % à 70, ~50 % à 80. Les morts « jeunes »
/// restent l'affaire de la faim, de la soif et du froid.
pub fn annual_mortality(age_years: f64) -> f64 {
    (0.006 * (0.11 * (age_years - 40.0)).exp()).min(0.9)
}

/// Capacité de travail selon l'âge : nulle au sein, partielle dans
/// l'enfance (un enfant de 8 ans cueille, mal), pleine à l'âge adulte.
/// C'est le « coûteux et improductif » du brief, en une courbe.
pub fn work_capacity(age_years: f64) -> f32 {
    if age_years >= ADULT_AGE_YEARS {
        1.0
    } else if age_years <= NURSING_AGE_YEARS {
        0.0
    } else {
        let t = (age_years - NURSING_AGE_YEARS) / (ADULT_AGE_YEARS - NURSING_AGE_YEARS);
        (0.35 + 0.65 * t) as f32
    }
}

/// Démographie d'un **fondateur** (population initiale) : un adulte de
/// 16–40 ans, sexe et traits tirés de la seed — même seed, même peuplade.
pub(crate) fn founder(seed: WorldSeed, id: u64, tick: u64) -> (Demographics, Traits) {
    let mut rng = Pcg32::new(seed.derive(salt::DEMOGRAPHY), id);
    let sex = if rng.next_u32() & 1 == 0 { Sex::Female } else { Sex::Male };
    let age_years = 16.0 + rng.next_f64() * 24.0;
    let born_tick = tick as i64 - (age_years * TICKS_PER_YEAR as f64) as i64;
    (
        Demographics { sex, born_tick, pregnancy: None },
        Traits::sample(&mut rng),
    )
}

/// Soin des nourrissons, chaque tick : un nourrisson est **porté** (sa
/// position est celle de sa mère) et **allaité** (le lait couvre faim et
/// soif, au prix d'un surcoût pour la mère). Une mère affamée n'a plus de
/// lait ; un orphelin n'a personne — dans les deux cas la dérive
/// physiologique fait son œuvre, sans règle de mort spéciale.
pub(crate) fn nurse_infants(sim: &mut Sim) {
    let tick = sim.time.tick;

    // Index id → entité : la filiation référence des AgentId, l'ECS des
    // entités. BTreeMap : ordre d'itération stable.
    let mut index: BTreeMap<u64, hecs::Entity> = BTreeMap::new();
    for (entity, id) in sim.agents.query::<&AgentId>().iter() {
        index.insert(id.0, entity);
    }

    // La mère, ou, si elle est morte, la femme qui l'a pris (allomaternage).
    let infants: Vec<(hecs::Entity, Option<u64>)> = sim
        .agents
        .query::<(&AgentId, &Demographics, &Kinship)>()
        .iter()
        .filter(|(_, (_, demo, _))| demo.is_infant(tick))
        .map(|(entity, (id, _, kin))| {
            let mother = kin.mother.map(|m| m.0).filter(|m| index.contains_key(m));
            (entity, mother.or_else(|| sim.fosters.get(&id.0).map(|f| f.0)))
        })
        .collect();

    for (infant, mother_id) in infants {
        let mother_entity = mother_id.and_then(|m| index.get(&m)).copied();
        // L'état de la mère, copié : on ne garde aucun emprunt pendant la
        // mutation du nourrisson.
        let mother_state = mother_entity.and_then(|entity| {
            let pos = sim.agents.get::<&Position>(entity).ok()?;
            let phys = sim.agents.get::<&Physiology>(entity).ok()?;
            let behavior = sim.agents.get::<&Behavior>(entity).ok()?;
            Some(((pos.x, pos.y), behavior.activity, phys.hunger))
        });

        let Some(((mx, my), mother_activity, mother_hunger)) = mother_state else {
            continue; // orphelin : cloué sur place, les besoins montent
        };
        let has_milk = mother_hunger < STARVING_MOTHER_HUNGER;
        if let Ok((pos, phys, behavior)) = sim
            .agents
            .query_one_mut::<(&mut Position, &mut Physiology, &mut Behavior)>(infant)
        {
            pos.x = mx;
            pos.y = my;
            // Porté, le nourrisson partage le sommeil et l'abri de sa mère.
            behavior.activity = match mother_activity {
                Activity::Sleeping => Activity::Sleeping,
                Activity::Sheltering => Activity::Sheltering,
                _ => Activity::Idle,
            };
            if has_milk {
                crate::food_stats::fed(crate::food_stats::Source::Milk, NURSE_RELIEF.min(phys.hunger));
                phys.hunger = (phys.hunger - NURSE_RELIEF).max(0.0);
                phys.thirst = (phys.thirst - NURSE_RELIEF).max(0.0);
            }
        }
        if has_milk
            && let Some(entity) = mother_entity
            && let Ok(phys) = sim.agents.query_one_mut::<&mut Physiology>(entity)
        {
            phys.hunger = (phys.hunger + NURSING_HUNGER_PER_TICK).min(1.0);
            phys.thirst = (phys.thirst + NURSING_THIRST_PER_TICK).min(1.0);
        }
    }
}

/// Chance quotidienne qu'une femme de la même bande, sans lien de parenté,
/// prenne un orphelin qu'elle peut allaiter : l'allomaternage passe surtout
/// par la parenté proche (Hewlett et Winn 2014 ; grand-mères, sœurs, tantes),
/// rarement hors d'elle. Ordre de grandeur, choix.
const ADOPT_SAME_BAND_P: f64 = 0.05;
/// La même chance pour une femme d'un autre groupe : rare (aucune source ne
/// la décrit pour un nourrisson), jamais nulle.
const ADOPT_STRANGER_P: f64 = 0.01;

/// La proximité de parenté entre deux humains : le coefficient de parenté
/// (somme sur les ancêtres communs, jusqu'aux grands-parents, de ½ puissance
/// la longueur du chemin) — sœur 0,5, grand-mère et tante 0,25, cousine
/// 0,125. `parents` vient des naissances : il survit à la mort des parents.
pub(crate) fn kin_closeness(parents: &BTreeMap<u64, (u64, u64)>, a: u64, b: u64) -> f64 {
    let ancestors = |x: u64| {
        let mut out: BTreeMap<u64, u32> = BTreeMap::new();
        out.insert(x, 0);
        if let Some(&(m, f)) = parents.get(&x) {
            for p in [m, f] {
                out.entry(p).or_insert(1);
                if let Some(&(gm, gf)) = parents.get(&p) {
                    out.entry(gm).or_insert(2);
                    out.entry(gf).or_insert(2);
                }
            }
        }
        out
    };
    let (aa, bb) = (ancestors(a), ancestors(b));
    aa.iter()
        .filter_map(|(c, da)| bb.get(c).map(|db| 0.5f64.powi((da + db) as i32)))
        .sum()
}

/// L'allomaternage (passe quotidienne) : un nourrisson dont la mère est
/// morte peut être pris par une femme qui allaite déjà, qui n'est pas
/// affamée et vit assez près — **d'autant plus volontiers qu'elle lui est
/// proche** (vision de l'utilisateur, sourcée : Hewlett et Winn 2014 ; Sear et
/// Mace 2008). Rien n'est forcé : un tirage par jour. Le lait lui coûte comme pour le sien
/// (`nurse_infants`) ; si elle meurt ou s'affame, l'enfant est de nouveau
/// sans lait. La mort de la mère reste dangereuse, comme dans la réalité.
/// Chaque jour, seule la candidate la plus proche tire sa chance.
pub(crate) fn adopt_orphans(sim: &mut Sim) {
    let tick = sim.time.tick;
    let day = tick / TICKS_PER_DAY;
    struct V {
        pos: (f64, f64),
        hunger: f32,
        female_adult: bool,
        infant: bool,
        mother: Option<u64>,
        clan: Option<crate::social::ClanId>,
    }
    let views: BTreeMap<u64, V> = sim
        .agents
        .query::<(&AgentId, &Position, &Physiology, &Demographics, &Kinship, &crate::social::ClanMembership)>()
        .iter()
        .map(|(_, (id, pos, phys, demo, kin, clan))| {
            (
                id.0,
                V {
                    pos: (pos.x, pos.y),
                    hunger: phys.hunger,
                    female_adult: demo.sex == Sex::Female && demo.is_adult(tick),
                    infant: demo.is_infant(tick),
                    mother: kin.mother.map(|m| m.0),
                    clan: clan.0,
                },
            )
        })
        .collect();
    // L'enfant a grandi, ou l'un des deux est mort : le lien se défait.
    sim.fosters.retain(|child, woman| {
        views.get(child).is_some_and(|v| v.infant) && views.contains_key(&woman.0)
    });
    // Qui allaite : les mères d'un nourrisson vivant, et celles qui en ont pris un.
    let mut nursing: BTreeSet<u64> =
        views.values().filter(|v| v.infant).filter_map(|v| v.mother).collect();
    nursing.extend(sim.fosters.values().map(|w| w.0));
    let parents: BTreeMap<u64, (u64, u64)> =
        sim.births.iter().map(|b| (b.child.0, (b.mother.0, b.father.0))).collect();
    let seed = sim.world.seed();
    let orphans: Vec<u64> = views
        .iter()
        .filter(|(id, v)| {
            v.infant
                && v.mother.is_none_or(|m| !views.contains_key(&m))
                && !sim.fosters.contains_key(id)
        })
        .map(|(id, _)| *id)
        .collect();
    for child in orphans {
        let c = &views[&child];
        let mut candidates: Vec<(f64, u64)> = nursing
            .iter()
            .filter_map(|w| views.get(w).map(|v| (*w, v)))
            .filter(|(_, v)| {
                v.female_adult
                    && v.hunger < STARVING_MOTHER_HUNGER
                    && (v.pos.0 - c.pos.0).hypot(v.pos.1 - c.pos.1) <= crate::social::RESIDENCE_RADIUS_TILES
            })
            .map(|(w, v)| {
                let kin = kin_closeness(&parents, w, child);
                let p = if kin > 0.0 {
                    kin
                } else if c.clan.is_some() && v.clan == c.clan {
                    ADOPT_SAME_BAND_P
                } else {
                    ADOPT_STRANGER_P
                };
                (p, w)
            })
            .collect();
        // Seule la plus proche tente sa chance ce jour-là (égalité départagée
        // par l'identifiant) : c'est la parenté qui décide, pas le nombre de
        // femmes alentour — vingt étrangères ne valent pas une sœur.
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut rng = Pcg32::new(seed.derive(salt::ADOPTION) ^ splitmix64(day), child);
        if let Some(&(p, w)) = candidates.first()
            && rng.next_f64() < p
        {
            sim.fosters.insert(child, AgentId(w));
        }
    }
}

/// La passe quotidienne (à minuit) : naissances, conceptions, sénescence.
/// Tous les tirages dérivent de (seed, jour, id) — l'ordre d'itération ne
/// change aucun résultat.
pub(crate) fn daily(sim: &mut Sim) {
    let tick = sim.time.tick;
    let day = tick / TICKS_PER_DAY;
    let seed = sim.world.seed();

    // 1. Naissances : les grossesses à terme. On collecte pendant la
    // requête, on spawne après — pas de création d'entité en pleine itération.
    let mut due: Vec<(AgentId, (f64, f64), Traits, Pregnancy)> = Vec::new();
    for (_, (id, pos, traits, demo)) in sim
        .agents
        .query_mut::<(&AgentId, &Position, &Traits, &mut Demographics)>()
    {
        if let Some(pregnancy) = demo.pregnancy
            && pregnancy.due_tick <= tick
        {
            demo.pregnancy = None;
            due.push((*id, (pos.x, pos.y), *traits, pregnancy));
        }
    }
    for (mother, pos, mother_traits, pregnancy) in due {
        let mut rng = Pcg32::new(seed.derive(salt::HEREDITY) ^ splitmix64(tick), mother.0);
        let sex = if rng.next_u32() & 1 == 0 { Sex::Female } else { Sex::Male };
        let traits = Traits::inherit(&mother_traits, &pregnancy.father_traits, &mut rng);
        let child = sim.spawn_child(
            pos.0,
            pos.1,
            sex,
            traits,
            Kinship { mother: Some(mother), father: Some(pregnancy.father) },
        );
        sim.births.push(BirthRecord { tick, mother, father: pregnancy.father, child });
    }

    // 2. Aménorrhée de lactation : les mères d'un nourrisson vivant.
    let mut nursing: BTreeSet<u64> = BTreeSet::new();
    for (_, (demo, kin)) in sim.agents.query::<(&Demographics, &Kinship)>().iter() {
        if demo.is_infant(tick)
            && let Some(mother) = kin.mother
        {
            nursing.insert(mother.0);
        }
    }

    // 3. Conceptions. Les pères candidats d'abord (instantané), puis chaque
    // femme féconde cherche le plus proche à portée de rencontre.
    let males: Vec<((f64, f64), AgentId, Traits)> = sim
        .agents
        .query::<(&AgentId, &Position, &Physiology, &Traits, &Demographics)>()
        .iter()
        .filter(|(_, (_, _, phys, _, demo))| {
            demo.sex == Sex::Male
                && MALE_FERTILE_YEARS.contains(&demo.age_years(tick))
                && phys.health > 0.3
        })
        .map(|(_, (id, pos, _, traits, _))| ((pos.x, pos.y), *id, *traits))
        .collect();

    for (_, (id, pos, phys, demo)) in sim
        .agents
        .query_mut::<(&AgentId, &Position, &Physiology, &mut Demographics)>()
    {
        let fertile = demo.sex == Sex::Female
            && FEMALE_FERTILE_YEARS.contains(&demo.age_years(tick))
            && demo.pregnancy.is_none()
            && !nursing.contains(&id.0)
            // Les gardes de condition : la disette suspend la fécondité.
            && phys.health > 0.6
            && phys.hunger < 0.85
            && phys.thirst < 0.9;
        if !fertile {
            continue;
        }
        let mut nearest: Option<(f64, AgentId, Traits)> = None;
        for (mpos, mid, mtraits) in &males {
            let d2 = (pos.x - mpos.0).powi(2) + (pos.y - mpos.1).powi(2);
            if d2 <= MATE_RADIUS_TILES * MATE_RADIUS_TILES
                && nearest.is_none_or(|(bd, _, _)| d2 < bd)
            {
                nearest = Some((d2, *mid, *mtraits));
            }
        }
        let Some((_, father, father_traits)) = nearest else {
            continue;
        };
        let mut rng = Pcg32::new(seed.derive(salt::CONCEPTION) ^ splitmix64(day), id.0);
        if rng.next_f64() < CONCEPTION_DAILY_P {
            demo.pregnancy = Some(Pregnancy {
                due_tick: tick + GESTATION_DAYS * TICKS_PER_DAY,
                father,
                father_traits,
            });
        }
    }

    // 4. Sénescence : un tirage quotidien par agent contre sa mortalité
    // d'âge. Collecte puis retrait, comme les morts physiologiques.
    let mut dead: Vec<(hecs::Entity, AgentId, (i64, i64))> = Vec::new();
    for (entity, (id, pos, demo)) in sim
        .agents
        .query::<(&AgentId, &Position, &Demographics)>()
        .iter()
    {
        let p_daily = annual_mortality(demo.age_years(tick)) / DAYS_PER_YEAR as f64;
        let mut rng = Pcg32::new(seed.derive(salt::SENESCENCE) ^ splitmix64(day), id.0);
        if rng.next_f64() < p_daily {
            dead.push((entity, *id, pos.tile()));
        }
    }
    for (entity, agent, pos) in dead {
        let _ = sim.agents.despawn(entity);
        sim.routes.remove(&agent.0);
        sim.deaths.push(DeathRecord { tick, agent, cause: DeathCause::OldAge, pos });
    }

    // 5. Allomaternage : les orphelins que quelqu'un peut allaiter.
    adopt_orphans(sim);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_heredite_moyenne_les_parents_et_mute() {
        let mother = Traits { curiosity: 0.2, ..Default::default() };
        let father = Traits { curiosity: 0.8, ..Default::default() };
        let mut rng = Pcg32::new(7, 0);
        let mut sum = 0.0;
        let mut min: f32 = 1.0;
        let mut max: f32 = 0.0;
        let n = 500;
        for _ in 0..n {
            let child = Traits::inherit(&mother, &father, &mut rng);
            assert!((0.0..=1.0).contains(&child.curiosity));
            sum += child.curiosity;
            min = min.min(child.curiosity);
            max = max.max(child.curiosity);
        }
        let mean = sum / n as f32;
        assert!(
            (mean - 0.5).abs() < 0.02,
            "moyenne {mean:.3}, attendu ~0,5 (milieu parental)"
        );
        assert!(max - min > 0.05, "sans mutation, pas d'évolution possible");
    }

    #[test]
    fn l_heredite_est_deterministe() {
        let a = Traits::inherit(&Traits::default(), &Traits::default(), &mut Pcg32::new(9, 1));
        let b = Traits::inherit(&Traits::default(), &Traits::default(), &mut Pcg32::new(9, 1));
        assert_eq!(a, b);
    }

    #[test]
    fn la_mortalite_croit_avec_l_age() {
        assert!(annual_mortality(20.0) < 0.002, "à 20 ans, la vieillesse ne tue pas");
        assert!(annual_mortality(60.0) > 0.02);
        assert!(annual_mortality(80.0) > 0.3, "à 80 ans, chaque année est un pari");
        for age in 1..100 {
            assert!(
                annual_mortality(age as f64) <= annual_mortality(age as f64 + 1.0),
                "la mortalité doit être croissante"
            );
        }
    }

    #[test]
    fn la_capacite_de_travail_grandit_avec_l_age() {
        assert_eq!(work_capacity(1.0), 0.0, "un nourrisson ne produit rien");
        assert!(work_capacity(8.0) > 0.3 && work_capacity(8.0) < 0.9);
        assert_eq!(work_capacity(25.0), 1.0);
        assert!(work_capacity(5.0) < work_capacity(10.0));
    }

    #[test]
    fn les_stades_de_vie_derivent_du_tick_de_naissance() {
        let tick = 10 * TICKS_PER_YEAR;
        let infant = Demographics {
            sex: Sex::Female,
            born_tick: (tick - TICKS_PER_YEAR) as i64,
            pregnancy: None,
        };
        assert!(infant.is_infant(tick) && !infant.is_adult(tick));
        let elder = Demographics { sex: Sex::Male, born_tick: -(60 * TICKS_PER_YEAR as i64), pregnancy: None };
        assert!(elder.is_adult(tick));
        assert!((elder.age_years(tick) - 70.0).abs() < 0.01);
    }

    #[test]
    fn les_fondateurs_sont_des_adultes_varies() {
        let seed = WorldSeed(42);
        let mut females = 0;
        for id in 0..100 {
            let (demo, traits) = founder(seed, id, 0);
            let age = demo.age_years(0);
            assert!((16.0..40.0).contains(&age), "fondateur de {age:.1} ans");
            assert!((0.0..=1.0).contains(&traits.curiosity));
            if demo.sex == Sex::Female {
                females += 1;
            }
        }
        assert!(
            (30..=70).contains(&females),
            "sex-ratio dégénéré : {females} femmes sur 100"
        );
        // Déterminisme : même seed, même fondateur.
        assert_eq!(founder(seed, 7, 0).0, founder(seed, 7, 0).0);
    }
}
