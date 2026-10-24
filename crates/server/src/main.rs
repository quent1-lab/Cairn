//! Le serveur de Cairn : le monde tourne ici, que quelqu'un regarde ou non
//! (BRIEF §1). Un navigateur qui se connecte reçoit la seed — il régénère le
//! terrain lui-même — puis, à cadence fixe, l'instantané de la fenêtre qu'il
//! regarde.
//!
//! Deux mondes d'exécution cohabitent, et ne se touchent que par le [`Hub`] :
//! - **la simulation**, dans un fil à elle, au pas fixe (20 ticks/s visés,
//!   BRIEF §8.2) ; elle est la seule à posséder le `Sim` ;
//! - **les connexions**, sur tokio : une tâche par navigateur, qui ne fait que
//!   relayer des octets.
//!
//! Sans persistance (SRV-1) : arrêter le serveur, c'est perdre le monde.
//!
//! Usage : `cairn-server [seed] [tempere|froid] [port] [ticks/s] [dossier du client]`

mod hub;
mod scene;
mod web;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use hub::Hub;

/// Cadence des instantanés, en hertz réels. Le BRIEF dit 5-10 Hz ; le client
/// interpole entre deux images, 5 suffit tant qu'on n'a pas mesuré mieux.
const PUBLISH_HZ: f64 = 5.0;
/// Intervalle du relevé de cadence, en secondes réelles.
const REPORT_EVERY_S: u64 = 30;

/// Ce que les connexions savent du monde sans le toucher.
pub struct Shared {
    pub hub: Hub,
    pub seed: u64,
    pub tick: AtomicU64,
    pub client_dir: std::path::PathBuf,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(42);
    let cold = args.next().is_some_and(|s| s == "froid");
    let port: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(8080);
    let tps: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(20.0);
    let client_dir = args.next().unwrap_or_else(|| "crates/client/dist".into()).into();

    let shared = Arc::new(Shared { hub: Hub::default(), seed, tick: AtomicU64::new(0), client_dir });

    let sim_shared = shared.clone();
    std::thread::Builder::new()
        .name("simulation".into())
        .spawn(move || run_simulation(&sim_shared, seed, cold, tps))
        .expect("fil de simulation");

    // Un seul fil pour les connexions : la machine a deux cœurs, l'autre est
    // à la simulation. Relayer des octets n'en demande pas plus.
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio");
    rt.block_on(web::serve(shared, port));
}

/// La boucle du monde : un pas toutes les `1/tps` secondes, une publication
/// toutes les `1/PUBLISH_HZ`. Si un pas coûte plus que sa part, on ne dort
/// pas — le monde ralentit, il ne saute pas de tick (le déterminisme tient au
/// nombre de pas, pas à l'horloge).
fn run_simulation(shared: &Shared, seed: u64, cold: bool, tps: f64) {
    let mut sim = scene::build(seed, cold);
    shared.tick.store(sim.time.tick, Ordering::Relaxed);
    eprintln!(
        "monde prêt : seed {seed}, scène {}, {} humains, tick {}",
        if cold { "froid" } else { "tempéré" },
        sim.population(),
        sim.time.tick
    );

    let step_period = Duration::from_secs_f64(1.0 / tps);
    let publish_period = Duration::from_secs_f64(1.0 / PUBLISH_HZ);
    let mut next_step = Instant::now();
    let mut next_publish = Instant::now();

    // Relevé : ce qui dira si l'on tient la cadence visée (PERF).
    let mut report_at = Instant::now() + Duration::from_secs(REPORT_EVERY_S);
    let (mut steps, mut step_time, mut pub_time) = (0u64, Duration::ZERO, Duration::ZERO);
    let (mut frames, mut bytes) = (0usize, 0usize);

    loop {
        let now = Instant::now();
        if now >= next_publish {
            let t = Instant::now();
            let p = shared.hub.publish(&sim);
            pub_time += t.elapsed();
            frames += p.subscribers;
            bytes += p.bytes;
            next_publish += publish_period;
            if next_publish < now {
                next_publish = now + publish_period;
            }
        }
        if now >= next_step {
            let t = Instant::now();
            sim.step();
            step_time += t.elapsed();
            steps += 1;
            shared.tick.store(sim.time.tick, Ordering::Relaxed);
            next_step += step_period;
            // En retard de plus d'une seconde : on renonce à rattraper.
            if next_step + Duration::from_secs(1) < now {
                next_step = now;
            }
        }
        if now >= report_at {
            let span = REPORT_EVERY_S as f64;
            eprintln!(
                "tick {} · {:.1} ticks/s (visé {tps}) · pas {:.1} ms · publication {:.1} ms · {} images, {:.0} o/image · {} humains",
                sim.time.tick,
                steps as f64 / span,
                step_time.as_secs_f64() * 1e3 / steps.max(1) as f64,
                pub_time.as_secs_f64() * 1e3 / (span * PUBLISH_HZ),
                frames,
                bytes as f64 / frames.max(1) as f64,
                sim.population(),
            );
            report_at += Duration::from_secs(REPORT_EVERY_S);
            (steps, step_time, pub_time, frames, bytes) = (0, Duration::ZERO, Duration::ZERO, 0, 0);
        }
        let wake = next_step.min(next_publish);
        let now = Instant::now();
        if wake > now {
            std::thread::sleep(wake - now);
        }
    }
}
