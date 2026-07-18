//! La boucle de simulation à tick fixe (BRIEF §8.2).
//!
//! `Sim` réunit le monde de tuiles (chunké, mutable), la population d'agents
//! (entités `hecs`) et l'horloge. Chaque [`step`](Sim::step) déroule les
//! systèmes **dans un ordre fixe, écrit ici noir sur blanc** — c'est notre
//! scheduler, et c'est ce qui rend la simulation déterministe :
//!
//! 0. **Instantanés** : positions du gibier, des meutes, des humains.
//! 1. **Délibération** (bucketée) : les agents « dus » choisissent une tâche.
//! 2. **Exécution** : chacun avance sa tâche — marche, mange, boit, chasse.
//! 3. **Physiologie** : les besoins dérivent, le climat mord, on meurt.
//! 4. **Faune** : les meutes chassent, les troupeaux paissent, fuient, migrent.
//! 5. **Écologie** (une fois par jour) : la biomasse consommée repousse.
//!
//! Humains et faune vivent dans **deux mondes `hecs` distincts** : ils
//! partagent le composant `Position`, et une requête sur `&Position` dans un
//! monde unique ramasserait les deux — source de bugs silencieux. La
//! coordination passe par des instantanés (`HerdView`) : on lit un état figé,
//! puis on mute. Aucune lecture ne dépend donc de l'ordre des écritures.
//!
//! L'itération `hecs` est déterministe pour notre usage : les entités d'un
//! même type partagent leur jeu de composants (une seule archétype), l'ordre
//! est l'ordre d'apparition, et les retraits sont eux-mêmes ordonnés.

use cairn_core::{SimTime, TICKS_PER_DAY, WorldSeed};
use cairn_worldgen::WorldGenConfig;

use crate::agent::{
    Activity, AgentId, Behavior, DeathCause, FOREST_BONUS_C, Physiology, Position,
    SHELTER_BONUS_C, TaskKind, WALK_TILES_PER_TICK,
};
use crate::brain::{self, DELIBERATION_PERIOD};
use crate::climate::Climate;
use crate::ecology;
use crate::fauna::{self, FaunaId, Herd, HerdView, Kill, Pack};
use crate::world::World;

/// Un agent est « arrivé » sous une tuile et demie de sa cible.
const ARRIVAL_TILES: f64 = 1.5;
/// Pas d'échantillonnage du terrain pendant la marche : on vérifie la
/// franchissabilité toutes les 8 tuiles (16 m), pas à chaque tuile.
const WALK_SAMPLE_TILES: f64 = 8.0;
/// Une bouchée d'une heure : ce qu'un agent peut réduire de faim en mangeant.
const EAT_HUNGER_PER_TICK: f32 = 0.3;
/// Valeur nutritive d'une unité de biomasse, en points de faim.
const NUTRITION_PER_BIOMASS: f32 = 0.02;
/// Portée d'une mise à mort : le chasseur doit être à ~200 m du troupeau.
const HUNT_REACH_TILES: f64 = cairn_core::km_to_tiles(0.2);
/// Têtes prélevées par chasse réussie.
const HUNT_YIELD_HEAD: f32 = 1.0;
/// Ce qu'une prise retire de faim : une bête nourrit bien mieux que des
/// baies — c'est tout l'intérêt du risque et du trajet.
const HUNT_NUTRITION: f32 = 0.7;

/// Trace d'un décès, pour les statistiques — et, un jour, la Chronique.
#[derive(Debug, Clone, Copy)]
pub struct DeathRecord {
    pub tick: u64,
    pub agent: AgentId,
    pub cause: DeathCause,
    pub pos: (i64, i64),
}

pub struct Sim {
    pub world: World,
    pub climate: Climate,
    pub agents: hecs::World,
    /// Troupeaux et meutes — monde séparé des humains (voir l'en-tête).
    pub fauna: hecs::World,
    pub time: SimTime,
    pub deaths: Vec<DeathRecord>,
    /// Têtes de gibier prélevées par les humains depuis le début : le compteur
    /// de la pression de chasse.
    pub hunted_head: f32,
    next_agent_id: u64,
    next_fauna_id: u64,
}

