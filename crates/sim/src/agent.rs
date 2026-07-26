//! L'agent et sa physiologie (BRIEF §3.1) : faim, soif, fatigue, froid,
//! santé, mort. Les autres blocs du brief vivent dans leurs propres modules —
//! traits et démographie dans `demography`, mémoire spatiale dans `memory`,
//! compétences dans `skills` — pour que chacun reste lisible seul.
//!
//! Chaque agent est une entité `hecs` composée de petits **composants** :
//! son identité, sa position, sa physiologie, son comportement. Les systèmes
//! de la boucle requêtent les combinaisons dont ils ont besoin.
//!
//! Toutes les constantes sont **par tick, donc par heure de jeu** — c'est ce
//! qui les rend lisibles : « la soif sature en 24 h », « on meurt en 2 jours
//! de soif maximale ».

use cairn_core::km_to_tiles;

/// Identifiant stable et monotone, indépendant des identifiants internes de
/// `hecs` (qui recyclent les slots). C'est lui qui nourrit les flux RNG par
/// agent — le déterminisme ne doit pas dépendre des détails de l'ECS — et,
/// plus tard, la Chronique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AgentId(pub u64);

/// Position **continue** en coordonnées de tuiles (1 tuile = 2 m). Continue :
/// à 2 000 tuiles/h de marche, des positions entières feraient des sauts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

impl Position {
    /// La tuile sous les pieds de l'agent.
    pub fn tile(&self) -> (i64, i64) {
        (self.x.floor() as i64, self.y.floor() as i64)
    }

