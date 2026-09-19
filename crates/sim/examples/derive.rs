//! Banc de **dérive** : rend visible en minutes ce que le banc long met huit
//! heures à montrer.
//!
//! Le run de référence du 2026-09-15 (8 h 18, seed 42, 15,6 années de jeu) a
//! établi que le débit s'effondre d'un facteur 636 **sans la moindre fuite
//! mémoire** — l'empreinte plafonne à 1,13 Go et le processus ne swappe jamais.
//! Ce qui diverge, ce sont des **effectifs** : le nombre de troupeaux (22 → 919)
//! et, plus tôt, la dispersion de la population (2 → 59 km). Le coût suit.
//!
//! ## Pourquoi un banc de plus
//!
//! Le problème ne se déclenche qu'au-delà de la centaine de troupeaux, que la
//! scène de référence met **quatorze années de jeu** à atteindre naturellement.
//! Or ce qu'on veut corriger n'est pas la mise en place, c'est la **dynamique du
//! nombre** : un système qui diverge à 150 troupeaux diverge aussi si on le
//! *démarre* à 150. On paie donc la densité en condition initiale (via le
//! `herd_grid` de `scenario::populate`, qui existe déjà) au lieu de la payer en
//! heures de calcul.
//!
//! Ce n'est pas une entorse à l'émergence stricte : c'est une **condition
//! initiale** de scène, au même titre que `find_home_where` choisit où lâcher
//! la population. Aucune règle du cœur de simulation n'est touchée.
//!
//! ## Ce qu'il mesure, et ce qu'il ne mesure pas
//!
//! Il ne mesure **pas** le débit absolu — la scène est volontairement dense et
//! le chiffre n'est pas comparable à celui de `client_render`. Il mesure des
//! **tendances** : sur la seconde moitié du run, chaque effectif converge-t-il
//! ou diverge-t-il ? Le verdict compare le 3ᵉ quart au 4ᵉ, ce qui est robuste
//! au bruit saisonnier sans demander d'ajustement de courbe.
//!
//! **Trois grandeurs, pas une** (2026-09-19). Comparer deux **moyennes**
//! suppose une grandeur monotone, et ce banc s'est trompé deux fois sur le même
//! run pour l'avoir supposé : il a condamné un correctif qui divisait un stock
//! par six, parce que la base était six fois plus basse et que l'écart relatif
//! y paraissait donc plus grand. Le verdict rapporte désormais la **médiane**
//! (niveau, insensible aux pointes), le **minimum** (plancher — c'est lui qui
//! sépare un *cliquet*, qui ne redescend jamais, d'un *réservoir* qui revient à
//! son étiage) et l'**amplitude** `max/min` (une oscillation dont l'amplitude
//! grossit n'est pas amortie). Un effectif n'est déclaré sans borne que si son
//! niveau **et** son plancher montent.
//!
//! ## Densité sans étalement — la contrainte qui a dicté la scène
//!
//! Première version de ce banc : `scenario::populate` avec un `herd_grid` large,
//! qui pose le gibier sur une grille de pas 1,5 km. Pour 164 troupeaux, cela
//! étale la scène sur **18 km** — or 4096 chunks résidents ne couvrent que
//! 8,2 km de côté (un chunk fait 128 m). Le store passait alors son temps à
//! évincer puis régénérer : on mesurait du **thrashing de LRU**, pas la faune.
//! Mesuré : moins de 0,7 tps, contre 2,8 sur la scène de référence à densité
//! comparable.
//!
//! D'où la scène actuelle : les troupeaux sont posés **à la main sur un pas
//! serré** (`HERD_SPACING_TILES`, ~400 m), ce qui donne la densité voulue sur
//! ~5 km — un ensemble de travail qui tient largement dans le store. La densité
//! est le sujet ; l'étalement est un parasite qu'on écarte.
//!
//! *(À noter pour l'analyse du run long : avec 59 km de dispersion pour 16 384
//! chunks — soit 16,4 km de couverture — la scène de référence thrashait
//! certainement elle aussi. C'est une cause candidate distincte du balayage
//! écologique, et elle reste à départager par un profil.)*
//!
//! Usage :
//!   cargo run --release -p cairn-sim --example derive -- \
//!       [seed] [jours] [troupeaux] [meutes] [agents] [capacité]
//!
//! Défauts : seed 42, 240 jours, 150 troupeaux, 12 meutes, 20 agents,
//! 4096 chunks.

use std::io::Write;
use std::time::Instant;

use cairn_core::{TICKS_PER_DAY, WorldSeed, km_to_tiles, tiles_to_km};
use cairn_sim::{Herd, Pack, Position, Sim, fauna, scenario};

