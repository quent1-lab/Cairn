//! La maladie : un hôte contre un pathogène (chantier de la mortalité de fond,
//! 2026-10-05).
//!
//! Le principe, décidé avec l'utilisateur : **chaque élément de la vie qui
//! peut rendre malade porte un risque, qui tue ou non**. Aucun taux de
//! mortalité n'est écrit ici. Une infection naît d'une exposition (boire à
//! une eau souillée, dormir près d'un malade) ; elle a une **virulence**
//! tirée à l'infection ; l'hôte la combat avec une **clairance** qui dépend de
//! son état. Si la virulence l'emporte, la gravité monte et ronge la santé
//! jusqu'à la mort ; sinon elle décroît et l'hôte guérit, plus aguerri.
//!
//! Ce qui en émerge, sans être écrit : la mortalité infantile (immunité
//! immature, aucun épisode vécu), la protection du lait maternel (l'orphelin
//! meurt plus), la synergie malnutrition-infection (l'affamé se défend mal),
//! la souillure des eaux autour des camps (plus on est nombreux près d'une
//! source, plus on y tombe malade).
//!
//! Trois voies, parce qu'elles n'ont ni les mêmes sources ni les mêmes
//! remèdes futurs : **digestive** (eau, contact), **respiratoire** (portage,
//! contagion), **plaie** (infection d'une blessure). Les remèdes, l'hygiène,
//! l'immunisation viendront des techniques (vision de l'utilisateur) : ils
//! agiront sur l'exposition, l'immunité ou la clairance — jamais sur une
//! létalité, qui n'existe pas comme paramètre.
//!
//! Vieillesse : pas d'immunosénescence ici, la loi de Gompertz de
//! `demography` la contient déjà.

use cairn_core::{Pcg32, TICKS_PER_DAY, km_to_tiles, splitmix64};

use crate::agent::{AgentId, Position};
use crate::salt;
use crate::sim::Sim;

/// Les voies d'infection : indices des tableaux de [`Illness`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Gut = 0,
    Lung = 1,
    Wound = 2,
}

pub const ROUTES: usize = 3;

/// Gravité d'une infection à son début.
const ONSET_SEVERITY: f32 = 0.1;
/// Sous cette gravité, l'infection est vaincue.
const CURED_SEVERITY: f32 = 0.01;
/// Atteinte de la santé par heure à gravité pleine : la mort en ~2 jours,
/// l'ordre de la déshydratation d'une diarrhée grave.
pub const DISEASE_DAMAGE: f32 = 1.0 / 48.0;
/// Clairance d'un hôte pleinement défendu, par jour (gravité ÷ e chaque jour
/// net de la virulence).
const CLEARANCE_PER_DAY: f32 = 1.0;
/// Virulence par jour : log-normale de médiane `VIRULENCE_MEDIAN` et d'écart
/// logarithmique `VIRULENCE_SIGMA`. Choix (ordre de grandeur) : un épisode
/// chez un adulte aguerri et nourri tue de l'ordre d'une fois sur mille
/// (P(r > 1) ≈ 0,2 %) — la virulence est une propriété de la nature, la
/// mortalité par âge en découle.
const VIRULENCE_MEDIAN: f32 = 0.165;
const VIRULENCE_SIGMA: f32 = 0.6;
/// Après guérison, la voie est fermée à une réinfection (convalescence,
/// immunité de court terme).
const IMMUNE_DAYS: u64 = 30;
/// Épisodes déjà vécus par un fondateur adulte : il a un passé.
const FOUNDER_EPISODES: u8 = 10;

/// Digestive, par gorgée : souillure de fond (faune) et charge par humain
/// présent à `SOIL_RADIUS` de l'eau. Choix : une source de camp de trente
/// personnes rend malade un enfant quelques fois par an (ordre de grandeur
/// des épisodes diarrhéiques en milieu sans assainissement).
const WATER_BACKGROUND_P: f64 = 0.002;
const WATER_PER_HUMAN_P: f64 = 0.0002;
pub const SOIL_RADIUS_TILES: f64 = km_to_tiles(0.1);
/// Contagion de nuit : distance d'un dormeur à un malade, chance par nuit et
/// par malade (voie digestive : mains, nourriture partagée).
const CONTACT_RADIUS_TILES: f64 = km_to_tiles(0.01);
const GUT_CONTACT_P: f64 = 0.05;

