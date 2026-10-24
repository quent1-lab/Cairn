//! La scène que le serveur fait vivre : la scène de référence
//! (`cairn_sim::scenario::reference`), celle du banc d'acceptation — ce qu'on
//! regarde dans le navigateur est ce qu'on a mesuré.

use cairn_sim::{Sim, scenario};

/// Jour de départ, celui des bancs : la fin du printemps.
const START_DAY: u64 = 135;

pub fn build(seed: u64, cold: bool) -> Sim {
    scenario::reference(seed, cold, START_DAY).0
}
