//! **La Chronique** (BRIEF §6.4) — le journal narratif du monde.
//!
//! > *An 342 — Après trois hivers de disette, les Ashkar quittent la vallée de
//! > Karn. Sur le chemin, Vela, fille de Toru, découvre qu'on peut durcir la
//! > pointe d'un épieu en la passant au feu.*
//!
//! Le brief est catégorique : c'est « probablement le meilleur produit du jeu,
//! à traiter comme un système de première classe, **pas comme un log** ». La
//! distinction commande toute la conception de ce module.
//!
//! ## Un journal, pas un log
//!
//! Un log enregistre **tout** et laisse le lecteur trier. La Chronique
//! **choisit** : rien n'entre ici qui ne mérite d'être raconté. Les naissances
//! et les morts ordinaires restent donc où elles sont (`Sim::births`,
//! `Sim::deaths`) — ce sont des **statistiques**, elles alimentent les courbes
//! démographiques du client, pas le récit. N'entre dans la Chronique que ce
//! qui *fait date* : un clan qui naît ou s'éteint, un savoir découvert ou
//! perdu, un raid et ses morts, un chef qui tombe, une expédition au bout du
//! monde, un incendie, un premier cheptel.
//!
//! Conséquence pratique : le journal reste **petit**. Sur un monde de 500 ans,
//! il compte des milliers d'entrées, pas des millions — il tient en mémoire
//! sans stratégie d'éviction, et le jour où le serveur le persistera en SQLite
//! (BRIEF §8.4), il y aura peu à écrire.
//!
//! ## Un fait est figé
//!
//! Chaque [`Event`] porte **tout ce qu'il faut pour être raconté plus tard** :
//! le nom se dérive de l'identifiant, mais le **sexe**, l'**effectif**, la
//! **cause** sont copiés dans la variante au moment des faits. C'est
//! indispensable : la Chronique raconte au passé, et l'agent dont elle parle
//! est le plus souvent mort et retiré de l'ECS quand on la lit. On ne peut
//! plus aller lui demander son sexe — il fallait le noter.
//!
//! ## Séparation faits / rédaction
//!
//! [`Event`] est le **fait brut**, structuré et interrogeable (« liste tous les
//! clans ayant découvert le feu », §8.4) ; [`tell`] en est la **rédaction**
//! française. Les deux ne sont jamais mélangés : on ne stocke pas de phrases.
//! Le client peut ainsi filtrer, styler, traduire — et le serveur indexer.

use cairn_core::{SimTime, TICKS_PER_DAY, WorldSeed};

use crate::agent::{AgentId, DeathCause};
use crate::climate::Climate;
use crate::demography::Sex;
use crate::fauna::Species;
use crate::names;
use crate::sim::Sim;
use crate::social::ClanId;
use crate::tech::{TechId, TechTree};

/// Ancienneté qu'un clan doit atteindre pour entrer dans la Chronique — un mois
/// de jeu. Les clans se détectent (et se perdent) tous les jours par cohésion
/// et co-résidence : un groupe qui se forme le matin et se disperse le soir est
/// un **artefact de détection**, pas un peuple. Le chroniqueur attend donc de
/// pouvoir l'affirmer avant d'écrire « ils se reconnaissent comme un peuple »,
/// et il ne mentionne la fin que de ceux dont il avait annoncé la naissance.
///
/// Ce seuil ne touche **rien** dans la simulation : les clans, eux, se forment
/// et se dissolvent exactement comme avant. Il ne gouverne que le récit.
pub const CLAN_NOTABLE_DAYS: u64 = 30;

/// Un fait notable, daté et situé. `pos` est en tuiles : le client peut y
/// centrer la caméra d'un clic (« montrer sur la carte »), et la saison du
/// récit s'en déduit — un hiver n'a pas lieu au même moment aux deux
/// hémisphères (voir [`Climate::season`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Event {
    pub tick: u64,
    pub pos: (i64, i64),
    pub kind: EventKind,
}

