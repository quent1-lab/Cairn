---
name: debat
description: Faire débattre un chantier de Cairn par l'agent contradicteur (Opus) — quand le lancer, quoi lui donner, comment répondre point par point dans le fichier du chantier, et quand s'arrêter. À charger à la fin d'un jalon mesuré, après deux prédictions réfutées de suite, ou avant de présenter une question de vision à l'utilisateur.
---

# Débat — un regard neuf qui conteste

## Quand
- À la **fin d'un jalon mesuré**, avant la clôture (c'est l'étape « ce que j'ai pu
  manquer », règle 9.6, faite par un autre).
- Après **deux prédictions réfutées de suite** sur le même chantier.
- Avant de porter une **question de vision** à l'utilisateur : arriver avec les objections.
Pas pour un jalon d'observation seule, ni pour une correction de typographie.

## Comment
1. Le fichier du chantier doit être à jour (prédictions, tableau mesuré, lecture).
2. Lancer l'agent `contradicteur` (modèle Opus, en tâche de fond) — s'il n'est pas
   proposé (les agents de `.claude/agents/` ne se chargent qu'au démarrage d'une session),
   un agent `general-purpose` en `model: opus` avec le corps de
   `.claude/agents/contradicteur.md` en tête de consigne — avec seulement :
   le chemin du fichier du chantier, le chemin du dossier de domaine, et la question
   (« conteste les conclusions de la section X »). Ne rien lui résumer : il lit.
3. À son retour, écrire dans le fichier du chantier une section
   `## Débat (<date>)` : chaque point de l'agent en une ligne, puis **ma réponse** —
   *retenu* (avec l'action : mesure à lancer, prédiction à ajouter) ou *écarté* (avec la
   raison, chiffre à l'appui).
4. Un second tour (le relancer avec mes réponses) seulement si un point écarté reste
   douteux. **Deux tours au plus.**

## Ce qui remonte à l'utilisateur
Seulement les points retenus qui changent une conclusion ou posent une question de
vision. Le reste reste dans le fichier.
