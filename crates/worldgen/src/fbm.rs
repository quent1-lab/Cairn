//! Fractal Brownian motion : superposition d'octaves de bruit Simplex.
//! Chaque octave double la fréquence (lacunarité) et divise l'amplitude
//! par deux (gain) — les basses fréquences dessinent les grandes formes,
//! les hautes ajoutent le détail.

use cairn_core::Pcg32;
use noise::{NoiseFn, Simplex};

pub struct Fbm {
    octaves: Vec<Simplex>,
    frequency: f64,
    lacunarity: f64,
    gain: f64,
}

impl Fbm {
    /// `frequency` est la fréquence de la première octave, en 1/tuiles :
    /// une fréquence de 1/2048 produit des motifs d'environ 2048 tuiles.
    pub fn new(seed: u64, octaves: usize, frequency: f64) -> Self {
        // Une seed distincte par octave, dérivée de façon déterministe :
        // sinon toutes les octaves échantillonnent le même motif à des
        // échelles différentes et l'auto-similarité se voit.
        let mut rng = Pcg32::new(seed, 0);
        let octaves = (0..octaves).map(|_| Simplex::new(rng.next_u32())).collect();
        Self {
            octaves,
            frequency,
            lacunarity: 2.0,
            gain: 0.5,
        }
    }

    /// Valeur du champ en (x, y), normalisée dans [-1, 1].
    pub fn get(&self, x: f64, y: f64) -> f64 {
        let mut freq = self.frequency;
        let mut amp = 1.0;
        let mut sum = 0.0;
        let mut norm = 0.0;
        for simplex in &self.octaves {
            sum += simplex.get([x * freq, y * freq]) * amp;
            norm += amp;
            freq *= self.lacunarity;
            amp *= self.gain;
        }
        sum / norm
    }
}
