//! L'état du monde tel qu'un observateur le reçoit.
//!
//! Deux régimes cohabitent ici, et les confondre coûterait cher :
//!
//! - **Ce qui se dessine** est filtré par la région ([`Region`]) : agents,
//!   faune, structures, feux, météo, lieux sacrés. Un observateur du nord ne
//!   paie pas les agents du sud — c'est tout l'intérêt de l'abonnement
//!   (BRIEF §8.5), et la raison pour laquelle trois clients ne coûtent pas
//!   trois mondes.
//! - **Ce qui se résume** est global et **pré-agrégé** ([`WorldStatus`]) :
//!   pyramide des âges, morts par cause, compétences moyennes. Ces chiffres
//!   parlent de *l'humanité*, pas d'une fenêtre ; les calculer au serveur
//!   coûte une passe qu'on faisait de toute façon, et évite d'envoyer une
//!   ligne par habitant pour en afficher un histogramme.
//!
//! Ce qui n'est **pas** ici : le terrain (fonction pure de la seed, le client
//! le régénère) et le détail d'un être (requête à la demande, §7.2).

use serde::{Deserialize, Serialize};

use cairn_sim::{
    Activity, Age, Creed, DeathCause, Sex, Sim, Species, StructureKind, TechId, WeatherKind,
};

/// La fenêtre à laquelle un client s'abonne, en tuiles. Rectangle demi-ouvert
/// aux bornes hautes n'a aucune importance ici : à l'échelle où une tuile fait
/// 2 m, une entité au pixel près de la bordure n'existe pas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub min_x: i64,
    pub min_y: i64,
    pub max_x: i64,
    pub max_y: i64,
}

impl Region {
    /// La région qui ne filtre rien. Sert au rendu natif (`client_render`), qui
    /// n'a pas de viewport à respecter, et aux tests.
    pub fn everything() -> Self {
        Self { min_x: i64::MIN, min_y: i64::MIN, max_x: i64::MAX, max_y: i64::MAX }
    }

    /// Une fenêtre centrée, exprimée en tuiles.
    pub fn around(center: (f64, f64), half_width: i64, half_height: i64) -> Self {
        let (cx, cy) = (center.0 as i64, center.1 as i64);
        Self {
            min_x: cx.saturating_sub(half_width),
            min_y: cy.saturating_sub(half_height),
            max_x: cx.saturating_add(half_width),
            max_y: cy.saturating_add(half_height),
        }
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        // Les coordonnées de simulation sont des `f64` sur un monde `i64` : on
        // compare en flottant pour ne pas perdre la fraction de tuile.
        x >= self.min_x as f64 && x <= self.max_x as f64
            && y >= self.min_y as f64 && y <= self.max_y as f64
    }
}

/// Un humain tel qu'il se **dessine** : de quoi le placer, le colorer et le
/// désigner. Rien de plus — sa physiologie, ses savoirs et sa lignée
/// n'arrivent que si on clique dessus.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AgentDot {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub sex: Sex,
    /// En années. Le client en tire la taille du carré (un enfant est plus
    /// petit) sans avoir à connaître les seuils d'âge de la simulation.
    pub age_years: f32,
    pub activity: Activity,
    pub clan: Option<u64>,
}

/// Troupeau ou meute : l'entité de simulation est le **groupe**, pas la bête
/// (choix de la Phase 2), et c'est ce que le fil transporte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FaunaRole {
    Herd,
    Pack,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FaunaDot {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub role: FaunaRole,
    pub species: Species,
    pub population: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StructureDot {
    pub kind: StructureKind,
    pub clan: u64,
    pub x: f64,
    pub y: f64,
    /// Un bâtiment que plus aucun clan ne revendique tombe en ruine ; le client
    /// le grise plutôt que de le faire disparaître d'un coup.
    pub abandoned: bool,
}

/// Une cellule météo ou un incendie : deux disques qui s'affichent pareil, et
/// dont l'un dit où le sol peut encore prendre feu.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeatherDot {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    /// `None` = un incendie. Un feu n'est pas une météo, mais il se dessine
    /// dans la même couche et se filtre par la même région.
    pub kind: Option<WeatherKind>,
}

