//! Les structures bâties par les clans (BRIEF §5.1, Phase 4 « LE CLAN »,
//! incrément 9 — le dernier).
//!
//! Un clan qui a un territoire (incrément 7), un stock (incrément 3) et un
//! chef (incrément 8) finit par **bâtir**. Trois structures, trois effets de
//! jeu réels, chacun branché sur un système déjà là plutôt qu'inventé pour
//! l'occasion :
//!
//! - **Hutte** (`Hut`) : chaleur passive. Un membre à portée d'une hutte de
//!   **son** clan gagne quelques degrés ressentis, sans avoir à s'arrêter
//!   pour s'abriter (`Activity::Sheltering`) — c'est ce qui rend un foyer
//!   fixe survivable là où l'errance ne l'était pas.
//! - **Grenier** (`Granary`) : double la durée de vie de la réserve commune
//!   du clan (qui pourrit, voir `sim::FRESH_KEEP_DAYS`), donc ce qu'il peut
//!   garder contre la disette. Désiré quand la réserve dépasse l'échelle
//!   `STOCK_SCALE_PER_MEMBER × membres`.
//! - **Palissade** (`Palisade`) : la tension inter-clans (incrément 6) monte
//!   **moins vite** contre un clan protégé — un groupe qui tient sa position
//!   escalade moins. Pas de combat pour autant (`force`/`agressivité`
//!   restent en attente, Phase 3) : l'effet reste au niveau de la tension
//!   *mesurée*, cohérent avec la portée de l'incrément 6.
//!
//! ## Ni scriptée, ni instantanée par décret
//!
//! Le BRIEF interdit de scripter (« si le clan atteint la taille N, bâtir un
//! campement »). Comme partout dans cette phase, la construction est une
//! **conséquence mesurée**, à deux niveaux :
//!
//! 1. **Quoi bâtir** (`plan`, passe quotidienne) : chaque clan mesure ses
//!    trois *pressions* — froid moyen de ses membres, remplissage de son
//!    stock, tension maximale avec un voisin — et « désire » la structure,
//!    encore non bâtie, dont la pression dépasse le plus largement son seuil
//!    (rapport pression/seuil, une comparaison à échelle commune, jamais une
//!    priorité fixe). Un clan au chaud, à stock vide et sans voisin ne désire
//!    rien.
//! 2. **Qui bâtit** (`TaskKind::Build`, `brain::decide` → `sim::execute`) :
//!    un membre adulte, au foyer, dont les besoins immédiats sont couverts
//!    (on bâtit repu, comme on explore repu), propose de bâtir la structure
//!    désirée **si le clan a de quoi la payer** — la construction consomme le
//!    stock commun (le coût du BRIEF), un candidat de plus dans le même
//!    softmax que tous les autres. Le premier arrivé bâtit ; les suivants
//!    trouvent la structure déjà là et re-délibèrent (aucun doublon possible,
//!    un type par clan).
//!
//! ## Un registre, pas un champ de tuile
//!
//! Comme le territoire (incrément 7), les structures **ne vivent pas dans
//! `Tile`** malgré le champ `structure` que le BRIEF §2.3 anticipait : elles
//! sont rares (au plus trois par clan) et clairsemées, un `Vec<Structure>`
//! côté `Sim` les porte sans peser sur les 16 octets de chaque tuile ni sur
//! l'éviction LRU (voir la leçon de l'incrément 7). Un clan qui s'efface
//! emporte ses structures (`prune_orphans`) — un campement abandonné, comme
//! le stock qui disparaît avec son clan.

use std::collections::{BTreeMap, BTreeSet};

use crate::agent::{Physiology, Prestige};
use crate::sim::{STOCK_SCALE_PER_MEMBER, Sim};
use crate::skills::Skills;
use crate::social::{ClanId, ClanMembership};
use cairn_core::km_to_tiles;

