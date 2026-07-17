//! La boucle de simulation à tick fixe (BRIEF §8.2).
//!
//! `Sim` réunit le monde de tuiles (chunké, mutable), la population d'agents
//! (entités `hecs`) et l'horloge. Chaque [`step`](Sim::step) déroule les
//! systèmes **dans un ordre fixe, écrit ici noir sur blanc** — c'est notre
//! scheduler, et c'est ce qui rend la simulation déterministe :
//!
//! 1. **Délibération** (bucketée) : les agents « dus » choisissent une tâche.
//! 2. **Exécution** : chacun avance sa tâche — marche, mange, boit, dort.
//! 3. **Physiologie** : les besoins dérivent, le climat mord, on meurt.
//! 4. **Écologie** (une fois par jour) : la biomasse consommée repousse.
//!
//! L'itération `hecs` est déterministe pour notre usage : tous les agents
//! partagent le même jeu de composants (une seule archétype), l'ordre est
//! l'ordre d'apparition, et les retraits (morts) sont eux-mêmes ordonnés.

use cairn_core::{SimTime, TICKS_PER_DAY, WorldSeed};

use crate::agent::{
    Activity, AgentId, Behavior, DeathCause, FOREST_BONUS_C, Physiology, Position,
    SHELTER_BONUS_C, TaskKind, WALK_TILES_PER_TICK,
};
use crate::brain::{self, DELIBERATION_PERIOD};
use crate::climate::Climate;
use crate::ecology;
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
    pub time: SimTime,
    pub deaths: Vec<DeathRecord>,
    next_agent_id: u64,
}

impl Sim {
    /// `chunk_capacity` : nombre de chunks résidents du LRU. Doit couvrir
    /// largement la zone active de la population (un chunk = 128 m de côté).
    pub fn new(seed: WorldSeed, chunk_capacity: usize) -> Self {
        let world = World::new(seed, chunk_capacity);
        let climate = Climate::new(world.worldgen().temperature.latitude());
        Self {
            world,
            climate,
            agents: hecs::World::new(),
            time: SimTime::default(),
            deaths: Vec::new(),
            next_agent_id: 0,
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

    pub fn population(&self) -> usize {
        self.agents.len() as usize
    }

    /// Un tick : une heure de monde.
    pub fn step(&mut self) {
        let time = self.time;

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
                    brain::decide(&mut self.world, time, *id, pos, phys, current);
            }
        }

        // 2. Exécution des tâches.
        for (_, (pos, phys, behavior)) in self
            .agents
            .query_mut::<(&mut Position, &mut Physiology, &mut Behavior)>()
        {
            execute(&mut self.world, pos, phys, behavior);
        }

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

        // 4. Écologie quotidienne, à minuit.
        if time.tick.is_multiple_of(TICKS_PER_DAY) {
            ecology::daily_regrowth(&mut self.world, &self.climate, time);
        }

        self.time.tick += 1;
    }
}

/// Avance la tâche courante d'un agent : marche vers la cible, puis agit.
fn execute(world: &mut World, pos: &mut Position, phys: &mut Physiology, behavior: &mut Behavior) {
    let Some(task) = behavior.task else {
        behavior.activity = Activity::Idle;
        return;
    };

    // Phase trajet : la cible est trop loin, on marche (budget d'une heure).
    if pos.distance_tiles(task.target) > ARRIVAL_TILES {
        behavior.activity = Activity::Walking;
        if !walk_towards(world, pos, task.target) {
            behavior.task = None; // bloqué (eau) : on re-délibérera
        }
        return;
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
    }
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

    /// Construit un essaim d'agents sur la terre ferme la plus proche de
    /// l'origine (qui peut être en mer !) — sondée via le worldgen, sans
    /// générer de chunks pour rien.
    fn scenario(seed: u64, agents: u32, ticks: u64) -> Sim {
        let mut sim = Sim::new(WorldSeed(seed), 512);
        // Les océans entre continents font des milliers de km : on sonde par
        // pas de 16 km, dans 8 directions, jusqu'à 6 400 km.
        let step = cairn_core::km_to_tiles(16.0) as i64;
        let mut home = None;
        'search: for r in 0..400i64 {
            let d = r * step;
            for &(x, y) in &[
                (d, 0), (-d, 0), (0, d), (0, -d),
                (d, d), (-d, d), (d, -d), (-d, -d),
            ] {
                // Marge d'élévation : franchement à l'intérieur des terres.
                if sim.world.worldgen().elevation(x, y) > 0.05 {
                    home = Some((x, y));
                    break 'search;
                }
            }
        }
        let home = home.expect("aucune terre à moins de 6 400 km de l'origine");
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
        for _ in 0..ticks {
            sim.step();
        }
        sim
    }

    #[test]
    fn deux_executions_identiques_bit_a_bit() {
        let a = scenario(42, 30, 24 * 8);
        let b = scenario(42, 30, 24 * 8);
        assert_eq!(fingerprint(&a), fingerprint(&b));
        assert_eq!(a.deaths.len(), b.deaths.len());
        assert_eq!(a.world.dirty_count(), b.world.dirty_count());
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
