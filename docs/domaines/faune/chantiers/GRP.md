# GRP — la taille des groupes d'ongulés

Ouvert le 2026-11-06 depuis CAP 3.3 (question de l'utilisateur : « le gibier est-il réel et
cohérent avec le modèle du loup ? »). Décision de l'utilisateur : suivre la méthode — prédire,
mesurer, corriger — et se rapprocher du réel, en sachant que les mesures seront d'abord faussées
par le reste (vigilance parfaite des proies, chantier du vent **VEN** à venir).

## Lecture (référence : le réel, `../SOURCES.md` « Taille des groupes d'ongulés »)

- Le modèle : un troupeau naît à `HERD_START` = 25 têtes, fusionne avec un voisin de même
  espèce à portée d'alerte (0,7 km pour le cerf) tant que la somme ne dépasse pas 90, se scinde
  en deux au-delà de `HERD_FISSION` = 90, disparaît sous `HERD_MIN` = 4. Mesuré
  (`out/cap/r1/`, grande scène, forêt, an 1) : ~60 têtes par troupeau, ~0,25 troupeau par km².
- Le réel : cerf élaphe 2,7 têtes par groupe l'été, 6,9 l'hiver (Białowieża) ; bison mâle 1,3-1,8 ;
  gazelle de Mongolie ~5 ; groupes fission-fusion qui vivent quelques heures, probabilité de
  scission croissant comme le carré de la taille (chevreuil).
- Écart : **10 à 20 fois trop gros**, à densité de têtes juste (10-12 /km²) ; ~16 fois trop peu
  de groupes. Classement : **modèle** (la fusion à portée d'alerte et le seuil de 90 n'ont pas
  d'équivalent réel ; `HERD_MIN` = 4 efface des groupes réels de 1 à 3).
- Ce que ça touche : les rencontres des meutes (une meute qui erre rencontre des groupes), la
  chasse humaine (CHA : pistes, prises), la pâture par maille, et le **coût** (entités ×~12).

## GRP-0 — mesure de l'effet (variante, branche `grp-mesure`)

Variante de mesure, pas une correction : `HERD_START` 5, `HERD_FISSION` 10, `HERD_MIN` 1,
**aucune fusion** ; `cap_nature` place les troupeaux à densité de têtes égale (pas de
1,5 km × √(START/25)). Régime : grande scène (1 024 km², boîte), forêt, sans humains, jour
135, 1 an, seeds 42, 7, 1337, 2024 ; témoin `out/cap/r1/` (`b79e719`).

Prédictions (écrites avant) :
- troupeaux ×8 au moins (250 → ≥ 2 000), 4 à 9 têtes en moyenne ;
- acquisitions de proie ×5 au moins (27-36 → ≥ 135 par an) ; prises ×3 au moins ;
- loups vivants au jour 180 sur au moins 2 seeds sur 4 (0 sur 4 au témoin) ;
- coût ×5 au moins par année de jeu (~5 min → ≥ 25 min).
**Réfutée** si les acquisitions montent de moins de ×2 : la taille des groupes ne serait pas
le goulot des rencontres (ce serait la vigilance parfaite, VEN).
