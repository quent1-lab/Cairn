# Cairn — instructions pour Claude

Simulateur d'évolution culturelle et technologique **émergente** : monde infini procédural, humains simulés individuellement, serveur Rust persistant 24/7, client web WASM pixel-art, joueur-divinité (Foi, interventions ambivalentes), Chronique narrative.

**Référence obligatoire : [docs/BRIEF.md](docs/BRIEF.md)** — vision, systèmes, architecture cible, plan en 6 phases avec critères d'acceptation. À consulter avant toute décision de conception.

## Contexte de travail

- Projet d'**apprentissage Rust** + pièce de portfolio (étudiant ingénieur, fort en C/C++/embarqué, Rust en cours d'acquisition).
- Répondre **en français**. Expliquer les idiomes Rust nouveaux dans la réponse (pas en commentaires tutoriels dans le code — les commentaires du code documentent le domaine, pas le langage).

## Règles non négociables (BRIEF §0 et §11)

1. **Émergence stricte** : aucun comportement narratif scripté. Les « âges » sont des étiquettes dérivées, jamais des conditions de déblocage.
2. **Aucune dépendance sans justification** face à une implémentation maison de ~50 lignes.
3. Simple et lisible d'abord ; **optimiser seulement après mesure**.
4. **Déterminisme bit-à-bit** : RNG dérivé de la seed globale (`cairn-core`), jamais d'itération sur `HashMap` dans la simulation.
5. Chaque incrément produit quelque chose de **visible et testable**.

## Architecture

Workspace cible (BRIEF §8) : `crates/{core, worldgen, sim, protocol, server, client}` — `protocol` partagé serveur↔client. Seuls `core` et `worldgen` existent pour l'instant.

## Commandes

- `cargo check` / `cargo test` — vérification et tests.
- `cargo run --release -p cairn-worldgen --example map_png -- <seed>` — rend une image PNG par couche du pipeline dans `out/` (`_alt`, `_temp`…) + % de terres émergées (toujours en `--release` : le bruit est ~30× plus lent en debug).
- **git : uniquement via Git Bash** — absent du PATH PowerShell. Les commits sont gérés par Claude (demande explicite de l'utilisateur), messages en français, style conventional commits.

## Environnement

- Windows 10, toolchain `x86_64-pc-windows-gnu`, Rust ≥ 1.97, édition 2024.
- Cible `wasm32-unknown-unknown` requise pour le client (pas encore installée).

## État d'avancement

- **Phase 1 en cours** : altitude, température (gradient quadratique en latitude), vent (-sin(3πλ)), humidité par advection (ombre pluviométrique), biomes de Whittaker, hydrologie D8 (priority-flood + ε, rivières et lacs, sur région bornée `Region`) — couches point-à-point assemblées dans `WorldGen` (pipeline.rs) ; l'hydrologie est à part (bornée). Export PNG par couche + zoom, stats de répartition. Reste : géologie (cuivre/étain jamais co-localisés), chunking, client WASM. Améliorations en attente : montagnes en chaînes (ridged noise) ; humidité macro-grille + cache ; mangrove/marais (proximité eau) ; artefacts cosmétiques (stries diagonales advection/D8) ; hydrologie inter-chunks.
