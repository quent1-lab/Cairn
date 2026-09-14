//! **Le culte émergent** (BRIEF §6.3) — ce qu'un peuple croit de vous.
//!
//! > « Les clans construisent une religion à partir de ce que la divinité
//! > **fait réellement**. Si les miracles arrivent systématiquement après une
//! > famine → dieu nourricier → rites d'offrande. Si la divinité foudroie →
//! > dieu de colère → crainte, sacrifices. Si elle n'intervient jamais → dieu
//! > absent, ou athéisme, ou schisme. »
//!
//! ## Une théologie n'est pas un état, c'est une lecture
//!
//! Rien n'est stocké ici. Une [`Theology`] est l'**agrégat des croyances
//! individuelles** de membres vivants, calculé à la demande — patron exact de
//! `Sim::clan_corpus`, qui agrège de la même façon les savoirs. Les
//! conséquences sont les mêmes, et elles sont bonnes :
//!
//! - un peuple dont les fidèles meurent **perd sa religion** sans qu'aucun code
//!   ne l'efface : elle quitte l'agrégat d'elle-même ;
//! - deux clans voisins peuvent croire des choses opposées du **même** dieu,
//!   parce qu'ils n'ont pas vécu les mêmes gestes ;
//! - un clan qui se scinde emporte deux théologies distinctes, sans schisme
//!   scripté — c'est le §6.3 « ou schisme », obtenu par soustraction.
//!
//! ## Rien ne la lit encore
//!
//! Arbitrage acté au démarrage de la phase : **mesurer d'abord, rétroagir
//! après**. La théologie est ici dérivée, racontée et affichable ; elle ne
//! modifie aucune norme et aucune innovation. On veut d'abord voir *quelles*
//! théologies émergent réellement selon la conduite du joueur — la même méthode
//! qu'aux incréments 6 à 8 de la Phase 4, où le territoire, la tension et le
//! chef ont été mesurés bien avant d'agir sur quoi que ce soit.
//!
//! L'expérience du projet dit que c'est la bonne façon : trois hypothèses de
//! calibrage social ont déjà été réfutées par la mesure au chantier des clans.

use crate::faith::Faith;
use crate::sim::Sim;
use crate::social::{ClanId, ClanMembership};

/// Part des membres qu'il faut convaincre pour qu'on puisse parler d'une
/// religion de clan, et non de quelques illuminés. En deçà, le peuple n'a pas
/// de théologie — il a des croyants.
const FOLLOWING_FRACTION: f32 = 0.25;

/// Au-delà de cet écart moyen, un peuple penche franchement d'un côté : il ne
/// voit plus un dieu ambivalent mais un dieu qui donne, ou qui punit.
const CREED_TILT: f32 = 0.35;

/// Ce qu'un peuple croit de sa divinité, à cet instant. **Dérivé, jamais
/// stocké** — voir l'en-tête.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theology {
    /// Combien de ses membres croient à quelque chose.
    pub believers: usize,
    /// Combien ils sont en tout — c'est le rapport qui compte, pas le nombre.
    pub members: usize,
    /// Ferveur moyenne **des croyants** (et non du peuple entier : diluer la
    /// foi des fidèles dans la masse des indifférents dirait autre chose).
    pub fervor: f32,
    /// Le caractère prêté à la divinité, dans `[-1, 1]` : ce que ses fidèles
    /// ont vécu d'elle, en moyenne.
    pub benevolence: f32,
}

impl Theology {
    /// Le peuple a-t-il une religion, ou seulement quelques croyants ?
    pub fn is_shared(&self) -> bool {
        self.members > 0
            && self.believers as f32 >= FOLLOWING_FRACTION * self.members as f32
    }

    /// L'étiquette du §6.3 — **dérivée, et jamais une condition** : rien dans le
    /// code ne teste un `Creed` pour autoriser un comportement, exactement comme
    /// `tech::Age` ne gate aucune découverte.
    pub fn creed(&self) -> Creed {
        if !self.is_shared() {
            // « Si elle n'intervient jamais → dieu absent, ou athéisme. »
            return Creed::Absent;
        }
        if self.benevolence >= CREED_TILT {
            Creed::Nourishing
        } else if self.benevolence <= -CREED_TILT {
            Creed::Wrathful
        } else {
            // Un dieu qui a autant donné que pris. Le plus difficile à servir,
            // et le plus probable si le joueur agit sans ligne de conduite.
            Creed::Capricious
        }
    }
}

/// Ce qu'un peuple dit de son dieu. Étiquette d'affichage et de récit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Creed {
    /// Personne n'y croit assez pour qu'on en parle.
    Absent,
    /// Il donne : la santé, la pluie quand on a soif, le feu.
    Nourishing,
    /// Il frappe : la foudre, le mal, la sécheresse sur les affamés.
    Wrathful,
    /// Il fait les deux, et l'on ne sait jamais lequel viendra.
    Capricious,
}