impl Sim {
    /// `chunk_capacity` : nombre de chunks résidents du LRU. Doit couvrir
    /// largement la zone active de la population (un chunk = 128 m de côté).
    pub fn new(seed: WorldSeed, chunk_capacity: usize) -> Self {
        Self::with_config(seed, chunk_capacity, WorldGenConfig::default())
    }

    /// Comme [`new`](Self::new), mais avec une config de worldgen (le client
    /// veut une humidité rapide).
    pub fn with_config(seed: WorldSeed, chunk_capacity: usize, cfg: WorldGenConfig) -> Self {
        let world = World::with_config(seed, chunk_capacity, cfg);
        let climate = Climate::new(world.worldgen().temperature.latitude());
        Self {
            world,
            climate,
            agents: hecs::World::new(),
            fauna: hecs::World::new(),
            time: SimTime::default(),
            deaths: Vec::new(),
            hunted_head: 0.0,
            next_agent_id: 0,
            next_fauna_id: 0,
        }
    }

    pub fn spawn_agent(&mut self, x: f64, y: f64) -> AgentId {
        let id = AgentId(self.next_agent_id);
        self.next_agent_id += 1;
        self.agents.spawn((
            id,
            Position { x, y },
            Physiology::default(),
            Behavior::default(),
        ));
        id
    }

    pub fn spawn_herd(&mut self, x: f64, y: f64, population: f32) -> FaunaId {
        let id = FaunaId(self.next_fauna_id);
        self.next_fauna_id += 1;
        self.fauna.spawn((id, Position { x, y }, Herd::new(population)));
        id
    }

    pub fn spawn_pack(&mut self, x: f64, y: f64, population: f32) -> FaunaId {
        let id = FaunaId(self.next_fauna_id);
        self.next_fauna_id += 1;
        self.fauna.spawn((id, Position { x, y }, Pack::new(population)));
        id
    }

    pub fn population(&self) -> usize {
        self.agents.len() as usize
    }

    /// Instantané des troupeaux, dans l'ordre d'itération de `hecs`.
    pub fn herd_views(&self) -> Vec<HerdView> {
        self.fauna
            .query::<(&Herd, &Position)>()
            .iter()
            .map(|(entity, (herd, pos))| HerdView {
                entity,
                pos: (pos.x, pos.y),
                population: herd.population,
            })
            .collect()
    }

    /// Effectifs totaux (herbivores, prédateurs) et nombre de groupes.
    pub fn fauna_census(&self) -> (f32, f32, usize, usize) {
        let mut herbivores = 0.0;
        let mut herds = 0;
        for (_, herd) in self.fauna.query::<&Herd>().iter() {
            herbivores += herd.population;
            herds += 1;
        }
        let mut predators = 0.0;
        let mut packs = 0;
        for (_, pack) in self.fauna.query::<&Pack>().iter() {
            predators += pack.population;
            packs += 1;
        }
        (herbivores, predators, herds, packs)
    }