    pub fn distance_tiles(&self, to: (i64, i64)) -> f64 {
        let dx = to.0 as f64 + 0.5 - self.x;
        let dy = to.1 as f64 + 0.5 - self.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// Vitesse de marche : 4 km/h en terrain sauvage.
pub const WALK_TILES_PER_TICK: f64 = km_to_tiles(4.0);

// — Rythmes physiologiques, par tick (= par heure) —

/// La faim sature en 2 jours sans manger.
pub const HUNGER_PER_TICK: f32 = 1.0 / 48.0;
/// La soif sature en 24 h sans boire.
pub const THIRST_PER_TICK: f32 = 1.0 / 24.0;
/// La fatigue sature après 16 h d'éveil…
pub const FATIGUE_AWAKE_PER_TICK: f32 = 1.0 / 16.0;
/// …et se résorbe en 8 h de sommeil.
pub const FATIGUE_SLEEP_RECOVERY: f32 = 1.0 / 8.0;

/// En dessous de cette température ressentie, le froid mord.
pub const COLD_THRESHOLD_C: f64 = 0.0;
/// Abri comportemental (se blottir à couvert) : gain de température ressentie.
pub const SHELTER_BONUS_C: f64 = 8.0;
/// Abri passif d'un biome forestier (coupe-vent, canopée).
pub const FOREST_BONUS_C: f64 = 4.0;

// — Érosion de la santé quand un besoin est à saturation, par tick —

/// Mort de soif en ~2 jours après saturation (≈ 3 jours sans boire).
pub const DAMAGE_DEHYDRATION: f32 = 1.0 / 48.0;
/// Mort de faim en ~2 semaines après saturation (≈ 16 jours sans manger).
pub const DAMAGE_STARVATION: f32 = 1.0 / 336.0;
/// Mort de froid en ~1 jour de stress thermique maximal.
pub const DAMAGE_HYPOTHERMIA: f32 = 1.0 / 24.0;
/// Récupération quand tous les besoins sont contenus : ~10 jours pour
/// remonter de mourant à plein.
pub const HEALTH_REGEN: f32 = 1.0 / 240.0;

/// Ce que l'agent fait de son heure. Posé par l'exécution des tâches, lu par
/// la dérive physiologique (dormir récupère, s'abriter réchauffe).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Activity {
    #[default]
    Idle,
    Walking,
    Eating,
    Drinking,
    Sleeping,
    Sheltering,
    Hunting,
}

/// Tâche persistante de moyen terme (BRIEF §4 : on ne re-délibère pas à
/// chaque tick, sinon les agents papillonnent et rien ne s'achève).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Task {
    pub kind: TaskKind,
    /// Tuile visée. Pour les tâches sur place, la tuile courante.
    pub target: (i64, i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    Drink,
    Forage,
    /// Chasser un troupeau : bien plus nourrissant que la cueillette, mais il
    /// faut rejoindre du gibier qui fuit. Réservé aux adultes.
    Hunt,
    Sleep,
    Shelter,
    Wander,
    /// Un enfant rejoint son parent : la cible est la dernière position
    /// connue du parent — re-visée à chaque délibération, ce qui suffit à le
    /// suivre sans machinerie de cible mobile.
    Follow,
    /// Rejoindre le plus proche congénère quand on est isolé. C'est le drive
    /// « appartenir » réduit à sa plus simple expression : sans lui, une
    /// population sans clans diffuse jusqu'à ce que plus personne ne se
    /// croise — et sans rencontres, ni reproduction ni transmission.
    Socialize,
    /// Marcher vers une cellule jamais visitée : le drive « comprendre »,
    /// réservé aux curieux dont les besoins sont contenus (voir `brain`).
    Explore,
    /// Rejoindre le territoire de son clan (voir `crate::social::Clan::home`)
    /// quand on s'en est trop éloigné. La rétroaction qui donne un sens à la
    /// co-résidence : sans elle, la population diffuse sans jamais revenir
    /// (voir le commentaire de module de `social`).
    ReturnToClan,
    /// Puiser dans le stock commun du clan (`crate::social::Clan::stock`)
    /// quand rien de local ne répond à la faim. La cible est le foyer du
    /// clan : le stock se puise sur place, il ne se livre pas.
    EatFromStock,
    /// Rapporter au foyer le surplus d'une chasse (`Carrying`) pour qu'il
    /// rejoigne le stock commun. Symétrique d'`EatFromStock` : le dépôt, comme
    /// le retrait, exige d'être sur place — la viande ne se téléporte pas.
    BringSurplusHome,
    /// Bâtir, au foyer du clan, la structure que le clan désire
    /// (`crate::structures`). Le type est porté par la tâche : un membre
    /// s'engage sur *cette* construction précise. Payée par le stock commun,
    /// exige d'être sur place — un chantier ne se mène pas à distance.
    Build(crate::structures::StructureKind),
    /// Marcher vers l'étape courante d'une expédition (`crate::commerce`) :
    /// l'étain lointain à l'aller, le foyer au retour. Assignée directement par
    /// `sim::step` à un envoyé, hors délibération normale (tant qu'aucun besoin
    /// vital ne presse) — c'est ce qui la fait *résister au rappel du clan* le
    /// temps d'aller au bout de la route.
    Expedition,
}

/// Le composant « comportement » : la tâche en cours et l'activité de l'heure.
#[derive(Debug, Clone, Copy, Default)]
pub struct Behavior {
    pub task: Option<Task>,
    pub activity: Activity,
}

/// Surplus de chasse porté vers le foyer du clan, en attente de dépôt
/// (`TaskKind::BringSurplusHome`) — mêmes unités que `Physiology::hunger`.
/// **Perdu si l'agent meurt en chemin**, ou change durablement de priorité
/// sans jamais rentrer : la viande crue livrée à elle-même ne se conserve
/// pas, et rien ne force le retour — c'est juste un candidat de plus parmi
/// d'autres à la délibération (voir `brain::decide`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Carrying(pub f32);

/// Prestige accumulé, à vie — jamais plafonné, jamais relâché (BRIEF §5.1,
/// Phase 4 « le chef »). Un seul déclencheur pour l'instant : ce qu'un agent
/// a effectivement rapporté à son clan (`TaskKind::BringSurplusHome`, même
/// montant que le stock reçoit — voir `sim::execute`). Un compagnon qui a
/// nourri le groupe pendant des années garde son ascendant même le jour où
/// il chasse moins bien qu'un jeune loup : c'est voulu, pas un oubli de
/// décroissance — voir `social::elect_chiefs`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Prestige(pub f32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeathCause {
    Starvation,
    Dehydration,
    Hypothermia,
    /// Sénescence (tirage quotidien de Gompertz, voir `demography`).
    OldAge,
}

/// Les besoins vitaux, tous dans [0, 1] : 0 = comblé, 1 = critique.
/// La santé, elle, va de 1 (plein) à 0 (mort).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Physiology {
    pub hunger: f32,
    pub thirst: f32,
    pub fatigue: f32,
    /// Stress thermique accumulé (hypothermie progressive).
    pub cold: f32,
    pub health: f32,
    /// Le besoin qui a rongé la santé en dernier — le futur « cause du
    /// décès » de la Chronique.
    pub last_damage: Option<DeathCause>,
}

impl Default for Physiology {
    fn default() -> Self {
        Self {
            hunger: 0.3,
            thirst: 0.3,
            fatigue: 0.2,
            cold: 0.0,
            health: 1.0,
            last_damage: None,
        }
    }
}