/// Ce qui peut faire date. Chaque variante est autonome : elle porte les
/// attributs volatils (sexe, effectif, cause) que l'on ne pourra plus
/// retrouver au moment de la lecture — voir l'en-tête « un fait est figé ».
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EventKind {
    /// Un groupe a franchi les seuils de cohésion et de co-résidence : un clan
    /// existe (`social::detect_clans`).
    ClanFormed { clan: ClanId, members: usize },
    /// Un clan s'est éteint ou dispersé — il ne se reconnaît plus.
    ClanDissolved { clan: ClanId, members: usize },
    /// Un insight : quelqu'un a compris quelque chose (`tech::insight`).
    TechDiscovered { tech: TechId, agent: AgentId, sex: Sex, clan: Option<ClanId> },
    /// Le dernier porteur d'un savoir s'est éteint sans l'avoir transmis
    /// (`tech::forget`) — « l'humanité peut régresser » (§5.4).
    TechForgotten { tech: TechId },
    /// Un raid a eu lieu : la tension entre voisins s'est dénouée dans le sang
    /// (`combat::resolve_clashes`).
    Raid { attacker: ClanId, defender: ClanId, casualties: usize, plunder: f32 },
    /// Le chef d'un clan est mort. Un fait notable en soi : c'est une
    /// succession (`social::elect_chiefs` désignera un autre dès demain).
    ChiefFallen { clan: ClanId, agent: AgentId, sex: Sex, cause: DeathCause },
    /// La foudre — ou la friction — a mis le feu à la brousse (`fire::daily`).
    Wildfire,
    /// Un clan dépêche un envoyé chercher l'étain au loin (`commerce::dispatch`)
    /// : le voyage sans lequel il n'y a jamais de bronze.
    ExpeditionDeparted { clan: ClanId, agent: AgentId, sex: Sex, distance_km: f32 },
    /// L'envoyé est rentré, l'étain avec lui.
    ExpeditionReturned { clan: ClanId, agent: AgentId, sex: Sex },
    /// Un troupeau a fini par s'apprivoiser : le premier cheptel d'un clan
    /// (`pastoral::daily`).
    Domesticated { clan: ClanId, species: Species },
}

/// Consigne un raid en **consolidant** ceux du même jour entre les mêmes clans.
///
/// Un affrontement ne se joue pas en une heure : tant que des agresseurs sont au
/// contact, `combat::resolve_clashes` se rejoue à chaque tick. Journaliser
/// chacun donnerait dix lignes « Les Meibram tombent sur les Ibourn » d'affilée
/// — précisément le log que le §6.4 refuse. On cherche donc en arrière la
/// mention du jour et on y **ajoute** le bilan : le fait raconté est la journée
/// de combat, pas l'heure.
pub(crate) fn record_raid(
    sim: &mut Sim,
    pos: (i64, i64),
    attacker: ClanId,
    defender: ClanId,
    casualties: usize,
    plunder: f32,
) {
    let tick = sim.time.tick;
    // Fenêtre glissante de 24 h plutôt que jour calendaire : une mêlée à cheval
    // sur minuit reste un seul affrontement.
    let today = tick.saturating_sub(TICKS_PER_DAY);
    // Le journal est ordonné : on remonte tant qu'on est dans la fenêtre, et on
    // s'arrête dès qu'on en sort — la recherche est donc bornée, pas un balayage.
    for event in sim.chronicle.iter_mut().rev() {
        if event.tick < today {
            break;
        }
        if let EventKind::Raid { attacker: a, defender: d, casualties: c, plunder: p } =
            &mut event.kind
            && *a == attacker
            && *d == defender
        {
            *c += casualties;
            *p += plunder;
            return;
        }
    }
    sim.record(pos, EventKind::Raid { attacker, defender, casualties, plunder });
}

/// Rédige un fait en français, en une seule ligne : « *An 3, hiver — …* ».
/// Voir [`tell_parts`] quand la date et la phrase doivent être mises en forme
/// séparément (le panneau du client le fait).
pub fn tell(event: &Event, seed: WorldSeed, tree: &TechTree, climate: &Climate) -> String {
    let (when, what) = tell_parts(event, seed, tree, climate);
    format!("{when} — {what}")
}

