---
name: chantier
description: La boucle de travail de Cairn pour tout chantier ou jalon (MAR-6b, CHA-3, MOR-4…) — lire le code contre une référence, classer, prédire avant de mesurer, figer le témoin, voir le test échouer, un changement à la fois, valider sur 4 seeds, chercher ce qu'on a manqué, reboucher. À charger avant de toucher au modèle de simulation, et à relire à chaque jalon.
---

# Chantier — la boucle (CLAUDE.md, règles 1-10)

Suivre les étapes **dans l'ordre**. Cocher en écrivant dans
`out/<chantier>/predictions.md` : ce fichier est la trace, pas la réponse.

## 0. Avant tout
- Relire « Où reprendre » (CLAUDE.md) et l'ordre (`docs/CODE.md` §5.1-5.2).
- Relire la fin de `out/<chantier>/predictions.md` (`tail`, ou `grep -n "^## "`) : les
  décisions de l'utilisateur y sont consignées. Ne jamais lire `docs/JOURNAL.md` en
  entier : `grep`.
- Décision de modèle ou de vision non tranchée ? **Demander** (règle 10) avant de coder.
  Une décision reçue s'écrit tout de suite dans `predictions.md`, avec sa date.

## 1. Lire
- Lire le code concerné par `grep -n` et plages de lignes (pas de sous-agent pour lire du
  code à diagnostiquer).
- Nommer le défaut comme un **écart entre le code et une référence** : le BRIEF (citer la
  ligne) ou la réalité (citer la source). Sources : chercher un chiffre réel (WebSearch) ;
  il sert à **juger**, jamais à régler une constante.
- Chercher les **portes tout ou rien** et les **correctifs de résultat** passés (`grep`
  JOURNAL) : ils cachent souvent le vrai défaut (ex. `anchor_homes`).

## 2. Classer
- Modèle, bug ou coût. **L'amont avant l'aval** : si un comportement ne devrait pas
  exister, on ne l'optimise pas. Si l'amont est une décision, la demander.

## 3. Mesurer le témoin d'abord (si la grandeur n'est pas déjà mesurée)
- Instrumenter **à la source**, en commit séparé « observation seule » : le banc doit
  alors reproduire **à l'identique** les colonnes anciennes (`diff` avec la sortie
  précédente) — c'est la preuve qu'il n'influe sur rien.
- Deux compteurs indépendants qui doivent concorder (règle 5).

## 4. Prédire, avant de lancer
Écrire dans `predictions.md`, **avant** le banc :
- le changement en une phrase, et ce qui est un choix non sourcé (constante, forme) ;
- des prédictions **chiffrées** (fourchettes), une par grandeur ;
- **ce qui réfuterait** (une clause explicite) ;
- le régime (saisons, scènes, seeds, horizon).

## 5. Le test de reproduction rouge
- Écrire le test qui doit échouer **sans** le changement. Le voir échouer : neutraliser
  le changement par une ligne marquée `// TEMP-ROUGE`, lancer le test ciblé, rétablir,
  vérifier `grep -c TEMP-ROUGE` = 0.
- Un test qui passe avant le correctif ne prouve rien (règle 6).

## 6. Corriger — un changement, une chose
- Une branche ou un commit par mécanisme. **Jamais empiler** deux expériences : chaque
  variante part de `main` (leçon MAR-6b : explorer/raconter empilés sur « sans ancre »
  mêlaient leurs effets à un effondrement).
- Vérifier que ce n'est pas scripté : un facteur plutôt qu'un seuil ; pas de constante
  réglée vers un critère du BRIEF ; regarder les deux bouts de la distribution.
- Suite complète `cargo test --release -p cairn-sim --lib` **avant chaque commit qui
  touche `sim`** (≈ 4-9 min ; en tâche de fond si des bancs tournent). Un test rouge
  ailleurs : le lire, ne pas toucher son seuil pour le faire passer ; le consigner.

## 7. Mesurer (skill `banc`)
- 4 seeds × 2 scènes, cadence du phénomène : 90 j au banc `marche` pour un effet de
  comportement, 10 ans au banc `longue` pour les effectifs, acceptation 1 an après le
  chantier, 5 ans avant un report.
- Une affirmation sur les effectifs ne compte que si elle va **dans le même sens sur
  toutes les runs** (règle 7) ; sinon « sans direction commune ».

## 8. Valider et relever les voisins
- Confronter chaque prédiction : tenu / réfuté / sens réfuté, avec les chiffres, dans un
  tableau avant/après.
- Une prédiction réfutée est une connaissance : l'écrire, expliquer si possible, ne pas
  réécrire la prédiction.
- Relever les voisins que le changement a pu bouger (soif, naissances, faune, marche…).

## 9. Ce que j'ai pu manquer (obligatoire, jamais « non »)
Nommer au moins une chose : régime non couvert, constante non balayée, autre explication
que les chiffres n'excluent pas, voisin non regardé, cause mêlée.

## 10. Reboucher (skill `cloture`)
Trois traces : `predictions.md`, une entrée au JOURNAL, une ligne dans `docs/CODE.md`
(dans le même mouvement que le commit du module). Une expérience non reportée reste sur
sa branche `exp/<nom>` avec la mention « à valider ».

## À l'utilisateur
Seulement les conclusions d'une mesure ou d'un chantier, et les questions de
fonctionnement (décision de modèle, vision, blocage). Pas de récit d'étape.
