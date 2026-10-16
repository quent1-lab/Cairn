---
name: banc
description: Lancer, attendre et lire les bancs de Cairn sans tuer la session ni mêler les variantes — binaires figés par variante, scopes mémoire plafonnées, files de nuit, attente en tâche de fond, lecture ciblée des sorties, synthèse des runs longues. À charger avant tout `cargo run --example` long, toute run longue ou toute comparaison témoin/variante.
---

# Banc — mesurer sans se tromper de binaire ni de régime

Durées, arguments et pièges détaillés : `docs/COMMANDES.md`. Machine : 2 vCPU, 2,9 Go.

## Règles de sécurité (chacune a déjà coûté une session)
- **Chaque banc dans sa scope plafonnée** :
  `systemd-run --user --scope -q -p MemoryMax=<M> -p MemorySwapMax=<S> -- <binaire> <args>`.
  `marche` : 2400M/1500M ; `longue` : 1400M/1G ; `acceptation` : 2400M/1500M.
- **Deux bancs lourds au plus** en même temps (tests compris si possible).
- `marche` en été : **1 fil** (2 fils dépassent 2,2 Go) ; en hiver : 2 fils.
- Lancer détaché : `setsid nohup <script> > <log> 2>&1 &`. Tuer **par PID**
  (`ps -eo pid,args | grep …`), jamais `pkill -f`.
- Ne jamais rebâtir `target/release/examples/<banc>` pendant qu'une file l'utilise.

## Figer un binaire par variante
Avant toute série, copier les binaires dans `out/bin/<variante>/` :
```
git checkout -q <branche> && cargo build --release -p cairn-sim --example marche --example longue
mkdir -p out/bin/<variante> && cp target/release/examples/{marche,longue} out/bin/<variante>/
git checkout -q main
```
Le témoin est une variante comme les autres (`out/bin/main/`). On peut alors recompiler
librement pendant que les runs tournent.

## Files de nuit
Un script par file dans `out/<chantier>/queueN.sh` : bancs courts d'abord (signal
rapide), puis runs longues en `xargs -P 2`, et un fichier `queueN_done` en fin. Une file
suivante attend la précédente : `until [ -f …/queueN_done ]; do sleep 60; done`.
Run longue : `out/bin/$v/longue $seed $scene 135 $L.csv 10 > $L.log 2>&1`
(10 ans ≈ 20-60 min par run selon la seed).

## Attendre
- Jamais de `sleep` bloquant. Une commande en tâche de fond (`run_in_background`) :
  `until [ -f <done> ]; do sleep 60; done; <extraction>` — la notification réveille.
- Attendre un **fichier**, jamais un processus par `pgrep -f <motif>` : la boucle contient
  le motif et se trouve elle-même (attente sans fin). Pour une commande lancée en tâche de
  fond (tests), sa propre notification suffit.
- À chaque lancement, annoncer une fourchette de durée et l'heure de Paris
  (`TZ=Europe/Paris date +%H:%M`).

## Lire
- Jamais une sortie entière : `grep -E "^marche :|^## chasse"`, `sed -n '/## rendements/,/hommes/p'`,
  `grep -E "^  (Track|SeekGame|Hunt|ReturnToClan) "`.
- Runs longues : `DIR=out/longue/<chantier> python3 scripts/analyse_longue.py <variantes…>` (pop, morts, soif, foyer,
  huttes, têtes/an, marche) ; pour les naissances et causes, lire la dernière ligne du CSV.
- Toujours afficher le **régime** (scène, saison de départ, seeds, horizon) avec les
  chiffres, et comparer au témoin **mesuré dans le même régime**.
- Les sorties brutes restent dans `out/` (non suivi) ; le tableau qui en est tiré va dans
  le fichier du chantier (`docs/domaines/…/chantiers/<CODE>.md`).
- Un banc qui ne couvre pas le phénomène se dit : l'acceptation d'un an ne voit pas le
  sevrage (aucun enfant n'atteint 6 mois la première année) — choisir l'horizon d'après le
  phénomène.

## Régimes de référence
- `marche 90 315 42,7,1337,2024 2` (hiver) et `marche 90 135 42,7,1337,2024 1` (été).
- `longue <seed> {froid,tempere} 135 <csv> 10` × seeds 42, 7, 1337, 2024.
- `acceptation 1 135 42,7,1337,2024 1` après un chantier ; 5 ans avant un report.
