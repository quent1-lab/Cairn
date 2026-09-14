//! Le protocole : ce que le serveur dit du monde, et ce que le joueur lui
//! demande. **Partagé serveur ↔ client** (BRIEF §8.1), donc impossible à
//! désynchroniser.
//!
//! Trois natures de données, et c'est ce découpage qui porte tout le reste :
//!
//! 1. **Ce qui ne change jamais** — la seed et la configuration du worldgen.
//!    Dit une fois, à la connexion ([`Hello`]). Le terrain n'est donc **pas**
//!    transmis : il est fonction pure de la seed, le client le régénère
//!    lui-même. C'est la propriété la plus précieuse du projet et elle se
//!    paie ici en octets qu'on n'envoie pas.
//! 2. **Ce qui bouge** — les entités visibles, filtrées par la région à
//!    laquelle le client s'est abonné ([`WorldSnapshot`]).
//! 3. **Ce qu'on demande** — le détail d'un être, trop volumineux pour être
//!    diffusé en continu (BRIEF §7.2 : « une requête à la demande, pas un
//!    flux »). Viendra à l'incrément dédié.
//!
//! La règle de tri entre 2 et 3 : entre dans le flux ce qui se **dessine** ou
//! ce qui tient en quelques octets ; reste à la demande tout ce qui ne
//! concerne qu'un seul être qu'on a cliqué.

pub mod snapshot;

pub use snapshot::{
    AgentDot, ClanSummary, FaunaDot, FaunaRole, Region, StructureDot, WeatherDot, WorldSnapshot,
    WorldStatus,
};

use serde::{Deserialize, Serialize};

/// Version du protocole. Le serveur refuse un client qui n'a pas la sienne :
/// les deux binaires sont compilés depuis ce crate, mais rien ne garantit
/// qu'un onglet ouvert depuis trois jours ait rechargé le wasm.
pub const PROTOCOL_VERSION: u16 = 1;

/// Ce que le serveur dit une fois pour toutes, à la connexion : de quel monde
/// il s'agit. Avec ça, le client peut peindre le terrain entier sans qu'un
/// octet de tuile ne transite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub protocol: u16,
    pub seed: u64,
    /// Le monde tourne depuis ce tick — un client qui arrive n'assiste pas au
    /// commencement, et c'est le propos (BRIEF §1 : « une civilisation qui a
    /// continué sans vous »).
    pub tick: u64,
}

/// Du serveur vers le client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServerMsg {
    Hello(Hello),
    /// L'état de la région abonnée. Le nom dit « snapshot » et pas « delta » :
    /// tant qu'on n'a pas **mesuré** qu'un état complet coûte trop cher à
    /// 5-10 Hz, on n'écrit pas de code de différence (règle §0.3).
    Snapshot(Box<WorldSnapshot>),
    /// Les faits notables survenus depuis le dernier envoi. Le fait est
    /// transmis **structuré**, jamais rédigé : la phrase se compose chez le
    /// lecteur (`cairn_sim::chronicle::tell`), ce qui garde le journal
    /// interrogeable (BRIEF §8.4).
    Chronicle(Vec<cairn_sim::Event>),
    /// Ce que le monde a répondu à un geste divin — ou pourquoi il n'a rien
    /// répondu.
    MiracleResult(Result<cairn_sim::Outcome, DivineFailure>),
    /// Le serveur refuse la connexion (version incompatible, surcharge).
    Refused(String),
}

/// Du client vers le serveur.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMsg {
    /// Je regarde ici. Tout ce qui suit sera filtré par cette fenêtre —
    /// c'est l'interest management du §8.5, et la seule raison pour laquelle
    /// trois observateurs de trois régions ne coûtent pas trois mondes.
    Subscribe { region: Region },
    /// Un geste divin. `cairn_sim::Sim::invoke` reste la porte unique : ce
    /// message ne fait que la franchir depuis l'autre bout du fil.
    Invoke(cairn_sim::Intervention),
    /// Rends-moi la Chronique depuis ce tick (reconnexion après absence).
    ChronicleSince { tick: u64 },
}

/// L'échec d'une intervention, transportable. `cairn_sim::DivineError` n'est
/// pas sérialisable et n'a pas à l'être : c'est un type de la simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DivineFailure {
    /// Celui qu'on voulait inspirer n'est plus là. Seule la Révélation vise un
    /// être, donc elle seule peut échouer ainsi.
    NoSuchTarget,
    /// Pas assez de Foi. Sans croyants, la divinité est impuissante (§6.1).
    NotEnoughFaith,
}

impl From<cairn_sim::DivineError> for DivineFailure {
    fn from(e: cairn_sim::DivineError) -> Self {
        match e {
            cairn_sim::DivineError::NoSuchTarget => DivineFailure::NoSuchTarget,
        }
    }
}

/// Encode un message. Le format est binaire : on ne lit pas le fil à l'œil,
/// on le lit avec le même crate des deux côtés.
pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, postcard::Error> {
    postcard::to_stdvec(msg)
}

/// Décode un message.
pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, postcard::Error> {
    postcard::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_message_survit_a_l_aller_retour() {
        let msg = ServerMsg::Hello(Hello { protocol: PROTOCOL_VERSION, seed: 42, tick: 8640 });
        let bytes = encode(&msg).expect("encodage");
        let back: ServerMsg = decode(&bytes).expect("décodage");
        assert_eq!(msg, back);
    }

    #[test]
    fn un_geste_divin_passe_le_fil_sans_se_deformer() {
        // Le client n'envoie pas « foudroie ici » en texte : il envoie le type
        // même que `Sim::invoke` attend. C'est tout l'intérêt du crate partagé.
        let msg = ClientMsg::Invoke(cairn_sim::Intervention::Lightning { pos: (-1_234, 5_678) });
        let bytes = encode(&msg).expect("encodage");
        let back: ClientMsg = decode(&bytes).expect("décodage");
        assert_eq!(msg, back);
    }
}
