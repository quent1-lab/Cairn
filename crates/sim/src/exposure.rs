//! L'exposition : « ce que l'agent a déjà **vu** » (BRIEF §5.2), le substrat
//! de l'innovation (Phase 5 « L'ÉTINCELLE », incrément 1).
//!
//! L'insight — la découverte d'une technologie — a pour facteur, dans le
//! BRIEF, l'**exposition** : on n'invente la poterie que si l'on a vu de
//! l'argile, la métallurgie que si l'on a croisé un affleurement de cuivre.
//! Ce module tient, par agent, l'ensemble des choses effectivement
//! rencontrées. C'est le pendant matériel de la carte mentale (`memory`) :
//! l'une retient *où* l'on est allé, l'autre *ce que l'on y a vu*.
//!
//! ## Exposition (persistante) vs environnement (courant)
//!
//! Le BRIEF distingue deux prérequis contextuels d'une technologie :
//! `prereq_exposure` (« a déjà VU : du feu, de l'argile, du cuivre ») et
//! `prereq_environment` (« a ACCÈS : à une rivière, à une forêt, à un four »).
//! Ce module ne couvre que le **premier** : une exposition est un souvenir
//! qui ne s'efface pas — on ne « désapprend » pas avoir vu du cuivre. L'accès
//! environnemental, lui, est une condition de l'**instant** (suis-je près
//! d'une rivière *maintenant* ?), qui se lit sur la tuile courante au moment
//! de l'insight, et n'a donc pas à être stocké — il viendra avec le moteur
//! d'insight (incrément 3).
//!
//! ## Individuel, jamais oublié
//!
//! L'exposition est une perception **personnelle** : c'est *cet* individu qui
//! a vu l'argile. Le corpus de savoirs d'un clan (l'union des technologies de
//! ses membres) est une autre affaire, à venir. Ici, un simple jeu de bits
//! par agent (`Copy`, quelques octets), rempli au fil des déplacements —
//! aucune décroissance : contrairement à une source d'eau lointaine qui finit
//! par s'oublier (`memory`), avoir vu un matériau reste acquis.
//!
//! Le **feu** est une exposition à part : il n'a pas de source dans le monde
//! de base (aucune tuile n'est « en feu » à la génération). Il sera alimenté
//! plus tard par les incendies naturels et les foyers de clan — la variante
//! existe dès maintenant pour que l'arbre technologique puisse s'y référer,
//! mais rien ne la déclenche encore.

use cairn_worldgen::{Biome, Deposit};
use serde::Deserialize;

use crate::tile::Tile;

/// Ce à quoi un agent peut être exposé. Chaque variante occupe **un bit** de
/// [`Exposures`] : l'ordre de déclaration fixe donc les bits — on peut en
/// ajouter à la fin, jamais en réordonner (les valeurs seraient décalées).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[repr(u8)]
pub enum Exposure {
    // — Gisements croisés au sol (l'ordre suit `worldgen::Deposit`) —
    Flint,
    Clay,
    Obsidian,
    Copper,
    Tin,
    Gold,
    Iron,
    // — Matières du paysage —
    /// Avoir foulé une forêt : bois d'œuvre et combustible.
    Wood,
    /// Graminées sauvages (prairie, steppe, savane) — le germe de
    /// l'agriculture (« observation des graminées », BRIEF §5.3).
    WildGrasses,
    /// Le feu. Sans source dans le monde de base (voir l'en-tête) : alimenté
    /// plus tard par les incendies et les foyers.
    Fire,
}

impl Exposure {
    /// Toutes les variantes, dans l'ordre des bits — pour l'itération
    /// (affichage du panneau d'agent, tests). À maintenir synchrone avec
    /// l'`enum` ci-dessus.
    pub const ALL: [Exposure; 10] = [
        Exposure::Flint,
        Exposure::Clay,
        Exposure::Obsidian,
        Exposure::Copper,
        Exposure::Tin,
        Exposure::Gold,
        Exposure::Iron,
        Exposure::Wood,
        Exposure::WildGrasses,
        Exposure::Fire,
    ];

    /// Les expositions qui se transmettent par le **troc** : des matières
    /// **portables** (minerais, argile, silex) qu'un marchand fait voir à son
    /// interlocuteur. Le bois, les graminées ou le feu ne se colportent pas
    /// ainsi. C'est ce qui laisse l'étain voyager de clan en clan une fois
    /// qu'une caravane l'a ramené dans la région (voir `tech::diffuse`).
    pub const TRADEABLE: [Exposure; 7] = [
        Exposure::Flint,
        Exposure::Clay,
        Exposure::Obsidian,
        Exposure::Copper,
        Exposure::Tin,
        Exposure::Gold,
        Exposure::Iron,
    ];

    /// Cette matière peut-elle changer de mains par le troc ?
    pub fn is_tradeable(self) -> bool {
        Self::TRADEABLE.contains(&self)
    }

    /// Le masque du bit de cette exposition. `self as u16` lit le discriminant
    /// (fixé par l'ordre de déclaration) et le décale : c'est l'idiome des
    /// drapeaux binaires maison, sans dépendance (BRIEF règle n°2).
    fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

/// Le jeu des expositions d'un agent : un bit par [`Exposure`]. Composant
/// léger `Copy`, comme les autres petits composants de la simulation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Exposures(pub u16);

impl Exposures {
    /// L'agent a-t-il déjà été exposé à `e` ?
    pub fn has(self, e: Exposure) -> bool {
        self.0 & e.bit() != 0
    }

