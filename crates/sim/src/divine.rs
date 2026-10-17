//! **Les interventions divines** (BRIEF §6.2) — le seul pouvoir du joueur.
//!
//! Le joueur n'a aucun contrôle direct sur un agent. Il agit sur **le monde**,
//! et observe le résultat.
//!
//! ## L'ambivalence n'est pas une option
//!
//! > « Toutes les interventions sont volontairement ambivalentes. Aucune n'est
//! > un bouton bien. »
//!
//! Ce n'est pas une contrainte esthétique, c'est le moteur du culte émergent
//! (§6.3) : un peuple ne peut se construire une théologie *cohérente avec le
//! comportement réel* de sa divinité que si ce comportement est lisible sans
//! être univoque. Une foudre qui ne ferait que du bien produirait un dieu
//! unanimement adoré — donc aucune histoire.
//!
//! L'ambivalence n'est donc **jamais** un tirage « une chance sur deux de
//! rater ». Elle est **géographique et gratuite** : c'est le lieu et le moment
//! choisis qui décident du bien et du mal, et le joueur en assume le choix.
//! Frapper une savane sèche près d'un peuple sans silex lui offre le feu ;
//! frapper vingt mètres trop près le tue. Rien dans le code ne pondère « bon »
//! contre « mauvais » : les deux tombent des mêmes règles locales.
//!
//! ## Un journal, donc un monde rejouable
//!
//! Le BRIEF §8.2 promet le « replay complet du monde depuis la seed + le
//! journal des interventions divines ». C'est la raison d'être de
//! [`Sim::miracles`] : la simulation étant déterministe, **seed + ce journal**
//! suffisent à rejouer une histoire entière. Une intervention n'est donc pas un
//! appel de fonction qu'on oublie, c'est un **fait daté** qu'on garde.
//!
//! ## Une porte unique
//!
//! [`Sim::invoke`] est le seul chemin. Le client WASM fait tourner le `Sim` en
//! local et l'appelle directement ; le jour où le serveur existera (Phase 6,
//! partie réseau), cette signature deviendra l'endpoint sans que la simulation
//! change d'une ligne.
//!
//! **La Foi ne coûte rien pour l'instant** (arbitrage acté) : elle arrive à
//! l'incrément suivant. Sans croyants, aucun miracle ne serait finançable, donc
//! aucun ne serait testable — on branche donc le coût quand il y aura de quoi
//! le payer.

use cairn_core::km_to_tiles;

use crate::agent::{AgentId, DeathCause, Physiology, Position};
use crate::chronicle::EventKind;
use crate::demography::Demographics;
use crate::fire;
use crate::sim::Sim;
use crate::weather::WeatherKind;

/// Rayon dans lequel la foudre tue net (~20 m). Court : c'est le prix d'un
/// geste mal placé, pas une arme de zone.
const LIGHTNING_LETHAL_TILES: f64 = 10.0;