impl Creed {
    pub fn label(self) -> &'static str {
        match self {
            Creed::Absent => "sans dieu",
            Creed::Nourishing => "un dieu nourricier",
            Creed::Wrathful => "un dieu de colère",
            Creed::Capricious => "un dieu capricieux",
        }
    }
}

/// Un lieu que la divinité a désigné (BRIEF §6.2, « le Signe »).
///
/// C'est la seule intervention **sans effet direct** : elle ne brûle rien, ne
/// soigne personne, ne fait pousser aucune plante. Elle n'agit que parce que
/// des hommes y croient — d'où son report jusqu'ici, faute de croyants.
///
/// Registre clairsemé côté `Sim`, comme les feux et la météo. Un lieu sacré ne
/// se dissipe pas : on n'oublie pas où le ciel s'est manifesté. Ce qui s'oublie,
/// c'est la **ferveur** de ceux qui s'en souvenaient — et un sanctuaire dont
/// plus personne n'a la foi n'attire plus rien, sans qu'on ait à l'effacer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shrine {
    pub pos: (f64, f64),
    /// Le tick où le signe a été donné : la Chronique le date, et le client
    /// peut dire depuis quand le lieu est tenu pour sacré.
    pub since: u64,
}

/// Rayon dans lequel un croyant ressent l'appel d'un lieu sacré (~6 km) : plus
/// large que le territoire d'un clan, sinon le Signe ne ferait jamais bouger
/// personne — il ne toucherait que ceux qui sont déjà là.
pub const SHRINE_PULL_TILES: f64 = 3000.0;

/// Ferveur en deçà de laquelle on ne se dérange pas pour un lieu saint.
pub const PILGRIM_FERVOR: f32 = 0.3;

