---
name: cloture
description: Reboucher un jalon ou clore un chantier de Cairn — les trois traces (prédictions, journal, carte), « Où reprendre », l'ordre des chantiers, le commit sans co-auteur, la poussée, la republication de la carte lisible en fin de chantier, et la réponse finale réduite aux conclusions et questions. À charger après chaque mesure validée et à la fin d'un chantier.
---

# Clôture — un résultat, trois traces

## Après chaque jalon (commit d'un module)
1. **`out/<chantier>/predictions.md`** : tableau mesuré, verdict de chaque prédiction,
   lecture, « ce que j'ai pu manquer ».
2. **`docs/CODE.md`** : la ligne du module (§1.x) — ce que le code fait désormais, et le
   chiffre mesuré ; le registre §5.1 (✔ / ▶ / ○ / ?) ; l'ordre §5.2, **redonné à chaque
   modification** (titre « révisé <date> »).
3. **`docs/JOURNAL.md`** : une entrée en tête du chantier concerné (insérer par `python3`
   après l'entrée voisine, jamais lire le fichier entier).
4. **CLAUDE.md, « Où reprendre »** : une seule ligne à jour (en cours, prochain, ordre).

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