    /// Un tick : une heure de monde.
    pub fn step(&mut self) {
        let time = self.time;

        // 0. Instantanés : l'état de la faune tel qu'il est *au début* du
        // tick. Tout le monde délibère sur la même photo.
        let herds = self.herd_views();

        // 1. Délibération — bucketée : l'agent i ne repense sa tâche qu'aux
        // ticks (tick + i) % période == 0, ou dès qu'il n'a plus de tâche.
        for (_, (id, pos, phys, behavior)) in self
            .agents
            .query_mut::<(&AgentId, &Position, &Physiology, &mut Behavior)>()
        {
            let due = behavior.task.is_none()
                || (time.tick.wrapping_add(id.0)) % DELIBERATION_PERIOD == 0;
            if due {
                let current = behavior.task.map(|t| t.kind);
                behavior.task =
                    brain::decide(&mut self.world, time, *id, pos, phys, current, &herds);
            }
        }

        // 2. Exécution des tâches. Les chasses réussies sont collectées : on
        // n'entame pas le gibier pendant que les autres délibèrent dessus.
        let mut kills: Vec<Kill> = Vec::new();
        for (_, (pos, phys, behavior)) in self
            .agents
            .query_mut::<(&mut Position, &mut Physiology, &mut Behavior)>()
        {
            if let Some(kill) = execute(&mut self.world, pos, phys, behavior, &herds) {
                kills.push(kill);
            }
        }
        self.hunted_head += kills.iter().map(|k| k.head).sum::<f32>();

        // 3. Physiologie et morts. On collecte d'abord (on ne peut pas
        // retirer une entité pendant qu'on itère dessus), on retire après.
        let mut dead = Vec::new();
        for (entity, (id, pos, phys, behavior)) in self
            .agents
            .query_mut::<(&AgentId, &Position, &mut Physiology, &Behavior)>()
        {
            let (x, y) = pos.tile();
            let tile = self.world.tile(x, y);
            let mut felt = self.climate.instant(&tile, y, time);
            if matches!(
                tile.biome,
                cairn_worldgen::Biome::TemperateForest
                    | cairn_worldgen::Biome::TropicalForest
                    | cairn_worldgen::Biome::Taiga
            ) {
                felt += FOREST_BONUS_C;
            }
            if behavior.activity == Activity::Sheltering {
                felt += SHELTER_BONUS_C;
            }
            phys.drift(felt, behavior.activity);
            if phys.is_dead() {
                let cause = phys.last_damage.unwrap_or(DeathCause::Starvation);
                dead.push((entity, *id, cause, (x, y)));
            }
        }
        for (entity, agent, cause, pos) in dead {
            // Le despawn est hors itération : l'emprunt de la requête est
            // rendu, l'ordre de retrait suit l'ordre de collecte.
            let _ = self.agents.despawn(entity);
            self.deaths.push(DeathRecord { tick: time.tick, agent, cause, pos });
        }

        // 4. Faune. Les meutes chassent d'abord (sur l'instantané), puis
        // toutes les prises — prédation et chasse humaine — sont appliquées
        // avant que les troupeaux ne fassent leurs petits : un troupeau
        // décimé ne doit pas engendrer comme s'il était intact.
        let (pred_kills, dead_packs) = fauna::update_packs(&mut self.fauna, &self.world, &herds);
        kills.extend(pred_kills);
        fauna::apply_kills(&mut self.fauna, &kills);
        for entity in dead_packs {
            let _ = self.fauna.despawn(entity);
        }

        // Ce qui fait fuir un troupeau : les prédateurs et les hommes.
        let mut threats: Vec<(f64, f64)> = self
            .fauna
            .query::<(&Pack, &Position)>()
            .iter()
            .map(|(_, (_, pos))| (pos.x, pos.y))
            .collect();
        threats.extend(
            self.agents
                .query::<&Position>()
                .iter()
                .map(|(_, pos)| (pos.x, pos.y)),
        );

        let seed = self.world.seed();
        let (dead_herds, fissions) = fauna::update_herds(
            &mut self.fauna,
            &mut self.world,
            &self.climate,
            time,
            seed,
            &threats,
        );
        for entity in dead_herds {
            let _ = self.fauna.despawn(entity);
        }
        for (x, y, population) in fissions {
            self.spawn_herd(x, y, population);
        }

        // 5. Écologie quotidienne, à minuit.
        if time.tick.is_multiple_of(TICKS_PER_DAY) {
            ecology::daily_regrowth(&mut self.world, &self.climate, time);
        }

        self.time.tick += 1;
    }
}