/// Un relevé, pris tous les `SAMPLE_DAYS` jours de jeu.
struct Sample {
    day: u64,
    herds: usize,
    packs: usize,
    head: f32,
    predators: f32,
    dirty: usize,
    /// Tuiles **marquées** (`Chunk::touched`) des chunks sales résidents, et
    /// parmi elles celles dont la biomasse dépasse leur capacité de **temps
    /// sec**. Les deux compteurs que la météo rend indispensables.
    ///
    /// `marquees` dit si le balayage épars se dégrade en balayage complet ;
    /// `sur_cap` dit si le surplus laissé par une averse **redescend**. Un
    /// `sur_cap` qui croît sans fin signifie que le don de la pluie est
    /// définitif — ce que la logistique ne fait pas d'elle-même, et ce que
    /// l'éviction masquait jusqu'à 5099f8c en rendant la tuile au baseline.
    marquees: usize,
    sur_cap: usize,
    spread_km: f64,
    pop: usize,
    tps: f64,
    /// Troupeaux par km² — **le régime dans lequel on mesure**.
    ///
    /// Le banc a longtemps donné « aucune divergence » sans que je voie qu'il
    /// testait toujours le même régime : une faune dense, où le prédateur ne
    /// peut que gagner. Une mesure qui ne dit pas dans quelles conditions elle
    /// a été prise ne permet pas de savoir ce qu'elle réfute.
    density: f64,
    /// **Prises réellement réalisées** par prédateur et par jour, contre un
    /// maximum théorique de `PRED_KILL_PER_DAY` quand le gibier est à portée.
    ///
    /// La mesure qui départage « le prédateur ne trouve pas ses proies » de
    /// « l'équation est mal réglée ». Déduite du run long à ~0,044 contre 0,18
    /// possible ; ici elle est comptée, pas déduite.
    kills_per_pred_day: f64,
    /// Part des troupeaux **hors de vue de tout humain** (> 2 km, la portée de
    /// `brain::HERD_SIGHT_TILES`). C'est la fraction que le LOD pourrait cesser
    /// de simuler à plein régime sans que personne ne s'en aperçoive.
    far_pct: f64,
    /// Chunks **régénérés** depuis le relevé précédent, ramenés au jour de jeu.
    ///
    /// Le discriminateur qui manquait. Un chunk évincé puis redemandé se
    /// regénère intégralement — le bruit fBm du worldgen sur 4 096 tuiles. Tant
    /// que l'ensemble de travail tient dans le store, ce nombre reste proche de
    /// zéro une fois la scène chargée ; dès qu'il le dépasse, le store entre en
    /// thrashing et ce compteur s'envole pendant que le débit s'effondre.
    /// Distinguer « la simulation a plus de travail » de « le store rame » sans
    /// ce chiffre relevait de la divination.
    regen_per_day: f64,
}

const SAMPLE_DAYS: u64 = 5;

/// Périodicité du rapport de profil, en jours de jeu.
const PROFILE_EVERY_DAYS: u64 = 200;