/// Ce que le joueur peut demander au monde. Chaque variante porte sa cible :
/// une intervention est un geste **situé**, jamais global.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Intervention {
    /// La foudre frappe une tuile. Elle allume un feu **si le site peut
    /// brûler** — c'est le sol qui décide, pas la divinité — et tue net qui se
    /// trouvait dessous. Les deux dans le même geste : « peut *donner le feu* à
    /// un clan… ou le brûler vif ».
    Lightning { pos: (i64, i64) },

    /// La santé rendue à qui vit alentour (§6.2, « fertilité »).
    ///
    /// Aucune fécondité n'est touchée directement : c'est **la santé qui est le
    /// levier**, puisque la conception exige déjà un corps en état
    /// (`demography::daily`). Rendre la santé, c'est donc lever le frein
    /// malthusien — et voilà l'ambivalence, qui n'a pas besoin d'être codée :
    /// un peuple béni au mauvais moment fait des enfants que la terre ne peut
    /// pas nourrir. Les nouveau-nés sont improductifs et coûteux ; si le
    /// fourrage ne suit pas, la bénédiction se paie en famine l'hiver suivant.
    Fertility { pos: (i64, i64) },

    /// Le contraire, du même geste : la santé retirée. Ce qui descend à zéro
    /// meurt (`DeathCause::Disease`).
    ///
    /// Son revers est plus cruel que la mort : une épidémie emporte des
    /// **porteurs de savoir**. Si les derniers qui savaient cuire l'argile
    /// tombent ensemble, la technique quitte le monde (`tech::forget`) — le
    /// « l'humanité peut régresser » du §5.4, cette fois de la main du joueur.
    Plague { pos: (i64, i64) },

    /// Une averse (§6.2, « pluie »). Elle ne fait pas apparaître d'eau : elle
    /// déclenche une **cellule météo** ordinaire (`crate::weather`), celle-là
    /// même qui se forme d'elle-même dans un ciel sans dieu. Ce que le joueur
    /// obtient, c'est une averse, pas un miracle d'un genre particulier.
    ///
    /// Ambivalence, qu'aucune ligne n'écrit : la pluie nourrit la végétation —
    /// et **éteint la voie du feu**. Un peuple sans silex ne découvre la
    /// maîtrise du feu qu'en voyant brûler ; bénir sa vallée d'une pluie
    /// généreuse, c'est le condamner à ne jamais voir d'incendie.
    Rain { pos: (i64, i64) },

    /// La même chose, de l'autre signe. La sécheresse affame — et c'est le
    /// moteur de l'invention (`pressure`), en même temps qu'elle rend la
    /// brousse inflammable. La disette qui pousse à penser et le feu qui attend
    /// d'être vu arrivent par le même geste.
    Drought { pos: (i64, i64) },

    /// **La révélation** : souffler un insight à un homme précis (§6.2, « très
    /// coûteux »). Voir `tech::reveal` — elle n'offre pas un savoir, elle fait
    /// comprendre ce qu'on avait déjà sous les yeux, et tous les prérequis
    /// restent exigés.
    ///
    /// La seule intervention qui vise un **être** et non un lieu, donc la seule
    /// qui puisse échouer faute de cible (`DivineError::NoSuchTarget`) : celui
    /// qu'on inspirait a pu mourir entre-temps.
    Revelation { agent: AgentId },

    /// **Le Signe** : désigner un lieu comme sacré (§6.2). La seule intervention
    /// **sans effet direct** — elle ne brûle rien, ne soigne personne. Elle
    /// n'agit que parce que des hommes y croient, ce qui explique qu'elle ait
    /// attendu qu'il y en ait.
    ///
    /// Son ambivalence est la plus belle du lot, et elle est entièrement
    /// empruntée : le sanctuaire attire les fidèles de **tous** les peuples qui
    /// y croient, donc rapproche leurs foyers, donc fait monter la tension que
    /// `social::update_relations` mesure déjà à cette distance-là. « Les clans y
    /// bâtissent, s'y rassemblent… **et s'y battent**. » Pas une ligne pour ça.
    Sign { pos: (i64, i64) },
}

/// Rayon d'une bénédiction ou d'une épidémie (~500 m) : un campement, pas une
/// région. Le joueur touche un peuple, pas une civilisation.
const BLESSING_RADIUS_TILES: f64 = 250.0;
/// À quelle distance on voit tomber la foudre (~1 km) : de bien plus loin
/// qu'elle ne tue, comme on voit un incendie de plus loin qu'il ne brûle.
const LIGHTNING_SIGHT_TILES: f64 = 500.0;
/// Étendue sous laquelle on ressent une averse ou une sécheresse : celle de la
/// cellule météo elle-même (~1,5 km).
const WEATHER_SIGHT_TILES: f64 = 750.0;

/// Qui témoigne d'un prodige, et comment il le prend — ou `None` si le geste
/// **ne fait aucun croyant**.
///
/// Deux règles, et elles portent tout l'équilibre du §6.1.
///
/// **Un dieu se juge à ses effets.** Un geste qui ne produit rien ne convainc
/// personne : une foudre dans le vide, une grâce sur une lande déserte, une
/// révélation qui n'éclaire pas. Le ciel a beau s'agiter, s'il ne se passe rien,
/// il n'y a rien à croire.
///
/// **La météo se juge à part**, et pas ici : voir `faith::witness_weather`. Une
/// averse est indiscernable du ciel ordinaire, mais ce n'est pas le phénomène
/// qui fait le signe — c'est sa coïncidence avec le besoin. Le jugement y est
/// donc individuel (l'assoiffé y voit une réponse, son voisin repu n'y voit
/// rien), ce que cette fonction, qui décide pour tout le monde à la fois, ne
/// saurait exprimer.
fn witness_effect(intervention: Intervention, outcome: &Outcome) -> Option<(f64, bool)> {
    match intervention {
        Intervention::Lightning { .. } => {
            if outcome.killed > 0 {
                Some((LIGHTNING_SIGHT_TILES, false)) // le ciel a tué : on le craint
            } else if outcome.ignited {
                Some((LIGHTNING_SIGHT_TILES, true)) // il a donné le feu
            } else {
                None // un éclair, et puis rien
            }
        }
        Intervention::Fertility { .. } => {
            (outcome.touched > 0).then_some((BLESSING_RADIUS_TILES, true))
        }
        Intervention::Plague { .. } => {
            (outcome.touched + outcome.killed > 0).then_some((BLESSING_RADIUS_TILES, false))
        }
        // Jugée par `faith::witness_weather`, agent par agent : voir plus haut.
        Intervention::Rain { .. } | Intervention::Drought { .. } => None,
        // L'inspiré est le seul témoin, et il n'a rien vu — il a compris.
        Intervention::Revelation { .. } => (outcome.touched > 0).then_some((1.0, true)),
        // Un signe se **voit** : il marque ceux qui étaient là, et pour eux
        // c'est une faveur — le ciel a choisi leur terre.
        Intervention::Sign { .. } => Some((LIGHTNING_SIGHT_TILES, true)),
    }
}
/// Ce qu'un geste rend — ou retire — de santé. La moitié d'une vie : assez pour
/// arracher un mourant, ou pour emporter un affaibli.
const BLESSING_HEALTH: f32 = 0.5;