/// Ce qu'un peuple montre de lui à distance. Les clans sont peu nombreux et
/// tiennent en quelques centaines d'octets : ils ne sont **pas** filtrés par
/// région — on veut pouvoir lire les tensions d'un peuple qu'on ne regarde pas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClanSummary {
    pub id: u64,
    /// Le nom est calculé au serveur (`cairn_sim::names`, fonction pure de la
    /// seed) : le client pourrait le refaire, mais l'envoyer garantit que les
    /// deux disent le même mot.
    pub name: String,
    pub home_x: f64,
    pub home_y: f64,
    pub members: usize,
    pub stock: f32,
    pub chief: u64,
    pub chief_name: String,
    pub founded_tick: u64,
    pub age: Age,
    pub desired: Option<StructureKind>,
    /// Les techs encore portées par au moins un membre vivant. Un savoir que
    /// plus personne ne porte quitte cette liste de lui-même — l'oubli du §5.4
    /// se voit ici sans qu'on ait rien à effacer.
    pub corpus: Vec<TechId>,
    /// Ce que ce peuple croit de vous, si tant est qu'il croie quelque chose.
    pub creed: Creed,
    pub believers: usize,
    pub fervor: f32,
}

/// L'humanité en chiffres, à l'échelle du monde entier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldStatus {
    pub tick: u64,
    pub faith: f32,
    pub population: usize,
    pub females: usize,
    pub males: usize,
    pub children: usize,
    pub births: usize,
    pub deaths: usize,
    /// Ventilation des morts, dans l'ordre de déclaration de [`DeathCause`].
    /// Le contre-champ de la Chronique : un bilan de raid ne veut rien dire
    /// s'il ne correspond pas à des morts enregistrées.
    pub deaths_by_cause: Vec<(DeathCause, u32)>,
    /// Effectifs par tranche de dix ans, de 0-9 à 80+.
    pub age_pyramid: Vec<u32>,
    pub herbivores: f32,
    pub predators: f32,
    pub herds: usize,
    pub packs: usize,
    /// Ce que l'humanité sait encore faire — l'union de tous les corpus.
    pub known_techs: Vec<TechId>,
    /// Longueur du journal : le client sait s'il a du retard à rattraper.
    pub chronicle_len: usize,
    pub mean_foraging: f32,
    pub mean_hunting: f32,
}

/// Tout ce qu'un observateur reçoit d'un tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub status: WorldStatus,
    /// La fenêtre effectivement servie — le client vérifie qu'on lui répond
    /// bien sur ce qu'il a demandé.
    pub region: Region,
    pub agents: Vec<AgentDot>,
    pub fauna: Vec<FaunaDot>,
    pub structures: Vec<StructureDot>,
    pub weather: Vec<WeatherDot>,
    pub shrines: Vec<(f64, f64)>,
    pub clans: Vec<ClanSummary>,
    /// Tension entre deux peuples, clé ordonnée. Ce qui monte ici finit en
    /// affrontement : c'est la donnée la plus prédictive du panneau.
    pub tensions: Vec<(u64, u64, f32)>,
}

/// Nombre de tranches de la pyramide des âges (0-9 … 80+).
const AGE_BUCKETS: usize = 9;