/// Les structures qu'un clan peut bâtir. `repr(u8)` implicite via l'ordre :
/// `Ord` sert au tri déterministe du registre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub enum StructureKind {
    Hut,
    Granary,
    Palisade,
    /// La hutte du chef : le siège du clan. Contrairement aux trois autres,
    /// son « effet de jeu » n'est pas physiologique — elle **fixe le foyer**
    /// (`Clan::home` cesse d'être le centroïde flottant du jour et devient sa
    /// position, voir `anchor_homes`). Un clan qui la bâtit **se sédentarise**
    /// et cesse de dériver ; avant, il reste semi-nomade. Son moteur émergent
    /// est le **prestige du chef** (gagné en nourrissant le clan, incrément
    /// 8) — un clan mené avec succès sur la durée finit par poser ses pierres.
    ChiefHut,
}

impl StructureKind {
    /// Coût de construction, en unités de stock commun (`Clan::stock`).
    pub fn cost(self) -> f32 {
        match self {
            StructureKind::Hut => HUT_COST,
            StructureKind::Granary => GRANARY_COST,
            StructureKind::Palisade => PALISADE_COST,
            StructureKind::ChiefHut => CHIEF_HUT_COST,
        }
    }
}

// — Coûts (unités de stock) —
pub const HUT_COST: f32 = 6.0;
pub const GRANARY_COST: f32 = 9.0;
pub const PALISADE_COST: f32 = 12.0;
/// Plus cher : c'est un chantier de sédentarisation, pas un abri d'appoint.
pub const CHIEF_HUT_COST: f32 = 15.0;

// — Effets —
/// Chaleur passive (°C ressentis) qu'une hutte de son clan apporte à un
/// membre à portée. Un peu moins qu'un abri actif (`SHELTER_BONUS_C = 8`) :
/// la hutte aide sans dispenser de se mettre vraiment à couvert par grand
/// froid.
pub const HUT_WARMTH_C: f64 = 6.0;
/// Rayon d'effet d'une hutte (~300 m) : on en profite en vivant à côté.
pub const HUT_RADIUS_TILES: f64 = km_to_tiles(0.3);
/// Un grenier double la durée de vie de la réserve : au sec, hors d'atteinte
/// des bêtes. Plusieurs greniers ne font pas mieux qu'un.
pub const GRANARY_KEEP_FACTOR: f32 = 2.0;
/// Facteur appliqué à la montée de tension contre un clan qui a une
/// palissade : elle monte à 40 % de son rythme normal.
pub const PALISADE_TENSION_FACTOR: f32 = 0.4;

// — Seuils de pression (quand un clan « désire » une structure) —
/// Froid moyen des membres au-delà duquel une hutte devient désirable.
const COLD_PRESSURE_THRESHOLD: f32 = 0.15;
/// Remplissage du stock (stock / échelle `STOCK_SCALE_PER_MEMBER × membres`)
/// au-delà duquel un grenier
/// devient désirable.
const STORAGE_PRESSURE_THRESHOLD: f32 = 0.8;
/// Tension maximale avec un voisin au-delà de laquelle une palissade devient
/// désirable.
const THREAT_PRESSURE_THRESHOLD: f32 = 0.3;
/// Score du chef (`oratoire × prestige`) au-delà duquel le clan désire bâtir
/// la hutte du chef et se sédentariser. Calibré sur un run réel (banc
/// `chronicle`, colonne `chief_score_max`) : le score d'un chef établi
/// dépasse largement cette valeur après un an ou deux de provisionnement,
/// mais pas un chef fraîchement désigné d'un clan neuf — la sédentarisation
/// arrive donc quand le clan a fait ses preuves, sans lire ni sa taille ni
/// son âge.
const CHIEF_PRESTIGE_THRESHOLD: f32 = 2.5;
/// Une structure qu'aucun clan ne revendient depuis ce nombre de jours tombe
/// en ruine (un mois : le temps qu'un campement vraiment abandonné
/// disparaisse, mais assez pour survivre au simple va-et-vient des identités
/// de clan d'un jour à l'autre).
const STRUCTURE_DECAY_DAYS: u64 = 30;

