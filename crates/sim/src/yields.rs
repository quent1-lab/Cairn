//! Ce que rapporte chaque façon de se nourrir (MAR-6) : le registre, par
//! humain, des heures passées à chaque activité de subsistance et des kcal
//! qu'elle a acquises.
//!
//! Une activité se compte **trajets compris** : l'heure de marche vers la
//! tuile à cueillir ou vers le troupeau fait partie de ce que coûte la
//! cueillette ou la chasse. La chasse réunit la quête, la piste et
//! l'approche : une prise est le fruit de toute la chaîne. Les kcal sont
//! celles **acquises** (la bête entière, partagée ou non ; la cueillette
//! mangée), pas celles mangées par l'acquéreur.
//!
//! MAR-6a : ce registre nourrit aussi un **rendement appris**. Chacun garde
//! son expérience récente (heures, kcal nettes du coût de l'effort), qui
//! s'efface avec le temps, et ce qu'il **voit faire** autour de lui : la
//! moyenne glissante de l'expérience récente des proches qu'il côtoie
//! (`memory::exchange_knowledge`). Son estimation pèse les deux à égalité,
//! heure pour heure : un enfant, qui n'a rien fait, juge sur ce qu'il voit ;
//! un adulte affine par ce qu'il fait. Aucune moyenne n'est connue de tous,
//! aucune valeur n'est donnée d'avance (décision de l'utilisateur,
//! 2026-10-10) : sans assez d'heures pour juger, l'activité n'est ni
//! préférée ni écartée.

use crate::agent::TaskKind;

/// Demi-vie de l'expérience (jours) : ce qu'une activité rapportait il y a un
/// mois compte moitié moins — le pays, la saison et le gibier changent. Choix
/// de forme, non sourcé ; sa sensibilité se mesure au banc.
pub const MEMORY_HALF_LIFE_DAYS: f64 = 30.0;
/// En deçà de ces heures d'expérience (la sienne et celle qu'on a vue), on ne
/// juge pas encore une activité : deux jours de travail. Choix, non sourcé.
pub const EVIDENCE_HOURS: f32 = 48.0;
/// Pas d'observation à chaque échange (toutes les 4 h) : ce qu'on voit faire
/// rejoint l'expérience des proches en une journée environ.
pub const SEEN_STEP: f32 = 1.0 / 6.0;

/// Les activités de subsistance dont on compte le rendement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pursuit {
    Gather = 0,
    Hunt = 1,
}

pub const PURSUITS: usize = 2;

impl Pursuit {
    /// L'activité de subsistance que sert une tâche, s'il y en a une.
    pub fn of(kind: TaskKind) -> Option<Self> {
        match kind {
            TaskKind::Forage => Some(Self::Gather),
            TaskKind::SeekGame | TaskKind::Track | TaskKind::Hunt => Some(Self::Hunt),
            _ => None,
        }
    }
}

/// Le registre d'un humain : heures passées, kcal acquises et kcal dépensées
/// au-delà du repos, par activité, depuis sa naissance (ou son arrivée pour un
/// fondateur).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Yields {
    pub hours: [f64; PURSUITS],
    pub kcal: [f64; PURSUITS],
    pub cost: [f64; PURSUITS],
    /// Expérience récente : heures et kcal nettes, qui s'effacent.
    own_h: [f32; PURSUITS],
    own_net: [f32; PURSUITS],
    /// Ce qu'on a vu faire : l'expérience récente moyenne des proches.
    seen_h: [f32; PURSUITS],
    seen_net: [f32; PURSUITS],
}

impl Yields {
    /// Une heure passée à cette activité, qui a coûté `extra_kcal` de plus que
    /// le repos.
    pub fn spend_hour(&mut self, p: Pursuit, extra_kcal: f64) {
        self.hours[p as usize] += 1.0;
        self.cost[p as usize] += extra_kcal;
        self.own_h[p as usize] += 1.0;
        self.own_net[p as usize] -= extra_kcal as f32;
    }

    /// Des kcal acquises par cette activité.
    pub fn gain(&mut self, p: Pursuit, kcal: f64) {
        self.kcal[p as usize] += kcal.max(0.0);
        self.own_net[p as usize] += kcal.max(0.0) as f32;
    }

