# cairn-client

Client web (WASM) de Cairn : une fenêtre plein écran sur le monde **vivant**.
Le client fait tourner un `Sim` localement (Phase 2 ; en Phase 6 la simulation
passera au serveur et le client s'y abonnera). Caméra scrollable/zoomable,
couches de debug (biomes, altitude, température, humidité, géologie), et les
agents + la faune dessinés en temps réel par-dessus le terrain.

## Panneau — deux onglets

**Carte** : couches de terrain ; contrôles de simulation (**pause**, vitesse
**×1/×4/×16**) ; **Suivre la population** (la caméra cadre le groupe, centre et
zoom — utile car sans clans la population se disperse vite) ; légende des
entités (agents colorés par activité) ; bandeau date/effectifs.

**Paramètres** : **seed**, **humains initiaux**, **densité de gibier**, puis
**Régénérer le monde** ; et un **placement manuel** — armez « Poser des
humains », cliquez la carte : une bande y est déposée et le gibier se génère
automatiquement autour.

## Hooks d'URL (debug / démarrage)

- `?ticks=N` — pré-avance la simulation de N ticks avant le premier rendu.
- `?static=1` — rend **une seule** frame puis s'arrête (pas de boucle) ; sert
  aux captures reproductibles.

## Construire et lancer

```sh
rustup target add wasm32-unknown-unknown

crates/client/build.sh                              # → crates/client/dist/
cargo run --release -p cairn-client --example serve # sert dist/ sur :8080
# puis ouvrir http://localhost:8080
```

`build.sh` compile le wasm et lance `wasm-bindgen`. Le serveur `serve`
(std pur, faute de `python`) sert `dist/` avec le bon type MIME
`application/wasm`. Installation de `wasm-bindgen-cli` : voir la note d'env.

Commandes : **glisser** pour déplacer la carte, **molette** pour zoomer
(géométrique, centré sur le curseur). Le panneau se déplace par son en-tête et
se replie.

## Vérifier le rendu sans navigateur

La logique de rendu (`src/render.rs`) est isolée du DOM et testable nativement :

```sh
cargo run --release -p cairn-client --example preview -- biome 80
# → out/client_biome.png  (mêmes couleurs que le canvas)
```

Pour vérifier le **rendu vivant** (terrain + agents + faune) exactement comme
le navigateur, mais en PNG — même `render_to_buffer`, même projection des
entités, caméra cadrée sur la population :

```sh
cargo run --release -p cairn-client --example client_render -- [seed] [ticks]
# → out/client_<seed>_<ticks>.png
```

> **Note d'environnement** : sur cette machine, Edge **headless** ne capture
> pas le calque canvas 2D (le DOM et la simulation, eux, tournent — les
> bandeaux d'état se mettent à jour). Vérifier le rendu via `client_render`
> ci-dessus, ou dans un vrai navigateur.

## Note d'environnement (toolchain windows-gnu) — RÉSOLU

Installer `wasm-bindgen-cli` a demandé de contourner deux limites de cette
machine (`x86_64-pc-windows-gnu`, pas de runtime VC++/UCRT complet) :

1. `windows-sys` (dép. de wasm-bindgen-cli) appelle `dlltool` → l'assembleur
   `as.exe`, absent du set « self-contained » de rustup. Fourni depuis un
   mingw-w64 winlibs : `as.exe`, `dlltool.exe` et `libwinpthread-1.dll` copiés
   dans `~/.cargo/bin` (sur le PATH). **Ne pas** mettre tout le `bin/` de
   winlibs sur le PATH : son `gcc` UCRT deviendrait le linker et produirait des
   binaires liés UCRT qui ne tournent pas ici.
2. `ring` (via `wasm-bindgen-test-runner`) veut un compilateur C → on l'écarte
   avec `--no-default-features`. Rust linke alors avec son `gcc` self-contained
   (MSVCRT, `msvcrt.dll` présent partout) et le binaire tourne.

Commande qui marche ici :
```sh
cargo install --force wasm-bindgen-cli --version 0.2.126 --no-default-features
```
(avec `as.exe`/`dlltool.exe`/`libwinpthread-1.dll` dans `~/.cargo/bin`).
