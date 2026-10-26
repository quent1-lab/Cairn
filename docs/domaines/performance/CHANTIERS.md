# Chantiers — Performance — débit, mémoire, chunks

Registre local : code, état (✔ fait · ▶ en cours · ○ à faire · ? en attente d'une
décision), une ligne, le fichier du chantier. L'ordre global vit dans `docs/CODE.md` §5.2.

Codes de ce domaine : PERF, D7

- ? **PERF-MAR6d** (problème vu, 2026-10-26, décision de l'utilisateur) : sur `main` depuis le report de MAR-6d (6b38f1c), débit
  plancher 19,3 ticks/s (t42, 1 an ; `main` 24-73 en acceptation). Profil : `5 ecologie` 127 s/an
  contre 50 sur `main` (une bande qui reste dans son pays retouche les mêmes chunks : la liste
  `Chunk::touched` à repousser grossit) ; `3t echanges` 35 s (était 81, 2,3 sur `main` : la
  parole échange 256 sources par paire). Pistes : retirer de `touched` une tuile revenue au
  baseline ; ne transmettre que les sources plus récentes que ce que sait l'autre. Détail :
  `docs/domaines/humain/deplacement/chantiers/MAR.md` (MAR-6d, étape 4).
