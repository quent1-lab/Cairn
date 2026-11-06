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

### GRP-0 — mesure (binaire `out/bin/grp0/`, branche `grp-mesure`, `out/cap/grp0/`, 2026-11-06)

Témoin `out/cap/r1/` (`b79e719`) → variante, an 1 :

| seed | troupeaux j5 | têtes/troupeau fin | acquisitions | prises | loups j90 | loups j180 | durée de l'an |
|---|---|---|---|---|---|---|---|
| 42 | 307 → 2 208 | 58,7 → 7,1 | 27 → 246 | 9 → 43 | 5,1 → 10,2 | 0 → 0 | ~5 → 74 min |
| 7 | 330 → 2 304 | 60,0 → 6,7 | 28 → 221 | 4 → 16 | 1,0 → 3,1 | 0 → 0 | ~5 → 51 min |
| 1337 | 242 → 1 739 | 58,2 → 7,1 | 36 → 901 | 4 → 147 | 4,7 → 15,2 | 0 → 8,7 | ~5 → 45 min |
| 2024 | 319 → 2 304 | 59,7 → 6,6 | 36 → 276 | 3 → 15 | 0 → 5,8 | 0 → 1,8 | ~6 → 48 min |

- Troupeaux ×8 (×7 à ×9), 6,6-7,1 têtes en fin d'an : **tenue**.
- Acquisitions ×5 au moins : **tenue** (×8 à ×25). Prises ×3 au moins : **tenue** (×4 à ×37).
- Loups vivants au jour 180 sur 2 seeds sur 4 au moins : **tenue de justesse** (1337 : 8,7 ;
  2024 : 1,8) ; à l'an 1, une seule meute résiduelle (1337 : 1,3 loup).
- Coût ×5 au moins : **tenue** (×9 à ×15) — la grande scène passe à ~45-75 min par an de jeu.
- Densité de têtes inchangée (carré 11-15 /km² des deux côtés). Aucun troupeau n'a dépassé
  10 têtes ni ne s'est dissous dans l'année : la dynamique de scission n'est pas mesurée ici.

**Conclusion** : la taille des groupes est bien un goulot des rencontres — ×8 à ×25 —, mais
pas le seul : les meutes errent encore 91-98 % de leurs heures de faim et s'éteignent quand
même. Ordre de grandeur réel (déduit, non mesuré) : une meute de 5 mange ~22 kg/j, un cerf
tous les 3-4 jours, ~100 prises par an, soit ~1 000 rencontres à 8-12 % de succès ; la
variante en donne 50 à 200 par meute. L'écart restant (×5-20) est l'objet de **VEN** (la proie
voit toute meute à 0,7 km).

**Ce que j'ai pu manquer** : la variante fige les groupes (scission à 10, sans fusion) — elle
ne dit rien de la taille qu'une vraie dynamique fission-fusion produirait, ni de la saison
(groupes d'hiver ×2,5). Elle ne touche pas la chasse humaine (non mesurée). Un an, forêt seule.
Le coût ×10 rend la correction dépendante d'un travail de performance (grille spatiale des
requêtes de voisinage : `nearest_threat`, `owns`, la recherche de proie, `merge_plan` sont
linéaires en troupeaux, par meute ou par troupeau).