impl Physiology {
    /// Une heure de dérive passive : les besoins montent, le corps subit la
    /// température **ressentie** `felt_c` (air + abris), la santé s'érode ou
    /// se répare. Pure — c'est ce qui la rend testable sans monde.
    ///
    /// `endurance` est le trait hérité (0–1) : un corps endurant fatigue
    /// moins vite (×0,75 à endurance 1, ×1,25 à endurance 0).
    pub fn drift(&mut self, felt_c: f64, activity: Activity, endurance: f32) {
        self.hunger = (self.hunger + HUNGER_PER_TICK).min(1.0);
        self.thirst = (self.thirst + THIRST_PER_TICK).min(1.0);

        if activity == Activity::Sleeping {
            self.fatigue = (self.fatigue - FATIGUE_SLEEP_RECOVERY).max(0.0);
        } else {
            let rate = FATIGUE_AWAKE_PER_TICK * (1.25 - 0.5 * endurance);
            self.fatigue = (self.fatigue + rate).min(1.0);
        }

        // Froid : accumulation proportionnelle au déficit (à -20 °C ressenti,
        // saturation en 12 h) ; récupération plus rapide au chaud.
        if felt_c < COLD_THRESHOLD_C {
            let deficit = COLD_THRESHOLD_C - felt_c;
            self.cold = (self.cold + (deficit / 20.0 / 12.0) as f32).min(1.0);
        } else {
            let warmth = (felt_c - COLD_THRESHOLD_C) / 15.0;
            self.cold = (self.cold - (warmth.min(1.0) / 6.0) as f32).max(0.0);
        }

        // Érosion : chaque besoin saturé mord ; on retient le plus mordant
        // comme cause dominante.
        let mut worst: Option<(f32, DeathCause)> = None;
        let mut bite = |damage: f32, cause: DeathCause, worst: &mut Option<(f32, DeathCause)>| {
            self.health -= damage;
            if worst.is_none_or(|(w, _)| damage > w) {
                *worst = Some((damage, cause));
            }
        };
        if self.thirst >= 1.0 {
            bite(DAMAGE_DEHYDRATION, DeathCause::Dehydration, &mut worst);
        }
        if self.hunger >= 1.0 {
            bite(DAMAGE_STARVATION, DeathCause::Starvation, &mut worst);
        }
        if self.cold >= 1.0 {
            bite(DAMAGE_HYPOTHERMIA, DeathCause::Hypothermia, &mut worst);
        }
        if let Some((_, cause)) = worst {
            self.last_damage = Some(cause);
        } else if self.hunger < 0.8 && self.thirst < 0.8 && self.cold < 0.8 {
            self.health = (self.health + HEALTH_REGEN).min(1.0);
        }
        self.health = self.health.max(0.0);
    }

    pub fn is_dead(&self) -> bool {
        self.health <= 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sans_boire_on_meurt_en_trois_jours_environ() {
        let mut p = Physiology { thirst: 0.0, ..Default::default() };
        let mut heures = 0;
        while !p.is_dead() && heures < 24 * 10 {
            // Climat doux, repos : seule la soif tue.
            p.drift(15.0, Activity::Idle, 0.5);
            p.hunger = 0.5; // nourri de force : isole la soif
            heures += 1;
        }
        assert!(
            (2 * 24..4 * 24).contains(&heures),
            "mort de soif en {heures} h, attendu ~72 h"
        );
        assert_eq!(p.last_damage, Some(DeathCause::Dehydration));
    }

    #[test]
    fn le_froid_extreme_tue_plus_vite_que_la_faim() {
        let mut gele = Physiology::default();
        let mut heures_froid = 0;
        while !gele.is_dead() && heures_froid < 24 * 30 {
            gele.drift(-25.0, Activity::Idle, 0.5);
            gele.thirst = 0.5;
            gele.hunger = 0.5;
            heures_froid += 1;
        }
        assert!(
            heures_froid < 3 * 24,
            "à -25 °C on doit mourir en moins de 3 jours ({heures_froid} h)"
        );
        assert_eq!(gele.last_damage, Some(DeathCause::Hypothermia));
    }

    #[test]
    fn bien_pourvu_on_recupere() {
        let mut p = Physiology { health: 0.5, ..Default::default() };
        for _ in 0..24 {
            p.drift(15.0, Activity::Idle, 0.5);
            p.hunger = 0.2;
            p.thirst = 0.2;
        }
        assert!(p.health > 0.5, "la santé doit remonter au calme");
    }

    #[test]
    fn dormir_efface_la_fatigue() {
        let mut p = Physiology { fatigue: 0.9, ..Default::default() };
        for _ in 0..8 {
            p.drift(15.0, Activity::Sleeping, 0.5);
        }
        assert!(p.fatigue < 0.05);
    }

    #[test]
    fn l_abri_change_la_donne_par_grand_froid() {
        // À -10 °C d'air : à découvert on gèle, à couvert en forêt (+12 °C
        // ressenti) on tient. C'est le critère « s'abritent quand il fait
        // froid » rendu mesurable.
        let felt_expose = -10.0;
        let felt_abrite = -10.0 + SHELTER_BONUS_C + FOREST_BONUS_C;
        let mut expose = Physiology::default();
        let mut abrite = Physiology::default();
        for _ in 0..48 {
            expose.drift(felt_expose, Activity::Idle, 0.5);
            abrite.drift(felt_abrite, Activity::Sheltering, 0.5);
        }
        assert!(expose.cold > 0.15, "à découvert le froid s'accumule");
        assert_eq!(abrite.cold, 0.0, "abrité, aucun stress thermique");
    }
}