impl Sim {
    /// Le lieu sacré le plus proche à portée d'appel, s'il en est un — et **s'il
    /// y a de quoi être appelé** (`fervor` suffisante). Lu par `brain::decide`.
    ///
    /// C'est ici que se joue l'ambivalence du Signe, sans qu'une ligne ne
    /// l'écrive : le sanctuaire attire les fidèles de **tous** les peuples qui y
    /// croient. Deux clans convergent donc au même endroit, leurs foyers se
    /// rapprochent — et la tension entre voisins, qui se mesure déjà à cette
    /// distance-là (`social::update_relations`), monte toute seule. « Les clans
    /// y bâtissent, s'y rassemblent… **et s'y battent**. »
    pub fn shrine_call(&self, from: (f64, f64), fervor: f32) -> Option<(f64, f64)> {
        if fervor < PILGRIM_FERVOR {
            return None;
        }
        self.shrines
            .iter()
            .map(|s| (s.pos, (from.0 - s.pos.0).hypot(from.1 - s.pos.1)))
            .filter(|&(_, d)| d <= SHRINE_PULL_TILES)
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(pos, _)| pos)
    }

    /// La théologie d'un clan : l'agrégat des croyances de ses membres vivants,
    /// **calculé à la demande**. C'est ce qui rend l'oubli automatique — une
    /// religion que plus personne ne porte quitte l'agrégat d'elle-même.
    pub fn clan_theology(&self, clan: ClanId) -> Theology {
        let (mut believers, mut members, mut fervor, mut benevolence) = (0usize, 0usize, 0.0, 0.0);
        for (_, (membership, faith)) in self.agents.query::<(&ClanMembership, &Faith)>().iter() {
            if membership.0 != Some(clan) {
                continue;
            }
            members += 1;
            if faith.believes() {
                believers += 1;
                fervor += faith.fervor;
                benevolence += faith.benevolence();
            }
        }
        if believers > 0 {
            fervor /= believers as f32;
            benevolence /= believers as f32;
        }
        Theology { believers, members, fervor, benevolence }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentId;
    use crate::social::ClanId;
    use cairn_core::WorldSeed;

    /// Installe `n` agents dans un clan, dont les `croyants` premiers ont vécu
    /// `boons` bienfaits et `banes` malheurs.
    fn scene(n: usize, croyants: usize, boons: u16, banes: u16) -> Sim {
        let mut sim = Sim::new(WorldSeed(1), 64);
        for _ in 0..n {
            sim.spawn_agent(0.0, 0.0);
        }
        for (i, (_, (_, membership, faith))) in sim
            .agents
            .query_mut::<(&AgentId, &mut ClanMembership, &mut Faith)>()
            .into_iter()
            .enumerate()
        {
            membership.0 = Some(ClanId(1));
            if i < croyants {
                faith.fervor = 0.8;
                faith.boons = boons;
                faith.banes = banes;
            }
        }
        sim
    }

    /// Le §6.3 mot pour mot : ce qu'un peuple croit se dérive de ce que la
    /// divinité lui a **fait**, et de rien d'autre.
    #[test]
    fn un_peuple_croit_ce_qu_il_a_vecu() {
        assert_eq!(scene(8, 8, 4, 0).clan_theology(ClanId(1)).creed(), Creed::Nourishing);
        assert_eq!(scene(8, 8, 0, 4).clan_theology(ClanId(1)).creed(), Creed::Wrathful);
        assert_eq!(scene(8, 8, 2, 2).clan_theology(ClanId(1)).creed(), Creed::Capricious);
        // Une divinité qui ne s'est jamais manifestée n'a pas de fidèles.
        assert_eq!(scene(8, 0, 0, 0).clan_theology(ClanId(1)).creed(), Creed::Absent);
    }

    /// Quelques illuminés ne font pas une religion : il faut qu'une part du
    /// peuple y croie pour qu'on puisse parler de sa théologie.
    #[test]
    fn quelques_croyants_ne_font_pas_une_religion() {
        let un_seul = scene(20, 1, 5, 0).clan_theology(ClanId(1));
        assert!(!un_seul.is_shared(), "1 sur 20 : un illuminé, pas un culte");
        assert_eq!(un_seul.creed(), Creed::Absent);

        let partage = scene(20, 8, 5, 0).clan_theology(ClanId(1));
        assert!(partage.is_shared(), "8 sur 20 : le peuple croit");
        assert_eq!(partage.creed(), Creed::Nourishing);
    }

    /// **L'oubli est automatique**, comme pour les savoirs : la théologie étant
    /// un agrégat calculé, elle disparaît quand ses porteurs cessent de croire —
    /// aucun code ne l'efface.
    #[test]
    fn une_religion_que_personne_ne_porte_plus_s_efface_d_elle_meme() {
        let mut sim = scene(10, 10, 3, 0);
        assert!(sim.clan_theology(ClanId(1)).is_shared());

        // La ferveur s'éteint (le ciel s'est tu assez longtemps).
        for (_, faith) in sim.agents.query_mut::<&mut Faith>() {
            faith.fervor = 0.0;
        }
        let apres = sim.clan_theology(ClanId(1));
        assert!(!apres.is_shared(), "sans fidèles, plus de religion");
        assert_eq!(apres.creed(), Creed::Absent);
        assert_eq!(apres.members, 10, "le peuple, lui, est toujours là");
    }

    /// Le Signe n'appelle que ceux qui croient, et pas au-delà de sa portée.
    #[test]
    fn un_lieu_sacre_n_appelle_que_les_fideles() {
        use crate::divine::Intervention;
        let mut sim = Sim::new(WorldSeed(1), 64);
        sim.invoke(Intervention::Sign { pos: (0, 0) }).unwrap();
        assert_eq!(sim.shrines.len(), 1, "le lieu est désigné");

        let pres = (100.0, 0.0);
        assert!(sim.shrine_call(pres, 0.8).is_some(), "un fervent entend l'appel");
        assert!(
            sim.shrine_call(pres, 0.05).is_none(),
            "un tiède ne se dérange pas pour un lieu saint"
        );
        assert!(
            sim.shrine_call((SHRINE_PULL_TILES + 100.0, 0.0), 0.9).is_none(),
            "et l'appel ne porte pas jusqu'au bout du monde"
        );
    }

    /// La délibération ne voit pas `Sim` : elle relit la même géométrie sur un
    /// instantané. Les deux lectures doivent s'accorder — une duplication est
    /// une occasion de divergence (même garde que pour le territoire).
    #[test]
    fn les_deux_lectures_du_lieu_sacre_s_accordent() {
        use crate::divine::Intervention;
        let mut sim = Sim::new(WorldSeed(1), 64);
        sim.invoke(Intervention::Sign { pos: (0, 0) }).unwrap();
        sim.invoke(Intervention::Sign { pos: (2000, 0) }).unwrap();
        let shrines = sim.shrines.clone();

        for from in [(0.0, 0.0), (900.0, 0.0), (1500.0, 0.0), (50_000.0, 0.0)] {
            for fervor in [0.0, 0.2, 0.5, 1.0] {
                assert_eq!(
                    sim.shrine_call(from, fervor),
                    crate::brain::nearest_shrine_for_test(&shrines, from, fervor),
                    "verdicts divergents depuis {from:?} à ferveur {fervor}"
                );
            }
        }
    }

    /// La ferveur moyenne se calcule **sur les croyants**, pas sur le peuple :
    /// diluer la foi des fidèles dans la masse des indifférents dirait autre
    /// chose que ce qu'on veut mesurer.
    #[test]
    fn la_ferveur_se_moyenne_sur_ceux_qui_croient() {
        let t = scene(10, 5, 2, 0).clan_theology(ClanId(1));
        assert_eq!(t.believers, 5);
        assert_eq!(t.members, 10);
        assert!((t.fervor - 0.8).abs() < 1e-5, "la ferveur des fidèles, pas la moyenne du clan");
    }
}