/// Une infection en cours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Infection {
    pub severity: f32,
    /// Croissance du pathogène, par jour.
    pub virulence: f32,
}

/// L'état infectieux d'un humain.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Illness {
    pub active: [Option<Infection>; ROUTES],
    /// Épisodes surmontés par voie : l'immunité acquise.
    pub episodes: [u8; ROUTES],
    /// Tick avant lequel la voie ne peut être réinfectée.
    pub immune_until: [u64; ROUTES],
    /// Allaité cette heure : les anticorps du lait (posé par
    /// `demography::nurse_infants`, consommé par la physiologie).
    pub milk: bool,
}

impl Illness {
    /// Un fondateur adulte, aguerri par sa vie d'avant.
    pub fn founder() -> Self {
        Self { episodes: [FOUNDER_EPISODES; ROUTES], ..Self::default() }
    }

    pub fn is_sick(&self, route: Route) -> bool {
        self.active[route as usize].is_some()
    }

    /// Une exposition a réussi : l'infection commence, sauf si la voie est
    /// déjà prise ou en convalescence. Renvoie vrai si elle commence.
    pub fn infect(&mut self, route: Route, tick: u64, rng: &mut Pcg32) -> bool {
        let r = route as usize;
        if self.active[r].is_some() || tick < self.immune_until[r] {
            return false;
        }
        let virulence = VIRULENCE_MEDIAN * (VIRULENCE_SIGMA * gaussian(rng)).exp();
        self.active[r] = Some(Infection { severity: ONSET_SEVERITY, virulence });
        true
    }

    /// Une heure de lutte. `defense` est l'immunité hors expérience (voir
    /// [`defense`]) ; l'expérience de chaque voie s'y ajoute. Renvoie
    /// l'atteinte à la santé de l'heure.
    pub fn course(&mut self, defense: f32, tick: u64) -> f32 {
        let mut damage = 0.0;
        for r in 0..ROUTES {
            let Some(inf) = self.active[r].as_mut() else { continue };
            let imm = defense * experience(self.episodes[r]);
            let net = (inf.virulence - CLEARANCE_PER_DAY * imm) / TICKS_PER_DAY as f32;
            inf.severity = (inf.severity * net.exp()).min(1.0);
            damage += inf.severity * DISEASE_DAMAGE;
            if inf.severity < CURED_SEVERITY {
                self.active[r] = None;
                self.episodes[r] = self.episodes[r].saturating_add(1);
                self.immune_until[r] = tick + IMMUNE_DAYS * TICKS_PER_DAY;
            }
        }
        damage
    }
}

/// Immunité acquise : un hôte naïf ne se défend qu'à 60 %, chaque épisode
/// surmonté comble 30 % de l'écart.
pub fn experience(episodes: u8) -> f32 {
    1.0 - 0.4 * 0.7f32.powi(i32::from(episodes))
}

/// La défense du corps, hors expérience : maturité (0,4 à la naissance, 1 à
/// 5 ans ; plancher 0,8 sous les anticorps du lait), nutrition, chaleur.
pub fn defense(age_years: f64, milk: bool, hunger: f32, cold: f32) -> f32 {
    let mut maturity = (0.4 + 0.6 * (age_years / 5.0).min(1.0)) as f32;
    if milk {
        maturity = maturity.max(0.8);
    }
    maturity * (1.0 - 0.5 * hunger) * (1.0 - 0.3 * cold)
}

/// Probabilité qu'une gorgée rende malade, selon les humains présents autour
/// de l'eau (la souillure d'un camp).
pub fn water_risk(humans_near: usize) -> f64 {
    WATER_BACKGROUND_P + WATER_PER_HUMAN_P * humans_near as f64
}