/// Une structure bâtie : son type, le clan qui la possède **actuellement**,
/// sa position (le foyer du clan qui l'a bâtie) et sa date. `Copy` —
/// quelques octets, comme les autres composants légers de la simulation.
///
/// Le champ `clan` n'est pas figé au bâtisseur : une structure appartient à
/// **qui contrôle sa tuile** (`social::claim_at`, incrément 7), réévalué
/// chaque jour (`maintain`). C'est ce qui la fait survivre au va-et-vient
/// des identités de clan : quand un clan s'efface et qu'un clan quasi
/// identique se reforme au même endroit (mêmes gens, nouvel identifiant), il
/// **hérite** du campement plutôt que de le voir disparaître. Une structure
/// que plus aucun clan ne revendique est *abandonnée* (`abandoned_since`) et
/// finit par tomber en ruine (`STRUCTURE_DECAY_DAYS`).
#[derive(Debug, Clone, Copy)]
pub struct Structure {
    pub kind: StructureKind,
    pub clan: ClanId,
    pub pos: (f64, f64),
    pub built_tick: u64,
    /// Jour où la structure s'est retrouvée sans clan pour la revendiquer,
    /// `None` tant qu'elle est occupée. Sert à la faire décliner si
    /// l'abandon dure (voir `maintain`).
    pub abandoned_since: Option<u64>,
}

/// La chaleur passive ressentie en `pos` par un membre du clan `clan` : un
/// bonus fixe si au moins une hutte de **son** clan est à portée (les huttes
/// ne se cumulent pas — une seule suffit à s'abriter). Un non-membre ne
/// profite pas des huttes d'autrui, et un agent sans clan n'en profite pas
/// du tout. `huts` est l'instantané `(x, y, clan)` des huttes existantes.
pub fn hut_warmth(pos: (f64, f64), clan: Option<ClanId>, huts: &[(f64, f64, ClanId)]) -> f64 {
    let Some(clan) = clan else { return 0.0 };
    let sheltered = huts.iter().any(|&(hx, hy, hc)| {
        hc == clan && (pos.0 - hx).hypot(pos.1 - hy) <= HUT_RADIUS_TILES
    });
    if sheltered { HUT_WARMTH_C } else { 0.0 }
}

/// Facteur de durée de vie de la réserve d'un clan : `GRANARY_KEEP_FACTOR`
/// s'il a un grenier, 1 sinon.
pub fn granary_keep_factor(clan: ClanId, structures: &[Structure]) -> f32 {
    if structures.iter().any(|s| s.clan == clan && s.kind == StructureKind::Granary) {
        GRANARY_KEEP_FACTOR
    } else {
        1.0
    }
}

/// Le clan a-t-il une palissade ? (lu par `social::update_relations`).
pub fn has_palisade(clan: ClanId, structures: &[Structure]) -> bool {
    structures.iter().any(|s| s.clan == clan && s.kind == StructureKind::Palisade)
}

