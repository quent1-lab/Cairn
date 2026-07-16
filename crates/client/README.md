# cairn-client

Client web (WASM) de Cairn : une fenêtre plein écran sur le monde, avec caméra
scrollable/zoomable et couches de debug (biomes, altitude, température,
humidité, géologie) dans un panneau flottant.

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