/// Écart normal centré réduit (Box-Muller).
fn gaussian(rng: &mut Pcg32) -> f32 {
    let u1 = rng.next_f64().max(f64::MIN_POSITIVE);
    let u2 = rng.next_f64();
    ((-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()) as f32
}

/// La contagion de la nuit, à minuit : chaque dormeur sain s'expose aux
/// malades couchés près de lui.
pub(crate) fn daily(sim: &mut Sim) {
    let tick = sim.time.tick;
    let seed = sim.world.seed();
    let sick: Vec<(f64, f64)> = sim
        .agents
        .query::<(&Position, &Illness)>()
        .iter()
        .filter(|(_, (_, ill))| ill.is_sick(Route::Gut))
        .map(|(_, (pos, _))| (pos.x, pos.y))
        .collect();
    if sick.is_empty() {
        return;
    }
    let r2 = CONTACT_RADIUS_TILES * CONTACT_RADIUS_TILES;
    for (_, (id, pos, ill)) in sim.agents.query_mut::<(&AgentId, &Position, &mut Illness)>() {
        if ill.is_sick(Route::Gut) {
            continue;
        }
        let k = sick.iter().filter(|(x, y)| (pos.x - x).powi(2) + (pos.y - y).powi(2) <= r2).count();
        if k == 0 {
            continue;
        }
        let p = 1.0 - (1.0 - GUT_CONTACT_P).powi(k as i32);
        let mut rng = Pcg32::new(seed.derive(salt::CONTAGION) ^ splitmix64(tick), id.0);
        if rng.next_f64() < p {
            ill.infect(Route::Gut, tick, &mut rng);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rng(stream: u64) -> Pcg32 {
        Pcg32::new(7, stream)
    }

    /// Létalité d'un épisode, mesurée en rejouant la lutte seule (sans
    /// régénération ni besoins) jusqu'à guérison ou santé nulle.
    fn lethality(defense: f32, episodes: u8, trials: u64) -> f64 {
        let mut deaths = 0;
        for k in 0..trials {
            let mut ill = Illness { episodes: [episodes; ROUTES], ..Illness::default() };
            ill.infect(Route::Gut, 0, &mut rng(k));
            let mut health = 1.0f32;
            for t in 0..(120 * TICKS_PER_DAY) {
                health -= ill.course(defense, t);
                if health <= 0.0 {
                    deaths += 1;
                    break;
                }
                if !ill.is_sick(Route::Gut) {
                    break;
                }
            }
        }
        deaths as f64 / trials as f64
    }

    #[test]
    fn la_letalite_emerge_de_la_defense() {
        let adult = lethality(defense(30.0, false, 0.1, 0.0), FOUNDER_EPISODES, 4000);
        let fed_infant = lethality(defense(0.5, true, 0.1, 0.0), 0, 4000);
        let orphan = lethality(defense(0.5, false, 0.1, 0.0), 0, 4000);
        let starving_child = lethality(defense(6.0, false, 0.9, 0.0), 2, 4000);
        let fed_child = lethality(defense(6.0, false, 0.1, 0.0), 2, 4000);
        assert!(adult < 0.01, "un adulte aguerri survit presque toujours : {adult}");
        assert!(orphan > fed_infant, "le lait protège : {orphan} vs {fed_infant}");
        assert!(starving_child > 2.0 * fed_child, "la faim tue par l'infection : {starving_child} vs {fed_child}");
    }

    #[test]
    fn un_episode_surmonte_aguerrit_et_protege_un_temps() {
        let mut ill = Illness::default();
        let mut r = rng(1);
        // Une virulence faible : on force la guérison.
        assert!(ill.infect(Route::Gut, 0, &mut r));
        ill.active[0].as_mut().unwrap().virulence = 0.0;
        let mut t = 0;
        while ill.is_sick(Route::Gut) {
            ill.course(1.0, t);
            t += 1;
        }
        assert_eq!(ill.episodes[0], 1);
        assert!(!ill.infect(Route::Gut, t, &mut r), "convalescence : pas de réinfection immédiate");
        assert!(ill.infect(Route::Gut, t + IMMUNE_DAYS * TICKS_PER_DAY, &mut r));
        assert!(experience(1) > experience(0));
    }
}
