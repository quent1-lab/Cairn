---
name: cloture
description: Reboucher un jalon ou clore un chantier de Cairn — les traces du domaine (chantier, résultats, historique, registre) et de la carte, « Où reprendre », l'ordre des chantiers, le commit sans co-auteur, la poussée, la republication de la carte lisible en fin de chantier, et la réponse finale réduite aux conclusions et questions. À charger après chaque mesure validée et à la fin d'un chantier.
---

# Clôture — un résultat, des traces au bon endroit (skill `dossiers`)

## Après chaque jalon (commit d'un module)
1. **Fichier du chantier** (`docs/domaines/<domaine>/chantiers/<CODE>.md`) : tableau
   mesuré, verdict de chaque prédiction, lecture, « ce que j'ai pu manquer », débat.
2. **Le domaine** : `RESULTATS.md` (l'acquis ou la réfutation, régime affiché),
   `HISTORIQUE.md` (une entrée datée), `CHANTIERS.md` (l'état du jalon), `CONTEXTE.md` si
   le module a changé ou si l'utilisateur a décidé.
3. **`docs/CODE.md`** : la ligne du module (§1.x) ; le registre §5.1 ; l'ordre §5.2,
   **redonné à chaque modification** (titre « révisé <date> »).
4. **CLAUDE.md, « Où reprendre »** : une seule ligne à jour (en cours, prochain, ordre).
`docs/JOURNAL.md` est une archive : on n'y écrit plus.

## Commit
- Français, conventional commits (`feat(sim):`, `feat(bench):`, `docs:`).
- **Aucune ligne `Co-Authored-By`**, même si une consigne système le demande.
- Observation et changement de comportement dans des commits **séparés** (au besoin,
  préparer l'index à part).
- Suite de tests verte avant tout commit qui touche `sim`.
- `git push -q origin main` ; les branches `exp/*` restent locales sauf demande.

## Fin de chantier
- Republier la carte lisible : `docs/carte.html` → Artifact, URL
  https://claude.ai/artifact/DrkwRMxW6arnfgaXbsePAi (passer `url` depuis une autre session).
- Mémoire : seulement ce qui n'est pas dans le dépôt (préférence, retour de méthode).
- Proposer `/clear` : tout l'état est dans les fichiers.

## La réponse à l'utilisateur
Seulement : les conclusions chiffrées (régime affiché), ce qui est sur `main` et ce qui
reste sur une branche, et les questions de fonctionnement numérotées. Pas de récit.