/// Un miracle **accompli**, daté : le journal qui, avec la seed, rejoue le
/// monde (voir l'en-tête). On garde l'issue à côté de l'intention, car c'est
/// elle qui distingue une foudre qui embrase d'une foudre qui n'a rien fait.
#[derive(Debug, Clone, Copy)]
pub struct Miracle {
    pub tick: u64,
    pub intervention: Intervention,
    pub outcome: Outcome,
}

/// Ce que le geste a **réellement** produit. Le joueur vise ; le monde répond.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Outcome {
    /// Un feu a-t-il pris ? (Non si le sol était détrempé, nu ou gelé.)
    pub ignited: bool,
    /// Combien d'humains le geste a-t-il tués.
    pub killed: usize,
    /// Combien d'humains il a touchés sans les tuer — soignés ou affligés.
    /// C'est le nombre de **témoins** : ceux qui auront quelque chose à
    /// raconter, et qui feront la théologie de leur peuple (§6.3).
    pub touched: usize,
}

/// Pourquoi une intervention n'a pas pu être tentée. Distinct d'un
/// [`Outcome`] sans effet : ici le geste n'a pas eu lieu du tout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivineError {
    /// La cible désignée n'existe pas (ou plus).
    NoSuchTarget,
}

impl Sim {
    /// **La porte unique du joueur-divinité.** Accomplit l'intervention, la
    /// journalise (rejouabilité), et en fait un fait de Chronique.
    ///
    /// Ne consomme aucune Foi pour l'instant — voir l'en-tête du module.
    pub fn invoke(&mut self, intervention: Intervention) -> Result<Outcome, DivineError> {
        // Le lieu d'abord. La révélation en a besoin **avant** d'agir (celui
        // qu'on inspirait a pu mourir), et c'est la seule qui puisse échouer.
        let pos = match intervention {
            Intervention::Lightning { pos }
            | Intervention::Fertility { pos }
            | Intervention::Plague { pos }
            | Intervention::Rain { pos }
            | Intervention::Drought { pos }
            | Intervention::Sign { pos } => pos,
            Intervention::Revelation { agent } => {
                self.agent_position(agent).ok_or(DivineError::NoSuchTarget)?
            }
        };

        let outcome = match intervention {
            Intervention::Lightning { .. } => self.strike_lightning(pos),
            // Un seul mécanisme, deux signes : le montant change, rien d'autre.
            // Écrire deux fonctions symétriques aurait invité l'une à diverger
            // de l'autre — la leçon des mécaniques à deux faces du projet.
            Intervention::Fertility { .. } => self.touch_health(pos, BLESSING_HEALTH),
            Intervention::Plague { .. } => self.touch_health(pos, -BLESSING_HEALTH),
            Intervention::Rain { .. } => self.summon_weather(pos, WeatherKind::Rain),
            Intervention::Drought { .. } => self.summon_weather(pos, WeatherKind::Drought),
            Intervention::Sign { .. } => {
                self.shrines.push(crate::cult::Shrine {
                    pos: (pos.0 as f64 + 0.5, pos.1 as f64 + 0.5),
                    since: self.time.tick,
                });
                Outcome { ignited: false, killed: 0, touched: 0 }
            }
            Intervention::Revelation { agent } => {
                let understood = crate::tech::reveal(self, agent).is_some();
                Outcome { ignited: false, killed: 0, touched: usize::from(understood) }
            }
        };

        // Qui a vu, et comment il l'a pris (§6.1). Un geste qui ne produit rien
        // ne fait aucun croyant : **un dieu se juge à ses effets**.
        let center = (pos.0 as f64 + 0.5, pos.1 as f64 + 0.5);
        match intervention {
            // La météo se juge au besoin de chacun, pas au geste : le même
            // orage répond à l'assoiffé et laisse son voisin indifférent.
            Intervention::Rain { .. } => {
                crate::faith::witness_weather(self, center, WEATHER_SIGHT_TILES, true);
            }
            Intervention::Drought { .. } => {
                crate::faith::witness_weather(self, center, WEATHER_SIGHT_TILES, false);
            }
            _ => {
                if let Some((radius, boon)) = witness_effect(intervention, &outcome) {
                    crate::faith::witness(self, center, radius, boon);
                }
            }
        }

        self.miracles.push(Miracle { tick: self.time.tick, intervention, outcome });
        self.record(pos, EventKind::Miracle { intervention, outcome });
        Ok(outcome)
    }