impl WorldSnapshot {
    /// Extrait du monde ce qu'un observateur de cette région doit voir.
    ///
    /// Une seule passe sur les agents sert les deux régimes : elle agrège pour
    /// tout le monde (pyramide, sexes, compétences) **et** retient les points à
    /// dessiner pour cette fenêtre. Faire deux passes coûterait deux fois le
    /// prix du régime le plus cher, pour rien.
    pub fn from_sim(sim: &Sim, region: Region) -> Self {
        use cairn_sim::{AgentId, Behavior, Demographics, FaunaId, Herd, Pack, Position, Skills};

        let tick = sim.time.tick;
        let mut agents = Vec::new();
        let mut age_pyramid = vec![0u32; AGE_BUCKETS];
        let (mut females, mut males, mut children) = (0usize, 0usize, 0usize);
        let (mut sum_foraging, mut sum_hunting) = (0.0f32, 0.0f32);

        for (_, (id, pos, demo, behavior, membership, skills)) in sim
            .agents
            .query::<(
                &AgentId,
                &Position,
                &Demographics,
                &Behavior,
                &cairn_sim::ClanMembership,
                &Skills,
            )>()
            .iter()
        {
            let age_years = (tick as i64 - demo.born_tick) as f32
                / (cairn_core::TICKS_PER_DAY as f32 * cairn_core::DAYS_PER_YEAR as f32);

            match demo.sex {
                Sex::Female => females += 1,
                Sex::Male => males += 1,
            }
            if age_years < ADULT_AGE_YEARS {
                children += 1;
            }
            let bucket = ((age_years / 10.0) as usize).min(AGE_BUCKETS - 1);
            age_pyramid[bucket] += 1;
            sum_foraging += skills.foraging;
            sum_hunting += skills.hunting;

            if region.contains(pos.x, pos.y) {
                agents.push(AgentDot {
                    id: id.0,
                    x: pos.x,
                    y: pos.y,
                    sex: demo.sex,
                    age_years,
                    activity: behavior.activity,
                    clan: membership.0.map(|c| c.0),
                });
            }
        }

        let population = females + males;
        let inv = if population == 0 { 0.0 } else { 1.0 / population as f32 };

        let mut fauna = Vec::new();
        for (_, (id, herd, pos)) in sim.fauna.query::<(&FaunaId, &Herd, &Position)>().iter() {
            if region.contains(pos.x, pos.y) {
                fauna.push(FaunaDot {
                    id: id.0,
                    x: pos.x,
                    y: pos.y,
                    role: FaunaRole::Herd,
                    species: herd.species,
                    population: herd.population,
                });
            }
        }
        for (_, (id, pack, pos)) in sim.fauna.query::<(&FaunaId, &Pack, &Position)>().iter() {
            if region.contains(pos.x, pos.y) {
                fauna.push(FaunaDot {
                    id: id.0,
                    x: pos.x,
                    y: pos.y,
                    role: FaunaRole::Pack,
                    species: pack.species,
                    population: pack.population,
                });
            }
        }

        let structures = sim
            .structures
            .iter()
            .filter(|s| region.contains(s.pos.0, s.pos.1))
            .map(|s| StructureDot {
                kind: s.kind,
                clan: s.clan.0,
                x: s.pos.0,
                y: s.pos.1,
                abandoned: s.abandoned_since.is_some(),
            })
            .collect();

        let weather = sim
            .weather
            .iter()
            .filter(|c| region.contains(c.pos.0, c.pos.1))
            .map(|c| WeatherDot { x: c.pos.0, y: c.pos.1, radius: c.radius, kind: Some(c.kind) })
            .chain(sim.fires.iter().filter(|f| region.contains(f.pos.0, f.pos.1)).map(|f| {
                WeatherDot { x: f.pos.0, y: f.pos.1, radius: f.radius, kind: None }
            }))
            .collect();

        let shrines = sim
            .shrines
            .iter()
            .filter(|s| region.contains(s.pos.0, s.pos.1))
            .map(|s| s.pos)
            .collect();

        let clans = sim
            .clans
            .iter()
            .map(|c| {
                let theology = sim.clan_theology(c.id);
                ClanSummary {
                    id: c.id.0,
                    name: sim.clan_name(c.id),
                    home_x: c.home.0,
                    home_y: c.home.1,
                    members: c.members.len(),
                    stock: c.stock,
                    chief: c.chief.0,
                    chief_name: sim.agent_name(c.chief).unwrap_or_default(),
                    founded_tick: c.founded_tick,
                    age: sim.clan_age(c.id),
                    desired: c.desired,
                    corpus: sim.clan_corpus(c.id).into_iter().collect(),
                    creed: theology.creed(),
                    believers: theology.believers,
                    fervor: theology.fervor,
                }
            })
            .collect::<Vec<_>>();

        let mut tensions = Vec::new();
        for i in 0..sim.clans.len() {
            for j in (i + 1)..sim.clans.len() {
                let (a, b) = (sim.clans[i].id, sim.clans[j].id);
                let t = sim.clan_relations.tension_between(a, b);
                if t > 0.0 {
                    tensions.push((a.0, b.0, t));
                }
            }
        }

        let mut deaths_by_cause: Vec<(DeathCause, u32)> = Vec::new();
        for d in &sim.deaths {
            match deaths_by_cause.iter_mut().find(|(c, _)| *c == d.cause) {
                Some((_, n)) => *n += 1,
                None => deaths_by_cause.push((d.cause, 1)),
            }
        }

        let (herbivores, predators, herds, packs) = sim.fauna_census();

        Self {
            status: WorldStatus {
                tick,
                faith: sim.faith,
                population,
                females,
                males,
                children,
                births: sim.births.len(),
                deaths: sim.deaths.len(),
                deaths_by_cause,
                age_pyramid,
                herbivores,
                predators,
                herds,
                packs,
                known_techs: sim.known_techs.iter().copied().collect(),
                chronicle_len: sim.chronicle.len(),
                mean_foraging: sum_foraging * inv,
                mean_hunting: sum_hunting * inv,
            },
            region,
            agents,
            fauna,
            structures,
            weather,
            shrines,
            clans,
            tensions,
        }
    }
}