/// Pas par défaut de la grille de troupeaux, en tuiles (~400 m). Assez serré
/// pour que 150 troupeaux tiennent sur 5 km — donc dans le store de chunks — et
/// assez lâche pour qu'ils ne démarrent pas tous sur la même touffe d'herbe.
///
/// **C'est la variable de l'expérience** : à pas serré la prédation atteint
/// tout le monde, à pas large elle ne couvre plus le territoire (rayon de chasse
/// d'une meute : 500 m). Réglable en ligne de commande, parce que départager
/// « la faune diverge d'elle-même » de « la faune diverge quand le monde
/// s'étire » demande de ne bouger que ça.
const HERD_SPACING_TILES: i64 = 200;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(42);
    let days: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(240).max(40);
    let n_herds: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(150);
    let n_packs: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(12);
    let n_agents: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(20);
    let capacity: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(4096);
    let spacing: i64 =
        args.next().and_then(|s| s.parse().ok()).unwrap_or(HERD_SPACING_TILES).max(1);
    // Pas de la grille humaine, en tuiles. 0 = les humains restent groupés
    // comme les pose `scenario::populate`.
    let human_spread: i64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0).max(0);
    // Pas de LOD pour la faune hors de vue, en ticks. 1 = désactivé.
    let lod: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1).max(1);

    // — **Exactement le foyer de `chronicle`** : même point de départ, même
    //   sélection. C'est délibéré — les deux bancs deviennent comparables, et
    //   celui-ci n'a pas besoin d'une pression climatique particulière (c'est
    //   l'affaire d'`etincelle`), seulement d'un pays qui porte de l'herbe.
    //
    //   `find_home_where` serait ici un piège : il appelle `nearest_spring` sur
    //   chaque candidat, soit 81 chunks, et une bande de climat étroite le fait
    //   balayer des milliers de candidats avant d'aboutir — mesuré à plus de
    //   dix minutes avant le premier tick, pour un banc censé durer vingt. —
    let mut sim = Sim::new(WorldSeed(seed), capacity);
    let seed_point = (km_to_tiles(1500.0) as i64, km_to_tiles(2100.0) as i64);
    sim.fauna_lod_period = lod;
    let home = scenario::find_home(&mut sim, seed_point);

    // Les humains et le gibier « normal » ; la densité arrive juste après.
    let placed = scenario::populate(&mut sim, home, n_agents, 2);

    // — L'étalement des **humains**, et c'est la variable qui compte.
    //
    //   `fauna::daily_immigration` tire son site candidat à 1–4 km d'un humain
    //   pris au hasard, et le rejette s'il est à moins de 3 km d'un troupeau.
    //   Ce filtre est décrit comme auto-limitant — il l'est, mais seulement
    //   tant que les humains restent groupés : dès qu'ils s'étalent, un humain
    //   isolé fournit toujours un site vierge, et l'immigration tire alors à
    //   plein régime (0,15/jour) sans fin. Étaler les humains, c'est donc
    //   **débrancher le limiteur** — l'hypothèse que ce banc doit trancher. —
    if human_spread > 0 {
        // Grille carrée : les humains occupent le territoire au lieu de se
        // tenir en tas, sans qu'aucun ne parte à l'infini.
        let side = (placed as f64).sqrt().ceil() as i64;
        for (i, (_, pos)) in sim.agents.query_mut::<&mut Position>().into_iter().enumerate() {
            let i = i as i64;
            let (gx, gy) = (i % side - side / 2, i / side - side / 2);
            pos.x = home.0 as f64 + (gx * human_spread) as f64;
            pos.y = home.1 as f64 + (gy * human_spread) as f64;
        }
    }

    // — La densité, posée à la main sur un pas serré : c'est la **condition
    //   initiale** du banc. Spirale carrée depuis le foyer, pour que les
    //   troupeaux restent groupés quel que soit leur nombre. —
    let step = spacing;
    let mut ring = 0i64;
    'dense: while sim.fauna.query::<&Herd>().iter().count() < n_herds && ring < 60 {
        ring += 1;
        for dy in -ring..=ring {
            for dx in -ring..=ring {
                if dx.abs() < ring && dy.abs() < ring {
                    continue; // seulement la couronne courante
                }
                let (x, y) = (home.0 + dx * step, home.1 + dy * step);
                let tile = sim.world.tile(x, y);
                if tile.is_walkable() && tile.biomass > 40 {
                    sim.spawn_herd(x as f64, y as f64, fauna::HERD_START);
                    if sim.fauna.query::<&Herd>().iter().count() >= n_herds {
                        break 'dense;
                    }
                }
            }
        }
    }

    // — Des meutes en plus de celles que `populate` pose : sans prédateurs en
    //   nombre comparable aux proies, on mesurerait une divergence qu'on aurait
    //   fabriquée soi-même. On leur donne toutes leurs chances. —
    let existing = sim.fauna.query::<&Pack>().iter().count();
    for i in existing..n_packs {
        let angle = i as f64 * 0.9;
        let r = km_to_tiles(2.0 + (i % 5) as f64);
        let (x, y) = (
            home.0 + (angle.cos() * r) as i64,
            home.1 + (angle.sin() * r) as i64,
        );
        if sim.world.tile(x, y).is_walkable() {
            sim.spawn_pack(x as f64, y as f64, fauna::PACK_START);
        }
    }

    let (h0, p0, nh0, np0) = census(&sim);
    println!("╔══ BANC DE DÉRIVE ═══════════════════════════════════════════════════╗");
    println!("  seed {seed} · foyer ({}, {}) · {days} jours de jeu", home.0, home.1);
    println!(
        "  départ : {placed} humains · {nh0} troupeaux ({h0:.0} têtes) · {np0} meutes ({p0:.0} prédateurs)"
    );
    println!(
        "  pas de {:.2} km ⇒ étendue ~{:.1} km · capacité {capacity} chunks ({:.1} km de côté)",
        tiles_to_km(spacing as f64),
        tiles_to_km((2 * ring * step) as f64),
        tiles_to_km((capacity as f64).sqrt() * cairn_sim::CHUNK_SIZE as f64),
    );
    if human_spread > 0 {
        println!(
            "  humains ÉTALÉS : pas de {:.1} km (le limiteur d'immigration est débranché)",
            tiles_to_km(human_spread as f64)
        );
    }
    if lod > 1 {
        println!("  LOD faune : pas de {lod} ticks hors de vue d'un humain (marge 4 km)");
    }
    println!("  relevé tous les {SAMPLE_DAYS} j");
    println!("╚═════════════════════════════════════════════════════════════════════╝\n");
    println!(
        "{:>5} {:>7} {:>8} {:>7} {:>8} {:>8} {:>8} {:>7} {:>8} {:>6} {:>9} {:>9} {:>8} {:>7}",
        "jour", "troup.", "têtes", "meutes", "préd.", "trp/km²", "prises", "loin%", "disp.km", "pop", "regen/j", "marquées", "sur-cap", "tps"
    );
    let _ = std::io::stdout().flush();

    let mut samples: Vec<Sample> = Vec::new();
    let t_start = Instant::now();
    let mut t_window = Instant::now();
    let mut ticks_window = 0u64;
    let mut regen_prev = sim.world.generated;
    let mut kills_window = 0.0f64;
    let mut pred_ticks = 0.0f64;

    for day in 1..=days {
        for _ in 0..TICKS_PER_DAY {
            sim.step();
            ticks_window += 1;
            // Les prises du tick, relevées à la source : `Pack::last_kills`
            // est écrit par `update_packs` puis écrasé au tick suivant, donc
            // c'est ici — et nulle part ailleurs — qu'on peut les totaliser.
            // On accumule aussi les prédateurs-ticks pour normaliser : une
            // moyenne de prises n'a de sens que rapportée aux bouches.
            for (_, p) in sim.fauna.query::<&Pack>().iter() {
                kills_window += p.last_kills as f64;
                pred_ticks += p.population as f64;
            }
        }
        if !day.is_multiple_of(SAMPLE_DAYS) {
            continue;
        }
        let tps = ticks_window as f64 / t_window.elapsed().as_secs_f64().max(1e-9);
        t_window = Instant::now();
        ticks_window = 0;

        let regen_per_day =
            (sim.world.generated - regen_prev) as f64 / SAMPLE_DAYS as f64;
        regen_prev = sim.world.generated;

        let (head, predators, herds, packs) = census(&sim);
        // Deux compteurs indépendants du reste : ils ne parlent pas de la
        // faune mais du **sol** qu'elle broute, et c'est ce qui les rend utiles
        // — une divergence d'effectif et une dérive de la végétation ne se
        // confondent pas si on les lit séparément.
        let recensement = sim.world.biomass_census();
        // Densité **mesurée** sur l'étendue réelle des troupeaux, pas déduite
        // des paramètres de départ : la faune migre, et c'est la densité du
        // moment qui gouverne la rencontre.
        let density = herd_density(&sim, herds);
        let buckets = herds_by_distance(&sim);
        let far_pct = if herds == 0 {
            0.0
        } else {
            (herds - buckets[0]) as f64 / herds as f64 * 100.0
        };
        let s = Sample {
            day,
            herds,
            packs,
            head,
            predators,
            dirty: sim.world.dirty_count(),
            marquees: recensement.0,
            sur_cap: recensement.1,
            spread_km: spread(&sim),
            pop: sim.population(),
            tps,
            regen_per_day,
            density,
            far_pct,
            // `pred_ticks` compte des prédateurs-ticks ; ramené au jour, c'est
            // le nombre de prédateurs-jours sur la fenêtre.
            kills_per_pred_day: if pred_ticks > 0.0 {
                kills_window / (pred_ticks / TICKS_PER_DAY as f64)
            } else {
                0.0
            },
        };
        kills_window = 0.0;
        pred_ticks = 0.0;
        println!(
            "{:>5} {:>7} {:>8.0} {:>7} {:>8.0} {:>8.2} {:>8.3} {:>6.0} {:>8.1} {:>6} {:>9.0} {:>9} {:>8} {:>7.1}",
            s.day,
            s.herds,
            s.head,
            s.packs,
            s.predators,
            s.density,
            s.kills_per_pred_day,
            s.far_pct,
            s.spread_km,
            s.pop,
            s.regen_per_day,
            s.marquees,
            s.sur_cap,
            s.tps
        );
        // Un banc qui tourne des dizaines de minutes doit être lisible *pendant*
        // qu'il tourne : sans ce vidage, la sortie reste bloquée dans le tampon
        // dès qu'on la passe dans un tube (`tee`, `head`) et on croit le
        // processus figé.
        let _ = std::io::stdout().flush();
        // Profil périodique : un run long s'arrête à la main, il doit donc
        // rendre ses chiffres en cours de route et pas seulement à la fin.
        if day.is_multiple_of(PROFILE_EVERY_DAYS) {
            profile_report(&sim);
            let _ = std::io::stdout().flush();
        }
        samples.push(s);
    }

    verdict(&samples, t_start.elapsed().as_secs_f64(), days);
    profile_report(&sim);
    utilization_report(&sim);
}