/// Passe quotidienne : mesure les pressions de chaque clan et fixe la
/// structure qu'il désire (voir le commentaire de module). Reconstruit
/// `Clan::desired` en entier, comme tout le reste de l'état de clan.
pub(crate) fn plan(sim: &mut Sim) {
    // Structures déjà présentes, par clan (pour ne désirer que ce qui manque).
    let mut existing: BTreeMap<u64, BTreeSet<StructureKind>> = BTreeMap::new();
    for s in &sim.structures {
        existing.entry(s.clan.0).or_default().insert(s.kind);
    }
    // Pression thermique : froid moyen des membres, par clan.
    let mut cold: BTreeMap<u64, (f32, u32)> = BTreeMap::new();
    for (_, (membership, phys)) in sim.agents.query::<(&ClanMembership, &Physiology)>().iter() {
        if let Some(cid) = membership.0 {
            let e = cold.entry(cid.0).or_insert((0.0, 0));
            e.0 += phys.cold;
            e.1 += 1;
        }
    }
    // Pression de menace : tension maximale de chaque clan avec un voisin.
    let mut threat: BTreeMap<u64, f32> = BTreeMap::new();
    for (&(a, b), &t) in &sim.clan_relations.tension {
        let ea = threat.entry(a).or_insert(0.0);
        *ea = ea.max(t);
        let eb = threat.entry(b).or_insert(0.0);
        *eb = eb.max(t);
    }
    // Score de chaque agent (`oratoire × prestige`) : sert à lire celui du
    // chef de chaque clan — la pression qui fait bâtir la hutte du chef.
    let mut score: BTreeMap<u64, f32> = BTreeMap::new();
    for (_, (id, skills, prestige)) in
        sim.agents.query::<(&crate::agent::AgentId, &Skills, &Prestige)>().iter()
    {
        score.insert(id.0, skills.oratory * prestige.0);
    }

    for clan in &mut sim.clans {
        let present = existing.get(&clan.id.0);
        let mean_cold =
            cold.get(&clan.id.0).map(|&(sum, n)| sum / n.max(1) as f32).unwrap_or(0.0);
        let base_cap = STOCK_SCALE_PER_MEMBER * clan.members.len() as f32;
        let storage = if base_cap > 0.0 { clan.stock / base_cap } else { 0.0 };
        let menace = threat.get(&clan.id.0).copied().unwrap_or(0.0);
        let chief_score = score.get(&clan.chief.0).copied().unwrap_or(0.0);

        // Chaque candidat non encore bâti dont la pression dépasse son seuil,
        // départagé par la marge relative (pression / seuil) — comparaison à
        // échelle commune entre des pressions de natures différentes, jamais
        // une priorité fixe entre les types.
        let candidates = [
            (StructureKind::Hut, mean_cold, COLD_PRESSURE_THRESHOLD),
            (StructureKind::Granary, storage, STORAGE_PRESSURE_THRESHOLD),
            (StructureKind::Palisade, menace, THREAT_PRESSURE_THRESHOLD),
            (StructureKind::ChiefHut, chief_score, CHIEF_PRESTIGE_THRESHOLD),
        ];
        let mut best: Option<(StructureKind, f32)> = None;
        for (kind, pressure, threshold) in candidates {
            let lacks = present.is_none_or(|s| !s.contains(&kind));
            if lacks && pressure > threshold {
                let ratio = pressure / threshold;
                if best.is_none_or(|(_, r)| ratio > r) {
                    best = Some((kind, ratio));
                }
            }
        }
        clan.desired = best.map(|(k, _)| k);
    }
}

/// Ancre le foyer des clans sédentarisés à leur hutte du chef. Un clan qui en
/// a bâti une voit `Clan::home` (le point qui *attire* ses membres via
/// `TaskKind::ReturnToClan`) figé sur la position de la hutte, au lieu du
/// centroïde flottant recalculé chaque jour par `detect_clans` — c'est ce qui
/// empêche le clan de dériver sans fin (voir le run 5 ans : sans ancre, les
/// clans s'écartaient jusqu'à des dizaines de km). Appelée **en dernier** dans
/// la passe quotidienne, pour que la valeur ancrée soit celle lue par la
/// délibération du lendemain. La **détection** de clan, elle, continue
/// d'utiliser le centroïde vivant (calculé localement dans `detect_clans`),
/// pas `home` — un clan momentanément parti chasser ne se dissout donc pas ;
/// et comme le rappel ramène les membres vers la hutte, le centroïde reste de
/// toute façon près d'elle.
pub(crate) fn anchor_homes(sim: &mut Sim) {
    let mut anchor: BTreeMap<u64, (f64, f64)> = BTreeMap::new();
    for s in &sim.structures {
        if s.kind == StructureKind::ChiefHut {
            anchor.insert(s.clan.0, s.pos);
        }
    }
    for clan in &mut sim.clans {
        if let Some(&pos) = anchor.get(&clan.id.0) {
            clan.home = pos;
        }
    }
}