    /// Une heure passe : l'expérience et le souvenir de ce qu'on a vu
    /// s'effacent d'autant.
    pub fn forget_hour(&mut self) {
        let keep = (-std::f64::consts::LN_2 / (MEMORY_HALF_LIFE_DAYS * 24.0)).exp() as f32;
        for p in 0..PURSUITS {
            self.own_h[p] *= keep;
            self.own_net[p] *= keep;
            self.seen_h[p] *= keep;
            self.seen_net[p] *= keep;
        }
    }

    /// L'expérience récente qu'on montre aux autres : (heures, kcal nettes).
    pub fn shown(&self) -> ([f32; PURSUITS], [f32; PURSUITS]) {
        (self.own_h, self.own_net)
    }

    /// Ce qu'on a vu faire cette fois : l'expérience moyenne des proches
    /// côtoyés (heures, kcal nettes), vers laquelle on glisse d'un pas.
    pub fn observe(&mut self, mean_h: [f32; PURSUITS], mean_net: [f32; PURSUITS]) {
        for p in 0..PURSUITS {
            self.seen_h[p] += SEEN_STEP * (mean_h[p] - self.seen_h[p]);
            self.seen_net[p] += SEEN_STEP * (mean_net[p] - self.seen_net[p]);
        }
    }

    /// Rendement net attendu d'une activité (kcal par heure), ou `None` sans
    /// assez d'heures pour en juger.
    pub fn expected(&self, p: Pursuit) -> Option<f32> {
        let i = p as usize;
        let h = self.own_h[i] + self.seen_h[i];
        (h >= EVIDENCE_HOURS).then(|| (self.own_net[i] + self.seen_net[i]) / h)
    }

    /// Le poids de chaque activité dans le choix, dans [0, 1] : son rendement
    /// net attendu rapporté au meilleur. On ne compare que ce qu'on sait
    /// juger : tant qu'une activité n'est pas jugée, rien n'est rabattu ; et
    /// si rien ne rapporte plus que l'effort, la comparaison ne dit rien.
    pub fn weights(&self) -> [f32; PURSUITS] {
        let e = [self.expected(Pursuit::Gather), self.expected(Pursuit::Hunt)];
        let mut w = [1.0; PURSUITS];
        if let [Some(g), Some(h)] = e {
            let best = g.max(h);
            if best > 0.0 {
                w = [(g / best).max(0.0), (h / best).max(0.0)];
            }
        }
        w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sans_assez_d_heures_on_ne_juge_pas() {
        let mut y = Yields::default();
        for _ in 0..10 {
            y.spend_hour(Pursuit::Hunt, 100.0);
        }
        y.spend_hour(Pursuit::Gather, 50.0);
        y.gain(Pursuit::Gather, 1_000.0);
        assert_eq!(y.expected(Pursuit::Hunt), None);
        assert_eq!(y.weights(), [1.0, 1.0]);
    }

    #[test]
    fn le_meilleur_rendement_net_pese_un_l_autre_au_prorata() {
        let mut y = Yields::default();
        for _ in 0..100 {
            y.spend_hour(Pursuit::Gather, 100.0);
            y.gain(Pursuit::Gather, 1_100.0); // 1 000 nettes par heure
            y.spend_hour(Pursuit::Hunt, 150.0);
            y.gain(Pursuit::Hunt, 400.0); // 250 nettes par heure
        }
        let w = y.weights();
        assert!((w[0] - 1.0).abs() < 1e-6);
        assert!((w[1] - 0.25).abs() < 1e-3, "{w:?}");
    }

    #[test]
    fn un_enfant_juge_sur_ce_qu_il_voit_faire() {
        let mut adult = Yields::default();
        for _ in 0..200 {
            adult.spend_hour(Pursuit::Gather, 0.0);
            adult.gain(Pursuit::Gather, 800.0);
            adult.spend_hour(Pursuit::Hunt, 0.0);
            adult.gain(Pursuit::Hunt, 200.0);
        }
        let mut child = Yields::default();
        let (h, n) = adult.shown();
        for _ in 0..30 {
            child.observe(h, n);
        }
        let w = child.weights();
        assert!((w[1] - 0.25).abs() < 0.01, "{w:?}");
    }

    #[test]
    fn l_experience_s_efface_de_moitie_en_une_demi_vie() {
        let mut y = Yields::default();
        y.spend_hour(Pursuit::Hunt, 0.0);
        for _ in 0..(MEMORY_HALF_LIFE_DAYS as usize * 24) {
            y.forget_hour();
        }
        assert!((y.own_h[1] - 0.5).abs() < 1e-3);
        assert_eq!(y.hours[1], 1.0, "le registre, lui, ne s'efface pas");
    }
}