/// Où passe le temps, par phase du tick. Ne s'affiche que sous la feature
/// `profile` :
///   cargo run --release -p cairn-sim --features profile --example derive
#[cfg(feature = "profile")]
fn profile_report(sim: &Sim) {
    println!("\n╔══ PROFIL — répartition du temps par phase ══════════════════════════╗");
    print!("{}", sim.prof.report());
    println!("╚═════════════════════════════════════════════════════════════════════╝");
}

#[cfg(not(feature = "profile"))]
fn profile_report(_sim: &Sim) {}

/// Mesure M1 — le taux d'utilisation des chunks. Ne s'affiche que sous la
/// feature `chunk-stats` :
///   cargo run --release -p cairn-sim --features chunk-stats --example derive
#[cfg(feature = "chunk-stats")]
fn utilization_report(sim: &Sim) {
    const TILES: f64 = (cairn_sim::CHUNK_SIZE * cairn_sim::CHUNK_SIZE) as f64;
    let (lives, mean, hist, dirty_lives) = sim.world.utilization();
    if lives == 0 {
        println!("\n(aucun chunk évincé : la scène tient dans le store, rien à mesurer)");
        return;
    }
    println!("\n╔══ M1 — UTILISATION DES CHUNKS ══════════════════════════════════════╗");
    println!("  {lives} vies de chunk achevées (générées puis évincées)");
    println!(
        "  tuiles distinctes touchées, en moyenne : {mean:.1} sur {TILES:.0}  ⇒  {:.3} %",
        mean / TILES * 100.0
    );
    println!(
        "  autrement dit : on paie ~{:.0} tuiles générées pour chaque tuile lue\n",
        TILES / mean.max(1e-9)
    );
    // La fraction qui décide de la portée de C1 : l'écologie balaie les 4 096
    // tuiles de tout chunk **écrit**, chaque jour. La génération paresseuse ne
    // peut donc rien pour ceux-là — son gain porte sur les chunks qu'on ne fait
    // que **lire** (les sondes de pâture, les scans de fourrage, la
    // franchissabilité du pathfinding).
    let clean = lives - dirty_lives;
    println!(
        "  vies SANS aucune écriture : {clean} sur {lives}  ⇒  {:.1} %  ← portée de C1",
        clean as f64 / lives as f64 * 100.0
    );
    println!(
        "  vies avec écriture : {dirty_lives}  (balayées en entier par l'écologie)\n"
    );
    println!("  répartition (nombre de tuiles touchées sur une vie de chunk) :");
    for (k, &n) in hist.iter().enumerate() {
        if n == 0 {
            continue;
        }
        let (lo, hi) = if k == 0 { (0, 0) } else { (1 << (k - 1), (1 << k) - 1) };
        let pct = n as f64 / lives as f64 * 100.0;
        let bar = "█".repeat(((pct / 2.0) as usize).min(40));
        let label =
            if k == 0 { "       0".to_string() } else { format!("{lo:>4}-{hi:<4}") };
        println!("    {label} {bar} {pct:>5.1} %  ({n})");
    }
    println!("╚═════════════════════════════════════════════════════════════════════╝");
}

