# Serveur — le monde qui tourne sans l'onglet, et ce qu'on en voit

Ce que le code fait aujourd'hui dans ce domaine, la référence (BRIEF) et les décisions de
l'utilisateur. Lu dans le code, tenu à jour à chaque jalon.

## Modules
- `crates/server` (SRV-1, 2026-10-24) : `main.rs` (arguments, fil de simulation au pas
  fixe, relevé de cadence toutes les 30 s), `hub.rs` (registre des observateurs, une boîte
  `watch` par abonné, instantané encodé une fois par abonné et par publication), `scene.rs`
  (la scène du banc d'acceptation, recopiée), `web.rs` (axum : `/ws` et fichiers du client).
- `crates/protocol` : `Hello`, `ServerMsg`, `ClientMsg`, `WorldSnapshot::from_sim`.
- `crates/client/src/live.rs` : mode observateur `?live=1` (ou `?server=hôte:port`).

## Référence
- BRIEF §8 (architecture : tokio + axum, WebSocket, abonnement par région, 5-10 Hz), §10
  phase 6 (critères : une semaine sans fuite ni dérive, trois clients sur trois régions
  sans surcoût notable).

## Décisions de l'utilisateur
- 2026-10-24 : SRV-1 avant MAR-6d — « voir le monde dans le navigateur » ; sans persistance.