/// Avance la tâche courante d'un agent : marche vers la cible, puis agit.
/// Renvoie une prise si l'agent a abattu du gibier ce tick.
fn execute(
    world: &mut World,
    pos: &mut Position,
    phys: &mut Physiology,
    behavior: &mut Behavior,
    herds: &[HerdView],
) -> Option<Kill> {
    let task = match behavior.task {
        Some(task) => task,
        None => {
            behavior.activity = Activity::Idle;
            return None;
        }
    };

    // La chasse est le seul cas où la cible **bouge** : on ne vise pas une
    // tuile mais une bête. On ré-évalue donc le troupeau à chaque tick, et
    // c'est ce qui fait que le chasseur *suit* le gibier au lieu de courir
    // vers l'herbe où il broutait il y a quatre heures.
    if task.kind == TaskKind::Hunt {
        let Some(prey) = closest_herd(pos, herds) else {
            behavior.task = None; // le troupeau est mort ou hors de vue
            behavior.activity = Activity::Idle;
            return None;
        };
        let target = (prey.pos.0.floor() as i64, prey.pos.1.floor() as i64);
        if pos.distance_tiles(target) > HUNT_REACH_TILES {
            behavior.activity = Activity::Walking;
            if !walk_towards(world, pos, target) {
                behavior.task = None;
            }
            return None;
        }
        behavior.activity = Activity::Hunting;
        phys.hunger = (phys.hunger - HUNT_NUTRITION).max(0.0);
        behavior.task = None;
        return Some(Kill { herd: prey.entity, head: HUNT_YIELD_HEAD });
    }

    // Phase trajet : la cible est trop loin, on marche (budget d'une heure).
    if pos.distance_tiles(task.target) > ARRIVAL_TILES {
        behavior.activity = Activity::Walking;
        if !walk_towards(world, pos, task.target) {
            behavior.task = None; // bloqué (eau) : on re-délibérera
        }
        return None;
    }

    // Phase action : sur place. Les actions ciblées (boire, fourrager)
    // s'appliquent à la tuile **cible** : « arrivé » signifie à moins de
    // 1,5 tuile d'elle, pas forcément dessus.
    match task.kind {
        TaskKind::Drink => {
            behavior.activity = Activity::Drinking;
            if world.tile(task.target.0, task.target.1).has_fresh_water() {
                phys.thirst = 0.0;
            }
            behavior.task = None;
        }
        TaskKind::Forage => {
            behavior.activity = Activity::Eating;
            let tile = world.tile_mut(task.target.0, task.target.1);
            let wanted = (phys.hunger.min(EAT_HUNGER_PER_TICK) / NUTRITION_PER_BIOMASS)
                .ceil() as u8;
            let taken = wanted.min(tile.biomass);
            tile.biomass -= taken;
            phys.hunger = (phys.hunger - f32::from(taken) * NUTRITION_PER_BIOMASS).max(0.0);
            if phys.hunger <= 0.05 || tile.biomass == 0 {
                behavior.task = None; // rassasié, ou tuile épuisée
            }
        }
        TaskKind::Sleep => {
            behavior.activity = Activity::Sleeping;
            // On dort jusqu'à récupération — sauf urgence qui réveille.
            if phys.fatigue <= 0.02 || phys.thirst > 0.95 || phys.cold > 0.95 {
                behavior.task = None;
            }
        }
        TaskKind::Shelter => {
            behavior.activity = Activity::Sheltering;
            if phys.cold <= 0.05 {
                behavior.task = None;
            }
        }
        TaskKind::Wander => {
            behavior.activity = Activity::Idle;
            behavior.task = None; // arrivé au bout de la jambe d'errance
        }
        TaskKind::Hunt => unreachable!("la chasse est traitée avant, sa cible bouge"),
    }
    None
}

/// Le troupeau le plus proche de l'agent, sans borne de distance : le
/// chasseur a déjà décidé de chasser, il ne perd pas sa proie de vue au
/// premier pas. La borne de perception est appliquée à la **délibération**
/// ([`brain::decide`]), pas ici.
fn closest_herd(pos: &Position, herds: &[HerdView]) -> Option<HerdView> {
    let mut best: Option<(f64, HerdView)> = None;
    for h in herds {
        let d2 = (pos.x - h.pos.0).powi(2) + (pos.y - h.pos.1).powi(2);
        if best.is_none_or(|(bd, _)| d2 < bd) {
            best = Some((d2, *h));
        }
    }
    best.map(|(_, h)| h)
}