#[cfg(not(feature = "chunk-stats"))]
fn utilization_report(_sim: &Sim) {}

/// (têtes, prédateurs, troupeaux, meutes) — même notion que `Sim::fauna_census`,
/// recopiée ici pour n'avoir besoin que d'un `&Sim`.
fn census(sim: &Sim) -> (f32, f32, usize, usize) {
    let mut head = 0.0;
    let mut herds = 0;
    for (_, h) in sim.fauna.query::<&Herd>().iter() {
        head += h.population;
        herds += 1;
    }
    let mut predators = 0.0;
    let mut packs = 0;
    for (_, p) in sim.fauna.query::<&Pack>().iter() {
        predators += p.population;
        packs += 1;
    }
    (head, predators, herds, packs)
}

/// Troupeaux par km², sur le **disque effectivement occupé** par la faune.
///
/// Mesurée sur le rayon quadratique moyen autour du centroïde des troupeaux
/// (et non sur la boîte englobante, qu'un seul troupeau égaré ferait exploser).
/// C'est la grandeur qui gouverne la rencontre prédateur-proie : une meute ne
/// tue que ce qui entre dans son disque de chasse, et ce disque ne voit pas le
/// nombre total de troupeaux du monde, seulement leur densité locale.
fn herd_density(sim: &Sim, herds: usize) -> f64 {
    if herds == 0 {
        return 0.0;
    }
    let mut n = 0.0;
    let (mut cx, mut cy) = (0.0, 0.0);
    for (_, (_, pos)) in sim.fauna.query::<(&Herd, &Position)>().iter() {
        cx += pos.x;
        cy += pos.y;
        n += 1.0;
    }
    cx /= n;
    cy /= n;
    let mut sum2 = 0.0;
    for (_, (_, pos)) in sim.fauna.query::<(&Herd, &Position)>().iter() {
        sum2 += (pos.x - cx).powi(2) + (pos.y - cy).powi(2);
    }
    let rms_km = tiles_to_km((sum2 / n).sqrt());
    // Un rayon nul (tous au même point) donnerait une densité infinie ; on
    // plancher sur le disque de chasse lui-même, la plus petite surface qui
    // ait un sens ici.
    let area = (std::f64::consts::PI * rms_km * rms_km).max(disc_km2());
    herds as f64 / area
}