    /// Enregistre une exposition (idempotent — un bit déjà posé le reste).
    pub fn expose(&mut self, e: Exposure) {
        self.0 |= e.bit();
    }

    /// Combien de choses distinctes cet agent a-t-il vues (popcount du masque).
    pub fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// Enregistre ce que la tuile sous l'agent lui donne à voir : le gisement
    /// qui y affleure, et la matière de son biome. Appelée à chaque tick sur
    /// la tuile courante (voir `sim::step`), en réutilisant la tuile déjà lue
    /// par la physiologie — aucune lecture de tuile supplémentaire.
    pub fn note_tile(&mut self, tile: &Tile) {
        if let Some(e) = deposit_exposure(tile.deposit) {
            self.expose(e);
        }
        if let Some(e) = biome_exposure(tile.biome) {
            self.expose(e);
        }
    }
}

/// L'exposition qu'apporte un gisement, s'il en apporte une (`None` = pas de
/// gisement). Le `return None` dans une branche de `match` diverge (type `!`),
/// ce qui laisse les autres branches produire un `Exposure` enveloppé par le
/// `Some(...)` extérieur — un idiome compact pour « cette valeur-ci n'en a
/// pas, les autres si ».
fn deposit_exposure(d: Deposit) -> Option<Exposure> {
    Some(match d {
        Deposit::None => return None,
        Deposit::Flint => Exposure::Flint,
        Deposit::Clay => Exposure::Clay,
        Deposit::Obsidian => Exposure::Obsidian,
        Deposit::Copper => Exposure::Copper,
        Deposit::Tin => Exposure::Tin,
        Deposit::Gold => Exposure::Gold,
        Deposit::Iron => Exposure::Iron,
    })
}

/// La matière qu'un biome donne à voir : bois en forêt, graminées en terrain
/// herbeux, rien ailleurs (désert, toundra, glace, eau).
fn biome_exposure(b: Biome) -> Option<Exposure> {
    use Biome::*;
    match b {
        TemperateForest | TropicalForest | Taiga => Some(Exposure::Wood),
        Grassland | Steppe | Savanna => Some(Exposure::WildGrasses),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_worldgen::RockType;

    /// Une tuile synthétique minimale, pour tester le mapping sans monde.
    fn tile(biome: Biome, deposit: Deposit) -> Tile {
        Tile {
            biome,
            rock: RockType::Sedimentary,
            deposit,
            elevation: 0.5,
            temperature: 12.0,
            humidity: 128,
            soil_fertility: 100,
            biomass: 100,
            flags: crate::tile::TileFlags::default(),
        }
    }

    #[test]
    fn le_jeu_de_bits_est_idempotent_et_compte_juste() {
        let mut e = Exposures::default();
        assert_eq!(e.count(), 0);
        assert!(!e.has(Exposure::Clay));
        e.expose(Exposure::Clay);
        e.expose(Exposure::Clay); // deux fois : un seul bit
        assert!(e.has(Exposure::Clay));
        assert_eq!(e.count(), 1);
        e.expose(Exposure::Copper);
        assert_eq!(e.count(), 2);
        assert!(!e.has(Exposure::Tin));
    }

    #[test]
    fn chaque_variante_a_un_bit_distinct() {
        // Aucune collision : exposer les dix variantes donne dix bits.
        let mut e = Exposures::default();
        for &kind in &Exposure::ALL {
            e.expose(kind);
        }
        assert_eq!(e.count(), Exposure::ALL.len() as u32);
    }

    #[test]
    fn une_tuile_expose_son_gisement_et_sa_matiere() {
        // Une forêt sur un affleurement de cuivre : bois **et** cuivre.
        let mut e = Exposures::default();
        e.note_tile(&tile(Biome::TemperateForest, Deposit::Copper));
        assert!(e.has(Exposure::Wood));
        assert!(e.has(Exposure::Copper));
        assert_eq!(e.count(), 2);

        // Une prairie sans gisement : seulement les graminées.
        let mut g = Exposures::default();
        g.note_tile(&tile(Biome::Grassland, Deposit::None));
        assert!(g.has(Exposure::WildGrasses));
        assert_eq!(g.count(), 1);

        // Un désert nu : rien à voir.
        let mut d = Exposures::default();
        d.note_tile(&tile(Biome::HotDesert, Deposit::None));
        assert_eq!(d.count(), 0);
    }

    #[test]
    fn le_feu_n_a_pas_de_source_dans_le_monde_de_base() {
        // Aucune tuile, quel que soit son biome ou son gisement, n'expose le
        // feu : il faut les incendies/foyers à venir.
        let mut e = Exposures::default();
        for &b in &[Biome::TemperateForest, Biome::Grassland, Biome::Savanna] {
            for &d in &[Deposit::None, Deposit::Flint, Deposit::Copper] {
                e.note_tile(&tile(b, d));
            }
        }
        assert!(!e.has(Exposure::Fire));
    }
}
