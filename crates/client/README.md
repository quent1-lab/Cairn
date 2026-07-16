# cairn-client

Client web (WASM) de Cairn : une fenêtre plein écran sur le monde, avec caméra
scrollable/zoomable et couches de debug (biomes, altitude, température,
humidité, géologie) dans un panneau flottant.

## Construire et lancer

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.126   # doit matcher le crate wasm-bindgen

crates/client/build.sh                              # → crates/client/dist/
python -m http.server -d crates/client/dist 8080    # puis http://localhost:8080
```

Commandes : **glisser** pour déplacer la carte, **molette** pour zoomer
(géométrique, centré sur le curseur). Le panneau se déplace par son en-tête et
se replie.

## Vérifier le rendu sans navigateur

La logique de rendu (`src/render.rs`) est isolée du DOM et testable nativement :

```sh
cargo run --release -p cairn-client --example preview -- biome 80
# → out/client_biome.png  (mêmes couleurs que le canvas)
```

## Note d'environnement (toolchain windows-gnu)

Sur une toolchain `x86_64-pc-windows-gnu` **sans mingw-w64 complet**,
`cargo install wasm-bindgen-cli` (et `trunk`) échoue à compiler `windows-sys` :
`dlltool` a besoin de l'assembleur `as.exe`, absent du set « self-contained »
de rustup. Les binaires précompilés (MSVC) ne se lancent pas non plus si le
runtime Visual C++ n'est pas installé (`STATUS_DLL_NOT_FOUND`).

Options pour débloquer :
- installer **mingw-w64** (fournit `as.exe`, `dlltool`, `gcc`) et l'ajouter au
  PATH, puis `cargo install wasm-bindgen-cli` ;
- **ou** installer le *Microsoft Visual C++ Redistributable* et utiliser les
  binaires précompilés de `trunk` / `wasm-bindgen` ;
- **ou** produire le bundle sur une autre machine (le crate compile en wasm
  partout : `cargo build --target wasm32-unknown-unknown -p cairn-client` passe
  déjà ici).