/// Répartition des troupeaux par distance au plus proche humain.
///
/// **La mesure qui arbitre le LOD.** Un humain ne perçoit un troupeau qu'à
/// `brain::HERD_SIGHT_TILES` (2 km) : au-delà, rien de ce que fait ce troupeau
/// n'est observable. S'ils sont majoritairement loin, alors la divergence de la
/// faune n'a pas besoin d'être *corrigée* — il suffit de ne plus la simuler à
/// plein régime, et le coût cesse de suivre le nombre (BRIEF §8.2, « clé de la
/// viabilité du monde infini »). Si au contraire ils se tiennent près des
/// hommes, le LOD ne gagnerait rien et c'est l'écologie qu'il faut corriger.
///
/// Renvoie les effectifs par tranche : ≤2 km, 2-5, 5-10, 10-30, >30.
fn herds_by_distance(sim: &Sim) -> [usize; 5] {
    let humans: Vec<(f64, f64)> =
        sim.agents.query::<&Position>().iter().map(|(_, p)| (p.x, p.y)).collect();
    let mut buckets = [0usize; 5];
    for (_, (_, pos)) in sim.fauna.query::<(&Herd, &Position)>().iter() {
        let d2 = humans
            .iter()
            .map(|h| (h.0 - pos.x).powi(2) + (h.1 - pos.y).powi(2))
            .fold(f64::INFINITY, f64::min);
        let km = tiles_to_km(d2.sqrt());
        let i = if km <= 2.0 {
            0
        } else if km <= 5.0 {
            1
        } else if km <= 10.0 {
            2
        } else if km <= 30.0 {
            3
        } else {
            4
        };
        buckets[i] += 1;
    }
    buckets
}

/// Surface du disque de chasse d'une meute, en km².
fn disc_km2() -> f64 {
    let r = tiles_to_km(cairn_sim::fauna::PACK_HUNT_RADIUS_TILES);
    std::f64::consts::PI * r * r
}

/// Rayon moyen de la population autour de son centroïde, en km — la même
/// définition que la colonne `spread_mean_km` du banc `chronicle`.
fn spread(sim: &Sim) -> f64 {
    let mut n = 0.0;
    let (mut cx, mut cy) = (0.0, 0.0);
    for (_, pos) in sim.agents.query::<&Position>().iter() {
        cx += pos.x;
        cy += pos.y;
        n += 1.0;
    }
    if n == 0.0 {
        return 0.0;
    }
    cx /= n;
    cy /= n;
    let mut sum = 0.0;
    for (_, pos) in sim.agents.query::<&Position>().iter() {
        sum += ((pos.x - cx).powi(2) + (pos.y - cy).powi(2)).sqrt();
    }
    tiles_to_km(sum / n)
}

/// Au-delà de cette croissance relative entre le 3ᵉ et le 4ᵉ quart du run, on
/// parle de divergence. 15 % sur un quart de run est très au-delà du bruit
/// saisonnier mesuré sur la scène de référence (le cycle proie-prédateur y
/// oscille, mais autour d'une valeur).
const DIVERGENCE_THRESHOLD: f64 = 0.15;

/// Minimum, médiane, maximum d'une grandeur sur un quart de run.
///
/// **La médiane remplace la moyenne comme niveau de référence.** Une grandeur
/// qui oscille — le surplus laissé par les averses, le cycle proie-prédateur —
/// voit sa moyenne tirée par ses pointes, et deux runs dont les pointes ne
/// tombent pas aux mêmes jours paraissent alors différer de niveau.
fn stats(samples: &[Sample], f: &dyn Fn(&Sample) -> f64) -> (f64, f64, f64) {
    let mut v: Vec<f64> = samples.iter().map(f).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let med = if v.len() % 2 == 0 {
        (v[v.len() / 2 - 1] + v[v.len() / 2]) / 2.0
    } else {
        v[v.len() / 2]
    };
    (v[0], med, v[v.len() - 1])
}

/// Écart relatif, nul plutôt qu'infini quand la référence est nulle.
fn ecart(a: f64, b: f64) -> f64 {
    if a.abs() < 1e-9 { 0.0 } else { (b - a) / a }
}

