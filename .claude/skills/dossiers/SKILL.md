---
name: dossiers
description: Où ranger et où lire le savoir de Cairn — l'arborescence par domaine (humain/cognition, humain/deplacement, faune, flore, monde…), la structure commune de chaque dossier (CONTEXTE, SOURCES, CHANTIERS, RESULTATS, HISTORIQUE, chantiers/), comment ouvrir un chantier ou un domaine, et ce qui reste global. À charger avant d'écrire une trace, d'ouvrir un chantier ou de chercher une décision passée.
---

# Dossiers — un domaine, une structure

Index : `docs/domaines/README.md`.

## Où écrire quoi
| Je veux… | J'écris dans… |
|---|---|
| prédire, mesurer, débattre pendant un chantier | `docs/domaines/<domaine>/chantiers/<CODE>.md` |
| noter un chiffre réel | `<domaine>/SOURCES.md` (valeur, unité, population, lien, date, qui, lu ou de seconde main) |
| consigner une décision de l'utilisateur | `<domaine>/CONTEXTE.md` § Décisions (datée) **et** le fichier du chantier |
| acter un résultat validé ou une réfutation | `<domaine>/RESULTATS.md` (régime affiché) |
| dater un jalon | `<domaine>/HISTORIQUE.md` |
| changer l'état d'un jalon | `<domaine>/CHANTIERS.md` et `docs/CODE.md` §5.1 |
| changer l'ordre global | `docs/CODE.md` §5.2 et « Où reprendre » de CLAUDE.md |
| décrire ce que fait un module | `docs/CODE.md` (carte globale) et `<domaine>/CONTEXTE.md` § Modules |
| garder une sortie brute de banc | `out/<chantier>/` (non suivi par git, jamais de conclusion) |

`docs/JOURNAL.md` et `docs/ETAT.md` sont des **archives** (avant le 2026-10-16) : on y
cherche par `grep`, on n'y écrit plus.

## Choisir le domaine
Un chantier vit dans le domaine de son **défaut** (là où le code s'écarte de la référence).
S'il en touche d'autres, une ligne de renvoi dans leur `CHANTIERS.md`. En cas de doute entre
deux domaines, le plus en amont de la carte.

## Ouvrir un chantier
1. Code : préfixe du domaine (MAR, CHA, MOR, ENE, MEM…) + numéro de jalon.
2. Créer `chantiers/<CODE>.md` : titre, date, décision qui l'ouvre, lecture du code.
3. Ajouter la ligne dans `<domaine>/CHANTIERS.md` et `docs/CODE.md` §5.1.

## Ouvrir un domaine ou un sous-domaine
Copier la structure (les cinq fichiers et `chantiers/`) d'un domaine voisin, vider leur
contenu sous les en-têtes, ajouter la branche dans `docs/domaines/README.md`.
