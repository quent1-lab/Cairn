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

Workspace cible (BRIEF §8) : `crates/{core, worldgen, sim, protocol, server, client}` — `protocol` partagé serveur↔client. Existent : `core`, `worldgen`, `sim`.

`sim` = monde **chunké** (store de tuiles) : le pont entre worldgen (baseline pur, immuable) et l'état mutable à venir. `Tile` (16 o) matérialise le baseline + slots mutables (fertilité, biomasse) ; `Chunk` 64×64 généré à la demande (humidité échantillonnée aux 4 coins partagés → aucune couture) ; `World` = LRU BTreeMap, capacité bornée, éviction sûre car régénérable. Le store sert le **local** ; la vue monde lit le worldgen en échantillonnage grossier (mode carte). **Règle : worldgen reste pur ; l'évolution/mutation vit dans `sim`.**

## Échelle

**1 tuile = 2 m** (échelle humaine : une hutte = plusieurs tuiles, un agent sur une, un village se parcourt tuile à tuile). Source unique : `core::scale` (`TILE_METERS`, `km_to_tiles`, `tiles_to_km`). Tout le worldgen exprime ses tailles en km via ces helpers. Le monde est donc énorme en tuiles (continents ~3000 km ≈ 1,5 M tuiles) — sans coût car procédural/chunké. Le zoom monde↔village est un problème de **rendu (LOD)**, jamais de taille de tuile : proche = sprites/structures, loin = mode carte agrégé.

## Commandes

- `cargo check` / `cargo test` — vérification et tests.
- Client : `cargo run --release -p cairn-client --example preview -- <couche> <km>` rend le canvas en PNG (vérif sans navigateur). Bundle web : `crates/client/build.sh` (nécessite `wasm-bindgen-cli` — voir note d'env).
- `cargo run --release -p cairn-worldgen --example map_png -- <seed> [tuiles/pixel]` — rend une image PNG par couche du pipeline dans `out/` (`_alt`, `_temp`, `_hum`, `_bio`) + % de terres émergées ; 2ᵉ arg = zoom (petit = gros plan). Toujours en `--release`.
- `cargo run --release -p cairn-worldgen --example analyze -- [nb_seeds] [onde_continent] [sea_bias]` — vérifs statistiques : connexité des masses terrestres + preuve du rain shadow (humidité par barrière au vent).
- **git : uniquement via Git Bash** — absent du PATH PowerShell. Les commits sont gérés par Claude (demande explicite de l'utilisateur), messages en français, style conventional commits.

## Environnement

- Windows 10, toolchain `x86_64-pc-windows-gnu`, Rust ≥ 1.97, édition 2024.
- Cible `wasm32-unknown-unknown` requise pour le client (pas encore installée).

## État d'avancement

- **Phase 1 quasi complète** : worldgen complet + chunking + **client WASM** écrit (crate `cairn-client` : canvas plein écran, pan/zoom géométrique, 5 couches, panneau flottant ; rendu vérifié via `example preview`). Seul reste bloqué : produire le **bundle web** ici (toolchain windows-gnu sans mingw-w64 complet → `wasm-bindgen-cli`/`trunk` ne s'installent pas ; binaires MSVC sans runtime VC++). Voir `crates/client/README.md`. Ne pas re-tenter d'installer ces outils sans avoir d'abord réglé l'environnement.

- **Phase 1 (worldgen)** : altitude (continents ~3000 km + relief 150 km→300 m, 10 octaves), température (gradient quadratique, latitude exacte sur tout l'axe i64), vent (-sin(3πλ)), humidité par advection ~256 km (ombre pluviométrique prouvée + diffusion latérale ±8 km anti-stries), biomes de Whittaker, hydrologie D8 (priority-flood + ε, sur `Region` bornée), **eau gelée** (`Water::frozen(temp)` : stagnante 0 °C, courante -6 °C — le débit reste géographique permanent). **géologie** (roches sédimentaire/métamorphique/ignée + gisements silex/argile/obsidienne/cuivre/étain/or/fer ; cuivre et étain **jamais co-localisés**, chacun près des pics de sa province + marge de domination, séparation > 50 km testée). Couches point-à-point dans `WorldGen` (pipeline.rs), hydrologie à part. Outils : `map_png` (couches alt/temp/hum/bio/geo + zoom + décalage centre en km + montage), `analyze` (connexité + rain shadow), `chunk_demo` (monde chunké). **Chunking fait** (crate `sim`). Reste en Phase 1 : **client WASM** (rendu scrollable/zoomable, LOD monde↔village). Améliorations en attente : montagnes en chaînes (ridged noise) ; humidité macro-grille + cache + vraie diffusion sur grille (le ×5 de la diffusion par trajets est un stopgap) ; mangrove/marais ; hydrologie inter-chunks ; borne globale ~2⁵³ tuiles (bruit en x as f64).
