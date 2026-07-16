#!/usr/bin/env bash
# Construit le bundle web du client dans crates/client/dist/.
#
# Prérequis (une fois) :
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version 0.2.126   # doit matcher le crate wasm-bindgen
#
# Puis, à chaque build :   crates/client/build.sh
# Pour servir :            python -m http.server -d crates/client/dist 8080
#                          → http://localhost:8080

set -euo pipefail
cd "$(dirname "$0")/../.."

OUT=crates/client/dist
WASM=target/wasm32-unknown-unknown/release/cairn_client.wasm

cargo build --release --target wasm32-unknown-unknown -p cairn-client
wasm-bindgen --target web --no-typescript --out-dir "$OUT" --out-name cairn "$WASM"
cp crates/client/index.html "$OUT/index.html"

echo "OK → $OUT"
echo "Servez-le :  python -m http.server -d $OUT 8080   puis http://localhost:8080"