/// Amplitude d'un quart : `max/min`. `None` quand le minimum est nul — la
/// grandeur a touché le fond, le rapport ne veut plus rien dire (et c'est
/// l'information intéressante : un effectif qui touche zéro est éteint).
fn amplitude(min: f64, max: f64) -> Option<f64> {
    (min.abs() > 1e-9).then(|| max / min)
}

fn verdict(samples: &[Sample], elapsed_s: f64, days: u64) {
    if samples.len() < 8 {
        println!("\nTrop peu de relevés pour conclure — allongez le run.");
        return;
    }
    let n = samples.len();
    let q3 = &samples[n / 2..3 * n / 4];
    let q4 = &samples[3 * n / 4..];

    println!("\n╔══ VERDICT ══════════════════════════════════════════════════════════╗");
    println!(
        "  {days} jours de jeu en {:.0} s réelles ({:.1} tps moyen)",
        elapsed_s,
        (days * TICKS_PER_DAY) as f64 / elapsed_s.max(1e-9)
    );
    println!("  Tendance mesurée entre le 3ᵉ et le 4ᵉ quart du run.");
    println!("  Δméd = niveau, Δmin = plancher, ampl = max/min dans le quart.\n");
    println!(
        "{:>22} {:>10} {:>10} {:>7} {:>7} {:>8} {:>8} {:>11}",
        "grandeur", "méd 3ᵉq", "méd 4ᵉq", "Δméd", "Δmin", "ampl 3ᵉq", "ampl 4ᵉq", "forme"
    );

    // — Pourquoi trois colonnes et non une. Le verdict d'origine comparait deux
    //   **moyennes** et criait « DIVERGE » au-delà de +15 %. Il supposait donc
    //   une grandeur monotone, et il s'est trompé deux fois sur le même run :
    //   il a condamné un correctif qui divisait le stock de tuiles sur-cap par
    //   six, simplement parce que la base était six fois plus basse et que
    //   l'écart *relatif* y paraissait plus grand (+127 % contre +13 %).
    //
    //   Ce qui départage un **cliquet** d'un **réservoir qui se vide** n'est pas
    //   le niveau, c'est le **plancher** : un cliquet ne redescend jamais, donc
    //   son minimum monte avec lui ; un réservoir revient à son étiage à chaque
    //   cycle, donc son minimum reste bas quoi que fasse sa médiane.
    //
    //   Et l'**amplitude** répond à une question que ce banc ne savait pas
    //   poser : une oscillation dont `max/min` grossit d'un quart à l'autre
    //   n'est pas amortie. C'est la mesure qui manquait pour la faune — le
    //   multi-seed a montré que le couple proie-prédateur n'a pas de point
    //   d'équilibre et part d'un côté ou de l'autre selon la seed, ce qu'aucune
    //   moyenne de quart ne pouvait dire. —
    let mut cliquets = Vec::new();
    let mut non_amorties = Vec::new();
    // Les grandeurs du **couple proie-prédateur**. L'amplitude d'un chunk sale
    // ou d'une dispersion grossit pour des raisons de store, pas d'écologie :
    // les mélanger ferait dire au verdict que le store n'a pas de point
    // d'équilibre. On sépare donc la liste, et seule la moitié faune reçoit la
    // lecture écologique.
    const FAUNE: [&str; 4] = ["troupeaux", "têtes de gibier", "meutes", "prédateurs"];
    for (nom, f, croissance_est_mauvaise) in [
        ("troupeaux", &(|s: &Sample| s.herds as f64) as &dyn Fn(&Sample) -> f64, true),
        ("têtes de gibier", &|s: &Sample| s.head as f64, true),
        ("meutes", &|s: &Sample| s.packs as f64, false),
        ("prédateurs", &|s: &Sample| s.predators as f64, false),
        ("chunks sales", &|s: &Sample| s.dirty as f64, true),
        ("tuiles marquées", &|s: &Sample| s.marquees as f64, true),
        ("tuiles sur-cap", &|s: &Sample| s.sur_cap as f64, true),
        ("dispersion (km)", &|s: &Sample| s.spread_km, true),
        ("population", &|s: &Sample| s.pop as f64, false),
    ] {
        let (min3, med3, max3) = stats(q3, f);
        let (min4, med4, max4) = stats(q4, f);
        let (d_med, d_min) = (ecart(med3, med4), ecart(min3, min4));
        let (a3, a4) = (amplitude(min3, max3), amplitude(min4, max4));

        // CLIQUET : le niveau **et** le plancher montent — l'effectif ne
        // redescend plus. C'est le seul « sans borne » que ce banc affirme.
        let cliquet = croissance_est_mauvaise
            && d_med > DIVERGENCE_THRESHOLD
            && d_min > DIVERGENCE_THRESHOLD;
        // OSCILLE : le niveau monte, le plancher non. Descriptif, pas une
        // alarme — c'est ici que tombe un réservoir qui se remplit et se vide.
        let oscille = !cliquet && d_med > DIVERGENCE_THRESHOLD;
        // Amplitude qui grossit : l'oscillation n'est pas amortie.
        let enfle = match (a3, a4) {
            (Some(x), Some(y)) => y > x * (1.0 + DIVERGENCE_THRESHOLD),
            // Toucher zéro au dernier quart après une amplitude finie : pire
            // qu'une amplitude qui grossit, l'effectif s'est éteint.
            (Some(_), None) => true,
            _ => false,
        };
        if cliquet {
            cliquets.push(nom);
        }
        if enfle {
            non_amorties.push(nom);
        }
        let fmt_ampl = |a: Option<f64>| match a {
            Some(v) => format!("{v:.1}×"),
            None => "—".to_string(),
        };
        println!(
            "{nom:>22} {med3:>10.1} {med4:>10.1} {:>+6.0}% {:>+6.0}% {:>8} {:>8} {:>11}",
            d_med * 100.0,
            d_min * 100.0,
            fmt_ampl(a3),
            fmt_ampl(a4),
            format!(
                "{}{}",
                if cliquet {
                    "CLIQUET"
                } else if oscille {
                    "OSCILLE"
                } else {
                    ""
                },
                if enfle { " ↑ampl" } else { "" }
            )
        );
    }

    // Le débit est la conséquence, pas la cause : on le rapporte à part, et une
    // *baisse* est ce qui est mauvais — d'où le test inversé.
    let (tmin3, tmed3, tmax3) = stats(q3, &|s: &Sample| s.tps);
    let (tmin4, tmed4, tmax4) = stats(q4, &|s: &Sample| s.tps);
    let rel = ecart(tmed3, tmed4);
    let fmt_ampl = |min: f64, max: f64| match amplitude(min, max) {
        Some(v) => format!("{v:.1}×"),
        None => "—".to_string(),
    };
    println!(
        "{:>22} {tmed3:>10.1} {tmed4:>10.1} {:>+6.0}% {:>+6.0}% {:>8} {:>8} {:>11}",
        "débit (tps)",
        rel * 100.0,
        ecart(tmin3, tmin4) * 100.0,
        fmt_ampl(tmin3, tmax3),
        fmt_ampl(tmin4, tmax4),
        if rel < -DIVERGENCE_THRESHOLD { "S'EFFONDRE" } else { "" }
    );

    println!("\n  ─────────────────────────────────────────────────────────────────");
    if cliquets.is_empty() {
        println!("  AUCUN CLIQUET — aucun effectif ne monte en emportant son plancher.");
    } else {
        println!("  SANS BORNE : {}", cliquets.join(", "));
        println!("  Niveau *et* plancher montent : cet effectif ne redescend plus.");
    }
    let (faune, autres): (Vec<&str>, Vec<&str>) =
        non_amorties.iter().copied().partition(|n| FAUNE.contains(n));
    if faune.is_empty() {
        println!("  Faune : aucune amplitude en expansion — les oscillations s'amortissent.");
    } else {
        println!("  OSCILLATION NON AMORTIE (faune) : {}", faune.join(", "));
        println!("  L'amplitude grossit d'un quart à l'autre, ou l'effectif a touché zéro.");
        println!("  Un couple proie-prédateur qui fait ça n'a pas de point d'équilibre, et");
        println!("  quel côté s'effondre en premier se joue à la seed — ce n'est donc pas");
        println!("  une propriété du modèle qu'on puisse lire sur un seul run.");
    }
    if !autres.is_empty() {
        // Sans lecture écologique : ces amplitudes-là parlent du store et de la
        // géographie, et une seule cause peut les bouger toutes (un monde qui
        // s'éteint arrête de salir des chunks).
        println!("  Amplitude en expansion, hors faune : {}", autres.join(", "));
    }
    println!("╚═════════════════════════════════════════════════════════════════════╝");

    // Courbe brute, à coller dans docs/perf-baseline.txt : une référence de
    // débit doit être une *courbe*, pas un nombre (le run du 2026-09-15 l'a
    // prouvé — 400 ticks mesurés à l'an 1 auraient annoncé un gain de 570 %).
    println!("\n# courbe tps (jour:tps)");
    let stride = (samples.len() / 12).max(1);
    let curve: Vec<String> = samples
        .iter()
        .step_by(stride)
        .map(|s| format!("{}:{:.1}", s.day, s.tps))
        .collect();
    println!("# {}", curve.join(" "));
}
