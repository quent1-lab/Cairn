//! Le carrefour entre la simulation et les observateurs.
//!
//! La simulation vit dans **un seul fil**, qui la possède : aucun verrou ne
//! la protège, parce que personne d'autre n'y touche. Ce que les connexions
//! partagent avec elle, c'est seulement ce registre : qui regarde, et quelle
//! fenêtre. À chaque publication, le fil de simulation parcourt le registre,
//! extrait l'instantané de chaque fenêtre et le dépose dans la boîte de
//! l'abonné.
//!
//! La boîte ne garde que le **dernier** instantané (`watch`) : un client lent
//! ne fait pas grossir une file, il saute des images. Un observateur veut voir
//! le monde tel qu'il est, pas rattraper son retard.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use cairn_protocol::{Region, ServerMsg, WorldSnapshot};
use cairn_sim::Sim;
use tokio::sync::watch;

/// Un instantané déjà encodé : on l'encode une fois dans le fil de simulation,
/// la connexion n'a plus qu'à pousser les octets.
pub type Frame = Arc<Vec<u8>>;

struct Subscriber {
    /// `None` tant que le client n'a pas dit ce qu'il regarde : on ne paie
    /// pas d'instantané pour une fenêtre inconnue.
    region: Option<Region>,
    tx: watch::Sender<Option<Frame>>,
}

/// Ce que mesure une publication — de quoi juger, plus tard, si un delta
/// devient nécessaire (BRIEF §8.5).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Published {
    pub subscribers: usize,
    pub bytes: usize,
}

#[derive(Default)]
pub struct Hub {
    // `BTreeMap` et non `HashMap` : l'ordre de parcours ne touche pas la
    // simulation, mais la règle du déterminisme ne coûte rien à suivre ici.
    subs: Mutex<BTreeMap<u64, Subscriber>>,
    next_id: Mutex<u64>,
}

impl Hub {
    /// Inscrit un observateur ; il reçoit sa boîte et son numéro.
    pub fn join(&self) -> (u64, watch::Receiver<Option<Frame>>) {
        let (tx, rx) = watch::channel(None);
        let id = {
            let mut next = self.next_id.lock().unwrap();
            *next += 1;
            *next
        };
        self.subs.lock().unwrap().insert(id, Subscriber { region: None, tx });
        (id, rx)
    }

    pub fn subscribe(&self, id: u64, region: Region) {
        if let Some(s) = self.subs.lock().unwrap().get_mut(&id) {
            s.region = Some(region);
        }
    }

    pub fn leave(&self, id: u64) {
        self.subs.lock().unwrap().remove(&id);
    }

    /// Appelé par le fil de simulation, entre deux pas : chaque abonné reçoit
    /// l'instantané de **sa** fenêtre.
    pub fn publish(&self, sim: &Sim) -> Published {
        let subs = self.subs.lock().unwrap();
        let mut out = Published::default();
        for s in subs.values() {
            let Some(region) = s.region else { continue };
            let snap = WorldSnapshot::from_sim(sim, region);
            let bytes = cairn_protocol::encode(&ServerMsg::Snapshot(Box::new(snap)))
                .expect("un instantané s'encode toujours");
            out.subscribers += 1;
            out.bytes += bytes.len();
            // Un envoi n'échoue que si la connexion est déjà tombée ; elle se
            // désinscrira d'elle-même.
            let _ = s.tx.send(Some(Arc::new(bytes)));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_core::WorldSeed;

    fn decode(frame: &Frame) -> WorldSnapshot {
        match cairn_protocol::decode::<ServerMsg>(frame).expect("décodage") {
            ServerMsg::Snapshot(s) => *s,
            other => panic!("attendu un instantané, reçu {other:?}"),
        }
    }

    /// Le critère du BRIEF (§10, phase 6) : plusieurs observateurs de régions
    /// différentes, chacun ne reçoit que la sienne.
    #[test]
    fn chaque_observateur_recoit_sa_fenetre() {
        let mut sim = Sim::new(WorldSeed(42), 256);
        sim.allow_wildfires = false;
        sim.allow_weather = false;
        sim.allow_fauna_immigration = false;
        sim.spawn_agent(0.0, 0.0);
        sim.spawn_agent(10_000.0, 0.0);
        sim.spawn_agent(10_001.0, 0.0);

        let hub = Hub::default();
        let (a, mut rx_a) = hub.join();
        let (b, mut rx_b) = hub.join();
        let (_c, rx_c) = hub.join();
        hub.subscribe(a, Region::around((0.0, 0.0), 100, 100));
        hub.subscribe(b, Region::around((10_000.0, 0.0), 100, 100));

        let p = hub.publish(&sim);
        assert_eq!(p.subscribers, 2, "celui qui n'a rien demandé ne coûte rien");

        let snap_a = decode(rx_a.borrow_and_update().as_ref().expect("a servi"));
        let snap_b = decode(rx_b.borrow_and_update().as_ref().expect("b servi"));
        assert_eq!(snap_a.agents.len(), 1);
        assert_eq!(snap_b.agents.len(), 2);
        assert!(rx_c.borrow().is_none());
        // Ce qui se résume reste le monde entier, des deux côtés.
        assert_eq!(snap_a.status.population, 3);
        assert_eq!(snap_b.status.population, 3);
    }

    #[test]
    fn un_observateur_parti_n_est_plus_servi() {
        let sim = Sim::new(WorldSeed(42), 256);
        let hub = Hub::default();
        let (a, _rx) = hub.join();
        hub.subscribe(a, Region::everything());
        hub.leave(a);
        assert_eq!(hub.publish(&sim).subscribers, 0);
    }
}