    /// Où se tient un agent, s'il est encore de ce monde.
    fn agent_position(&self, agent: AgentId) -> Option<(i64, i64)> {
        self.agents
            .query::<(&AgentId, &Position)>()
            .iter()
            .find(|(_, (id, _))| **id == agent)
            .map(|(_, (_, p))| p.tile())
    }

    /// Appelle le ciel. Passe par `weather::start` — **le même chemin** que la
    /// météo naturelle : rien ici ne distingue une averse divine d'une autre,
    /// et c'est voulu. Un peuple ne pourra jamais savoir, en regardant le ciel,
    /// si quelqu'un l'a voulu — toute la matière du culte est là.
    fn summon_weather(&mut self, pos: (i64, i64), kind: WeatherKind) -> Outcome {
        crate::weather::start(self, (pos.0 as f64 + 0.5, pos.1 as f64 + 0.5), kind);
        Outcome { ignited: false, killed: 0, touched: 0 }
    }

    /// Rend (ou retire) de la santé alentour. `delta` positif bénit, négatif
    /// afflige — c'est le **même geste**, au signe près.
    fn touch_health(&mut self, pos: (i64, i64), delta: f32) -> Outcome {
        let center = (pos.0 as f64 + 0.5, pos.1 as f64 + 0.5);
        let (mut touched, mut killed) = (0usize, 0usize);
        for (_, (agent_pos, phys)) in self.agents.query_mut::<(&Position, &mut Physiology)>() {
            let d = (agent_pos.x - center.0).hypot(agent_pos.y - center.1);
            if d > BLESSING_RADIUS_TILES || phys.is_dead() {
                continue;
            }
            phys.health = (phys.health + delta).clamp(0.0, 1.0);
            if phys.is_dead() {
                phys.last_damage = Some(DeathCause::Disease);
                killed += 1;
            } else {
                touched += 1;
            }
        }
        Outcome { ignited: false, killed, touched }
    }

    /// La foudre : un feu si le sol s'y prête, des morts si quelqu'un était là.
    /// Les deux sont indépendants — on peut tout à fait embraser une lande
    /// déserte, ou tuer un homme sans que rien ne brûle.
    fn strike_lightning(&mut self, pos: (i64, i64)) -> Outcome {
        let point = (pos.0 as f64 + 0.5, pos.1 as f64 + 0.5);
        let ignited = fire::ignite_at(self, point);

        // Qui se tenait sous le coup. On relève d'abord (l'itération emprunte
        // l'ECS), on retire ensuite — même patron que les morts ordinaires.
        let mut killed = 0usize;
        for (_, (agent_pos, phys, demo)) in
            self.agents.query_mut::<(&Position, &mut Physiology, &Demographics)>()
        {
            let d = (agent_pos.x - point.0).hypot(agent_pos.y - point.1);
            if d <= LIGHTNING_LETHAL_TILES && !phys.is_dead() {
                // On passe par le pipeline de mort ordinaire : santé à zéro et
                // cause inscrite. La passe de physiologie fera le reste, et la
                // Chronique nommera le mort si c'était un chef.
                phys.health = 0.0;
                phys.last_damage = Some(DeathCause::Lightning);
                let _ = demo;
                killed += 1;
            }
        }
        // La foudre ne « touche » personne : elle tue, ou elle passe.
        Outcome { ignited, killed, touched: 0 }
    }

    /// Distance (en tuiles) à laquelle la foudre est mortelle — le client s'en
    /// sert pour montrer au joueur ce qu'il risque de faire.
    pub fn lightning_lethal_radius() -> f64 {
        LIGHTNING_LETHAL_TILES
    }

