#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# verify.sh — socle MÉCANIQUE de l'audit Cairn (voir docs/AUDIT.md).
#
# Il ne JUGE pas : il compile, teste, MESURE, et POINTE des suspects à relire.
# La passe de jugement (émergence, couverture, optim) reste humaine.
#
# Usage :  scripts/verify.sh [ticks_bench]     (défaut 600)
#
# Ne dépend que de cargo + grep + awk. Ne lance PAS la suite complète
# (~40 min) : seulement le test de déterminisme, qui est la garde qui compte.
# ─────────────────────────────────────────────────────────────────────────────
set -uo pipefail
cd "$(dirname "$0")/.."

TICKS="${1:-600}"
FAIL=0

sec()  { printf '\n\033[1;36m══ %s ══\033[0m\n' "$1"; }
ok()   { printf '  \033[32m✓\033[0m %s\n' "$1"; }
bad()  { printf '  \033[31m✗ %s\033[0m\n' "$1"; FAIL=1; }
note() { printf '  \033[33m•\033[0m %s\n' "$1"; }

# ── 1. Clippy : idiomes Rust + lints de performance (gating) ────────────────
sec "Clippy (workspace)"
if cargo clippy --workspace -- -D warnings 2>&1 | tail -n 15; then
  ok "clippy propre (aucun lint)"
else
  bad "clippy signale des lints — voir ci-dessus"
fi

# ── 2. Déterminisme : la garde bit-à-bit (gating) ───────────────────────────
sec "Déterminisme (test bit-à-bit)"
if cargo test -p cairn-sim --lib --release bit_a_bit 2>&1 | tail -n 8; then
  ok "deux exécutions identiques bit-à-bit"
else
  bad "le déterminisme est cassé"
fi
note "suite complète (~40 min) : cargo test -p cairn-sim  — à lancer avant un gros merge"

# ── 3. Odeurs de scripting / déterminisme (informational : à RELIRE) ────────
# Ces greps ne prouvent rien : ils listent des points à confirmer au jugement.
sec "Suspects — émergence & déterminisme (à relire, voir docs/AUDIT.md)"

probe() { # <titre> <motif> <chemins…>
  local title="$1"; shift; local pat="$1"; shift
  local hits; hits=$(grep -rEn --include='*.rs' "$pat" "$@" 2>/dev/null || true)
  local n; n=$(printf '%s' "$hits" | grep -c . || true)
  if [ "$n" -eq 0 ]; then
    ok "$title : 0"
  else
    note "$title : $n à relire"
    printf '%s\n' "$hits" | sed 's/^/      /'
  fi
}

# A — un comportement déclenché par une TAILLE (interdit) vs une mesure (permis)
probe "taille dans un test (\`.len() > N\`)" '\.len\(\)[[:space:]]*(>=|>|==)[[:space:]]*[0-9A-Z]' crates/sim/src
# A — un ÂGE (étiquette dérivée) lu dans une condition (interdit hors affichage)
probe "\`Age::\` en dehors de label()/age_of()" 'Age::(Paleolithic|Neolithic|BronzeAge)' crates/sim/src
# A — un tick/date en dur qui scripterait un événement
probe "tick/date en dur (\`tick == N\`)" 'tick[[:space:]]*==[[:space:]]*[0-9]' crates/sim/src
# B — HashMap/HashSet dans la SIMULATION (ordre non déterministe)
probe "HashMap/HashSet en sim/worldgen/core" 'HashMap|HashSet' crates/sim/src crates/worldgen/src crates/core/src
# B — RNG non dérivé de la seed
probe "RNG non seedé (thread_rng, rand::random…)" 'thread_rng|rand::random|from_entropy|SmallRng|StdRng' crates

# ── 4. Débit (tps) : micro-banc + comparaison à la référence ────────────────
sec "Débit — micro-banc client_render ($TICKS ticks, seed 42)"
cargo build --release -p cairn-client --example client_render >/dev/null 2>&1 \
  || bad "build client_render échoué"
t0=$(date +%s.%N)
# Le suffixe `.exe` n'existe que sur Windows : on prend celui qui est là.
BENCH=target/release/examples/client_render
[ -x "$BENCH" ] || BENCH="$BENCH.exe"
"./$BENCH" 42 "$TICKS" 0.6 0 >/dev/null 2>&1 || true
t1=$(date +%s.%N)
tps=$(awk -v t="$TICKS" -v a="$t0" -v b="$t1" 'BEGIN{ e=b-a; if(e<=0)e=1; printf "%.2f", t/e }')
elapsed=$(awk -v a="$t0" -v b="$t1" 'BEGIN{ printf "%.0f", b-a }')
note "tps = $tps   ($TICKS ticks en ${elapsed} s)"

base="docs/perf-baseline.txt"
if [ -f "$base" ]; then
  ref=$(grep -oE 'tps=[0-9.]+' "$base" | head -1 | cut -d= -f2)
  if [ -n "${ref:-}" ]; then
    verdict=$(awk -v c="$tps" -v r="$ref" 'BEGIN{
      d=(c-r)/r*100;
      if(d < -15) printf "RÉGRESSION %.0f%% (réf %.2f)", d, r;
      else if(d > 15) printf "gain %.0f%% (réf %.2f)", d, r;
      else printf "stable (réf %.2f, %+.0f%%)", r, d }')
    case "$verdict" in
      RÉGRESSION*) bad "tps : $verdict — à investiguer" ;;
      *)           note "tps : $verdict" ;;
    esac
  fi
else
  echo "tps=$tps  # $(date +%F) seed42 ${TICKS}ticks client_render (machine dev)" > "$base"
  note "référence de perf créée : $base (à re-générer si tu changes de machine)"
fi

# ── Verdict ─────────────────────────────────────────────────────────────────
sec "Verdict mécanique"
if [ "$FAIL" -eq 0 ]; then
  ok "socle vert — passer à la passe de jugement (docs/AUDIT.md)"
else
  bad "des gardes ont échoué — corriger avant la passe de jugement"
fi
exit "$FAIL"