/// Rédige un fait en **deux morceaux** : la date (« An 3, hiver ») et la phrase.
/// Fonction **pure** : mêmes entrées, même texte — on peut donc la rejouer, la
/// tester, et la déplacer côté client sans risque.
///
/// Les emprunts (`&TechTree`, `&Climate`) sont en lecture seule : rédiger ne
/// touche à rien. C'est ce qui permet à `Sim::chronicle_tail` de l'appeler
/// depuis un `&self`.
pub fn tell_parts(
    event: &Event,
    seed: WorldSeed,
    tree: &TechTree,
    climate: &Climate,
) -> (String, String) {
    let when = date(event.tick, event.pos.1, climate);
    let clan = |id: ClanId| names::clan_name(seed, id);
    let who = |id: AgentId, sex: Sex| names::agent_name(seed, id, sex);
    // La forme narrative porte son déterminant (« la maîtrise du feu ») : c'est
    // pour ça qu'elle vient des données et non du libellé d'affichage.
    let tech = |id: TechId| tree.narrated(id);

    let body = match event.kind {
        EventKind::ClanFormed { clan: c, members } => format!(
            "les {} se reconnaissent comme un peuple — ils sont {members}.",
            clan(c)
        ),
        EventKind::ClanDissolved { clan: c, members } => {
            if members == 0 {
                format!("les {} ne sont plus.", clan(c))
            } else {
                format!(
                    "les {} se dispersent ; leurs {members} derniers s'en vont chacun de leur côté.",
                    clan(c)
                )
            }
        }
        EventKind::TechDiscovered { tech: t, agent, sex, clan: c } => match c {
            Some(c) => format!("{}, des {}, découvre {}.", who(agent, sex), clan(c), tech(t)),
            None => format!("{}, sans clan, découvre {}.", who(agent, sex), tech(t)),
        },
        // « oublier » est transitif direct : la phrase reste juste quel que soit
        // le genre du savoir perdu, là où « ne sait plus faire… » boiterait.
        EventKind::TechForgotten { tech: t } => format!("le monde oublie {}.", tech(t)),
        EventKind::Raid { attacker, defender, casualties, plunder } => {
            let bilan = match casualties {
                0 => "Nul n'y laisse la vie".to_string(),
                1 => "Un mort".to_string(),
                n => format!("{n} morts"),
            };
            let butin = if plunder > 0.0 {
                format!(" Les {} emportent leurs vivres.", clan(attacker))
            } else {
                String::new()
            };
            format!(
                "les {} tombent sur les {}. {bilan}.{butin}",
                clan(attacker),
                clan(defender)
            )
        }
        EventKind::ChiefFallen { clan: c, agent, sex, cause } => format!(
            "{}, qui menait les {}, meurt {}.",
            who(agent, sex),
            clan(c),
            death_circumstance(cause)
        ),
        EventKind::Wildfire => "un feu prend dans la broussaille et court sur la plaine.".to_string(),
        EventKind::ExpeditionDeparted { clan: c, agent, sex, distance_km } => format!(
            "{} quitte les {} pour chercher l'étain, à {distance_km:.0} km de là.",
            who(agent, sex),
            clan(c)
        ),
        EventKind::ExpeditionReturned { clan: c, agent, sex } => format!(
            "{} revient chez les {}, l'étain avec {}.",
            who(agent, sex),
            clan(c),
            if sex == Sex::Female { "elle" } else { "lui" }
        ),
        EventKind::Domesticated { clan: c, species } => format!(
            "les {} ne chassent plus le {} : ils le gardent.",
            clan(c),
            species.label()
        ),
    };
    (when, capitalize_first(&body))
}

/// « An 12, hiver » — ou « An 12 » sous les tropiques, où la saison ne veut
/// rien dire (voir [`Climate::season`]).
fn date(tick: u64, y: i64, climate: &Climate) -> String {
    let time = SimTime { tick };
    match climate.season(y, time) {
        Some(s) => format!("An {}, {}", time.year(), s.label()),
        None => format!("An {}", time.year()),
    }
}

/// La circonstance d'une mort, dite comme on la raconte.
fn death_circumstance(cause: DeathCause) -> &'static str {
    match cause {
        DeathCause::Starvation => "de faim",
        DeathCause::Dehydration => "de soif",
        DeathCause::Hypothermia => "de froid",
        DeathCause::OldAge => "de vieillesse",
        DeathCause::Predation => "sous les crocs",
        DeathCause::Violence => "de la main d'un homme",
    }
}

