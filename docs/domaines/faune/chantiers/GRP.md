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

## Coût de la grande scène (PERF, 2026-11-06)

Profil (`cap_nature` feature `profile`, variante GRP-0, seed 42, 30 jours) : 96 % du tick dans
`update_herds`, dont 82 % dans `best_pasture` ; **407 000 chunks générés en 720 ticks** (~570
par tick) : le cache de 16 384 chunks (~1 Go) ne couvre que ~270 km² des 1 024 de la scène,
et 2 200 troupeaux étalés le font tourner en rond. Le cache ne peut pas grandir (mémoire).
Dans le scénario réel (`longue` 42 tempéré, 15 min, `out/perf/longue/`), la variante ne coûte
rien (~10 troupeaux autour des humains, débit 48-79 tps contre 26-41) : **le coût est propre
au banc**. Mais la même mesure montre que l'immigration compte en troupeaux : à 5 têtes, le
gibier autour des humains tombe de 300-600 têtes à 2-56 — à traiter dans GRP-1.

Changement (1) : la victime du LRU se lit dans un index trié `(tick, coordonnée)` au lieu d'un
balayage des 16 384 résidents par éviction. Même départage, donc résultat identique au bit
près (vérifié en comparant le CSV `cap_nature` 42 forêt 1 an au témoin `out/cap/r1/`).
Prédiction : le temps de `4d troupeaux` baisse d'au moins 30 % sur 30 jours de GRP-0 ;
réfutée sous 10 % (la génération du chunk dominerait alors).

Mesure (1) : `CSV` `cap_nature` 42 forêt 1 an **identique** au témoin `r1` (`cmp`) ; tests du
monde verts. GRP-0, 30 jours, seed 42 : `4d troupeaux` 268 s → **150 s (−44 %)**, chunks générés
identiques (407 356) — **prédiction tenue** (un second banc tournait en même temps : gain sous-
estimé). Reste dominant : la génération des chunks (`best_pasture`, 100 s sur 150).
Ce que j'ai pu manquer : le gain dans le jeu réel n'est pas mesuré (là, presque aucune
éviction : le cache couvre le périmètre).

## État au 2026-11-06 au soir (consigné avant un redémarrage)

Ordre décidé par l'utilisateur : PERF → GRP-1 → VEN → clôture de CAP 3.3.
**Questions en attente de l'utilisateur** :
1. GRP-1 et l'immigration : la compter en têtes (recommandé : même flux de têtes, en groupes
   de taille réelle ; CHA-5 la remplacera) ou avancer CHA-5.
2. Arrêter PERF au gain du LRU (banc estimé à ~25-40 min par an de jeu, non mesuré sur un an
   entier ; le jeu réel n'est pas touché).
Modèle GRP-1 proposé (non validé) : scission avec une probabilité ∝ taille² (échelle à tirer
de la durée de vie des groupes de chevreuil, Pays et al. 2012), fusion de deux groupes qui se
croisent à courte portée, `HERD_MIN` 1. Variante GRP-0 : branche locale `grp-mesure`
(binaires `out/bin/grp0/`, sorties `out/cap/grp0/`, `out/perf/`).

## GRP-1 — décision et conception (2026-11-06)

Décision de l'utilisateur : suivre les recommandations — l'immigration en têtes, PERF arrêté au
gain du LRU, puis la dynamique des groupes. En deux changements séparés (règle 4).

### GRP-1a — les sites reçoivent des têtes (refactor, neutre)

`fauna::SITE_HEADS` = 25 têtes par site (placement des scènes, immigration), arrivées en groupes
de `HERD_START` au plus d'une même espèce (`Sim::spawn_heads`). Avec `HERD_START` = 25, un site
reçoit un groupe de 25 au même lieu, avec le même tirage d'espèce : **identique au bit près**
attendu (vérifié sur la Pi : `cap_nature` 42 forêt 1 an et `longue` 42 tempéré 1 an, avant et
après, `out/grp1a/` sur la Pi). L'autolimitation de l'immigration (aucun troupeau à 3 km du site)
reste une porte en nombre de troupeaux : CHA-5 la remplacera.

### GRP-1b — la dynamique des groupes (conception, à mesurer)

- `HERD_START` 5 (taille d'un groupe à sa formation ; réel 2,7 l'été, 6,9 l'hiver) ;
  `HERD_MIN` 1 : une bête seule reste un groupe.
- **Scission** : chaque heure, un groupe de n têtes se scinde avec une probabilité k·n²
  (Pays et al. 2012 : ∝ carré de la taille) ; les deux parts tirées uniformément (même source).
  Échelle k : un groupe de ~5 vit « quelques heures » (même source) ; avec 3 h, k ≈ 0,2/25 =
  0,008 par heure. Le seuil de 90 disparaît.
- **Fusion** : chaque heure (et non chaque jour : la cadence doit suivre le phénomène, des
  heures), deux groupes de même espèce à moins de ~200 m se rejoignent (portée de perception
  en forêt, même source). Plus de plafond de somme. Index spatial pour rester linéaire.
- La taille des groupes **naît** alors du rapport scission/rencontres (densité-dépendante,
  comme dans le réel) ; elle se **valide** contre 2,7-6,9 (cerf), elle n'est pas posée.
- Limites connues : sources de seconde main ou sur le chevreuil ; la saison (groupes d'hiver
  plus gros) n'a pas de mécanisme ici.

### GRP-1b — tests rouges et prédictions (écrites avant le code)

Tests rouges vus échouer : `un_grand_groupe_se_defait_en_quelques_heures` (60 cerfs, un jour :
1 groupe de 60,1) ; `deux_betes_seules_se_rejoignent_dans_l_heure` (aucun groupe : les deux
bêtes effacées sous `HERD_MIN` = 4). L'ancien `deux_troupeaux_qui_se_voient_se_rejoignent`
perd son volet « réunion au-delà du seuil de fission » (le seuil disparaît).

Prédictions, sur la Pi, grande scène (1 024 km², boîte), forêt, jour 135, 1 an, 4 seeds, témoin
`main` après GRP-1a :
- taille moyenne des groupes de cerfs **entre 2 et 10** sur toute l'année, sur 4 seeds
  (réel : 2,7-6,9) ; réfutée hors de [1,5 ; 15] ;
- acquisitions des meutes ×5 au moins par rapport au témoin (comme GRP-0) ;
- densité de têtes du carré à l'an 1 à ±25 % du témoin (la taille des groupes ne doit pas
  changer la nature, seulement sa forme) ;
- `longue` 42 tempéré 1 an (humains) : gibier à 10 km du foyer dans le même ordre de grandeur
  en têtes que le témoin (×0,5 à ×2) ; prises de la bande non effondrées (≥ 50 % du témoin).
