//! Les noms — ce qui fait qu'on lit « *Vela, fille de Toru, découvre qu'on peut
//! durcir la pointe d'un épieu* » plutôt que « agent #4127 → tech 3 ».
//!
//! Le BRIEF §6.4 écrit la Chronique avec des noms propres ; sans eux, le
//! « meilleur produit du jeu » reste un journal de débogage. Ce module en
//! fabrique.
//!
//! ## Dérivés, jamais stockés
//!
//! Un nom est une **fonction pure de (seed, identifiant)** — exactement comme
//! une tuile est une fonction pure de sa coordonnée dans le worldgen. Rien
//! n'est gardé en mémoire : ni composant `Name`, ni table. Trois conséquences,
//! toutes bonnes :
//!
//! - **zéro octet par agent** — on peut nommer 5 000 humains sans y penser ;
//! - **le nom survit à son porteur** : on peut encore nommer un mort, ce dont
//!   la Chronique a précisément besoin (elle raconte au passé) ;
//! - **déterminisme gratuit** : même seed ⇒ mêmes noms, bit pour bit.
//!
//! ## Fabrication
//!
//! Assemblage syllabique classique : une attaque (consonne ou digramme), un
//! noyau (voyelle), parfois une coda (consonne finale). Deux syllabes, parfois
//! trois. Les humains portent une marque de sexe **douce** — les noms féminins
//! se terminent sur le noyau, les masculins ferment souvent sur une coda — et
//! les clans un registre plus rude (coda quasi systématique, attaque initiale
//! parfois vide : *Ashkar*, *Orum*).
//!
//! Les collisions ne sont pas évitées : deux humains peuvent porter le même
//! nom, comme dans la vie. La Chronique lève l'ambiguïté par le clan.

use cairn_core::{Pcg32, WorldSeed};

use crate::agent::AgentId;
use crate::demography::Sex;
use crate::salt;
use crate::social::ClanId;

/// Attaques : consonnes simples et groupes prononçables. La répétition de
/// certaines n'est pas un oubli — elle pondère le tirage vers les sonorités
/// les plus lisibles.
const ONSETS: &[&str] = &[
    "b", "d", "g", "k", "m", "n", "r", "s", "t", "v", "z", "l", "m", "n", "r", "t", "th", "sh",
    "kh", "br", "dr", "gr", "kr", "tr", "vl", "sk", "nh",
];

/// Noyaux vocaliques.
const VOWELS: &[&str] = &["a", "e", "i", "o", "u", "a", "e", "i", "o", "ae", "ei", "ou"];

/// Codas : ce qui ferme une syllabe.
const CODAS: &[&str] = &["n", "r", "l", "s", "m", "k", "th", "rn", "sk", "kh"];

/// Le nom d'un humain. Le sexe n'entre que dans la **terminaison** : un même
/// identifiant donne la même racine, seule la fin diffère.
pub fn agent_name(seed: WorldSeed, id: AgentId, sex: Sex) -> String {
    // Flux propre à l'agent : `stream = id` sur la seed des noms. Deux agents
    // consécutifs ne se ressemblent donc pas (PCG décorrèle les flux).
    let mut rng = Pcg32::new(seed.derive(salt::NAMES), id.0);
    let syllables = if rng.next_u32() % 4 == 0 { 3 } else { 2 };
    let mut name = String::new();
    for _ in 0..syllables {
        name.push_str(pick(ONSETS, &mut rng));
        name.push_str(pick(VOWELS, &mut rng));
    }
    // Un homme ferme souvent sur une consonne, une femme jamais : la marque
    // reste une tendance lisible, pas une règle rigide (Toru comme Torun).
    if sex == Sex::Male && rng.next_f32() < 0.6 {
        name.push_str(pick(CODAS, &mut rng));
    }
    capitalize(&name)
}