/// Entretien quotidien du registre (voir la doc de `Structure`) : chaque
/// structure appartient à qui contrôle sa tuile aujourd'hui.
///
/// - Si son clan actuel existe encore : rien à faire.
/// - Sinon, si un autre clan revendient sa tuile (`social::claim_at`) : elle
///   change de mains — un campement repris par les nouveaux occupants.
/// - Sinon : elle est abandonnée ; on note depuis quand, et elle tombe en
///   ruine passé `STRUCTURE_DECAY_DAYS`.
pub(crate) fn maintain(sim: &mut Sim, day: u64) {
    // On sort les clans le temps du `retain_mut` pour emprunter `structures`
    // en mutable sans conflit — `claim_at` n'a besoin que d'un `&[Clan]`.
    let clans = std::mem::take(&mut sim.clans);
    let alive: BTreeSet<u64> = clans.iter().map(|c| c.id.0).collect();
    sim.structures.retain_mut(|s| {
        if alive.contains(&s.clan.0) {
            s.abandoned_since = None;
            true
        } else if let Some(owner) = crate::social::claim_at(s.pos, &clans) {
            s.clan = owner;
            s.abandoned_since = None;
            true
        } else {
            let since = *s.abandoned_since.get_or_insert(day);
            day.saturating_sub(since) < STRUCTURE_DECAY_DAYS
        }
    });
    sim.clans = clans;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentId;
    use crate::social::Clan;
    use cairn_core::WorldSeed;

    fn clan_with(id: u64, members: usize, stock: f32) -> Clan {
        Clan {
            id: ClanId(id),
            founded_tick: 0,
            members: (0..members as u64).map(AgentId).collect(),
            home: (0.0, 0.0),
            stock,
            chief: AgentId(0),
            desired: None,
            rivalry: 0.0,
        }
    }

    /// « Quoi bâtir » — le froid appelle une hutte. Quatre membres
    /// frigorifiés, stock vide, aucun voisin : la seule pression au-dessus de
    /// son seuil est thermique.
    #[test]
    fn un_clan_dont_les_membres_ont_froid_desire_une_hutte() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        for _ in 0..4 {
            sim.spawn_agent(0.0, 0.0);
        }
        for (_, (m, phys)) in sim.agents.query_mut::<(&mut ClanMembership, &mut Physiology)>() {
            m.0 = Some(ClanId(1));
            phys.cold = 0.5;
        }
        sim.clans.push(clan_with(1, 4, 0.0));
        plan(&mut sim);
        assert_eq!(sim.clans[0].desired, Some(StructureKind::Hut));
    }

    /// Un stock proche du plafond appelle un grenier — et rien d'autre ne
    /// presse (personne n'a froid, aucun voisin). `base_cap = 3 × 10 = 30` ;
    /// un stock de 27 met le remplissage à 0,9, au-dessus du seuil de 0,8.
    #[test]
    fn un_clan_au_stock_plein_desire_un_grenier() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        sim.clans.push(clan_with(1, 10, 27.0));
        plan(&mut sim);
        assert_eq!(sim.clans[0].desired, Some(StructureKind::Granary));
    }

    /// Aucune pression au-dessus de son seuil : le clan ne désire rien. Un
    /// membre à peine frais (cold 0,05 < seuil), stock quasi vide, pas de
    /// voisin — bâtir n'aurait aucun sens, et rien ne le force.
    #[test]
    fn un_clan_sans_pression_ne_desire_rien() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        sim.spawn_agent(0.0, 0.0);
        for (_, (m, phys)) in sim.agents.query_mut::<(&mut ClanMembership, &mut Physiology)>() {
            m.0 = Some(ClanId(1));
            phys.cold = 0.05;
        }
        sim.clans.push(clan_with(1, 8, 1.0));
        plan(&mut sim);
        assert_eq!(sim.clans[0].desired, None);
    }

    /// Ce qui est déjà bâti n'est plus désiré : mêmes membres frigorifiés,
    /// mais une hutte existe déjà → le clan ne redésire pas une hutte (et
    /// aucune autre pression ne prend le relais ici).
    #[test]
    fn une_structure_deja_batie_n_est_plus_desiree() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        for _ in 0..4 {
            sim.spawn_agent(0.0, 0.0);
        }
        for (_, (m, phys)) in sim.agents.query_mut::<(&mut ClanMembership, &mut Physiology)>() {
            m.0 = Some(ClanId(1));
            phys.cold = 0.5;
        }
        sim.clans.push(clan_with(1, 4, 0.0));
        sim.structures.push(Structure {
            kind: StructureKind::Hut,
            clan: ClanId(1),
            pos: (0.0, 0.0),
            built_tick: 0,
            abandoned_since: None,
        });
        plan(&mut sim);
        assert_eq!(sim.clans[0].desired, None, "une hutte déjà là ne se redésire pas");
    }

    /// « Quoi bâtir » — un chef au fort prestige appelle la hutte du chef
    /// (sédentarisation). Rien d'autre ne presse (pas de froid, stock vide,
    /// pas de voisin), et le score du chef (oratoire × prestige) dépasse son
    /// seuil : le clan désire se poser.
    #[test]
    fn un_clan_au_chef_prestigieux_desire_une_hutte_de_chef() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        for _ in 0..5 {
            sim.spawn_agent(0.0, 0.0);
        }
        // Le chef (AgentId(0), cf. `clan_with`) a un score bien au-dessus du
        // seuil ; les autres restent à zéro.
        for (_, (id, skills, prestige)) in
            sim.agents.query_mut::<(&AgentId, &mut Skills, &mut Prestige)>()
        {
            if id.0 == 0 {
                skills.oratory = 0.8;
                prestige.0 = 6.0; // score 4,8 > CHIEF_PRESTIGE_THRESHOLD
            }
        }
        for (_, m) in sim.agents.query_mut::<&mut ClanMembership>() {
            m.0 = Some(ClanId(1));
        }
        sim.clans.push(clan_with(1, 5, 0.0));
        plan(&mut sim);
        assert_eq!(sim.clans[0].desired, Some(StructureKind::ChiefHut));
    }

    /// Le cœur de l'ancrage : une fois la hutte du chef bâtie, `Clan::home`
    /// est **figé** sur sa position au lieu de suivre le centroïde flottant —
    /// c'est ce qui empêche le clan de dériver sans fin (run 5 ans : sans
    /// ancre, les clans s'écartaient jusqu'à des dizaines de km).
    #[test]
    fn la_hutte_du_chef_ancre_le_foyer() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let mut clan = clan_with(1, 5, 0.0);
        clan.home = (0.0, 0.0); // foyer « flottant » de départ
        sim.clans.push(clan);
        sim.structures.push(Structure {
            kind: StructureKind::ChiefHut,
            clan: ClanId(1),
            pos: (5000.0, -3000.0),
            built_tick: 0,
            abandoned_since: None,
        });
        anchor_homes(&mut sim);
        assert_eq!(sim.clans[0].home, (5000.0, -3000.0), "le foyer doit être figé sur la hutte du chef");
    }

    /// Contre-épreuve : sans hutte du chef, `anchor_homes` ne touche pas au
    /// foyer (le clan reste semi-nomade, foyer = centroïde comme avant).
    #[test]
    fn sans_hutte_du_chef_le_foyer_n_est_pas_ancre() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        let mut clan = clan_with(1, 5, 0.0);
        clan.home = (123.0, 456.0);
        sim.clans.push(clan);
        anchor_homes(&mut sim);
        assert_eq!(sim.clans[0].home, (123.0, 456.0), "sans hutte, le foyer flottant reste inchangé");
    }

    /// Le cœur du correctif de persistance : quand le clan bâtisseur s'efface
    /// mais qu'un autre clan contrôle désormais la tuile de la structure
    /// (`claim_at`), celle-ci **change de mains** au lieu de disparaître —
    /// c'est ce qui la fait survivre au va-et-vient des identités de clan.
    #[test]
    fn une_structure_orpheline_est_reprise_par_le_clan_qui_tient_le_lieu() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        // Le clan 1 (bâtisseur) n'existe plus ; le clan 2, chez lui à côté,
        // contrôle la tuile.
        sim.clans.push(clan_with(2, 10, 0.0)); // foyer en (0,0)
        sim.structures.push(Structure {
            kind: StructureKind::Hut,
            clan: ClanId(1),
            pos: (5.0, 0.0),
            built_tick: 0,
            abandoned_since: None,
        });
        maintain(&mut sim, 10);
        assert_eq!(sim.structures.len(), 1, "la structure ne doit pas disparaître : le lieu est tenu");
        assert_eq!(sim.structures[0].clan, ClanId(2), "elle passe au clan qui contrôle sa tuile");
    }

    /// Une structure que plus aucun clan ne revendient tombe en ruine — mais
    /// seulement après `STRUCTURE_DECAY_DAYS`, pas au premier jour d'abandon
    /// (sans quoi le simple va-et-vient des identités la ferait disparaître).
    #[test]
    fn une_structure_vraiment_abandonnee_tombe_en_ruine_apres_le_delai() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        // Aucun clan : la structure est orpheline et son lieu n'est revendiqué
        // par personne.
        sim.structures.push(Structure {
            kind: StructureKind::Hut,
            clan: ClanId(1),
            pos: (0.0, 0.0),
            built_tick: 0,
            abandoned_since: None,
        });
        maintain(&mut sim, 100); // premier jour d'abandon constaté
        assert_eq!(sim.structures.len(), 1, "un abandon tout frais ne ruine pas encore");
        assert_eq!(sim.structures[0].abandoned_since, Some(100));
        maintain(&mut sim, 100 + STRUCTURE_DECAY_DAYS - 1);
        assert_eq!(sim.structures.len(), 1, "toujours dans le délai de grâce");
        maintain(&mut sim, 100 + STRUCTURE_DECAY_DAYS);
        assert!(sim.structures.is_empty(), "passé le délai, la ruine emporte la structure");
    }

    #[test]
    fn une_hutte_rechauffe_les_membres_proches_de_son_clan_seulement() {
        let hut = (0.0, 0.0, ClanId(1));
        let huts = [hut];
        // Membre du clan 1, juste à côté : réchauffé.
        assert_eq!(hut_warmth((10.0, 0.0), Some(ClanId(1)), &huts), HUT_WARMTH_C);
        // Membre d'un autre clan, même position : rien.
        assert_eq!(hut_warmth((10.0, 0.0), Some(ClanId(2)), &huts), 0.0);
        // Membre du clan 1 mais hors de portée : rien.
        assert_eq!(hut_warmth((HUT_RADIUS_TILES + 10.0, 0.0), Some(ClanId(1)), &huts), 0.0);
        // Agent sans clan : rien.
        assert_eq!(hut_warmth((10.0, 0.0), None, &huts), 0.0);
    }

    #[test]
    fn le_grenier_allonge_la_vie_de_la_reserve() {
        let st = |kind, clan| Structure { kind, clan, pos: (0.0, 0.0), built_tick: 0, abandoned_since: None };
        let structures = [
            st(StructureKind::Granary, ClanId(1)),
            st(StructureKind::Hut, ClanId(1)),
            st(StructureKind::Granary, ClanId(2)),
        ];
        assert_eq!(granary_keep_factor(ClanId(1), &structures), GRANARY_KEEP_FACTOR);
        assert_eq!(granary_keep_factor(ClanId(3), &structures), 1.0, "un clan sans grenier ne garde pas mieux");
    }
}