/// Marche d'une heure vers `target` : jusqu'à [`WALK_TILES_PER_TICK`] tuiles,
/// par segments échantillonnés — on s'arrête net devant l'eau (l'océan ne se
/// traverse pas à pied). Renvoie `false` si bloqué.
///
/// La franchissabilité est sondée dans le **baseline** (élévation ≤ 0 = eau,
/// exactement le critère du drapeau WATER à la génération) : matérialiser un
/// chunk entier pour chaque point de passage écraserait le LRU — un agent
/// en errance traverse des dizaines de chunks par heure.
fn walk_towards(world: &mut World, pos: &mut Position, target: (i64, i64)) -> bool {
    let mut budget = WALK_TILES_PER_TICK;
    while budget > 0.0 {
        let dx = target.0 as f64 + 0.5 - pos.x;
        let dy = target.1 as f64 + 0.5 - pos.y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist <= ARRIVAL_TILES {
            return true;
        }
        let step = WALK_SAMPLE_TILES.min(dist).min(budget);
        let next = (pos.x + dx / dist * step, pos.y + dy / dist * step);
        if world.worldgen().elevation(next.0.floor() as i64, next.1.floor() as i64) <= 0.0 {
            return false;
        }
        pos.x = next.0;
        pos.y = next.1;
        budget -= step;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Empreinte compacte de l'état complet : positions et physiologies dans
    /// l'ordre des identifiants. Deux exécutions identiques ⇒ même empreinte.
    fn fingerprint(sim: &Sim) -> Vec<(u64, u64, u64, u32, u32)> {
        let mut all: Vec<_> = sim
            .agents
            .query::<(&AgentId, &Position, &Physiology)>()
            .iter()
            .map(|(_, (id, pos, phys))| {
                (
                    id.0,
                    pos.x.to_bits(),
                    pos.y.to_bits(),
                    phys.hunger.to_bits(),
                    phys.health.to_bits(),
                )
            })
            .collect();
        all.sort();
        all
    }

    /// Cherche la terre ferme la plus proche de l'origine (qui peut être en
    /// mer !) — sondée via le worldgen, sans générer de chunks pour rien.
    fn find_land(sim: &Sim) -> (i64, i64) {
        // Les océans entre continents font des milliers de km : on sonde par
        // pas de 16 km, dans 8 directions, jusqu'à 6 400 km.
        let step = cairn_core::km_to_tiles(16.0) as i64;
        for r in 0..400i64 {
            let d = r * step;
            for &(x, y) in &[
                (d, 0), (-d, 0), (0, d), (0, -d),
                (d, d), (-d, d), (d, -d), (-d, -d),
            ] {
                // Marge d'élévation : franchement à l'intérieur des terres.
                if sim.world.worldgen().elevation(x, y) > 0.05 {
                    return (x, y);
                }
            }
        }
        panic!("aucune terre à moins de 6 400 km de l'origine");
    }

    /// Construit un essaim d'agents (et du gibier) sur la terre ferme la plus
    /// proche de l'origine.
    fn scenario(seed: u64, agents: u32, ticks: u64) -> Sim {
        let (mut sim, _) = scenario_setup(seed, agents, 4);
        for _ in 0..ticks {
            sim.step();
        }
        sim
    }

    /// Cheptel présent dans un rayon donné (en km) autour d'un point.
    /// C'est l'observable du critère « effondrement **local** » : le total
    /// global mélangerait les troupeaux enfuis au loin, qui sont bien vivants.
    fn local_herbivores(sim: &Sim, center: (i64, i64), radius_km: f64) -> f32 {
        let r = cairn_core::km_to_tiles(radius_km);
        sim.fauna
            .query::<(&Herd, &Position)>()
            .iter()
            .filter(|(_, (_, p))| {
                (p.x - center.0 as f64).hypot(p.y - center.1 as f64) < r
            })
            .map(|(_, (h, _))| h.population)
            .sum()
    }

    /// Prépare le monde sans le faire tourner : `agents` humains et `herds`
    /// troupeaux serrés autour de la même clairière. Renvoie aussi le foyer.
    fn scenario_setup(seed: u64, agents: u32, herds: u32) -> (Sim, (i64, i64)) {
        // Capacité large : à 512 chunks, 60 agents dispersés dépassent le
        // working set et le LRU thrash (mesuré : 0,8 tick/s contre 15).
        let mut sim = Sim::new(WorldSeed(seed), 2048);
        let home = find_land(&sim);
        let (mut placed, mut k) = (0, 0i64);
        while placed < agents && k < 10_000 {
            let (dx, dy) = ((k % 20) * 6 - 60, (k / 20) * 6 - 60);
            k += 1;
            let (x, y) = (home.0 + dx, home.1 + dy);
            if sim.world.tile(x, y).is_walkable() {
                sim.spawn_agent(x as f64 + 0.5, y as f64 + 0.5);
                placed += 1;
            }
        }
        assert_eq!(placed, agents, "semis incomplet autour de {home:?}");
        // Le gibier à portée de vue des chasseurs.
        for i in 0..herds {
            let angle = i as f64 * 1.7;
            sim.spawn_herd(
                home.0 as f64 + angle.cos() * 300.0,
                home.1 as f64 + angle.sin() * 300.0,
                fauna::HERD_START,
            );
        }
        (sim, home)
    }

    #[test]
    fn deux_executions_identiques_bit_a_bit() {
        let a = scenario(42, 30, 24 * 8);
        let b = scenario(42, 30, 24 * 8);
        assert_eq!(fingerprint(&a), fingerprint(&b));
        assert_eq!(a.deaths.len(), b.deaths.len());
        assert_eq!(a.world.dirty_count(), b.world.dirty_count());
        // La faune aussi : effectifs bit à bit, pas seulement « à peu près ».
        let (ha, pa, nha, npa) = a.fauna_census();
        let (hb, pb, nhb, npb) = b.fauna_census();
        assert_eq!((ha.to_bits(), pa.to_bits()), (hb.to_bits(), pb.to_bits()));
        assert_eq!((nha, npa), (nhb, npb));
        assert_eq!(a.hunted_head.to_bits(), b.hunted_head.to_bits());
    }

    /// LE critère d'acceptation de la faune : « la surchasse d'une zone
    /// provoque un effondrement local observable ».
    ///
    /// Expérience **contrôlée** : même seed, même clairière, même gibier —
    /// seule la pression de chasse change. C'est ce qui prouve que
    /// l'effondrement vient des chasseurs, et non du climat ou du hasard.
    ///
    /// L'observable est le cheptel **local** (3 km) : le total global
    /// mélangerait les troupeaux qui ont fui au loin et se portent très bien.
    /// La zone se vide des deux façons — on en tue, et les autres décampent.
    #[test]
    fn la_surchasse_effondre_le_gibier_local() {
        let jours = 24 * 15;
        let cheptel = |agents: u32| {
            let (mut sim, home) = scenario_setup(42, agents, 4);
            let depart = local_herbivores(&sim, home, 3.0);
            for _ in 0..jours {
                sim.step();
            }
            (depart, local_herbivores(&sim, home, 3.0), sim.hunted_head)
        };

        let (depart, temoin, tue_temoin) = cheptel(0);
        let (_, surchasse, tue_surchasse) = cheptel(60);

        assert_eq!(tue_temoin, 0.0, "témoin : personne ne chasse");
        assert!(
            temoin > depart * 0.5,
            "témoin : le gibier doit rester sur place ({depart:.0} → {temoin:.0} têtes) — \
             sinon le test ne mesure pas la chasse mais la dérive des troupeaux"
        );
        assert!(
            tue_surchasse > 20.0,
            "les chasseurs doivent prélever du gibier ({tue_surchasse:.0} têtes)"
        );
        assert!(
            surchasse < temoin * 0.2,
            "effondrement local attendu : {surchasse:.0} têtes à 3 km sous 60 chasseurs, \
             contre {temoin:.0} sans (départ {depart:.0})"
        );
    }

    #[test]
    fn la_chasse_nourrit_mieux_que_la_cueillette() {
        // Un chasseur au contact du gibier doit rassasier, et le troupeau
        // doit le payer : c'est le couplage « chassent quand ils ont faim ».
        let (mut sim, _) = scenario_setup(42, 20, 3);
        for (_, phys) in sim.agents.query_mut::<&mut Physiology>() {
            phys.hunger = 0.9; // affamés : la chasse doit dominer
        }
        for _ in 0..24 * 3 {
            sim.step();
        }
        assert!(sim.hunted_head > 0.0, "des affamés près du gibier doivent chasser");
        let faim: f32 = sim
            .agents
            .query::<&Physiology>()
            .iter()
            .map(|(_, p)| p.hunger)
            .sum::<f32>()
            / sim.population().max(1) as f32;
        assert!(faim < 0.8, "la chasse doit faire retomber la faim (moyenne {faim:.2})");
    }

    #[test]
    fn sans_pression_le_gibier_se_maintient() {
        // Le pendant du test de surchasse : sans chasseurs ni prédateurs, un
        // troupeau sur sa pâture ne doit ni exploser ni s'éteindre. Si ce
        // test casse, l'écologie est déréglée, pas la chasse.
        let (mut sim, _) = scenario_setup(42, 0, 2);
        let depart = sim.fauna_census().0;
        for _ in 0..24 * 60 {
            sim.step();
        }
        let (fin, _, groupes, _) = sim.fauna_census();
        assert!(groupes > 0, "le gibier ne doit pas disparaître tout seul");
        assert!(
            fin > depart * 0.5 && fin < depart * 4.0,
            "cheptel hors bornes : {depart:.0} → {fin:.0} têtes en 2 mois"
        );
    }

    /// Un troupeau tranquille doit **errer** autour de sa pâture, pas filer
    /// en ligne droite. Garde-fou contre une régression mesurée : en
    /// départageant les pâtures équivalentes par l'ordre de la liste
    /// d'échantillonnage, tous les troupeaux marchaient plein est à
    /// 1,9 km/jour — 38 km en trois semaines, et un LRU en miettes.
    #[test]
    fn un_troupeau_erre_au_lieu_de_marcher_droit() {
        let (mut sim, home) = scenario_setup(42, 0, 3);
        let jours = 15.0;
        for _ in 0..(24.0 * jours) as u32 {
            sim.step();
        }
        let mut max_km: f64 = 0.0;
        for (_, (_, pos)) in sim.fauna.query::<(&Herd, &Position)>().iter() {
            let d = (pos.x - home.0 as f64).hypot(pos.y - home.1 as f64);
            max_km = max_km.max(cairn_core::tiles_to_km(d));
        }
        // Une dérive balistique à la vitesse de pâture ferait ~29 km ; une
        // marche diffusive reste très en deçà.
        assert!(
            max_km < 8.0,
            "dérive balistique suspectée : un troupeau à {max_km:.1} km du départ en {jours:.0} jours"
        );
    }

    #[test]
    fn deux_seeds_divergent() {
        let a = scenario(42, 30, 24 * 4);
        let b = scenario(43, 30, 24 * 4);
        assert!(a.population() > 0, "extinction : scénario invalide pour le test");
        assert_ne!(fingerprint(&a), fingerprint(&b));
    }

    #[test]
    fn le_temps_avance_et_les_agents_bougent() {
        let mut sim = scenario(42, 10, 0);
        let avant: Vec<_> = fingerprint(&sim).iter().map(|f| (f.1, f.2)).collect();
        for _ in 0..48 {
            sim.step();
        }
        assert_eq!(sim.time.tick, 48);
        let apres: Vec<_> = fingerprint(&sim).iter().map(|f| (f.1, f.2)).collect();
        assert_ne!(avant, apres, "aucun agent n'a bougé en 48 h");
    }
}