/// Majuscule en tête de phrase (le corps est rédigé en minuscule pour se
/// composer après la date).
fn capitalize_first(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    if let Some(c) = chars.next() {
        out.extend(c.to_uppercase());
        out.push_str(chars.as_str());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::Sim;
    use cairn_core::TICKS_PER_YEAR;

    fn scribe() -> (Sim, WorldSeed) {
        let seed = WorldSeed(42);
        (Sim::new(seed, 64), seed)
    }

    /// Le rendu nomme vraiment les protagonistes : aucune phrase ne doit
    /// laisser fuiter un identifiant brut.
    #[test]
    fn le_recit_nomme_au_lieu_de_numeroter() {
        let (sim, seed) = scribe();
        let fire = sim.tech_tree.id_of("fire_mastery").expect("le feu est dans l'arbre");
        let event = Event {
            tick: 3 * TICKS_PER_YEAR,
            pos: (0, 0),
            kind: EventKind::TechDiscovered {
                tech: fire,
                agent: AgentId(7),
                sex: Sex::Female,
                clan: Some(ClanId(1)),
            },
        };
        let line = tell(&event, seed, &sim.tech_tree, &sim.climate);
        assert!(line.starts_with("An 3"), "la date ouvre le récit : {line}");
        assert!(line.contains(&names::agent_name(seed, AgentId(7), Sex::Female)));
        assert!(line.contains(&names::clan_name(seed, ClanId(1))));
        assert!(
            line.contains("découvre la maîtrise du feu"),
            "le savoir se dit avec son article, dans le fil de la phrase : {line}"
        );
        assert!(!line.contains('#'), "aucun identifiant brut : {line}");
    }

    /// Toute tech doit être racontable : le repli sur le libellé décapitalisé
    /// garantit qu'une tech ajoutée au RON sans forme narrative reste lisible.
    #[test]
    fn toute_tech_de_l_arbre_se_raconte() {
        let (sim, seed) = scribe();
        for tech in sim.tech_tree.iter() {
            let e = Event {
                tick: 0,
                pos: (0, 0),
                kind: EventKind::TechForgotten { tech: tech.id },
            };
            let line = tell(&e, seed, &sim.tech_tree, &sim.climate);
            let narrated = tech.narrated();
            assert!(line.contains(&narrated), "{} doit se raconter : {line}", tech.name);
            assert!(
                !narrated.starts_with(|c: char| c.is_uppercase()),
                "une majuscule en plein milieu de phrase : « {narrated} » ({})",
                tech.name
            );
        }
    }

    /// Le bilan d'un raid s'accorde — c'est le détail qui fait qu'on lit un
    /// récit et pas un tableau.
    #[test]
    fn le_bilan_d_un_raid_s_accorde() {
        let (sim, seed) = scribe();
        let raid = |casualties, plunder| {
            let e = Event {
                tick: 0,
                pos: (0, 0),
                kind: EventKind::Raid {
                    attacker: ClanId(1),
                    defender: ClanId(2),
                    casualties,
                    plunder,
                },
            };
            tell(&e, seed, &sim.tech_tree, &sim.climate)
        };
        assert!(raid(0, 0.0).contains("Nul n'y laisse la vie"));
        assert!(raid(1, 0.0).contains("Un mort"));
        assert!(raid(12, 0.0).contains("12 morts"));
        assert!(!raid(3, 0.0).contains("emportent"), "sans butin, on n'en parle pas");
        assert!(raid(3, 2.5).contains("emportent"), "avec butin, on le dit");
    }

    /// La consolidation des raids : un affrontement qui dure des heures ne fait
    /// **qu'un** fait, dont le bilan s'accumule. Le lendemain, c'en est un autre.
    ///
    /// Correctif d'un vrai défaut observé : la première version journalisait
    /// chaque tick de contact, ce qui donnait huit lignes « Les Meibram tombent
    /// sur les Ibourn » d'affilée dans un run d'un an.
    #[test]
    fn un_affrontement_qui_dure_ne_fait_qu_un_fait() {
        let (mut sim, _) = scribe();
        let (a, b) = (ClanId(1), ClanId(2));
        // Trois heures de mêlée dans la même journée.
        for _ in 0..3 {
            sim.time.tick += 1;
            record_raid(&mut sim, (0, 0), a, b, 1, 0.5);
        }
        assert_eq!(sim.chronicle.len(), 1, "une seule entrée pour la journée");
        assert_eq!(
            sim.chronicle[0].kind,
            EventKind::Raid { attacker: a, defender: b, casualties: 3, plunder: 1.5 },
            "le bilan s'accumule au lieu de se répéter"
        );

        // Le lendemain : un nouvel affrontement, une nouvelle entrée.
        sim.time.tick += TICKS_PER_DAY;
        record_raid(&mut sim, (0, 0), a, b, 2, 0.0);
        assert_eq!(sim.chronicle.len(), 2, "un autre jour, un autre fait");

        // Et un autre agresseur ne se fond pas dans le récit du premier.
        record_raid(&mut sim, (0, 0), b, a, 1, 0.0);
        assert_eq!(sim.chronicle.len(), 3, "la riposte des Ibourn est un fait distinct");
    }

    /// Un fait situé sous les tropiques n'invente pas de saison ; le même fait
    /// en latitude tempérée en porte une.
    #[test]
    fn la_date_ne_nomme_une_saison_que_la_ou_il_y_en_a() {
        let (sim, seed) = scribe();
        let at = |y| {
            let e = Event { tick: TICKS_PER_YEAR, pos: (0, y), kind: EventKind::Wildfire };
            tell(&e, seed, &sim.tech_tree, &sim.climate)
        };
        let tempere = (cairn_worldgen::DEFAULT_PLANET_PERIOD / 4.0) as i64;
        assert_eq!(at(0), "An 1 — Un feu prend dans la broussaille et court sur la plaine.");
        assert!(at(tempere).starts_with("An 1, hiver —"), "{}", at(tempere));
    }
}