/// Le nom d'un clan — sans article : la Chronique dit « les {nom} ».
pub fn clan_name(seed: WorldSeed, id: ClanId) -> String {
    // Décalage du flux (`id.0 ^ CLAN_STREAM_MIX`) : sans lui, le clan n° 3 et
    // l'humain n° 3 tireraient la même séquence et porteraient la même racine.
    let mut rng = Pcg32::new(seed.derive(salt::NAMES), id.0 ^ CLAN_STREAM_MIX);
    let mut name = String::new();
    // Une chance sur trois de commencer par une voyelle nue (Ashkar, Orum).
    if rng.next_u32() % 3 > 0 {
        name.push_str(pick(ONSETS, &mut rng));
    }
    name.push_str(pick(VOWELS, &mut rng));
    name.push_str(pick(ONSETS, &mut rng));
    name.push_str(pick(VOWELS, &mut rng));
    // Un clan ferme presque toujours : c'est ce qui lui donne son registre dur.
    if rng.next_f32() < 0.85 {
        name.push_str(pick(CODAS, &mut rng));
    }
    capitalize(&name)
}

/// Sépare le flux des clans de celui des agents (voir [`clan_name`]).
const CLAN_STREAM_MIX: u64 = 0x9E37_79B9_7F4A_7C15;

/// Tire un élément d'une table. `&'static str` partout : les syllabes sont des
/// littéraux, aucune allocation n'a lieu ici — seul le `String` final en fait.
fn pick<'a>(table: &[&'a str], rng: &mut Pcg32) -> &'a str {
    table[(rng.next_u32() as usize) % table.len()]
}

/// Majuscule initiale. Toutes les syllabes sont ASCII, donc découper au premier
/// octet est sûr (ce ne serait pas vrai d'une chaîne UTF-8 quelconque).
fn capitalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    if let Some(first) = chars.next() {
        out.extend(first.to_uppercase());
        out.push_str(chars.as_str());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_nom_est_stable_et_propre_a_son_porteur() {
        let seed = WorldSeed(42);
        let a = agent_name(seed, AgentId(7), Sex::Female);
        assert_eq!(a, agent_name(seed, AgentId(7), Sex::Female), "même seed, même nom");
        assert_ne!(a, agent_name(seed, AgentId(8), Sex::Female), "deux agents, deux noms");
        assert_ne!(a, agent_name(WorldSeed(43), AgentId(7), Sex::Female), "autre monde, autre nom");
    }

    /// Un clan et un agent de même numéro ne doivent pas porter la même racine
    /// (le décalage de flux est là pour ça).
    #[test]
    fn les_clans_ne_calquent_pas_les_humains() {
        let seed = WorldSeed(42);
        let collisions = (0..64)
            .filter(|&i| clan_name(seed, ClanId(i)) == agent_name(seed, AgentId(i), Sex::Male))
            .count();
        assert_eq!(collisions, 0);
    }

    /// La marque de sexe : aucun nom féminin ne se ferme sur une consonne, et
    /// une bonne part des masculins le fait.
    #[test]
    fn la_terminaison_marque_le_sexe() {
        let seed = WorldSeed(42);
        let vowel = |s: &String| "aeiou".contains(s.chars().last().unwrap());
        let femmes: Vec<String> =
            (0..200).map(|i| agent_name(seed, AgentId(i), Sex::Female)).collect();
        assert!(femmes.iter().all(vowel), "un nom féminin se termine par une voyelle");
        let fermes = (0..200)
            .map(|i| agent_name(seed, AgentId(i), Sex::Male))
            .filter(|n| !vowel(n))
            .count();
        assert!((80..160).contains(&fermes), "~60 % des noms masculins ferment ({fermes}/200)");
    }

    /// Un générateur de noms n'a aucun intérêt s'il n'en produit qu'une poignée.
    #[test]
    fn la_variete_est_suffisante() {
        let seed = WorldSeed(42);
        let noms: std::collections::BTreeSet<String> =
            (0..500).map(|i| agent_name(seed, AgentId(i), Sex::Female)).collect();
        assert!(noms.len() > 400, "500 agents doivent donner >400 noms distincts ({})", noms.len());
        // Et rien de dégénéré : deux syllabes au moins, lisible.
        assert!(noms.iter().all(|n| n.len() >= 3 && n.len() <= 12), "longueurs plausibles");
    }
}
