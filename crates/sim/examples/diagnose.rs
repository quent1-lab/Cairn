//! Banc de **diagnostic** — il ne corrige rien, il mesure.
//!
//! Deux questions posées par un run long (seeds 42 et 7, 105 et 137 ans), dont
//! les logs disaient *que* quelque chose cloche sans dire *quoi* :
//!
//! 1. **Pourquoi les clans ne durent-ils pas ?** Un clan vit ~1,2 an et meurt
//!    petit (médiane 10-15 membres). Trois portes peuvent lui être fermées —
//!    taille, cohésion, dispersion — et les logs ne disent pas laquelle.
//!    `ClanDiagnostics` les compte désormais séparément.
//!
//! 2. **Un savoir se perd-il vraiment quelque part ?** La poterie a été
//!    redécouverte six fois indépendamment chez seed 7, ce qui n'arriverait pas
//!    si elle se transmettait dans la région. Mais `tech::forget` ne constate
//!    qu'une disparition **mondiale**, qui n'arrive jamais à 800 agents. On
//!    compte donc ici les **porteurs** de chaque tech, et leur minimum au fil du
//!    temps : c'est la rareté qui dit si un savoir est menacé (BRIEF §5.4,
//!    « détenu par trop peu de porteurs »).
//!
//! Usage : cargo run --release -p cairn-sim --example diagnose -- [seed] [années] [agents]

use std::collections::BTreeMap;

use cairn_core::{TICKS_PER_DAY, TICKS_PER_YEAR, WorldSeed, km_to_tiles};
use cairn_sim::social::{BOND_THRESHOLD, RESIDENCE_RADIUS_TILES};
use cairn_sim::{
    AgentId, ClanEventKind, ClanId, Knowledge, Sim, TechId, scenario,
};
use cairn_worldgen::Biome;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(42);
    let years: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(25);
    let n_agents: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(60);

    // Même scène que le banc « étincelle » : c'est elle qui a produit les runs
    // qu'on cherche à expliquer.
    let mut sim = Sim::new(WorldSeed(seed), 16384);
    let seed_point = (km_to_tiles(1500.0) as i64, km_to_tiles(2100.0) as i64);
    let home = scenario::find_home_where(
        &mut sim,
        seed_point,
        1.0..=5.0,
        &[Biome::TemperateForest, Biome::Taiga, Biome::Grassland],
    )
    .unwrap_or(seed_point);
    scenario::populate(&mut sim, home, n_agents, 2);

    // Porteurs minimum observés pour chaque tech, échantillonnés chaque jour.
    let mut min_bearers: BTreeMap<u16, usize> = BTreeMap::new();
    // Distribution des liens sociaux : combien sont au-dessus du seuil ?
    let mut bond_samples: Vec<(u64, usize, usize)> = Vec::new();
    // Suivi du plus gros clan, jour après jour : sa composition change-t-elle
    // vraiment, ou est-ce seulement son **identité** qui saute ? C'est la
    // question qui départage « la structure sociale oscille » de « la
    // réconciliation d'identité est trop stricte ».
    let mut prev: Option<(ClanId, std::collections::BTreeSet<AgentId>)> = None;
    let mut jaccards: Vec<f64> = Vec::new();
    let mut jaccard_on_rename: Vec<f64> = Vec::new();
    let mut sizes: Vec<usize> = Vec::new();
    // Le cycle fission/fusion : 554 fusions pour 423 fissions sur 15 ans. Un
    // clan se coupe, les moitiés se recollent, se recoupent. L'hypothèse est
    // qu'une fission **sociale** sans séparation **spatiale** n'en est pas une :
    // deux moitiés au même endroit continuent de se croiser, donc de se relier.
    // On mesure donc la distance qui séparait deux peuples la veille du jour où
    // ils n'en font plus qu'un — et, en regard, la distance entre peuples qui
    // coexistent sans fusionner.
    let mut homes: BTreeMap<u64, (f64, f64)> = BTreeMap::new();
    let mut merge_distances: Vec<f64> = Vec::new();
    let mut coexist_distances: Vec<f64> = Vec::new();
    let mut seen_events = 0usize;

    for _ in 0..years * TICKS_PER_YEAR {
        sim.step();
        if !sim.time.tick.is_multiple_of(TICKS_PER_DAY) {
            continue;
        }
        let mut bearers: BTreeMap<u16, usize> = BTreeMap::new();
        for (_, k) in sim.agents.query::<&Knowledge>().iter() {
            for t in k.iter() {
                *bearers.entry(t.0).or_default() += 1;
            }
        }
        for (tech, count) in bearers {
            let e = min_bearers.entry(tech).or_insert(usize::MAX);
            *e = (*e).min(count);
        }
        // Les fusions du jour, jugées sur les foyers **de la veille** : au tick
        // où l'on regarde, le clan absorbé n'existe déjà plus.
        for e in &sim.clan_events[seen_events..] {
            if let ClanEventKind::Merged { into } = e.kind
                && let (Some(a), Some(b)) = (homes.get(&e.clan.0), homes.get(&into.0))
            {
                merge_distances.push(dist_km(*a, *b));
            }
        }
        seen_events = sim.clan_events.len();
        // Puis les foyers du jour, et l'écartement des peuples qui coexistent.
        homes = sim.clans.iter().map(|c| (c.id.0, c.home)).collect();
        for (i, a) in sim.clans.iter().enumerate() {
            for b in &sim.clans[i + 1..] {
                coexist_distances.push(dist_km(a.home, b.home));
            }
        }

        // Le plus gros clan du jour, comparé à celui d'hier.
        if let Some(big) = sim.clans.iter().max_by_key(|c| (c.members.len(), c.id.0)) {
            sizes.push(big.members.len());
            if let Some((prev_id, prev_members)) = &prev {
                let inter = big.members.intersection(prev_members).count() as f64;
                let union = big.members.union(prev_members).count() as f64;
                let j = if union > 0.0 { inter / union } else { 0.0 };
                jaccards.push(j);
                if *prev_id != big.id {
                    jaccard_on_rename.push(j);
                }
            }
            prev = Some((big.id, big.members.clone()));
        }
        if (sim.time.tick / TICKS_PER_DAY).is_multiple_of(360) {
            let strong = sim.social.bonds.values().filter(|&&w| w >= BOND_THRESHOLD).count();
            bond_samples.push((
                sim.time.tick / TICKS_PER_YEAR,
                sim.social.bonds.len(),
                strong,
            ));
        }
    }

    report(&sim, seed, years, &min_bearers, &bond_samples, &merge_distances, &coexist_distances);

    println!("\n— 6. Le plus gros clan change-t-il de gens, ou seulement de nom ? —");
    let moyenne = |v: &[f64]| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
    println!(
        "  recouvrement moyen d'un jour à l'autre : {:.3}   (1,0 = exactement les mêmes gens)",
        moyenne(&jaccards)
    );
    println!(
        "  … les jours où l'identité change      : {:.3}   sur {} changements",
        moyenne(&jaccard_on_rename),
        jaccard_on_rename.len()
    );
    let stables = jaccard_on_rename.iter().filter(|&&j| j >= 0.5).count();
    println!(
        "  dont {stables} changements de nom avec plus de la moitié des gens en commun \
         → autant d'identités perdues sans raison"
    );
    if !sizes.is_empty() {
        let mut s = sizes.clone();
        s.sort_unstable();
        println!(
            "  taille du plus gros clan : médiane {}, p90 {}, max {}",
            s[s.len() / 2],
            s[(s.len() as f64 * 0.9) as usize],
            s[s.len() - 1]
        );
    }
    let d = &sim.clan_diagnostics;
    println!(
        "  fissions (lignes de faille suivies)    : {}   pour {} validations",
        d.split, d.validated
    );
}