    /// Rayon indicatif d'un embrasement, pour le même usage.
    pub fn lightning_sight_radius() -> f64 {
        km_to_tiles(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentId;
    use crate::tile::Tile;
    use cairn_core::WorldSeed;

    /// Cherche une tuile réellement inflammable (même méthode que les tests de
    /// `fire`) : on veut éprouver la foudre sur un vrai sol, pas sur un décor.
    fn find_flammable(sim: &mut Sim) -> Option<(i64, i64)> {
        let step = km_to_tiles(8.0) as i64;
        for r in 0..300i64 {
            let d = r * step;
            for &(x, y) in &[(d, 0), (-d, 0), (0, d), (0, -d), (d, d), (-d, -d)] {
                let t: Tile = sim.world.tile(x, y);
                if t.is_walkable() && t.biomass >= 60 && t.humidity <= 120 && t.temperature >= 5.0 {
                    return Some((x, y));
                }
            }
        }
        None
    }

    /// Le cœur du §6.2 : **le même geste** donne le feu et tue. Ce qui décide,
    /// c'est où l'on frappe — pas un tirage « bon ou mauvais ».
    #[test]
    fn la_foudre_donne_le_feu_et_tue_dans_le_meme_geste() {
        let mut sim = Sim::new(WorldSeed(42), 256);
        let Some((fx, fy)) = find_flammable(&mut sim) else {
            return; // seed sans zone sèche proche : test sans objet (rare)
        };
        // Un malheureux sous le coup, un autre à bonne distance.
        let dessous = sim.spawn_agent(fx as f64 + 0.5, fy as f64 + 0.5);
        let a_l_ecart = sim.spawn_agent(fx as f64 + 500.0, fy as f64 + 0.5);

        let outcome = sim.invoke(Intervention::Lightning { pos: (fx, fy) }).unwrap();

        assert!(outcome.ignited, "une savane sèche doit s'embraser");
        assert_eq!(outcome.killed, 1, "seul celui qui était dessous doit mourir");
        assert!(!sim.fires.is_empty(), "le feu doit exister dans le monde");

        let dead = |id| {
            sim.agents
                .query::<(&AgentId, &Physiology)>()
                .iter()
                .find(|(_, (a, _))| **a == id)
                .map(|(_, (_, p))| p.is_dead())
                .unwrap()
        };
        assert!(dead(dessous), "l'agent sous la foudre est tué");
        assert!(!dead(a_l_ecart), "celui qui était à l'écart est indemne");
    }

    /// L'ambivalence n'est pas un tirage : sur un sol qui ne peut pas brûler,
    /// la foudre ne donne rien — mais elle tue quand même.
    #[test]
    fn sur_un_sol_qui_ne_brule_pas_la_foudre_ne_donne_rien() {
        let mut sim = Sim::new(WorldSeed(42), 256);
        // Une tuile d'eau : rien n'y prend feu, par construction.
        let mut ocean = None;
        for r in 0..400i64 {
            let p = (r * 200, 0);
            if !sim.world.tile(p.0, p.1).is_walkable() {
                ocean = Some(p);
                break;
            }
        }
        let Some(pos) = ocean else { return };
        let outcome = sim.invoke(Intervention::Lightning { pos }).unwrap();
        assert!(!outcome.ignited, "l'eau ne prend pas feu");
        assert_eq!(outcome.killed, 0, "personne n'était là");
    }

    /// Bénir et affliger sont **le même geste au signe près**, et tous deux
    /// bornés par la distance. Le test les éprouve ensemble, précisément pour
    /// qu'ils ne puissent pas diverger l'un de l'autre.
    #[test]
    fn benir_et_affliger_sont_le_meme_geste_et_ne_portent_pas_loin() {
        let effet = |intervention: Intervention, sante_initiale: f32| {
            let mut sim = Sim::new(WorldSeed(7), 64);
            let proche = sim.spawn_agent(0.0, 0.0);
            let _loin = sim.spawn_agent(BLESSING_RADIUS_TILES + 50.0, 0.0);
            for (_, phys) in sim.agents.query_mut::<&mut Physiology>() {
                phys.health = sante_initiale;
            }
            let outcome = sim.invoke(intervention).unwrap();
            let sante = |id| {
                sim.agents
                    .query::<(&AgentId, &Physiology)>()
                    .iter()
                    .find(|(_, (a, _))| **a == id)
                    .map(|(_, (_, p))| p.health)
                    .unwrap()
            };
            (sante(proche), sante(_loin), outcome)
        };

        let (proche, loin, out) = effet(Intervention::Fertility { pos: (0, 0) }, 0.4);
        assert!(proche > 0.4, "la grâce relève celui qui est là ({proche:.2})");
        assert_eq!(loin, 0.4, "elle ne porte pas au-delà de son rayon");
        assert_eq!((out.touched, out.killed), (1, 0));

        let (proche, loin, out) = effet(Intervention::Plague { pos: (0, 0) }, 0.9);
        assert!(proche < 0.9, "le mal saisit celui qui est là ({proche:.2})");
        assert_eq!(loin, 0.9, "et pas les autres");
        assert_eq!((out.touched, out.killed), (1, 0));
    }

    /// Une épidémie tue pour de bon — et par le pipeline de mort ordinaire, pas
    /// par un chemin parallèle : c'est la leçon du bug où les plaies mortelles
    /// ne tuaient personne (la santé était régénérée dans le tick même).
    #[test]
    fn une_epidemie_acheve_les_affaiblis() {
        let mut sim = Sim::new(WorldSeed(7), 64);
        sim.spawn_agent(0.0, 0.0);
        for (_, phys) in sim.agents.query_mut::<&mut Physiology>() {
            phys.health = 0.3; // déjà bien bas
        }
        let outcome = sim.invoke(Intervention::Plague { pos: (0, 0) }).unwrap();
        assert_eq!(outcome.killed, 1, "un affaibli n'y survit pas");
        assert_eq!(outcome.touched, 0);

        // La cause est inscrite, donc la Chronique saura la dire.
        let cause = sim
            .agents
            .query::<&Physiology>()
            .iter()
            .map(|(_, p)| p.last_damage)
            .next()
            .flatten();
        assert_eq!(cause, Some(DeathCause::Disease));
    }

    /// **L'ambivalence de la fertilité, qui n'est écrite nulle part.** Rendre la
    /// santé lève le frein que la démographie pose déjà à la conception : le
    /// geste ne « donne » pas des enfants, il retire l'obstacle. Ce qui suit —
    /// des bouches de plus dans un monde qui ne les nourrit peut-être pas — ne
    /// dépend plus de la divinité.
    #[test]
    fn la_grace_leve_le_frein_a_la_conception_sans_le_dire() {
        // Le seuil de santé qu'exige la conception (voir `demography::daily`).
        const FECONDABLE: f32 = 0.6;
        let mut sim = Sim::new(WorldSeed(7), 64);
        sim.spawn_agent(0.0, 0.0);
        for (_, phys) in sim.agents.query_mut::<&mut Physiology>() {
            phys.health = 0.35; // trop faible pour concevoir
        }
        let sante = |sim: &Sim| sim.agents.query::<&Physiology>().iter().next().unwrap().1.health;
        assert!(sante(&sim) < FECONDABLE, "au départ, le corps ne suit pas");

        sim.invoke(Intervention::Fertility { pos: (0, 0) }).unwrap();
        assert!(
            sante(&sim) > FECONDABLE,
            "après la grâce, plus rien ne s'y oppose ({:.2})",
            sante(&sim)
        );
    }

    /// **L'ambivalence de la pluie, qui n'est écrite nulle part.** Bénir une
    /// vallée d'une averse, c'est y rendre le feu impossible — donc priver un
    /// peuple sans silex de la seule voie qui lui restait vers sa maîtrise. La
    /// sécheresse fait l'inverse : elle affame, et elle offre le feu.
    ///
    /// Aucune ligne ne relie la divinité à l'arbre technologique. Tout passe par
    /// l'humidité du sol, que `fire::is_flammable` lit déjà.
    #[test]
    fn la_pluie_prive_du_feu_et_la_secheresse_le_donne() {
        // Un sol qui brûle tout juste : c'est là que le ciel fait basculer.
        let brulant = |sim: &mut Sim, pos: (i64, i64)| {
            fire::ignite_at(sim, (pos.0 as f64 + 0.5, pos.1 as f64 + 0.5))
        };
        let mut sim = Sim::new(WorldSeed(42), 256);
        sim.allow_weather = false; // aucun ciel parasite : on ne veut que le nôtre
        let Some(site) = find_flammable(&mut sim) else { return };

        // Sans intervention, ce sol prend feu.
        assert!(brulant(&mut sim, site), "le site de référence doit brûler");
        sim.fires.clear();

        // Sous l'averse, il ne prend plus.
        sim.invoke(Intervention::Rain { pos: site }).unwrap();
        assert!(
            !brulant(&mut sim, site),
            "une pluie doit rendre le feu impossible — c'est le prix de la grâce"
        );

        // Et sur un sol trop humide pour brûler, la sécheresse ouvre la voie.
        let mut sec = Sim::new(WorldSeed(42), 256);
        sec.allow_weather = false;
        let humide = (0..600i64)
            .map(|r| (r * 300, 0i64))
            .find(|&(x, y)| {
                let t = sec.world.tile(x, y);
                t.is_walkable() && t.biomass >= 60 && t.humidity > 120 && t.temperature >= 5.0
            });
        let Some(site_humide) = humide else { return };
        assert!(!brulant(&mut sec, site_humide), "un sol détrempé ne prend pas");
        sec.invoke(Intervention::Drought { pos: site_humide }).unwrap();
        assert!(
            brulant(&mut sec, site_humide),
            "la sécheresse doit rendre inflammable ce qui ne l'était pas"
        );
    }

    /// Une averse divine est **indistinguable** d'une averse ordinaire : elle
    /// emprunte le même chemin et produit le même objet. C'est ce qui rendra la
    /// théologie d'un peuple faillible — nul ne peut savoir, en regardant le
    /// ciel, si quelqu'un l'a voulu.
    #[test]
    fn une_averse_divine_est_une_averse_comme_les_autres() {
        let mut sim = Sim::new(WorldSeed(1), 64);
        sim.allow_weather = false;
        sim.invoke(Intervention::Rain { pos: (0, 0) }).unwrap();
        assert_eq!(sim.weather.len(), 1, "elle produit une cellule météo ordinaire");
        assert_eq!(sim.weather[0].kind, crate::weather::WeatherKind::Rain);
        // Et elle se dissipe comme n'importe quelle autre : rien ne la distingue.
        for _ in 0..20 {
            crate::weather::daily(&mut sim);
        }
        assert!(sim.weather.is_empty(), "même une averse voulue finit par passer");
    }

    /// **La révélation ne crée rien.** Elle fait comprendre ce qu'on avait sous
    /// les yeux : sans les matières, l'inspiration ne donne rien — et c'est ce
    /// qui l'empêche d'être le « bouton bien » que le brief refuse.
    #[test]
    fn on_ne_revele_qu_a_qui_a_deja_tout_vu() {
        use crate::exposure::{Exposure, Exposures};
        use crate::tech::Knowledge;

        let inspire = |expose: bool| {
            let mut sim = Sim::new(WorldSeed(3), 64);
            let agent = sim.spawn_agent(0.0, 0.0);
            if expose {
                // Le silex et le bois : de quoi comprendre le feu par friction.
                for (_, (id, exposures)) in sim.agents.query_mut::<(&AgentId, &mut Exposures)>() {
                    if *id == agent {
                        exposures.expose(Exposure::Flint);
                        exposures.expose(Exposure::Wood);
                    }
                }
            }
            let outcome = sim.invoke(Intervention::Revelation { agent }).unwrap();
            // Le feu, et non « un savoir quelconque » : un savoir-faire sans
            // matériau (la conservation des aliments) se révèle à qui n'a rien
            // vu, et c'est cohérent — la révélation ne lève que les conditions
            // humaines.
            let fire = sim.tech_tree.id_of("fire_mastery").unwrap();
            let sait = sim
                .agents
                .query::<(&AgentId, &Knowledge)>()
                .iter()
                .find(|(_, (a, _))| **a == agent)
                .map(|(_, (_, k))| k.has(fire))
                .unwrap();
            (outcome, sait)
        };

        let (_, rien) = inspire(false);
        assert!(!rien, "sans avoir rien vu, l'inspiré ne comprend pas le feu");

        let (avec, su) = inspire(true);
        assert!(su, "avec le silex et le bois sous les yeux, il comprend le feu");
        assert_eq!(avec.touched, 1, "un témoin : lui-même");
    }

    /// La seule intervention qui vise un être plutôt qu'un lieu est aussi la
    /// seule qui puisse échouer faute de cible — celui qu'on inspirait a pu
    /// mourir entre-temps.
    #[test]
    fn inspirer_un_absent_echoue_proprement() {
        let mut sim = Sim::new(WorldSeed(3), 64);
        let fantome = AgentId(9999);
        assert_eq!(
            sim.invoke(Intervention::Revelation { agent: fantome }),
            Err(DivineError::NoSuchTarget)
        );
        assert!(sim.miracles.is_empty(), "un geste sans cible n'entre pas au journal");
    }

    /// Une révélation produit **deux faits** : le geste du ciel, et la
    /// découverte elle-même — laquelle est racontée comme n'importe quelle
    /// autre, avec le nom de celui qui a compris.
    #[test]
    fn une_revelation_est_aussi_une_decouverte_ordinaire() {
        use crate::exposure::{Exposure, Exposures};

        let mut sim = Sim::new(WorldSeed(3), 64);
        let agent = sim.spawn_agent(0.0, 0.0);
        for (_, (id, exposures)) in sim.agents.query_mut::<(&AgentId, &mut Exposures)>() {
            if *id == agent {
                exposures.expose(Exposure::Flint);
                exposures.expose(Exposure::Wood);
            }
        }
        sim.invoke(Intervention::Revelation { agent }).unwrap();

        assert!(
            sim.chronicle.iter().any(|e| matches!(e.kind, EventKind::Miracle { .. })),
            "le geste du ciel fait date"
        );
        assert!(
            sim.chronicle.iter().any(|e| matches!(e.kind, EventKind::TechDiscovered { .. })),
            "et la découverte aussi, séparément"
        );
        assert_eq!(sim.tech_events.len(), 1, "elle compte comme une vraie découverte");
    }

    /// **L'arbitrage central de la Foi (§6.1).** Un dieu se juge à ses effets,
    /// et il faut être spectaculaire pour être cru : le geste discret est
    /// puissant mais ne financera jamais le suivant.
    #[test]
    fn seuls_les_gestes_qui_se_voient_font_des_croyants() {
        use crate::faith::Faith;
        let croyants = |sim: &Sim| {
            sim.agents.query::<&Faith>().iter().filter(|(_, f)| f.believes()).count()
        };

        // Une grâce qui relève quelqu'un : on y croit.
        let mut sim = Sim::new(WorldSeed(5), 64);
        sim.spawn_agent(0.0, 0.0);
        for (_, phys) in sim.agents.query_mut::<&mut Physiology>() {
            phys.health = 0.4;
        }
        sim.invoke(Intervention::Fertility { pos: (0, 0) }).unwrap();
        assert_eq!(croyants(&sim), 1, "être guéri fait un fidèle");

        // La même grâce sur une lande déserte : rien ne s'est produit.
        let mut vide = Sim::new(WorldSeed(5), 64);
        vide.spawn_agent(10_000.0, 0.0); // trop loin pour être touché
        vide.invoke(Intervention::Fertility { pos: (0, 0) }).unwrap();
        assert_eq!(croyants(&vide), 0, "un geste sans effet ne convainc personne");

        // Et la pluie sur un peuple qui n'a besoin de rien : il pleut, voilà tout.
        let mut ciel = Sim::new(WorldSeed(5), 64);
        ciel.allow_weather = false;
        ciel.spawn_agent(0.0, 0.0);
        for (_, phys) in ciel.agents.query_mut::<&mut Physiology>() {
            phys.thirst = 0.1; // il ne manque de rien
        }
        ciel.invoke(Intervention::Rain { pos: (0, 0) }).unwrap();
        assert_eq!(croyants(&ciel), 0, "sur un homme comblé, une averse n'est que la météo");
    }

    /// **La nuance qui distingue une averse d'une réponse.** Ce n'est pas le
    /// phénomène qui fait le signe — il est indiscernable du ciel ordinaire —
    /// c'est sa coïncidence avec le besoin. La même pluie, au même instant, est
    /// un miracle pour l'assoiffé et un jour comme un autre pour son voisin.
    #[test]
    fn la_pluie_qui_arrive_quand_on_en_a_besoin_est_une_reponse() {
        use crate::faith::Faith;
        let mut sim = Sim::new(WorldSeed(5), 64);
        sim.allow_weather = false;
        let assoiffe = sim.spawn_agent(0.0, 0.0);
        let repu = sim.spawn_agent(20.0, 0.0); // à deux pas, sous la même averse
        for (_, (id, phys)) in sim.agents.query_mut::<(&AgentId, &mut Physiology)>() {
            phys.thirst = if *id == assoiffe { 0.8 } else { 0.1 };
        }

        sim.invoke(Intervention::Rain { pos: (0, 0) }).unwrap();

        let foi = |who| {
            sim.agents
                .query::<(&AgentId, &Faith)>()
                .iter()
                .find(|(_, (id, _))| **id == who)
                .map(|(_, (_, f))| *f)
                .unwrap()
        };
        assert!(foi(assoiffe).believes(), "celui qui mourait de soif y voit une réponse");
        assert_eq!(foi(assoiffe).boons, 1, "et il la porte au crédit du ciel");
        assert!(!foi(repu).believes(), "son voisin, lui, a seulement vu qu'il pleuvait");

        // Et le châtiment se lit pareil : la sécheresse ne frappe que qui a faim.
        let mut sec = Sim::new(WorldSeed(5), 64);
        sec.allow_weather = false;
        let affame = sec.spawn_agent(0.0, 0.0);
        for (_, phys) in sec.agents.query_mut::<&mut Physiology>() {
            phys.hunger = 0.8;
        }
        sec.invoke(Intervention::Drought { pos: (0, 0) }).unwrap();
        let f = sec
            .agents
            .query::<(&AgentId, &Faith)>()
            .iter()
            .find(|(_, (id, _))| **id == affame)
            .map(|(_, (_, f))| *f)
            .unwrap();
        assert_eq!(f.banes, 1, "une sécheresse sur un affamé est un châtiment");
        assert!(f.benevolence() < 0.0);
    }

    /// La foudre fait des croyants **des deux façons** — et ce qu'ils retiennent
    /// n'est pas le même. C'est de cet écart que naîtra la théologie (§6.3).
    #[test]
    fn le_ciel_est_cru_qu_il_donne_ou_qu_il_frappe() {
        use crate::faith::Faith;
        let mut sim = Sim::new(WorldSeed(42), 256);
        let Some((fx, fy)) = find_flammable(&mut sim) else { return };
        sim.spawn_agent(fx as f64 + 0.5, fy as f64 + 0.5); // sous le coup
        sim.spawn_agent(fx as f64 + 200.0, fy as f64); // à l'écart, mais il voit

        sim.invoke(Intervention::Lightning { pos: (fx, fy) }).unwrap();

        // Le mort ne croit plus rien ; le survivant, lui, a vu le ciel tuer.
        let temoin = sim
            .agents
            .query::<(&Physiology, &Faith)>()
            .iter()
            .find(|(_, (p, _))| !p.is_dead())
            .map(|(_, (_, f))| *f)
            .expect("un survivant");
        assert!(temoin.believes(), "voir la foudre tuer fait croire");
        assert_eq!(temoin.banes, 1, "et il en garde un mauvais souvenir");
        assert!(temoin.benevolence() < 0.0, "son dieu est un dieu de colère");
    }

    /// Le journal est ce qui rend le monde rejouable (§8.2) : chaque geste y
    /// laisse sa date, son intention **et** son issue.
    #[test]
    fn chaque_miracle_entre_au_journal_et_dans_la_chronique() {
        let mut sim = Sim::new(WorldSeed(42), 256);
        sim.time.tick = 1234;
        let pos = (0, 0);
        sim.invoke(Intervention::Lightning { pos }).unwrap();

        assert_eq!(sim.miracles.len(), 1);
        assert_eq!(sim.miracles[0].tick, 1234, "le miracle est daté");
        assert_eq!(sim.miracles[0].intervention, Intervention::Lightning { pos });
        assert!(
            sim.chronicle.iter().any(|e| matches!(e.kind, EventKind::Miracle { .. })),
            "un miracle fait toujours date"
        );
    }
}
