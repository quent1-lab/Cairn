//! Les briques de l'utility AI (BRIEF §4) : courbes de réponse et sélection
//! softmax.
//!
//! Une **courbe de réponse** transforme un signal brut (faim 0–1, distance…)
//! en utilité 0–1. La forme de la courbe **est** la personnalité de la
//! décision : une logistique raide fait un besoin ignoré puis soudain
//! impérieux (la soif), une quadratique monte en pression progressivement
//! (la faim), une linéaire répond proportionnellement.
//!
//! La **sélection softmax** choisit parmi les candidats proportionnellement à
//! `exp(score/τ)` : le meilleur est le plus probable, mais pas certain. C'est
//! ce qui injecte de la variété sans hasard pur — deux agents identiques ne
//! feront pas toujours pareil — et τ règle le curseur : τ → 0 = argmax
//! discipliné, τ grand = loterie.

use cairn_core::Pcg32;

/// Courbe de réponse : entrée attendue dans [0, 1], sortie serrée à [0, 1].
#[derive(Debug, Clone, Copy)]
pub enum Curve {
    /// `m·x + b`.
    Linear { m: f32, b: f32 },
    /// `x^k` : douce puis pressante pour k > 1 (faim), pressante d'emblée
    /// pour k < 1.
    Power { k: f32 },
    /// Sigmoïde de pente `steepness` centrée sur `midpoint` : seuil flou.
    Logistic { steepness: f32, midpoint: f32 },
    /// `(e^(k·x) − 1)/(e^k − 1)` : normalisée pour passer par (0,0) et (1,1),
    /// explosive vers 1 pour k > 0.
    Exponential { k: f32 },
}

impl Curve {
    pub fn eval(self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        let y = match self {
            Curve::Linear { m, b } => m * x + b,
            Curve::Power { k } => x.powf(k),
            Curve::Logistic { steepness, midpoint } => {
                1.0 / (1.0 + (-steepness * (x - midpoint)).exp())
            }
            Curve::Exponential { k } => ((k * x).exp() - 1.0) / (k.exp() - 1.0),
        };
        y.clamp(0.0, 1.0)
    }
}

/// Tire un indice parmi `scores`, proportionnellement à `exp(score/tau)`.
/// Le max est soustrait avant l'exponentielle : c'est l'astuce numérique
/// classique du softmax (les probabilités sont inchangées, mais on ne
/// manipule que des exposants ≤ 0, donc aucun débordement).
pub fn softmax_pick(scores: &[f32], tau: f32, rng: &mut Pcg32) -> usize {
    debug_assert!(!scores.is_empty());
    let max = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let weights: Vec<f32> = scores.iter().map(|s| ((s - max) / tau).exp()).collect();
    let total: f32 = weights.iter().sum();
    let mut draw = rng.next_f32() * total;
    for (i, w) in weights.iter().enumerate() {
        draw -= w;
        if draw <= 0.0 {
            return i;
        }
    }
    weights.len() - 1 // filet numérique : draw a survécu aux arrondis
}

/// Les **probabilités** du softmax pour `scores` à température `tau` — le
/// pendant *lisible* de [`softmax_pick`], pour **montrer** une délibération
/// (panneau d'agent, BRIEF §7.2 « pile de motivations avec scores ») sans rien
/// tirer. Même normalisation numérique que `softmax_pick` (le max soustrait
/// avant l'exponentielle). Purement d'affichage : `softmax_pick` reste seul
/// juge des choix réels, intact pour ne pas risquer le déterminisme.
pub fn softmax_weights(scores: &[f32], tau: f32) -> Vec<f32> {
    if scores.is_empty() {
        return Vec::new();
    }
    let max = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let weights: Vec<f32> = scores.iter().map(|s| ((s - max) / tau).exp()).collect();
    let total: f32 = weights.iter().sum();
    if total > 0.0 {
        weights.into_iter().map(|w| w / total).collect()
    } else {
        weights
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_courbes_restent_dans_l_unite() {
        let courbes = [
            Curve::Linear { m: 2.0, b: -0.3 },
            Curve::Power { k: 2.0 },
            Curve::Logistic { steepness: 10.0, midpoint: 0.5 },
            Curve::Exponential { k: 3.0 },
        ];
        for c in courbes {
            for i in 0..=20 {
                let y = c.eval(i as f32 / 20.0);
                assert!((0.0..=1.0).contains(&y), "{c:?} sort de [0,1] : {y}");
            }
        }
    }

    #[test]
    fn la_logistique_fait_un_seuil() {
        let c = Curve::Logistic { steepness: 12.0, midpoint: 0.5 };
        assert!(c.eval(0.2) < 0.05);
        assert!(c.eval(0.8) > 0.95);
    }

    #[test]
    fn softmax_favorise_le_meilleur_sans_l_imposer() {
        let mut rng = Pcg32::new(7, 0);
        let scores = [0.2f32, 0.8, 0.5];
        let mut compte = [0usize; 3];
        for _ in 0..2000 {
            compte[softmax_pick(&scores, 0.12, &mut rng)] += 1;
        }
        assert!(compte[1] > 1500, "le meilleur doit dominer : {compte:?}");
        assert!(compte[0] + compte[2] > 0, "mais pas être seul : {compte:?}");
    }

    #[test]
    fn softmax_deterministe_a_seed_egale() {
        let scores = [0.3f32, 0.6, 0.1, 0.5];
        let mut a = Pcg32::new(99, 1);
        let mut b = Pcg32::new(99, 1);
        for _ in 0..100 {
            assert_eq!(
                softmax_pick(&scores, 0.2, &mut a),
                softmax_pick(&scores, 0.2, &mut b)
            );
        }
    }
}