fn report(
    sim: &Sim,
    seed: u64,
    years: u64,
    min_bearers: &BTreeMap<u16, usize>,
    bond_samples: &[(u64, usize, usize)],
    merge_distances: &[f64],
    coexist_distances: &[f64],
) {
    let d = &sim.clan_diagnostics;
    let rejects = d.too_small + d.low_cohesion + d.scattered;
    let total = d.validated + rejects;
    println!("\n=== DIAGNOSTIC — seed {seed}, {years} ans, {} habitants ===", sim.population());

    println!("\n— 1. Pourquoi un groupe n'est-il pas un clan ? ({total} verdicts) —");
    let pct = |n: u64| if total > 0 { 100.0 * n as f64 / total as f64 } else { 0.0 };
    println!("  validés          {:>8}  ({:.1} %)", d.validated, pct(d.validated));
    println!(
        "  trop petits      {:>8}  ({:.1} %)   effectif moyen {:.1}  [seuil : 8]",
        d.too_small,
        pct(d.too_small),
        moy(d.too_small_members as f64, d.too_small),
    );
    println!(
        "  pas assez denses {:>8}  ({:.1} %)   effectif moyen {:.1}, densité moyenne {:.3}  [seuil : 0,280]",
        d.low_cohesion,
        pct(d.low_cohesion),
        moy(d.low_cohesion_members as f64, d.low_cohesion),
        moy(d.low_cohesion_density, d.low_cohesion),
    );
    println!(
        "  trop dispersés   {:>8}  ({:.1} %)   effectif moyen {:.1}, part résidente {:.3}  [seuil : 0,700]",
        d.scattered,
        pct(d.scattered),
        moy(d.scattered_members as f64, d.scattered),
        moy(d.scattered_fraction, d.scattered),
    );
    println!(
        "  dont insécables  {:>8}   (échec sans fission possible ; rayon de résidence {:.1} km)",
        d.unsplittable,
        RESIDENCE_RADIUS_TILES / 500.0,
    );

    println!("\n— 2. Durée de vie des clans —");
    let mut born: BTreeMap<u64, u64> = BTreeMap::new();
    let mut lifespans: Vec<f64> = Vec::new();
    // La question qui a motivé cette distinction : combien de ces « morts »
    // étaient en réalité des peuples qui en rejoignaient un autre ?
    let mut merged: Vec<f64> = Vec::new();
    for e in &sim.clan_events {
        match e.kind {
            ClanEventKind::Formed => {
                born.insert(e.clan.0, e.tick);
            }
            ClanEventKind::Dissolved => {
                if let Some(t0) = born.remove(&e.clan.0) {
                    lifespans.push((e.tick - t0) as f64 / TICKS_PER_YEAR as f64);
                }
            }
            ClanEventKind::Merged { .. } => {
                if let Some(t0) = born.remove(&e.clan.0) {
                    merged.push((e.tick - t0) as f64 / TICKS_PER_YEAR as f64);
                }
            }
        }
    }
    println!(
        "  {} fusions (absorbés par un voisin) contre {} vraies extinctions",
        merged.len(),
        lifespans.len()
    );
    lifespans.sort_by(f64::total_cmp);
    if lifespans.is_empty() {
        println!("  aucun clan dissous");
    } else {
        let n = lifespans.len();
        println!(
            "  {n} clans éteints — médiane {:.2} an, p90 {:.2} an, max {:.2} an, {} encore vivants",
            lifespans[n / 2],
            lifespans[(n as f64 * 0.9) as usize],
            lifespans[n - 1],
            sim.clans.len(),
        );
        let under_year = lifespans.iter().filter(|&&l| l < 1.0).count();
        println!("  {:.0} % n'atteignent pas un an", 100.0 * under_year as f64 / n as f64);
    }

    println!("\n— 3. Les liens sociaux tiennent-ils ? —");
    println!("  {:>4}  {:>8}  {:>8}  {:>7}", "an", "liens", "≥ seuil", "part");
    for &(year, all, strong) in bond_samples {
        println!(
            "  {year:>4}  {all:>8}  {strong:>8}  {:>6.1} %",
            if all > 0 { 100.0 * strong as f64 / all as f64 } else { 0.0 }
        );
    }

    println!("\n— 4. Un savoir est-il menacé ? (porteurs minimum observés) —");
    if min_bearers.is_empty() {
        println!("  aucune technologie découverte");
    }
    for (&tech, &min) in min_bearers {
        let now = sim
            .agents
            .query::<&Knowledge>()
            .iter()
            .filter(|(_, k)| k.has(TechId(tech)))
            .count();
        println!(
            "  {:<28} porteurs aujourd'hui {now:>4}, minimum jamais atteint {min:>4}",
            sim.tech_tree.get(TechId(tech)).label,
        );
    }

    println!("\n— 7. Une fission sépare-t-elle vraiment ? —");
    let mut merges = merge_distances.to_vec();
    let mut coexist = coexist_distances.to_vec();
    match quantiles(&mut merges) {
        Some((p10, med, p90)) => println!(
            "  distance la veille d'une fusion  : médiane {med:.2} km  (p10 {p10:.2}, p90 {p90:.2})  sur {} fusions",
            merges.len()
        ),
        None => println!("  aucune fusion observée"),
    }
    match quantiles(&mut coexist) {
        Some((p10, med, p90)) => println!(
            "  distance entre peuples coexistants: médiane {med:.2} km  (p10 {p10:.2}, p90 {p90:.2})",
            ),
        None => println!("  jamais deux peuples à la fois"),
    }
    println!(
        "  [rayon de résidence {:.1} km — en deçà, deux foyers se rappellent leurs membres au même endroit]",
        RESIDENCE_RADIUS_TILES / 500.0
    );

    println!("\n— 5. Redécouvertes (le même savoir trouvé plusieurs fois) —");
    let mut discoveries: BTreeMap<u16, Vec<(u64, Option<ClanId>, AgentId)>> = BTreeMap::new();
    for e in &sim.tech_events {
        if matches!(e.kind, cairn_sim::TechEventKind::Discovered)
            && let Some(agent) = e.agent
        {
            discoveries.entry(e.tech.0).or_default().push((e.tick, e.clan, agent));
        }
    }
    for (tech, list) in &discoveries {
        if list.len() > 1 {
            println!(
                "  {:<28} {} fois : ans {}",
                sim.tech_tree.get(TechId(*tech)).label,
                list.len(),
                list.iter()
                    .map(|(t, _, _)| (t / TICKS_PER_YEAR).to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
    }
    if discoveries.values().all(|l| l.len() <= 1) {
        println!("  aucune redécouverte");
    }
}

fn moy(sum: f64, n: u64) -> f64 {
    if n == 0 { 0.0 } else { sum / n as f64 }
}

/// Distance entre deux foyers, en kilomètres (1 tuile = 2 m).
fn dist_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1) / 500.0
}

/// Médiane, p10 et p90 d'un échantillon — de quoi juger une distribution sans
/// se laisser tromper par une moyenne.
fn quantiles(v: &mut [f64]) -> Option<(f64, f64, f64)> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let at = |q: f64| v[((v.len() as f64 - 1.0) * q).round() as usize];
    Some((at(0.1), at(0.5), at(0.9)))
}