/// Seuil d'âge adulte, en années — le même que celui de la simulation
/// (`demography`). Répété ici pour que le client n'ait pas à le connaître :
/// il reçoit un âge, pas une règle.
const ADULT_AGE_YEARS: f32 = 14.0;

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_core::WorldSeed;

    /// Une scène minuscule mais **réelle** : un vrai `Sim`, de vrais agents.
    fn scene() -> Sim {
        let mut sim = Sim::new(WorldSeed(42), 256);
        sim.allow_wildfires = false;
        sim.allow_weather = false;
        sim.allow_fauna_immigration = false;
        sim
    }

    #[test]
    fn la_region_filtre_ce_qui_se_dessine_mais_pas_ce_qui_se_resume() {
        let mut sim = scene();
        // Deux humains très loin l'un de l'autre : un dans la fenêtre, un hors.
        sim.spawn_agent(0.0, 0.0);
        sim.spawn_agent(100_000.0, 100_000.0);

        let window = Region::around((0.0, 0.0), 1_000, 1_000);
        let snap = WorldSnapshot::from_sim(&sim, window);

        // Ce qui se dessine est filtré…
        assert_eq!(snap.agents.len(), 1, "un seul agent est dans la fenêtre");
        // …ce qui se résume ne l'est pas : la population est celle du monde.
        assert_eq!(snap.status.population, 2, "l'humanité entière est comptée");
    }

    #[test]
    fn la_region_totale_ne_filtre_rien() {
        let mut sim = scene();
        sim.spawn_agent(0.0, 0.0);
        sim.spawn_agent(100_000.0, 100_000.0);

        let snap = WorldSnapshot::from_sim(&sim, Region::everything());
        assert_eq!(snap.agents.len(), 2);
    }

    #[test]
    fn un_instantane_survit_a_l_aller_retour() {
        let mut sim = scene();
        for i in 0..8 {
            sim.spawn_agent(i as f64 * 3.0, 0.0);
        }
        sim.step();

        let snap = WorldSnapshot::from_sim(&sim, Region::everything());
        let bytes = crate::encode(&snap).expect("encodage");
        let back: WorldSnapshot = crate::decode(&bytes).expect("décodage");
        assert_eq!(snap, back, "le fil ne déforme rien");
    }

    /// Ce test ne vérifie pas une valeur, il **mesure** : combien pèse un
    /// instantané ? C'est le chiffre qui décidera si un delta est nécessaire
    /// (BRIEF §8.5, deltas à 5-10 Hz) — on ne l'écrira qu'une fois ce coût
    /// jugé trop lourd, pas avant (règle §0.3).
    #[test]
    fn le_poids_d_un_instantane_est_mesure() {
        let mut sim = scene();
        for i in 0..40 {
            sim.spawn_agent((i % 8) as f64 * 4.0, (i / 8) as f64 * 4.0);
        }
        sim.step();

        let snap = WorldSnapshot::from_sim(&sim, Region::everything());
        let bytes = crate::encode(&snap).expect("encodage");
        let per_agent = bytes.len() as f64 / snap.agents.len().max(1) as f64;
        println!(
            "instantané : {} octets pour {} agents ({:.0} o/agent)",
            bytes.len(),
            snap.agents.len(),
            per_agent
        );
        // Une borne large, qui ne casse que si quelqu'un met le monde entier
        // dans le flux — la mesure est le propos, pas le seuil.
        assert!(per_agent < 200.0, "un agent ne doit pas coûter un roman sur le fil");
    }
}
