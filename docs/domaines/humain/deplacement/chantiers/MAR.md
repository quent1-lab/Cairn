# Chantier : la marche (2026-10-05)

Constat (banc d'acceptation, chantier énergie) : 33-40 km/j de marche pour
les deux sexes ; Hadza 5,8 (femmes) et 11,4 (hommes) — Pontzer et al. 2012.

## Témoin — banc `marche` (6b0a35d+lait, 90 j, départ jour 135, 4 seeds × 2 scènes)

- Marche : femmes 9,3 h/j (37 km), hommes 9,1 h/j (36 km) ; 32-40 km/j sur
  8/8 runs. Sommeil 7,3-7,4 h/j.
- **Le jour et la nuit ne sont pas vécus** : à 3 h du matin, 35 % dorment et
  37 % marchent ; à 10 h, 15 % dorment. Le sommeil est éparpillé sur 24 h.
- Marche par tâche (h/j, femmes ; hommes identiques à ±0,2) : **quête de
  gibier (SeekGame) 4,6** ; boire 1,8 ; cueillir 0,8 ; rappel au clan 0,5 ;
  rentrer dormir 0,4 ; errer 0,3 ; rejoindre 0,3 ; chasser 0,2 ; pister 0,2 ;
  explorer 0,2.

## Lecture du code

- Nuit = 20 h-6 h fixe, toute l'année, à toute latitude (`SimTime::is_night`).
  Seul effet : l'envie de dormir ×1,15 la nuit, ×0,55 le jour (`brain`).
  Aucune visibilité : on cueille, chasse, piste, part en quête la nuit comme
  le jour.
- Température : cycle diurne (sinusoïde, pic 15 h, amplitude 2-10 °C selon
  l'humidité) ; **pas de soleil** dans le ressenti ; forêt +4 °C jour et nuit.
- Quête (D10) : l'urgence de chasse est le max de la faim propre et du
  manque de la réserve du clan (`1 − stock / (3 points × membres)`) ; la
  réserve pourrit en 3 j : le manque est quasi permanent → tout adulte, femme
  portant un nourrisson comprise, part en quête jour et nuit.

## Sources (nouvelles)

- Yetish et al. 2015 (Hadza, San, Tsimane) : sommeil 5,7-7,1 h ; endormissement
  ~3,3 h après le coucher du soleil, réveil avant le lever ; ~1 h de plus en
  hiver ; le sommeil tombe pendant la baisse nocturne de température.
  https://pmc.ncbi.nlm.nih.gov/articles/PMC4720388/
- Chasse nocturne hadza : affût près des points d'eau, nuits de lune, à un ou
  deux, depuis des caches — l'exception, pas la quête de nuit.
  https://www.nationalgeographic.com/magazine/article/hadza
- Steadman 1984 (température apparente) : le rayonnement solaire ajoute
  jusqu'à ~6 °C au ressenti au soleil par rapport à l'ombre.
  https://en.wikipedia.org/wiki/Apparent_temperature
- Durée du jour : cos(ω₀) = −tan(φ)·tan(δ), δ = 23,44° × sinusoïde annuelle.
  https://bristol.ac.uk/media-library/sites/engineering/engineering-mathematics/documents/modelling/student/daylength.pdf
- Marlowe 2007 : division sexuelle du fourrage, plus forte en milieu
  saisonnier ; les femmes visent les ressources sûres (contraintes de
  l'enfant). https://hraf.yale.edu/documents/369

## MAR-1b — la nuit suit le soleil (prédictions, avant le banc)

Changement : l'envie de dormir lit la lumière (0,55 + 0,6 × (1 − lumière))
au lieu de la nuit fixe 20-6 h. Rien d'autre (la lumière ne gêne encore rien).
1. Effet faible sur la marche (< 10 %) : la nuit ne gêne encore aucune
   activité ; seul le moment du sommeil bouge.
2. La part du sommeil prise dans le noir varie avec la durée du jour : en été
   aux latitudes des scènes, les nuits sont plus courtes que 10 h → moins de
   sommeil « de nuit » qu'avec 20-6 h.
3. Le sommeil reste éparpillé (≥ 25 % des adultes éveillés au cœur de la
   nuit) : la nuit n'empêche rien tant que MAR-2 n'est pas fait.

## MAR-1 — mesuré (90 j, 4 seeds × 2 scènes, départ jour 135 → 225 ; latitudes 43-62°, 14-16 h de jour)

| | témoin (MAR-1a) | MAR-1b |
|---|---|---|
| marche femmes ; hommes (h/j) | 9,3 ; 9,1 | 9,3 ; 9,1 |
| part du noir passé à dormir | 45-54 % | 45-51 % |
| part du sommeil prise dans le noir | 13-49 % | 13-49 % |
1. Effet < 10 % sur la marche : **tenue** (0 %).
2. Part du sommeil dans le noir qui bouge : **réfutée** (inchangée) — dans
   ce régime (été, 43-62°), le noir réel et la nuit 20-6 h se recouvrent.
   Froid 1337 (62°) : seulement 13 % du sommeil dans le noir — nuits d'été
   presque sans noir à cette latitude.
3. Sommeil éparpillé : **tenue** — la moitié du noir se passe éveillé.
Non couvert : l'hiver (départ d'été, 90 j), où les nuits longues
changeront le régime.

## MAR-2a — l'obscurité gêne la vue (prédictions, avant le banc)

Changement : `climate::sight` (2 % de la vue de jour la nuit sans lune) ; la
portée à laquelle on voit le gibier (2 km × vue), le rendement de la
cueillette et la chance de mise à mort × vue ; le cerveau le sait (cueillir,
chasser, pister, quête, défendre, explorer, errer × vue). Boire, rentrer,
rejoindre, suivre : inchangés.
1. Marche dans le noir divisée par ≥ 2 ; marche totale 9,2 → 6-8 h/j.
2. Part du noir passée à dormir 45-54 % → ≥ 65 % (le sommeil gagne par défaut
   du reste) — mais le softmax (τ = 0,12) laisse des tirages parmi des scores
   tous faibles : il manque un « rester au campement ».
3. Pas de hausse des morts (banc court ; à vérifier au banc d'acceptation).
Réfutation : marche nocturne inchangée (la nuit ne serait pas ce qui fait
marcher) ou sommeil inchangé.

## MAR-2a — mesuré (banc `marche`, 90 j d'été, 8 runs) — non commité (out/marche/mar2a.patch)

| | MAR-1b | MAR-2a |
|---|---|---|
| marche femmes ; hommes (h/j) | 9,3 ; 9,1 | 9,0 ; 8,9 |
| part du noir passée à dormir | 45-51 % | 55-63 % |
| marche **dans le noir** (h/j, femmes) | — | ~1,3 (dont boire 0,45) |
| quête de gibier (h/j) | 4,6 | 3,6 (presque toute de jour) |
1. Marche ÷ 2 dans le noir, totale 6-8 h/j : **réfutée pour le total**
   (9,0 h/j) — en été (14-16 h de jour) presque toute la marche se fait de
   jour : la nuit n'est **pas** la cause principale ; c'est la quête de jour.
2. Noir passé à dormir ≥ 65 % : **réfutée de peu** (55-63 %).
3. Tests : **7 tests cassent**. Diagnostic sur le test de surchasse (60
   affamés forcés, 4 troupeaux à 300 m, 30 j) : têtes prélevées 22 → 5 **même
   en été** ; de jour, la chasse tombe de 0,67 à 0,08 h/agent/j et la
   cueillette l'emporte. Lecture : la nuit, on perd le contact du gibier (on
   ne le voit plus, on ne le suit plus) ; au matin il est hors de vue, la
   piste (score 0,55 × fraîcheur) perd contre la cueillette. Avant, on
   chassait 24 h sur 24 et on gardait le contact. Les tests de chasse ont été
   calibrés sur une chasse permanente (« ~9 h à portée par jour »).
   Autres tests cassés : `les_curieux_explorent_plus_loin`,
   `le_territoire_stabilise_la_derive_apres_formation`,
   `perdre_un_troupeau_de_vue_n_efface_pas_son_souvenir` (1 tick de nuit),
   `le_bord_de_la_faune_renvoie_le_fuyard`, `qui_porte_deja_de_la_viande…`.

## Sources — comment les chasseurs hadza trouvent le gibier (2026-10-05)

« Hadza Men's Follows, 1985-1986 » (O'Connell, Hawkes, Blurton Jones ;
PaleoAnthropology) — 73 sorties d'hommes suivis, tableaux relus ici :
https://paleoanthropology.org/ojs/index.php/paleo/article/view/1084
- Types de sortie : **rencontre** (« daylight walk in search of large animal
  prey » : marche de jour, l'animal est vu directement — les traces ne
  comptent pas) ; **pistage** (« pursuit of an animal thought to have been
  wounded » : on suit la trace d'une bête blessée, souvent le lendemain) ;
  **affût de nuit** aux points d'eau (saison sèche) ; affût de jour ; miel ;
  visite ; sortie avec les femmes.
- Fin de saison sèche (33 sorties) : affût de nuit 11 (16 h dont 13,6 h
  d'affût), rencontre 9 (4,5 h, ~5 km au plus loin, 2,2 h de recherche),
  pistage 4 (5,5 h). Saison humide : surtout miel et pistage, 5-10 h,
  5-15 km au plus loin.
- Départ le matin (7 h-8 h), retour l'après-midi ou le soir.
- Succès : une grosse prise pour ~30 jours-chasseur (3 %, Hawkes et al.
  1991) à ~1 % (Wood et Marlowe 2013).
  https://content.csbs.utah.edu/~hawkes/HawkesOConnellBLurtonJones14moreLessonsHadzaHunters.pdf
- Pistage : Liebenberg (L'art du pistage) — pistage systématique (suivre les
  signes) et spéculatif (anticiper l'animal d'après le terrain et son
  comportement). https://www.originalwisdom.com/wp-content/uploads/bsk-pdf-manager/2019/03/Liebenberg_1990_theartoftrackingtheoriginofscience.pdf
- Ongulés : déplacements quotidiens de l'ordre de 1,5-2 km, forte fidélité au
  domaine vital, activité crépusculaire (cerfs).

**Lecture pour le modèle** : on ne garde pas le contact d'un troupeau la
nuit ; on repart **chaque matin** du campement, à la rencontre, sur ~5 km, et
on rentre. MAR-2a (perdre le contact la nuit) est donc réaliste ; ce qui
manque, c'est la sortie de chasse du matin pour qui n'a pas de clan (la quête
n'existe que pour les membres d'un clan) et une sortie bornée (aller, chercher
2-5 h, rentrer) au lieu d'une quête pour tous, jour et nuit. → MAR-4.

## Sources — le nourrisson gêne-t-il la chasse ? (vérifié avant de l'écrire)

**Non, pas comme supposé.** Hypothèse écartée.
- BaYaka (Congo), GPS et cardiofréquencemètre sur 348 sorties : avec un
  nourrisson, les mères partent plus longtemps, mais **ni leur mobilité, ni
  leur dépense, ni leur rendement** n'en pâtissent ; les fillettes de 4-7 ans
  qui gardent l'enfant augmentent le rendement des mères.
  https://pmc.ncbi.nlm.nih.gov/articles/PMC9667358
- Agta (Philippines, Goodman et al. 1985) : la plupart des femmes chassent
  (en groupe, avec des chiens), nourrisson porté ou laissé au camp, sans
  surmortalité de leurs enfants.
  https://hgcosmos.org/assets/culture/AZ/A/Agta.pdf
- Les femmes chassent dans la grande majorité des sociétés de fourrageurs.
  https://www.sciencealert.com/women-hunt-in-most-foraging-societies-using-their-own-tools-and-strategies
Conséquence : MAR-4 n'ajoute **aucune** gêne de chasse liée au nourrisson.
La division du travail, si elle doit émerger, viendra d'autres contraintes
(coût de la charge, déjà là ; garde des enfants ; ce que rapporte chaque
activité).

## MAR-2a + MAR-4 — la sortie de chasse (prédictions, avant le banc)

Changement MAR-4 : la quête s'ouvre à tout adulte (camp = foyer du clan ou
gîte de la dernière nuit) ; la cible n'est offerte que si aller + retour
tiennent dans le jour qui reste (lumière ≥ 0,5) ; « dormir au camp » vaut
aussi pour le gîte. Aucune gêne liée au nourrisson (réfutée par les sources).
1. Quête dans le noir ≈ 0 ; quête de jour en légère hausse (les sans-clan
   s'y mettent).
2. Marche totale encore ≥ 7 h/j : le banc d'été ne la fera pas tomber à
   2-3 h — la quête reste quotidienne pour tous (réel : les hommes hadza
   sortent presque chaque jour ; mais 1,5 h de marche pour les femmes).
3. Les tests de chasse cassés par MAR-2a passent de nouveau (on repart au
   matin) ; risque : les tests de formation de clan (la quête solitaire
   dispersait les fondateurs avant D2).

## MAR-2a + MAR-4 — mesuré (banc `marche`, 90 j d'été, 8 runs ; non commité : out/marche/mar24.patch)

- Marche 8,9 h/j (35,5 km) — prédiction 2 tenue (≥ 7 h) ; quête 3,8 h/j de
  jour, ~0 dans le noir (prédiction 1 tenue).
- **Chasse dans les vraies scènes : intacte.** Témoin (MAR-1b) : 434 têtes,
  85 % des jours-adulte avec une sortie, **1,42 %** de prise par
  jour-chasseur ; MAR-2a+4 : 449 têtes, 93 %, **1,39 %** (Hadza : 1-3 %,
  critère de validation, pas de calibrage). Le modèle n'est pas bridé.
- Mais 85-93 % des adultes, femmes comprises, sortent chasser chaque jour.
- Tests : 3 corrigés en les mettant à midi (ils testaient la mémoire du
  gibier, le bord de la faune, l'envie de chasser — pas la nuit). 4 restent :
  `la_surchasse_effondre_le_gibier_local` (60 affamés **forcés** à faim 0,9
  pendant 30 j : 22 → 3 têtes en hiver, 0 en été — affamés, ils cueillent
  plutôt que de chercher un gibier hors de vue ; avant, ils gardaient le
  contact jour et nuit) ; `les_curieux_explorent_plus_loin` (56 contre 52
  cellules, seuil +20 %) ; `le_territoire_stabilise_la_derive…` (étalement
  2 000 → 2 830 tuiles) ; `un_enfant_suit_sa_mere` (208 tuiles après 3 j) —
  la mère, sans clan, part désormais en sortie de jour.

## MAR-6 — proposition (à valider avec l'utilisateur avant d'écrire)

Lecture : le score d'une tâche de nourriture est `urgence × attrait ×
décote de trajet` (½ à une heure de marche). Ni le **coût en énergie** du
trajet (`energy`, qui existe désormais), ni **ce que rapporte** l'activité
en kcal par heure n'y entrent. La quête : `urgence × 0,55` — elle vaut
autant qu'une cueillette sûre alors qu'elle rapporte ~1,4 % de prise par jour.
Sources : Hadza, chasse au gros gibier ≈ 67-91 kcal/h pour la famille
(Hawkes et al. 2010) ; une prise pour ~30 jours-chasseur.
https://content.csbs.utah.edu/~hawkes/Hawkes_al10notJustprovisioningG&Hcorrec.pdf

Forme proposée : chaque adulte **apprend ce que rapporte** chaque activité
(moyenne glissante des kcal obtenues par heure passée à la faire : cueillir,
sortir chasser, pister) et la compare à ce qu'elle coûte (kcal de marche et
d'effort de son corps). Le score devient urgence × rendement net attendu.
Rien de fixé : l'expérience de chacun (savoir-faire, pays, saison) fait le
choix ; une spécialisation pourrait en émerger. À trancher : appris
(proposé) ou moyennes connues de tous.

## MAR-2+4 au banc d'acceptation (a66cbc7, 3 ans, 8 runs) — **régression**

| | ENE-3 (lait) | MAR-2+4 |
|---|---|---|
| morts | 49 | **168** |
| dont maladie (dont affamés) | 31 (0-1) | 101 (24) |
| faim | 0 | 10 |
| violence | — | 21 |
| froid : population finale | 72-87 | **35-69** |
| mourir avant 15 ans | 22 % | 50 % |
Marche 29-35 km/j (31-40 avant).

**Diagnostic, banc `marche` en hiver** (départ jour 315, 90 j, 8-10 h de jour) :
| | MAR-1b | MAR-2+4 |
|---|---|---|
| têtes prises | 580 | **214** |
| prise par jour-chasseur | 1,85 % | **0,85 %** |
| marche (h/j) | 7,6 | 6,4 |
| noir passé à dormir | 39 % | 48 % |
En été la chasse tenait (1,42 → 1,39 %) ; en hiver elle s'effondre : on perd
le troupeau la nuit, et rien ne le fait retrouver au matin hors des 2 km de
vue — ni domaine vital connu, ni traces (CHA-1, CHA-2). La nuit d'hiver (14-16 h)
coûte plus que celle d'été. **C'est le modèle bridé que l'utilisateur
craignait** : MAR-2+4 a besoin de CHA-1/2 pour être juste.

## CHA-1a — fidélité au domaine vital (prédictions, avant le banc)

Mesure du témoin (a66cbc7, banc `marche`, 90 j) : **les troupeaux quittent
le pays** — des ~25 troupeaux à moins de 10 km du foyer au départ, il en
reste 0-12 après 90 j, été comme hiver ; déplacement net médian 9-50 km.
La chasse ne vit que de l'immigration de gibier. Sources : biche de cerf
2-4 km² (Rùm) ; bison d'Europe 8 (hiver) à 70 km² (été) ; les cerfs
reviennent à leur domaine en quelques heures après un dérangement, même un
incendie ; un cerf dérangé sort de son domaine hebdomadaire dans ~50 % des
approches. https://www.wildlifeonline.me.uk/animals/article/red-deer-territory-home-range
https://www.ncbi.nlm.nih.gov/pmc/articles/PMC8601917/
https://openarchive.usn.no/usn-xmlui/handle/11250/3047354
Changement : un centre de domaine par troupeau sauvage (posé au premier
tour) ; hors du rayon (cerf ~1 km, aurochs ~3 km, gazelle ~1,8, renne ~3),
il revient au pas ; l'hiver inchangé (CHA-1b).
1. Été : troupeaux à 10 km en fin de run ≥ la moitié du départ sur ≥ 6/8 runs ;
   déplacement net médian < 5 km.
2. Prises par jour-chasseur en hausse, été et hiver.
3. Hiver : peu d'effet sur la présence (la migration emporte encore tout).

## CHA-1a — mesuré (banc `marche`, 90 j, 8 runs)

| | été témoin → CHA-1a | hiver témoin → CHA-1a |
|---|---|---|
| têtes prises | 449 → 1 120 | 214 → 343 |
| prise par jour-chasseur | 1,39 → **3,64 %** | 0,85 → **1,34 %** |
| troupeaux à 10 km en fin de run | 0-12 → 3-12 | 0-12 → 0-11 |
| déplacement net médian | 9-50 → 4-14 km | 8-18 → 3-18 km |
| marche (h/j) | 8,9 → 7,9 | 6,4 → 6,3 |
1. ≥ la moitié des troupeaux encore là sur ≥ 6/8 runs, net < 5 km :
   **réfutée** (1/8 ; 2/8 sous 5 km) — les troupeaux restent repoussés par la
   présence humaine (fuite de 2 km à chaque approche à < 0,4-1 km, retour au
   pas de 80 m/h) ; les sorties quotidiennes balaient 4 km autour du camp.
2. Prises en hausse été et hiver : **tenue**. L'été dépasse la fourchette
   hadza (3,6 % contre 1-3 %) — critère de validation, pas de calibrage ;
   les Ju/'hoansi rapportent de la viande sur < 27 % des jours, petit
   gibier compris.
3. Hiver peu changé : **tenue** (la migration emporte les troupeaux).
Test `la_surchasse…` : un tirage (seed 42 : 53 → 12 têtes ; 1337 : 23 → 81) ;
déjà invalide en hiver sur 2 seeds sur 4 (le gibier migre sans chasseurs) →
jugé en été sur 4 seeds : effondrement local 4/4.

## CHA-1b — l'hiver, seuls les migrateurs partent (prédictions, avant le banc)

Sources : caribou jusqu'à ~1 200 km aller-retour par an, 19-55 km/j en
migration ; cerf migrateur partiel (45 % de migrants dans une population des
Carpates ; trajets de quelques dizaines de km) ; bison : l'hiver sur place,
domaine resserré, il gratte la neige et mange moins.
https://www.discoverwildlife.com/animal-facts/mammals/how-far-do-reindeer-migrate
https://hal.inrae.fr/hal-02621521
https://www.worldwildlife.org/stories/how-bison-survive-winter-in-the-northern-great-plains
Changement : seul le renne migre ; cerf, aurochs, gazelle restent dans leur
domaine l'hiver (le quartier d'hiver des cerfs migrants n'est pas modélisé).
1. Hiver : troupeaux à 10 km en fin de run en hausse sur ≥ 6/8 runs ; prises
   par jour-chasseur en hausse (1,34 % → ≥ 2 %).
2. **Risque (règle 5)** : sans repousse, les résidents s'affament l'hiver —
   herbivores en fin d'hiver en baisse par rapport au témoin ; une baisse de
   plus de 50 % sur ≥ 4 runs dirait que la satiété d'hiver est mal posée
   (les ongulés réels mangent moins et vivent sur leurs réserves l'hiver).

## CHA-1b — mesuré (banc `marche`, hiver, 90 j, 8 runs)

| | CHA-1a | CHA-1b |
|---|---|---|
| prise par jour-chasseur | 1,34 % | **2,02 %** |
| troupeaux à 10 km en fin de run | 0-11 | 4-16 |
| herbivores fin d'hiver (somme) | 5 745 | 6 284 |
| marche (h/j) | 6,3 | 5,9 |
1. Troupeaux proches en hausse ≥ 6/8 et prises ≥ 2 % : **tenue** (6/8 ;
   2,02 %). Les deux runs inchangés (tempéré 42, 1337) n'ont pas d'hiver
   au sens de la pâture.
2. Effondrement d'hiver des résidents : **non** — herbivores stables ou en
   hausse sur 6/8 runs, −1 % et −12 % sur les deux autres (froid 42, 1337),
   et froid 7 passe de 287 à 921 têtes (la migration le vidait).

## CHA-2 — les pistes (prédictions, avant le banc)

Changement : chaque troupeau sauvage laisse une position par heure, lisible
48 h (choix ; Liebenberg : l'âge d'une trace dépend du sol et de la météo,
aucun chiffre général) ; un adulte qui marche de jour (vue ≥ 0,5) et coupe
la piste à moins de 15 m la remarque et apprend son point le plus récent
(sa piste à suivre — `Track` existant). Non modélisé : la pluie qui efface,
la neige qui garde, le savoir-faire du pisteur.
1. Prises par jour-chasseur en hausse en hiver (2,02 % → ≥ 2,5 %) ; en été
   aussi (déjà 3,6 %).
2. Marche de quête (SeekGame) en baisse, pistage (Track) en hausse.
3. Marche totale : baisse faible (< 10 %) — le pistage reste de la marche.

## CHA-2 — mesuré (banc `marche`, 90 j, 8 runs ; hiver départ j315, été j135)

| | hiver CHA-1b | hiver CHA-2 | été CHA-1 | été CHA-2 |
|---|---|---|---|---|
| prise par jour-chasseur | 2,02 % | **2,15 %** | 3,64 % | **3,92 %** |
| têtes | 481 | 532 | 1 120 | 1 181 |
| Track de jour (h/j, femmes) | 0,16 | 0,33 | 0,49 | 0,91 |
| Track dans le noir | 0,16 | 0,26 | 0,09 | 0,13 |
| SeekGame (h/j, femmes) | 0,61 | 0,32 | 0,88 | 0,21 |
| marche (h/j, femmes) | 6,0 | 6,2 | 8,0 | 7,9 |
1. Prises en hausse : **direction tenue, ampleur réfutée en hiver** (+6 %, ≥ 2,5 % attendu) ;
   été +8 %. Agrégat seulement : le banc ne donne pas la prise par run (règle 7 non vérifiable).
2. SeekGame ↓, Track ↑ : **tenue** (été : la quête divisée par 4).
3. Marche en légère baisse : tenue l'été (−1 %), **réfutée l'hiver** (+3 %).
Voisin relevé (corrigé : le banc range « Track » et « Track (noir) » sur deux lignes) :
l'hiver, **44 % de la marche de pistage se fait dans le noir** (0,26 sur 0,59 h/j ; 50 %
avant CHA-2), 12 % l'été. Pas propre au pistage : de nuit on marche aussi pour boire
(1,14 h/j), errer (0,35), cueillir (0,23), chasser (0,11). Hypothèse à mesurer : le facteur
`sight` (0,02 la nuit) écrase des scores déjà petits, mais le softmax (τ = 0,12) juge des
écarts absolus — deux tâches à 0,01 et 0,0002 y sont presque équiprobables ; seul le
sommeil s'en détache.
Banc d'été avec 2 fils : tué par manque de mémoire (> 1,2 Go RAM + 1 Go swap) ; relancé avec 1 fil.

## CHA-2b — le pisteur s'arrête à la nuit (prédictions, avant le banc)

Décision de l'utilisateur (2026-10-06) : modèle (a), le pisteur s'arrête quand la lumière
tombe et reprend au matin ; voir dans le noir viendra des techniques. Changement : sous
une vue de 0,5 (`TRAIL_SIGHT`, même seuil que pour remarquer une piste), `Track` n'est
plus candidat et le pistage en cours s'interrompt ; `mem.game` reste. Porte tout ou rien :
lire une empreinte sans lumière est une impossibilité physique (lune non modélisée).
1. Track (noir) : hiver 0,26 → < 0,03 h/j ; été 0,13 → < 0,03.
2. Prises d'hiver : pas de baisse (≥ 2,1 %) — le pistage nocturne ne tuait rien (la mise
   à mort demande la vue) ; réfuté sous 2,0 %.
3. Hypothèse du softmax (voisin) : si les scores écrasés par la vue restent tirés au
   hasard, les heures de nuit libérées iront surtout à d'autres marches de nuit (Wander,
   Explore, Forage (noir) en hausse) plutôt qu'au sommeil ; la part du noir passée à
   dormir monterait alors de moins de 1 point.

## CHA-2b — mesuré (banc `marche`, 90 j, 8 runs, 1 fil)

| | hiver CHA-2 | hiver CHA-2b | été CHA-2 | été CHA-2b |
|---|---|---|---|---|
| Track (noir) h/j | 0,26 | **0,01** | 0,13 | **0,01** |
| Track de jour | 0,33 | 0,39 | 0,91 | 1,14 |
| têtes | 532 | **482** | 1 181 | **1 042** |
| jours-chasseur / jours-adulte | 70 % | 59 % | 84 % | 82 % |
| prise par jour-chasseur | 2,15 % | 2,29 % | 3,92 % | **3,55 %** |
| Wander + Forage + Explore (noir) | 0,69 | 0,85 | 0,28 | 0,28 |
| part du noir dormie (moy. des runs) | 46,4 % | 46,6 % | 54,1 % | 55,6 % |
1. Track (noir) → ~0 : **tenue**.
2. Prise par jour-chasseur d'hiver ≥ 2,1 % : tenue (2,29 %), mais trompeuse : **les têtes
   baissent** (hiver −9 %, été −12 %) ; l'été, la prise par jour-chasseur baisse aussi.
   Le pistage nocturne rapportait : il menait le chasseur près du troupeau au lever du jour.
3. Softmax : **tenue** — les heures de nuit libérées vont à d'autres marches de nuit
   (errer, cueillir, explorer : hiver 0,69 → 0,85 h/j), pas au sommeil (+0,2 point).
Tests : `la_chasse_nourrit_mieux_que_la_cueillette` casse avec CHA-2b (0 prise en 30 j,
affamés forcés, hiver) ; `la_surchasse_effondre_le_gibier_local` cassait déjà avec CHA-2
(seed 7 : 56 têtes à 3 km contre < 26 attendu ; 28 avec CHA-2b) — non vu au commit de CHA-2.

## CHA-2c — le test de surchasse rouge depuis CHA-2 (diagnostic, 2026-10-09)

Scène du test : été (j135), 4 troupeaux de 100, 30 j ; 60 chasseurs contre témoin à 0.
Têtes à 3 km en fin de run (chassé) / tuées / heures Hunt-Track-SeekGame (somme du run) :

| seed | CHA-1b | CHA-2b |
|---|---|---|
| 42 | 0 / 36 / 3 369-995-4 003 | 0 / 66 / 5 061-2 361-608 |
| 7 | 0 / 9 / 1 832-630-5 264 | **28** / 12 / 1 790-2 157-436 |
| 1337 | 0 / 58 / 3 809-620-4 054 | 0 / 57 / 5 225-2 983-756 |
| 2024 | 0 / 12 / 1 320-559-3 873 | 0 / 26 / 2 299-1 563-479 |
Témoin : 101-134 têtes à 3 km, identique. Tuées sur 4 seeds : 115 → 161.
Lecture : avant CHA-2, la zone se vidait **par la fuite** — la quête au hasard (SeekGame,
~4 000 h par run) balayait le pays et chassait les troupeaux au-delà de 6 km (seed 7 :
9 tuées, 0 à 6 km). Les pistes ont remplacé cette quête par le pistage (÷ 9) : on dérange
moins, on tue plus (3 seeds sur 4), mais là où l'on tue peu (seed 7 : 12 têtes), les
troupeaux restent ou reviennent — le domaine vital (CHA-1a) les ramène toujours.
Question posée à l'utilisateur : un troupeau sans cesse dérangé abandonne-t-il son domaine ?

## CHA-2c — le paysage de la peur (prédictions, avant le banc)

Décision de l'utilisateur (2026-10-09) : (a), un troupeau sans cesse dérangé abandonne son
domaine. Mécanisme : à chaque fuite (départ au galop), le centre du domaine glisse d'une
fraction α vers le point où la fuite a mené. Une fuite isolée le déplace à peine (le cerf
revient en quelques heures) ; des fuites répétées du même côté l'emportent loin de la
menace. Facteur continu, pas de seuil ; les fuites devant les meutes comptent aussi.
α = 0,05 (choix, ordre de grandeur : ~14 fuites du même côté déplacent le centre d'un rayon
de domaine de cerf ; sources sur l'abandon de domaine sous pression de chasse à préciser —
études du wapiti en saison de chasse).
1. Test `la_surchasse_effondre…` : seed 7, têtes à 3 km 28 → < 26 ; 4/4 vertes ; témoin
   inchangé (pas d'humain ; meutes absentes de la scène).
2. Banc `marche` : troupeaux à 10 km en fin de run en baisse sur ≥ 6/8 runs ; prises par
   jour-chasseur en légère baisse (≤ 10 %) — le gibier s'écarte des camps.
3. Risque : herbivores en fin de run en baisse > 20 % sur ≥ 4 runs dirait que les troupeaux
   chassés vers des terres pauvres s'affament (à regarder, pas à corriger d'office).

## CHA-2c — mesuré (banc `marche`, 90 j, 8 runs, 1 fil ; α = 0,05)

| | hiver CHA-2b | hiver CHA-2c | été CHA-2b | été CHA-2c |
|---|---|---|---|---|
| têtes | 482 | **325** (−33 %) | 1 042 | **870** (−17 %) |
| prise par jour-chasseur | 2,29 % | **1,52 %** | 3,55 % | **2,76 %** |
| troupeaux à 10 km en fin de run, en baisse | — | 3/8 | — | 6/8 |
| herbivores fin de run en baisse > 20 % | — | 1/8 | — | 2/8 |
1. Test de surchasse : **tenu** (4/4 vertes).
2. Troupeaux proches en baisse ≥ 6/8 : tenu l'été, **réfuté l'hiver** (3/8) ; prises en
   baisse ≤ 10 % : **réfuté** — −17 % l'été, −33 % l'hiver. Le gibier fuit les camps bien
   plus que prévu.
3. Effondrement des herbivores : non (3 runs sur 16, sans direction commune).
Lecture : les camps ne bougent pas quand le gibier s'en écarte ; un chasseur réel
déplacerait le camp (rendement décroissant — MAR-6). α est un choix non sourcé qui règle
l'ampleur. Test de reproduction : une menace fixe ne fait qu'une fuite par jour (retour au
pas, 25 h pour 2 km) — 162 tuiles de déplacement ; refait en harcèlement qui suit le troupeau.

## MAR-6 — décisions de l'utilisateur (2026-10-10) et plan

Décisions :
1. **Explorer** : rester sur le connu quand le gibier s'en va, c'est ne jamais le rattraper.
   Le camp suit le rendement, et la recherche sort du territoire connu.
2. **Appris, transmis** : chacun part de ce qu'il voit faire autour de lui (la mère, les
   proches), puis affine par sa propre expérience. Aucune moyenne connue de tous.
3. **On compare, on ne scripte pas** : les sources servent à juger la plausibilité des
   rendements appris, jamais à régler une constante.

Sources (plausibilité seulement) :
- Ache : 1 339 kcal/h (hommes), 1 221 kcal/h (femmes) avant préparation (Hill et al. 1987,
  https://content.csbs.utah.edu/~hawkes/Hill_al87ForagingAcheOFT.pdf).
- Hadza : la moitié des sorties de cueillette couvrent le besoin du jour en moins de 2 h
  (≥ ~1 000 kcal/h) ; ordre de rendement des hommes : miel > viande > baobab > baies >
  tubercules, qui est aussi leur ordre de préférence (Berbesque et Marlowe 2009,
  https://journals.sagepub.com/doi/full/10.1177/147470490900700409) ; gros gibier ≈ 67-91
  kcal/h pour la famille (Hawkes et al. 2010). Hadza : 3,4 % des sorties réussies.
- Batek (théorème de la valeur marginale) : 8,2 j par camp (3-24), déplacements 0,6-14,5 km,
  le départ anticipe le rendement décroissant ; la MVT explique 56 % de la variance des
  durées de séjour (Venkataraman et al. 2017, https://pmc.ncbi.nlm.nih.gov/articles/PMC5373393).
- Chasseurs-cueilleurs : ~160-175 km de déplacements résidentiels par an (Hamilton et al.
  2016, https://pmc.ncbi.nlm.nih.gov/articles/PMC5094510/).

Lecture du code (2026-10-10) :
- Cueillir : `urgence(faim) × abondance × vue × décote` — l'heure sur place rapporte
  0,3 point × (0,6-1,4) ≈ 900-2 100 kcal/h, plafonnée par l'appétit : **la cueillette ne se
  rapporte pas au camp** (rien ne se porte ni ne se partage), contre l'essentiel des calories
  hadza apportées par les femmes. Voisin noté, hors MAR-6 (question à l'utilisateur).
- Chasser : la quête et la piste valent `urgence(besoin de viande) × 0,55`, l'approche
  `× (0,55 + 0,45 × taille)`. Le besoin de viande prend le **manque du stock du clan** : le
  stock étant presque toujours bas, 88 % des jours-adulte sont des jours de chasse (été,
  CHA-2c). Le 0,55 n'est pas sourcé.
- Le camp : centroïde des résidences (14 nuits), ancré à la hutte une fois bâtie ; rien ne
  le déplace sinon la dérive des nuits des membres.

Plan (un changement, une chose) :
- **MAR-6a0** — registre des rendements (observation) : par adulte, heures et kcal acquises
  par activité (cueillir ; chasser = quête + piste + approche, trajets compris). Aucun effet
  sur les choix. Le banc `marche` en tire les kcal/h du modèle et les déplacements des
  foyers. Mesure du témoin.
- **MAR-6a** — le choix lit le rendement appris (prior de ce qu'on voit faire, affiné par
  l'expérience) au lieu de 0,55 et de l'abondance seule.
- **MAR-6b** — le camp suit le rendement ; la recherche sort du connu.

## MAR-6a0 — témoin mesuré (banc `marche`, 90 j, 8 runs ; 2212b8a)

Colonnes anciennes identiques à CHA-2c, hiver comme été : le registre n'influe sur rien.
| | hiver F | hiver H | été F | été H |
|---|---|---|---|---|
| cueillir h/j (trajets compris) | 3,66 | 3,96 | 3,29 | 3,62 |
| cueillir kcal/h | 645 | 710 | 660 | 740 |
| chasser h/j | 2,65 | 2,47 | 5,37 | 5,11 |
| chasser kcal/h | 406 | 319 | 407 | 366 |
| acquis kcal/j | 3 437 | 3 604 | 4 362 | 4 551 |
Foyers de clan : **0,1-16 km/an** de chemin, déplacement net médian 0-1 km en 90 j, sur les
16 runs. Réel : ~160-175 km/an (Hamilton 2016), un camp tous les 8 j (Batek). Le camp est
immobile : c'est le défaut que MAR-6b doit lire. Le coût de l'effort n'est pas dans ce
témoin (colonne ajoutée après) ; ordre de grandeur 70-100 kcal/h.

## MAR-6a — le choix lit le rendement appris (prédictions, avant le banc)

Changement : pour **sa propre faim**, la chasse est pesée par son rendement net appris
rapporté au meilleur (cueillir ou chasser) ; la cueillette aussi. Pour **les siens** (manque
du stock du clan), rien ne remplace la chasse : la cueillette ne se rapporte pas au camp.
Rendements nets attendus d'après le témoin : cueillir ~590, chasser ~290 → poids de la
chasse ~0,5 pour la faim propre. Demi-vie de l'expérience 30 j, jugement après 48 h.
1. Heures de chasse (h/j) en baisse de 10 à 40 % en été (5,2 → 3,1-4,7) et en hiver
   (2,55 → 1,5-2,3) ; heures de cueillette en hausse.
2. Marche en baisse de 0,2 à 1 h/j (Track et SeekGame baissent).
3. Têtes prises en baisse, au plus en proportion des heures de chasse.
4. Kcal acquises par jour à ±10 % : la cueillette compense pour soi.
5. Foyers de clan inchangés (aucun mécanisme ne les touche).
Réfuterait : heures de chasse à moins de 5 % du témoin (le manque du clan porte toute la
chasse : le poids n'agit sur rien) ; ou acquis en baisse de plus de 10 % (la cueillette ne
compense pas).

## MAR-6a — mesuré (banc `marche`, 90 j, 8 runs)

| | hiver témoin | hiver 6a | été témoin | été 6a |
|---|---|---|---|---|
| chasser h/j F ; H | 2,65 ; 2,47 | 2,68 ; 2,42 | 5,37 ; 5,11 | 5,26 ; 4,88 |
| cueillir h/j F ; H | 3,66 ; 3,96 | 3,53 ; 3,92 | 3,29 ; 3,62 | 3,21 ; 3,57 |
| têtes | 325 | 300 (−8 %) | 870 | 803 (−8 %) |
| jours-chasseur / jours-adulte | 60 % | 60 % | 88 % | 86 % |
| marche h/j F | 6,2 | 6,3 | 8,7 | 8,6 |
| acquis kcal/j F ; H | 3 437 ; 3 604 | 3 385 ; 3 565 | 4 362 ; 4 551 | 4 559 ; 4 511 |
Coût de l'effort au-delà du repos : cueillir 60-77 kcal/h, chasser 71-90 kcal/h ; rendement
net appris ~590 contre ~300 : poids de la chasse ~0,5 pour la faim propre.
1. **Réfuté** : heures de chasse à −0 à −4 % (prédit −10 à −40 %). C'est la clause de
   réfutation écrite : le **manque du stock du clan** porte presque toute la chasse ; pour
   les siens, rien ne remplace la chasse, puisque la cueillette ne se rapporte pas au camp.
2. Marche inchangée (réfuté). 3. Têtes −8 % : tenu. 4. Acquis à ±5 % : tenu. 5. Foyers
   inchangés : tenu.
Lecture : le mécanisme est en place (il servira à MAR-6b et à toute activité nouvelle) mais
il n'a presque rien à arbitrer. Le levier est en amont : **la cueillette ne nourrit que
celui qui cueille**. Chez les Hadza, les femmes apportent au camp l'essentiel des calories
(tubercules, baies, baobab). Tant que seule la viande se rapporte, tout adulte des deux
sexes chasse 5 h/j l'été pour le clan (88 % des jours). Question à l'utilisateur.

## MAR-6b, étape 1 — la hutte du chef n'ancre plus le foyer (expérience, branche `exp/sans-ancre`)

Lecture (2026-10-10) : 15 runs sur 16 bâtissent une hutte du chef en moins de 90 jours, et
`structures::anchor_homes` **fige** alors le foyer sur elle. Origine : correctif de résultat
du 2026-07-24 (les clans « random-walkaient » et s'écartaient, 0 → 37,8 km en 5 ans), avec un
seuil de prestige calibré sur le banc. Écart avec le BRIEF : « le nomadisme émerge tout
seul », la sédentarité vient après le stockage (poterie). Une hutte de chasseurs-cueilleurs
se quitte (Hadza : quelques heures de travail). Expérience : le foyer reste le centroïde
vivant des résidences ; la hutte garde sa chaleur. **Décision de l'utilisateur requise
avant tout commit sur `main`.**
Prédictions, banc `marche` (90 j, 8 runs) :
1. Chemin des foyers : 0,1-16 → 3 à 30 km/an, toujours ≪ 160 km/an ; déplacement net ≤ 2 km.
   Le centroïde reste tenu par les sorties rayonnantes (aller-retour au foyer) : sans moteur
   de départ, le foyer ne part pas.
2. Chasse et marche à ±10 %.
Prédictions, banc `longue` (10 ans, seeds 42, 7, 1337, 2024 × tempéré, froid, départ j135),
contre `main` :
3. Témoin : foyer à < 3 km du départ toute la run. Variante : distance médiane au départ en
   an 10 de 5 à 50 km sur ≥ 6/8 runs.
4. Population, morts, clans : pas de direction commune (règle 7).
Réfuterait 1 : chemin inchangé (le foyer ne bougeait pas à cause du centroïde, pas de l'ancre).

## MAR-6b, étape 1 — mesuré au banc `marche` (sans ancre, 90 j, 8 runs)

| | hiver 6a | hiver sans ancre | été 6a | été sans ancre |
|---|---|---|---|---|
| chemin des foyers (km/an) | 0,8-10,9 | 10,5-21,5 | 0,1-15,9 | 16,8-34,8 |
| déplacement net (km) | 0-0,6 | 0,2-2,5 | 0-1,0 | 0-4,1 |
| têtes | 300 | 330 | 803 | 874 |
| marche h/j F | 6,3 | 5,9 | 8,6 | 8,4 |
1. Tenu : chemin 10-35 km/an (deux runs juste au-dessus de 30), déplacement net ≤ 4 km. Le
   chemin compte aussi le tremblement quotidien du centroïde : c'est le déplacement net qui
   dit si le camp part. Il ne part pas : sans raison de partir, les sorties rayonnent du
   foyer et le ramènent au milieu.
2. Tenu : chasse +9 % l'été, +10 % l'hiver ; marche −0,2 à −0,4 h/j.

## MAR-6b, étape 2 — explorer quand le pays s'épuise (expérience, branche `exp/explorer`, sur `exp/sans-ancre`)

Changement : chacun garde aussi son expérience des **derniers jours** (demi-vie 4 j, ordre des
séjours batek), la sienne et celle des proches. Rapport q = rendement net récent / rendement
du mois, toutes activités confondues. Quand q < 1, la quête porte à portée / q (bornée par le
jour de marche) et le retour au camp compte pour q dans le temps de jour exigé : on dormira
là où l'on aura trouvé. Facteur, pas de seuil.
Prédictions, banc `marche` (90 j, 8 runs) contre sans ancre :
1. Marche SeekGame en hausse (0,6 → 0,8-1,5 h/j l'été) ; nuits loin du foyer en hausse.
2. Têtes : de −10 à +30 % (pays neuf, gibier pas encore effrayé ; mais trajets plus longs).
3. Foyers : déplacement net **toujours ≤ 5 km** sur ≥ 6/8 runs — les directions de quête
   sont tirées au hasard, l'étalement est symétrique et le centroïde ne part pas. Si cela
   se vérifie, il manque au camp une **direction commune** (où le gibier a été vu).
Réfuterait 3 : déplacement net > 5 km sur ≥ 4/8 runs (le camp suit déjà).

## MAR-6b, étape 3 — le gibier vu se raconte (expérience, branche `exp/raconter`, sur `exp/explorer`)

Changement : aux échanges (toutes les 4 h, à 120 m), chacun retient le troupeau aperçu le
plus récemment par ceux qu'il côtoie, s'il est plus frais que le sien. La piste (Track) le
vise ensuite comme un souvenir propre. Les chasseurs hadza se racontent le gibier vu.
Test de surchasse (30 j, 4 seeds, 60 chasseurs) : **rouge sur seed 1337** (46 têtes à 3 km
contre 131 sans chasseur, seuil 20 %) ; vert sur les trois autres. Les chasseurs vont vers le
gibier qu'on leur a signalé, plus loin : la zone proche se vide moins. Seuil non touché.
Prédictions, banc `marche` (90 j) contre explorer :
1. Marche Track en hausse (1,6 → 2-3 h/j l'été) ; têtes en hausse de 0 à +40 %.
2. Foyers : déplacement net > 5 km sur ≥ 4/8 runs (direction commune : le camp suit le
   gibier signalé). Réfuté si ≤ 5 km sur ≥ 6/8.

## MAR-6b, étape 1 — runs longues (10 ans, 4 seeds × 2 scènes, départ j135), `main` contre sans ancre

| run | pop an 10 main / sans | morts soif | morts froid | foyer → départ (km, an 10) |
|---|---|---|---|---|
| froid 42 | 45 / **0 (an 7,6)** | 12 / 31 | 9 / 33 | 0,2 / — |
| froid 7 | 41 / **17** | 14 / 32 | 4 / 9 | 0,1 / 20,3 |
| froid 1337 | 68 / **24** | 9 / 22 | 1 / 7 | 0,6 / 2,8 |
| froid 2024 | 56 / **6** | 9 / 35 | 0 / 2 | 0,6 / 28,5 |
| tempéré 42 | 55 / 65 | 3 / 2 | — | 0,3 / 38,7 |
| tempéré 7 | 37 / 33 | 12 / 15 | — | 1,4 / 28,0 |
| tempéré 1337 | 56 / 61 | 4 / 5 | — | 0,2 / 51,8 |
| tempéré 2024 | 39 / 35 | 15 / 16 | — | 0,7 / 5,8 |
3. Tenu : témoin à < 1,5 km du départ (la hutte est bâtie dès la première année, le foyer y
   est cloué) ; sans ancre, 3 à 52 km en an 10 sur 7/7 runs vivantes.
4. **Réfuté** : direction commune en scène froide — **effondrement sur 4/4** (0, 6, 17, 24
   contre 41-68), morts de soif ×2-4, de froid ×1-7, de faim 5-17 contre 0-5. Tempéré : pas
   de direction commune (règle 7). Seed 42 froid : 64 → 8 dans la 2ᵉ année (33 morts de froid).
Lecture : sans ancre, le foyer est le centroïde des nuits, et `ReturnToClan` ramène vers le
centroïde d'hier : une **marche au hasard** qui s'auto-entretient, sans rien qui sache où
sont l'eau et l'abri. La bande s'éloigne de sa hutte (sa chaleur) et de ses sources connues,
et meurt l'hiver. L'ancre empêchait une errance réelle, mais par une porte tout ou rien
(règle 10) : **un camp ne devrait bouger que par une décision qui sait où elle va** (eau,
abri, nourriture), et une hutte de chasseurs-cueilleurs se refait à chaque camp, elle ne se
traîne pas. Dans le modèle, une hutte coûte du stock et chaque clan n'en a qu'une de chaque
sorte : se déplacer, c'est perdre la chaleur. **Question à l'utilisateur.**
Méthode : les étapes 2 et 3 avaient été empilées sur la variante sans ancre — leurs effets
se seraient mêlés à l'effondrement. Refaites chacune **seule sur `main`** (branches
`exp/explorer-seul`, `exp/raconter-seul`) ; les prédictions 1-2 des étapes 2 et 3 valent
désormais contre `main`, sauf « foyers », que l'ancre fige : on y lit plutôt les nuits loin
du foyer et la chasse.
Test de surchasse : **vert** avec le récit seul sur main (le rouge venait de la combinaison avec explorer).

## MAR-6b, étapes 2 et 3 seules sur `main` — banc `marche` (90 j, 8 runs), contre MAR-6a

| | hiver 6a | explorer | raconter | été 6a | explorer | raconter |
|---|---|---|---|---|---|---|
| têtes | 300 | 289 (−4 %) | 316 (+5 %) | 803 | 898 (+12 %) | 858 (+7 %) |
| marche h/j F | 6,3 | 6,4 | 6,3 | 8,6 | 8,5 | 8,6 |
| Track h/j | 0,48 | 0,49 | **0,84** | 1,60 | 1,52 | **1,92** |
| SeekGame h/j | 0,55 | 0,59 | — | 0,60 | **0,36** | — |
| ReturnToClan h/j (jour) | — | — | 0,23 | 1,14 | 1,03 | 1,26 |
Explorer : prédiction 1 (quête en hausse) **réfutée l'été** (0,60 → 0,36) ; têtes +12 % l'été :
dans la fourchette. Le rapport local est bosselé : une prise (60 000 kcal) le porte bien
au-dessus de 1 pendant des jours, puis il retombe ; l'effet moyen reste faible.
Raconter : piste +0,3 à +0,4 h/j, têtes +5 à +7 % : dans la fourchette basse.
Aucune des deux ne touche la marche totale : elle tient à la structure (tout adulte chasse pour
le clan l'été, piste 1,6, retour au clan 1,1, eau 1,8, cueillette 1,1 h/j).

## MAR-6b, étapes 2 et 3 seules — runs longues (10 ans, 4 seeds × 2 scènes), contre `main`

| run | pop an 10 main / explorer / raconter | naissances main / raconter | morts de soif main / raconter |
|---|---|---|---|
| froid 42 | 45 / 52 / 42 | 63 / 56 | 12 / 9 |
| froid 7 | 41 / 37 / 29 | 64 / 42 | 14 / 22 |
| froid 1337 | 68 / 61 / 45 | 77 / 54 | 9 / 10 |
| froid 2024 | 56 / 48 / 38 | 65 / 56 | 9 / 13 |
| tempéré 42 | 55 / 56 / 53 | 47 / 53 | 3 / 5 |
| tempéré 7 | 37 / 34 / 32 | 38 / 29 | 12 / 14 |
| tempéré 1337 | 56 / 59 / 39 | 52 / 45 | 4 / 11 |
| tempéré 2024 | 39 / 31 / 33 | 51 / 45 | 15 / 18 |
Explorer : population sans direction commune (5 runs en baisse, 3 en hausse) ; foyers figés
(ancre). Raconter : **population en baisse sur 8/8** (−4 à −34 %), naissances en baisse sur
7/8, soif en hausse sur 6/8 ; têtes par an sans direction commune. Hypothèse, non testée : on
suit loin un troupeau signalé (la piste se juge à l'échelle d'une journée, sans peser ce
qu'elle coûte ni ce qu'elle a rapporté). Plus de marche loin de l'eau, et des corps moins en
état de concevoir. À vérifier : les réserves des femmes et l'éloignement des couples.
Conclusion de l'étape 3 : **ne pas reporter sur `main`** en l'état.

## MAR-6 — ce que j'ai pu manquer (règle 9.6, 2026-10-10)

- **Les constantes de l'apprentissage ne sont pas balayées** : demi-vies 30 j et 4 j, 48 h
  pour juger, pas d'observation 1/6. MAR-6a « sans effet » vaut pour ces valeurs ; l'effet
  nul s'explique par la structure (le manque du clan), mais une demi-vie courte rendrait la
  chasse plus volatile.
- **MAR-6a n'a pas de run longue contre CHA-2c sur 4 seeds** : les runs `main` de
  `out/longue/mar6` sont MAR-6a. Seule la seed 42 froide se compare aux runs CHA (45 contre
  47 en an 10, départ d'été) : rien de visible.
- **L'effondrement sans ancre a deux causes mêlées**, froid et soif, non séparées : la
  chaleur de la hutte laissée derrière, ou les sources connues quittées. Une run avec huttes
  sans ancre, mais chaleur partout, les départagerait.
- **Le rapport local d'explorer est bosselé par les prises** (60 000 kcal d'un coup) : une
  autre mesure du pays (par activité, ou la cueillette seule, qui ne bosse pas) pourrait agir
  tout autrement. Non essayé.
- **La baisse de population de « raconter » n'est qu'une hypothèse** (suivre loin un troupeau
  signalé) ; ni les réserves des femmes, ni l'éloignement des couples n'ont été mesurés.
- Le registre compte la cueillette mangée par le cueilleur, plafonnée par son appétit : son
  rendement horaire est celui d'un repas, pas celui d'une sortie de collecte. Comparé aux
  Ache, il sous-estime ce que la cueillette **pourrait** rapporter.

## Expérience « la cueillette se rapporte » (branche `exp/cueillette`, sur `main`, 2026-10-10)

Changement unique : un membre de clan qui cueille garde ce qui dépasse son appétit, jusqu'à
15 kg (ordre de grandeur d'un panier, à sourcer), et le rapporte au stock du clan comme la
viande ; il ne quitte la tuile, rassasié, qu'une fois chargé. On ne cueille toujours que
poussé par sa propre faim (le choix ne change pas). Sans clan, rien ne change.
Prédictions, banc `marche` (90 j, 8 runs), contre MAR-6a :
1. Le stock du clan se remplit de plantes, son manque baisse : heures de chasse en baisse
   de 15 à 50 % l'été (5,1-5,3 → 2,6-4,4 h/j), jours-chasseur 86 % → 50-75 %.
2. Marche en baisse de 0,5 à 2 h/j l'été (piste et quête), retours au foyer en hausse.
3. Heures de cueillette en hausse ; kcal/h de cueillette de 0 à −20 % (la maille s'épuise).
4. Têtes en baisse, au plus en proportion des heures de chasse.
Runs longues (10 ans, 4 seeds × 2 scènes) :
5. Population sans baisse commune ; enfants mieux nourris par le stock (morts d'enfants en
   baisse sur ≥ 5/8).
Réfuterait 1 : chasse à moins de 10 % sous MAR-6a (les plantes pourrissent en 3 j ou sont
mangées avant d'arriver au stock).

### Mesuré — banc `marche` (90 j, 8 runs), contre MAR-6a

| | hiver 6a | hiver cueillette | été 6a | été cueillette |
|---|---|---|---|---|
| chasser h/j F ; H | 2,68 ; 2,42 | 2,17 ; 1,99 | 5,26 ; 4,88 | **2,81 ; 2,59** |
| jours-chasseur | 60 % | 51 % | 86 % | **57 %** |
| cueillir h/j F ; H | 3,53 ; 3,92 | 3,93 ; 4,17 | 3,21 ; 3,57 | 4,60 ; 4,81 |
| cueillir kcal/h F | 652 | 742 | 665 | **913** |
| acquis kcal/j F | 3 385 | 3 604 | 4 559 | **5 751** |
| têtes | 300 | 239 (−20 %) | 803 | 469 (−42 %) |
| marche h/j F | 6,3 | 6,1 | 8,6 | **7,3** |
| piste ; retour au clan ; rapporter (h/j, été) | — | — | 1,60 ; 1,14 ; — | 0,71 ; 0,42 ; 0,66 |
1. Tenu : chasse −47 % l'été, −19 % l'hiver ; jours-chasseur 86 → 57 %.
2. Tenu : marche −1,3 h/j l'été (piste −0,9, retour au clan −0,7, rapporter +0,66), −0,2
   l'hiver.
3. Heures de cueillette en hausse : tenu. **Kcal/h en hausse (+12 à +37 %), sens réfuté** :
   le registre ne plafonne plus la cueillette à l'appétit (voir « ce que j'ai pu manquer ») ;
   la maille ne s'épuise pas en 90 j.
4. Têtes −20 à −42 % : tenu.
La marche reste à 7,3 h/j (29 km/j contre 5,8-11,4) : l'eau (1,9 h/j) devient le premier
poste — MAR-5.

### Mesuré — runs longues (10 ans, 4 seeds × 2 scènes), contre `main` (MAR-6a)

| run | pop an 10 | naissances | morts d'enfants | soif | têtes/an | marche h/j |
|---|---|---|---|---|---|---|
| froid 42 | 45 → 49 | 63 → 59 | 46 → 40 | 12 → 15 | 65 → 24 | 7,4 → 6,6 |
| froid 7 | 41 → 22 | 64 → 38 | 43 → 36 | 14 → **31** | 37 → 23 | 7,1 → 6,4 |
| froid 1337 | 68 → 52 | 77 → 70 | 41 → 44 | 9 → 6 | 346 → 185 | 7,0 → 6,7 |
| froid 2024 | 56 → 75 | 65 → 75 | 39 → 34 | 9 → 9 | 101 → 33 | 7,6 → 6,8 |
| tempéré 42 | 55 → 59 | 47 → 51 | 20 → 22 | 3 → 3 | 367 → 115 | 8,1 → 6,5 |
| tempéré 7 | 37 → 37 | 38 → 37 | 26 → 23 | 12 → 10 | 18 → 28 | 7,7 → 7,1 |
| tempéré 1337 | 56 → 53 | 52 → 54 | 24 → 25 | 4 → 3 | 258 → 69 | 8,2 → 6,6 |
| tempéré 2024 | 39 → 37 | 51 → 54 | 30 → 35 | 15 → 15 | 19 → 17 | 7,8 → 6,8 |
5. Population sans direction commune (4 en hausse ou égale, 4 en baisse) : tenu. Morts
   d'enfants en baisse sur 4/8 seulement : **réfuté** (prédit ≥ 5/8).
Marche en baisse sur **8/8** (−0,3 à −1,6 h/j sur 10 ans) ; têtes par an en baisse sur 7/8.
Seed 7 froide : population 41 → 22, morts de soif ×2 (14 → 31) — cas isolé, non expliqué.
Conclusion : le levier attendu agit (la chasse n'est plus le seul moyen de nourrir les siens)
et ne coûte rien de visible en population. Reste à l'utilisateur de dire s'il le veut :
porter des plantes au camp est réel (Hadza), mais la charge (15 kg) et le pourrissement (3 j
comme la viande) ne sont pas sourcés.

### Acceptation 5 ans (8 runs, départ j135), `main` contre cueillette — **le report est bloqué**

| | main | cueillette |
|---|---|---|
| marche km/j F (8 runs) | 28,0-32,5 | **25,8-28,3** (8/8 en baisse) |
| kcal/j mangées F | 2 578-2 678 | 2 441-2 630 (8/8 en baisse) |
| mortalité 1-4 ans (‰/an, 8 runs) | 150 | **200** (7/8 en hausse ; froid 7 : 726 contre 142) |
| morts de soif 1-4 ans ; 15-39 ans | 14 ; 6 | **27 ; 11** |
| morts de froid 1-4 ans | 3 | 6 |
| mourir avant 15 ans | 59 % | 67 % |
Règle 5 : le banc `marche` disait « acquis +26 % », l'acceptation dit « mangé −1 à −6 % » —
une part de ce qui est rapporté pourrit au stock (3 j) avant d'être mangée.
Deux hypothèses pour la soif, chacune seule (branches sur `exp/cueillette`) :
- **H1** `exp/cueillette-repu` : on ne quitte plus la tuile avant d'être chargé (15 kg) ; la
  cueilleuse et l'enfant qui la suit restent loin de l'eau jusqu'à la délibération suivante.
  Variante : on rapporte ce qui dépasse l'appétit, mais on s'arrête rassasié comme avant.
  Prédiction : soif des 1-4 ans revenue à ≤ 18 (main 14), mortalité 1-4 ans ≤ 170 ‰ ;
  marche toujours en baisse (≥ 1 km/j sous main sur 6/8).
- **H2** `exp/cueillette-adultes` : les enfants cueillent pour le clan. Variante : seuls les
  adultes rapportent. Prédiction si H1 est la bonne : soif des 1-4 ans toujours ≥ 22.
Réfuterait les deux : soif des 1-4 ans ≥ 22 dans les deux variantes.

## MAR-6b (a) — lever le camp (branche `exp/camp`, sur `main` ; décision de l'utilisateur, 2026-10-16 : « a », puis « b » pour le réalisme)

Changement : le foyer est un **camp** persistant (il ne suit plus le centroïde, la hutte du chef
ne l'ancre plus). Chaque nuit, le clan relève les heures et kcal nettes de subsistance de ses
adultes dans le registre du camp (demi-vie 4 j, remis à zéro au départ) et les compare à
l'habitude de ses membres (expérience du mois, la leur et celle des proches). Sous l'habitude
(valeur marginale), et dès qu'il a une journée de travail par adulte pour en juger, il lève
le camp vers une source connue d'un membre, à plus de 4,5 km (le pays battu), qui maximise
plantes par bouche × (1 + troupeaux vus ces 3 j à 3 km) × trajet sur une journée. Sans
source hors du pays, il reste. Choix non sourcés : demi-vie, rayon, poids du gibier.
Les huttes restent où elles sont et au clan (b viendra après) : en scène froide, la bande
s'éloigne de sa chaleur.
Prédictions, banc `marche` (90 j, 8 runs), contre `main` :
1. Déplacement net du foyer > 3 km sur ≥ 6/8 runs ; chemin 20-200 km/an (sur `main` :
   0,1-16, le tremblement du centroïde compris).
2. Têtes de 0 à +30 % (gibier non effrayé aux nouveaux camps) ; marche de −0,5 à +1 h/j.
Runs longues (10 ans, 4 × 2) :
3. Scène froide : morts de froid en hausse sur ≥ 3/4 (huttes laissées derrière) ;
   tempéré : population sans direction commune.
Réfuterait 1 : déplacement net ≤ 1 km sur ≥ 6/8 (le camp rend rarement moins que l'habitude,
ou aucune source n'est connue hors du pays).

## MAR-6b (b) — la hutte se laisse derrière (branche `exp/camp-huttes`, sur `exp/camp`)

Changement : une structure à plus de 4,5 km du camp de son clan est laissée derrière :
abandonnée (ruine en 30 j, ou reprise par les occupants du lieu), elle ne compte plus pour le
grenier, la palissade, ni pour ce que le clan possède — il peut en rebâtir une au nouveau
camp. Une hutte debout réchauffe encore qui dort dedans. Le coût d'une hutte (6 points de
stock) **n'est pas touché** : à discuter (une hutte hadza demande quelques heures de travail,
pas de nourriture).
Prédictions, contre (a) :
1. Huttes bâties sur 90 j en hausse (plusieurs par clan qui lève le camp, en scène froide).
2. Runs longues froides : morts de froid ramenées vers `main` (≤ ×1,5) sur ≥ 3/4.
3. Stock du clan plus bas (6 points par hutte rebâtie) ; population froide sans baisse
   commune par rapport à `main`.
Réfuterait 2 : froid toujours ≥ ×2 `main` sur ≥ 3/4 (le coût empêche de rebâtir, ou la
bande part plus vite qu'elle ne rebâtit).

### H1 et H2 — acceptation 5 ans (8 runs) : **les deux réfutées**

| | main | cueillette | H1 repu | H2 adultes |
|---|---|---|---|---|
| morts de soif 1-4 ans | 14 | 27 | **25** | **25** |
| mortalité 1-4 ans (‰/an) | 150 | 200 | 179 | 177 |
| mourir avant 15 ans | 59 % | 67 % | 63 % | 63 % |
| marche km/j F | 28,0-32,5 | 25,8-28,3 | **29,1-34,4** | 25,9-28,2 |
H1 : s'arrêter rassasiée ne ramène pas la soif des petits, et **efface la baisse de marche** :
c'est rester cueillir jusqu'à être chargée qui la faisait. H2 : que seuls les adultes
rapportent n'y change rien. La cause de la soif est ailleurs. Diagnostic : banc `soif` (ce
que font l'enfant et sa mère, distance à l'eau connue, au foyer), main contre cueillette,
3 ans, 8 runs. Hypothèses encore ouvertes : le stock plus plein retient au foyer, loin de
l'eau ; les allers-retours pour rapporter (`BringSurplusHome`) entraînent les petits loin
des sources ; la mère chargée se déplace sans repasser par l'eau.

### MAR-6b (a) — mesuré au banc `marche` (90 j, 8 runs), contre `main` (même binaire de banc)

| | hiver main | hiver camp | été main | été camp |
|---|---|---|---|---|
| levées de camp (> 1 km/j), par run | 0 | 2-18 | 0 | **9-25** |
| chemin des foyers (km/an) | 0,8-10,9 | 40-409 | 0,1-4,8 | **221-495** |
| déplacement net (km, 90 j) | 0-0,6 | 0,8-44 | 0-1,0 | 0-42 |
| têtes | 300 | 314 (+5 %) | 803 | **970 (+21 %)** |
| prise par jour-chasseur | 1,40 % | 1,64 % | 2,62 % | 3,31 % |
| marche h/j F | 6,3 | **5,4** | 8,6 | **7,9** |
| eau (h/j, jour + noir) | 1,69 | 1,25 | 1,81 | 1,55 |
1. Déplacement net > 3 km : 7/8 l'hiver, **4/8 l'été** (le camp va et vient entre des sources
   connues). Chemin au-delà de la fourchette l'été (221-495 contre 20-200 prédit) : **réfuté
   par excès** ; ordre batek : ~45 levées par an de 0,6-14,5 km ≈ 225 km/an ; l'été d'ici
   lève le camp tous les 4-10 j. Hadza : un camp tient plutôt des semaines.
2. Têtes +5 à +21 % : tenu. Marche −0,7 à −0,9 h/j : tenu (dans −0,5 à +1), l'eau baisse
   (le camp se pose sur une source).

### MAR-6b (a) — runs longues (10 ans, 4 × 2, départ j135), contre `main` : **effondrement**

| run | pop an 10 main / camp | foyer → départ (km) | soif | faim | froid |
|---|---|---|---|---|---|
| froid 42 | 45 / **0 (an 7,6)** | — | 12 / 12 | 0 / 4 | 9 / ? |
| froid 7 | 41 / 36 | 0,1 / 482 | 14 / 18 | 5 / 8 | 4 / 0 |
| froid 1337 | 68 / **22** | 0,6 / 21 | 9 / 25 | 0 / 14 | 1 / 4 |
| froid 2024 | 56 / **19** | 0,6 / 585 | 9 / 32 | 0 / 12 | 0 / 0 |
| tempéré 42 | 55 / **25** | 0,3 / 372 | 3 / 10 | 0 / 5 | — |
| tempéré 7 | 37 / **15** | 1,4 / 481 | 12 / 24 | 4 / 17 | — |
| tempéré 1337 | 56 / **12** | 0,2 / 210 | 4 / 24 | 0 / 14 | — |
| tempéré 2024 | 39 / 64 | 0,7 / 363 | 15 / 4 | 4 / 0 | — |
Chemin 150-380 km/an. Prédiction 3 (froid en hausse) **réfutée** : ce n'est pas le froid,
c'est la **soif et la faim d'une bande qui ne cesse de partir** — 200-585 km du départ en
10 ans, sur 7/8 runs en baisse de 30-80 %.
Lecture : la règle compare le rendement du camp à une habitude qui ne compte **ni le
trajet ni l'installation** (la MVT juge contre la moyenne *trajets compris*) ; la bande lève
le camp tous les 4-10 jours. La destination ne pèse **ni la familiarité ni ce qu'un lieu a
rapporté** : on part toujours vers du neuf, sans domaine vital (les Hadza tournent sur des
lieux connus). Il manque au moins : le coût du départ dans la comparaison, et un retour aux
lieux connus. **Question à l'utilisateur avant de reprendre.**
Méthode : les runs longues de (b) (huttes) sont annulées — même errance, elles n'auraient
mesuré qu'elle ; seul son banc `marche` est gardé.

## MAR-6b (a') — le camp se juge par jour, trajets compris (branche `exp/camp-jour`, sur `exp/camp`)

Changement unique : le registre du camp et l'habitude se comptent en **kcal nettes par jour
d'adulte** (et non par heure de subsistance). Un jour de marche vers un nouveau camp ne
rapporte rien et pèse dans l'habitude ; travailler plus d'heures pour autant ne fait pas
partir. Test : `une_bande_qui_tire_autant_par_jour_ne_leve_pas_le_camp`, rouge sur (a).
La destination ne change pas (ni familiarité, ni mémoire des lieux) : c'est la question
ouverte, posée à part.
Prédictions, banc `marche` (90 j), contre (a) :
1. Levées de camp en baisse de 30 à 70 % (l'été : 9-25 → 4-15 par run).
2. Têtes et marche entre `main` et (a).
Runs longues (10 ans, 4 × 2), contre `main` :
3. Foyer toujours loin du départ (≥ 50 km sur ≥ 5/8) : sans retour aux lieux connus, le
   rythme baisse mais la bande dérive encore.
4. Population : moins effondrée que (a) (≥ 2× les effectifs de (a) sur ≥ 5/7 vivantes),
   mais toujours sous `main` sur ≥ 5/8.
Réfuterait 1 : levées à ±20 % de (a).

### Banc `soif` (3 ans, 8 runs, départ j135) — main contre cueillette

| | main | cueillette |
|---|---|---|
| heures-enfant 1-4 ans en soif saturée | 441 (0,04 %) | 819 (0,08 %) |
| morts de soif 1-4 ans | 7 | 13 |
| … dont l'enfant sans mère | 68 % | 35 % |
| … mère à < 100 m | 32 % | 65 % |
| tâche de la mère (avec mère) | dort 9 %, retour au clan 5 %, chasse 3 % | **dort 21 %, s'abrite 8 %**, chasse 7 % |
| source connue à < 500 m | 78 % | 59 % |
**L'enfant n'a jamais de tâche** quand sa soif sature : il est porté (moins de 3 ans) et ne
boit que le lait. Avec cueillette, le cas dominant n'est plus l'orphelin mais l'enfant
**collé à sa mère**. Or le lait étanche la soif à 0,07/h au moins tant que la mère a des
réserves (relief 0,1 × lait ≥ 0,7, contre 0,042/h de soif) : sa soif ne sature que si la mère
est à réserves vides et santé basse, ou si l'enfant a 3-4 ans. Relance avec l'âge et l'état de
la mère (`soif2_cueillette`).
Défaut amont probable : **MOR-4** (le sevrage, « eau bue avec la mère » à 6 mois) — un
enfant de moins de 3 ans ne boit que le lait, et toute baisse des réserves maternelles le
met en danger.

### MAR-6b (b) et (a') — bancs `marche` (90 j, 8 runs)

| | hiver (a) | hiver (b) | hiver (a') | été (a) | été (b) | été (a') |
|---|---|---|---|---|---|---|
| levées de camp par run | 2-18 | 4-17 | 21-37 | 9-25 | 12-33 | **16-132** |
| huttes bâties (froid) | 1 | 1-5 | 1 | 0 | 0 | 0 |
| têtes | 314 | 325 | 450 | 970 | 1 091 | 1 087 |
| marche h/j F | 5,4 | 5,7 | 5,5 | 7,9 | 8,2 | 7,8 |
(b) : les huttes se rebâtissent en scène froide l'hiver (jusqu'à 5 par run) ; prédiction 1
tenue. (a') : **levées en hausse ×2 à ×5** (prédit −30 à −70 %) : **réfuté, sens inverse**.
Lecture : le jour du trajet comptait déjà pour le nouveau camp ; il ne rapporte rien, le camp
paraît pauvre dès le premier jour, et la bande repart — en cascade. Correction (a'') : un jour
d'adulte ne compte pour le camp que s'il y a dormi (à moins de 4,5 km) ; le trajet reste
dans l'habitude. Test `une_bande_en_chemin_ne_juge_pas_son_nouveau_camp`, vu rouge.
Prédictions (a''), banc `marche` contre (a) : levées en baisse de 30 à 70 % ; contre `main`,
têtes et marche entre `main` et (a). Runs longues : foyer encore loin du départ (≥ 50 km
sur ≥ 5/8) faute de retour aux lieux connus ; population moins effondrée que (a).

### Banc `soif` (3 ans, 8 runs) — âge, état des mères

- **100 %** des heures de soif saturée des 1-4 ans sont des enfants de **moins de 3 ans**,
  portés, qui ne boivent que le lait ; mère présente : réserves vides et santé < 0,5 (64 %).
- Mères d'un enfant de moins de 3 ans, à minuit (≈ 105 000 jours-mère) :

| | main | cueillette |
|---|---|---|
| réserves vides | 0,14 % | 0,19 % (froid 7 : **1,42 %** ; 0-0,32 % ailleurs) |
| faim moyenne | 0,423 | **0,345** (8/8 en baisse) |
| charge portée (points) | 0,02 | 0,06 |
Lecture : la cueillette rapportée **nourrit mieux les mères** sur les 8 runs ; la soif des
petits monte surtout par un épisode de famine maternelle (froid 7), et par la voie unique
du lait : un enfant de moins de 3 ans ne boit jamais d'eau. C'est MOR-4 (sevrage : eau et
nourriture données dès 6 mois) qui manque en amont — non la cueillette. H3 (la maille
s'épuise au fil des ans) : réfutée (faim moyenne plus basse chaque année, 8/8). H4 (la
charge épuise les mères) : réfutée en moyenne (PAL des femmes en baisse).
Ce que ça ne couvre pas : la mortalité 1-4 ans de l'acceptation 5 ans montait sur 7/8 runs,
modestement hors froid 7 ; un effet faible mais commun n'est pas exclu.

### MAR-6b (a') — runs longues : **3 extinctions sur 8**, foyers à 250-1 260 km (cascade de levées)

### MAR-6b (a'') — banc `marche` (90 j, 8 runs)

| | (a) | (a') | (a'') |
|---|---|---|---|
| levées par run, hiver | 2-18 | 21-37 | 6-26 |
| levées par run, été | 9-25 | 16-132 | **11-48** |
| déplacement net été (km, 90 j) | 0-42 | — | **0-323** |
| têtes hiver ; été | 314 ; 970 | 450 ; 1 087 | 358 ; 973 |
| marche h/j F hiver ; été | 5,4 ; 7,9 | 5,5 ; 7,8 | 5,7 ; 8,3 |
Prédiction (levées −30 à −70 % sous (a)) : **réfutée** — toujours plus que (a).
Lecture : la règle elle-même ne tient pas. (1) Le rendement d'une bande est fait de prises
de 60 000 kcal : sur 4 jours il est presque toujours sous l'habitude d'un mois qui lisse les
prises, et la bande part. (2) La destination ne garde aucun souvenir de ce qu'un lieu a
rendu, ni de la familiarité : on part toujours vers du neuf, sans domaine vital. Questions à
l'utilisateur. Les runs longues de (a'') tournent (`queue13`), résultats dans
`out/longue/mar6/camp-sejour_*`.

## Décisions de l'utilisateur (2026-10-16, soir)
- **MOR-4 (sevrage) avant de reporter la cueillette** : oui.
- **Camp** : « il manque un concept dans le modèle, ou on mesure mal ou pas assez longtemps.
  La mémoire est importante, et pour tous finalement : un humain sans mémoire ne peut rien
  faire. » → chantier mémoire des lieux (ce qu'un lieu a rendu), pour tous, avant de
  reprendre la règle de départ ; vérifier aussi le régime et l'horizon des mesures du camp.
- Règle des sous-agents : validée.

## Run longue à deux foyers (2026-10-18, demande de l'utilisateur ; `longue` 7452bc3, `main` 6dfd3ea)
Tempéré, seed 42, deux foyers de 80 à **20 km**, arrêt prévu à 1 tps ; arrêtée à la main à
l'an 18 (jour 6 620, 5 h de calcul) : la tendance ne changeait plus.
- **Aucune rencontre en 18 ans** : centres à 19,7-22,4 km tout du long, 0 contact à 100 m, 0
  lien, 0 clan ni naissance mixte, 0 contagion entre foyers. Les camps ne bougent pas (MAR-6b,
  MEM-3) : à 20 km, aucune sortie quotidienne n'atteint l'autre pays.
- Population stable ~235-250 (A 118, B 129) ; 285 naissances, 199 morts, dont 170 de maladie,
  0 de violence ; clans 117, 118 et 12 (BAN).
- Débit 26 → 6-7 tps (30 j) ; mémoire stable ~1,1 Go.
- **Faune en croissance continue** : 19 600 têtes (an 3) → 44 800 (an 18), 714 troupeaux — à
  lire (proie sans borne ? coût du débit ?).
Relancée à **6 km** le 2026-10-18 à 15 h 45 (`out/longue/deuxfoyers/t42_2x80_6km_3c0fe52.*`).

### À 6 km (3c0fe52) — point à l'an 17 (2026-10-18, 17 h 41 ; run toujours en cours)
- **Rencontre dès le jour 3** ; 45 liens A-B au jour 30. Mais la tension entre les deux clans
  monte à 0,95 au jour 60 et reste **saturée à 1,0 de 4 mois à 3 ans**.
- **Une guerre lente puis un massacre** : morts de violence A/B 5/3 (j120), 24/6 (j500), 63/21
  (j1040), puis **23 A tués en 7 jours** (j1050-1057) ; le clan A (32) disparaît, ses 5 survivants
  rejoignent B (un seul clan, mixte, ~70-90 depuis).
- Violence : **107 morts** sur ~225 (≈ 47 %), contre ~10 % visés par le BRIEF et quelques pour
  mille par an chez les Hadza ou les !Kung — D13.
- Population 160 → 68 (an 3), puis ~86 stable jusqu'à l'an 17 (à 20 km : ~245). La technique
  de conservation est perdue (`techs` vide).
- Après la fusion : 1 126 contagions entre origines (un seul camp), 17 naissances mixtes.
- Débit 15-30 tps, mémoire 1,1 Go.
À lire (D13) : pourquoi la tension sature et ne redescend jamais pendant des années de
contacts quotidiens et de liens (45-75 liens A-B) ; pourquoi la violence ne s'arrête qu'à
l'anéantissement d'un clan.

### À 6 km — arrêtée à l'an 197 (2026-10-20) : pourquoi le monde stagne
Arrêt à la main (40 h de calcul ; débit stable ~10 tps, jamais près de 1). Lecture du CSV par
périodes de 20 ans (`out/longue/deuxfoyers/t42_2x80_6km_3c0fe52.csv`) :
| an | pop moy. | natalité ‰ | mortalité ‰ | dont maladie ‰ | morts < 15 / naissances |
|---|---|---|---|---|---|
| 10-30 | 91 | 56 | 49 | 39 | 0,54 |
| 70-90 | 168 | 73 | 62 | 53 | 0,62 |
| 110-130 | 181 | 56 | 61 | 48 | 0,61 |
| 170-190 | 174 | 58 | 61 | 50 | 0,60 |
Constantes sur 197 ans : foyer à 0,59 km du point de départ, 3 huttes du chef, dispersion
maximale 1,5-6 km ; faim moyenne 0,21-0,25, **0 mort de faim**, 0 de froid ; exposition au
feu et au cuivre **0** ; aucune technique ; un seul clan (165-191) jamais scindé ; violence
nulle après l'an 3 ; faune stable ~28 000-33 000 têtes.
**La chaîne, de l'amont à l'aval** (une seed, une scène : à confirmer, règle 7) :
1. **Le camp ne bouge jamais** (à 20 km non plus : foyers fixes 18 ans). Sans levée du camp
   (MAR-6b), sans mémoire des lieux qui donnerait une raison d'aller ailleurs (MEM-3) et sans
   scission (BAN), toute la population vit 200 ans dans un rayon de quelques km.
2. **Donc rien de neuf n'est vu** : aucun feu de foudre dans ce rayon de forêt tempérée humide
   (D1 : le feu rare en climat humide), aucun gisement → aucune exposition → aucune technique
   possible (l'arbre exige des expositions).
3. **Donc la nourriture ne borne pas** (faim stable, aucune mort de faim) : c'est la **maladie**
   qui régule (80 % des morts), et elle frappe un camp unique de ~180 qui boit aux mêmes points
   et dort serré : **60 % des enfants meurent avant 15 ans** (réel ~40 % ; 43 % au banc de 5 ans,
   avec des camps de 40-90). Natalité et mortalité s'égalisent vers 55-65 ‰ : croissance nulle.
4. **Donc rien ne grossit** (ni population ni monde chargé) et le débit reste plat : la run ne
   pouvait pas « se tuer » seule. Le coût, lui, est notable : ~10 tps pour 170 humains et ~30 000
   têtes de faune.
**Ce que cela dit de l'ordre** : la stagnation vient de l'amont — la mobilité (MAR-6b, MEM-3) et
la scission (BAN) — plus que de la maladie, qui n'en est que le régulateur ; MOR-5c3 baissera ce
régulateur sans faire bouger le camp.
**Ce que j'ai pu manquer** : une seule seed, une seule scène (tempérée) ; le froid, où la faim et
le froid mordent, régulerait autrement ; le régime des enfants (5-14 ans) n'a pas été lu ici ;
la faune stable n'a pas été confrontée à la chasse (prises par jour non lues).

## MAR-6b + MEM-3 — le camp se lève vers les lieux dont la bande se souvient (branche `exp/camp-memoire`, 2026-10-20)
Ordre revu le 2026-10-20 (run à deux foyers : le camp immobile fait stagner le monde) ; MEM-4a
sans effet tant que le camp ne bouge pas. Reprise de MAR-6b (a'') sur `main` (règle de départ :
le camp, jugé par jour d'adulte qu'on y passe, rend moins que l'habitude — Charnov ; Batek
8,2 j par camp), avec **un changement** : la destination ne lit plus le stock réel des plantes
ni le nombre réel de bouches par maille (omniscience, MEM.md), mais **la carte vécue des
membres** (`Memory::places`, MEM-1) — kcal acquises par heure de subsistance sur le lieu de
2 km de chaque source connue hors du pays battu ; un lieu sans 48 h vécues vaut la moyenne des
lieux connus (ni préféré ni écarté) ; sans rien de vécu, on ne part pas. Gibier vu et trajet
pesés comme avant. Test `le_camp_part_vers_le_lieu_dont_on_se_souvient_qu_il_rendait` (rouge :
l'ancienne destination lisait le monde) ; `une_bande_leve_le_camp_quand_le_pays_s_epuise` donne
désormais à la bande un mois vécu au camp.
Non encore : la repousse d'un lieu quitté (un lieu épuisé reste jugé épuisé), la saison, le
danger ; MEM-4a (sources) n'est pas dans la branche.
Rappel (a'') : foyers à 250-1 260 km en 10 ans, populations effondrées, faute de mémoire.
Prédictions, `longue` 10 ans, 4 seeds × 2 scènes, départ j135, contre `main` (même binaire de banc) :
1. Distance du foyer médian au départ à l'an 10 : médiane des runs ≤ 30 km, et aucune run
   au-delà de 200 km (a'' : 250-1 260). Réfuterait : une run > 200 km.
2. Levées du camp (jours où le foyer d'un clan bouge de plus de 1 km) : 5 à 40 par clan et par
   an (Hadza ~12, Batek ~40).
3. Population à l'an 10 ≥ 70 % de `main` sur ≥ 6/8 runs.
4. Exposition au feu ou au cuivre : non prédite (à lire).

### Débat (2026-10-20, pré-mortem, contradicteur Opus)
1. *La mémoire départagerait peu : 92 % du temps dans ~10 mailles autour d'un camp jamais levé,
   presque toutes exclues ; au-delà, peu de lieux à 48 h vécues : tout vaut la moyenne, le gibier
   et le trajet décident, comme (a'').* — **Retenu.** Action : relever, à chaque levée, la part de
   candidats et la destination lus dans la mémoire (banc `marche` 90 j d'abord). Seuil : moins de
   30 % des destinations lues dans la mémoire → MEM-3 n'est pas testé.
2. *`(1 + game)` compte un troupeau par témoin : dix adultes, facteur 11, qui écrase la mémoire.* —
   **Retenu, bug** (un troupeau reste un troupeau) : compté une fois par maille de 2 km, correctif
   à part avec test rouge. Action : relever les trois facteurs du lieu retenu.
3. *La prédiction 2 (5-40 levées/an) juge la règle de départ de (a''), pas MEM-3.* — **Retenu** :
   elle sort du verdict de MEM-3, elle reste un relevé.
4. *Mémoire de vie entière : le camp qu'on vient d'épuiser garde sa bonne moyenne → navette ; la
   prédiction 1 ne la verrait pas.* — **Retenu.** Le plan validé de MEM-3 disait « un lieu quitté
   repousse » : la valeur d'un lieu suit son rendement **récent** (demi-vie 30 j, celle des
   rendements appris), et revient vers son rendement de long terme à mesure que le temps passe
   depuis le dernier passage (demi-vie 60 j : la repousse du modèle, une prairie rasée à mi-hauteur
   en ~2 mois, `ecology::GROWTH_RATE_PER_DAY`). Mesures : distance maximale, chemin par an, part des
   levées vers un ancien camp, durée des séjours.
5. *Population jugée sur un instant contre un témoin pathologique.* — **Retenu** : effectif moyen
   des ans 5-10 et causes de mort, contre `main` et (a'').
6. *Le froid n'est pas prédit : sans huttes rebâties, la bande qui part perd sa chaleur au premier
   hiver.* — **Retenu** : prédiction ajoutée (morts de froid en scène froide en hausse).
7. *Une prise de 60 000 kcal tombe sur la maille de l'abattage.* — **Retenu** comme lecture :
   rendements de chasse et de cueillette des lieux retenus lus à part.
8. *Commentaire de tête de `camp.rs` périmé.* — **Retenu**, corrigé.

### Prédictions révisées après le pré-mortem (avant tout banc)
Correctifs faits : gibier compté par troupeau (d957678) ; valeur d'un lieu = rendement récent
(demi-vie 4 j, celle du registre du camp) qui revient vers le long terme avec la repousse
(demi-vie 60 j) (test `un_lieu_epuise_vaut_peu_puis_repousse`). Instrument : `Sim::camp_moves`
(chaque levée : candidats, lus dans la mémoire, facteurs), lu par `marche` et `longue`.
**Étape 1, banc `marche` 90 j × 8 runs** (exp/camp-memoire contre `main`) :
- Seuil : si moins de 30 % des destinations sont jugées sur la mémoire, MEM-3 n'est pas
  testé — on n'engage pas la run longue, on regarde pourquoi.
- Prédiction : 30-80 % des destinations jugées sur la mémoire ; gibier présent au lieu
  retenu ≤ 60 % ; retours vers un ancien camp ≥ 10 % des levées.
- Relevé (hors verdict de MEM-3, il juge la règle de départ) : levées par clan.
**Étape 2, `longue` 10 ans × 8** (si l'étape 1 passe) :
1. Distance du foyer au départ : maximum sur 10 ans ≤ 200 km sur 8/8 ; médiane à l'an 10 ≤ 30 km.
2. Population moyenne des ans 5-10 ≥ 70 % de `main` sur ≥ 6/8.
3. Scène froide : morts de froid en hausse contre `main` (les huttes restent derrière) — lu à part.

### Étape 1 — mesurée (banc `marche` 90 j × 8 runs, 6b721d6) contre `main` (1e960f8)
- **101 levées** ; destination jugée sur la mémoire **41 %** (seuil 30 % : passé ; prédit 30-80 % :
  tenu) ; candidats connus 47 % (5,6 sur 11,9) ; vers un ancien camp **11 %** (≥ 10 % : tenu).
- **Gibier vu au lieu retenu : 91 %** (prédit ≤ 60 % : **réfuté**). Facteurs moyens du lieu
  retenu : mémoire 1 535 kcal/h contre 899 par défaut (×1,7) ; gibier **×3,9** ; trajet ×0,94 ;
  distance médiane 5,9 km (p90 9). Même compté par troupeau, le gibier décide.
- Par run (chemin du foyer, km/an ; déplacement net en 90 j ; levées) : t42 156 ; 6,9 ; 6 ·
  t7 581 ; 42 ; 22 · t1337 159 ; 6,8 ; 7 · t2024 657 ; 80 ; 23 · f42 70 ; 5,3 ; 4 · f7 512 ; 45 ;
  20 · f1337 188 ; 14 ; 8 · f2024 219 ; 6,1 ; 11. Réel : ~160-175 km/an de déplacements
  résidentiels. **5 runs dans l'ordre réel, 3 qui dérivent** (42-80 km nets en 90 j).
- Marche totale 7,6 → 7,1 h/j ; **vers l'eau 1,39 → 0,98 h/j (−30 %)** : le camp se pose à une
  source.
Lecture : la mémoire est lue (41 %), mais le facteur multiplicatif du gibier (1 + mailles où l'on
a vu du gibier, jusqu'à ×4) l'écrase ; les runs qui dérivent sont celles qui suivent des
troupeaux. Le gibier compte déjà dans la mémoire (les kcal de chasse d'un lieu) : le facteur le
compte deux fois, et l'information « gibier vu ces trois derniers jours » pèse plus que tout ce
qu'on sait du pays. Question à l'utilisateur avant l'étape 2.

### Choix 1 de l'utilisateur (2026-10-20) : le gibier vu ne pèse plus sur la destination
Le score d'une source candidate devient mémoire × trajet ; la chasse y entre par les kcal
tirées du lieu. Test `le_gibier_vu_ne_l_emporte_pas_sur_ce_dont_on_se_souvient`, vu rouge
(1 000 contre 600 kcal/h, un troupeau près du second : l'ancien score choisissait le second).
Prédictions, banc `marche` 90 j × 8 runs, contre l'étape 1 :
1. Aucune run au-delà de 30 km de déplacement net du foyer en 90 j (étape 1 : 42, 45, 80).
2. Chemin du foyer de 70 à 300 km/an sur ≥ 6/8 runs (réel ~160-175).
3. Destinations jugées sur la mémoire ≥ 41 % ; retours vers un ancien camp ≥ 11 %.
Réfuterait 1 : une run au-delà de 40 km.

### Choix 1 — mesuré (banc `marche` 90 j × 8 runs, ee90893)
| run | chemin km/an | net 90 j (km) | levées |
|---|---|---|---|
| t42 | 149 | 8,4 | 7 |
| t7 | 300 | 23,6 | 12 |
| t1337 | 72 | 5,7 | 3 |
| t2024 | 591 | **33,0** | 25 |
| f42 | 78 | 0,0 | 4 |
| f7 | 678 | 2,0 | 38 |
| f1337 | 113 | 10,7 | 5 |
| f2024 | 108 | 0,0 | 8 |
102 levées ; destination sur la mémoire **46 %** ; vers un ancien camp **19 %** ; gibier présent
au lieu retenu 79 % (il est là où l'on a chassé) ; distance médiane 5,2 km ; eau 1,0 h/j.
1. Aucune run > 30 km nets : **manquée de peu** (t2024 : 33 ; seuil de réfutation 40 non atteint ;
   étape 1 : 42, 45, 80). 2. 70-300 km/an sur ≥ 6/8 : **tenue (6/8)** ; f7 fait la navette (38
   levées, 2 km nets), t2024 parcourt. 3. Mémoire ≥ 41 %, retours ≥ 11 % : **tenue** (46 ; 19).
Étape 1 passée → étape 2 (`longue` 10 ans × 8, contre `main`).

### Étape 2 — mesurée (`longue` 10 ans × 8, camp-memoire ee90893 contre `main` 38dca99, départ j135)
| run | pop moy. ans 5-10 main → camp | foyer au départ, max (km) | levées (mém. ; retours) | soif ; faim ; froid main → camp | feu vu ; techs main → camp |
|---|---|---|---|---|---|
| t42 | 62 → 62 | 324 | 613 (375 ; 217) | 4→4 ; 0→0 ; 0→0 | 0→0 ; 0→0 |
| t7 | 49 → 31 | 460 | 902 (622 ; 438) | 9→14 ; 1→6 ; 0→0 | 42→26 ; 1→1 |
| t1337 | 65 → 50 | 173 | 608 (462 ; 268) | 4→20 ; 0→7 ; 0→0 | 0→1 ; 0→1 |
| t2024 | 57 → 46 | 550 | 906 (442 ; 244) | 19→11 ; 3→4 ; 0→0 | 5→37 ; 1→3 |
| f42 | 75 → 26 | 147 | 615 (470 ; 351) | 4→26 ; 0→17 ; 6→12 | 0→6 ; 3→6 |
| f7 | 23 → 43 | 457 | 973 (538 ; 356) | 7→21 ; 6→7 ; 3→0 | 0→28 ; 1→6 |
| f1337 | 94 → 27 | 24 | 467 (446 ; 406) | 3→25 ; 0→15 ; 2→16 | 0→0 ; 1→1 |
| f2024 | 94 → 49 | 193 | 861 (730 ; 527) | 4→21 ; 0→7 ; 0→5 | 0→36 ; 1→3 |
1. Foyer ≤ 200 km sur 8/8 : **réfutée** (4/8 ; 324-550 km pour les quatre autres).
2. Population ≥ 70 % de `main` sur ≥ 6/8 : **réfutée** (4/8 ; f42, f1337 à 29-35 %).
3. Froid en hausse en scène froide : **tenue sur 3/4** (f7 en baisse).
Ce que la mobilité apporte : **le monde s'ouvre** — feu vu sur 6/8 runs (2/8 sur `main`), plus
de techniques sur 5/8 (jusqu'à 6 contre 1). Ce qu'elle coûte : **la soif ×2 à ×8 et la faim**
sur 6/8, le froid en scène froide.
Lecture : 47 à 97 levées par an (Batek ~44, Hadza ~12) ; 35-87 % reviennent à un ancien camp,
mais les autres avancent : la bande dérive pas à pas. Deux mécanismes possibles, non
départagés : (a) la **règle de départ** (de a'') lève trop souvent, et chaque levée sort du pays
battu (au-delà du rayon de résidence) ; (b) **l'inconnu vaut la moyenne** des lieux connus :
juste après avoir épuisé son pays, tout lieu inconnu bat les lieux connus récemment exploités,
et la bande s'enfonce dans l'inconnu. La soif : à lire (arrivée à une source qu'un membre
connaît par ouï-dire, enfants qui suivent mal ?).

## Où le concept a été manqué (2026-10-20, relevé par l'utilisateur)
Décision : « si un humain a déjà vu un endroit lors de la chasse ou de l'exploration, on peut
comparer cet emplacement avec d'autres connus. On ne peut pas comparer un endroit que l'on ne
connaît pas. Cette analyse aurait dû être faite avant les runs. »
L'erreur, en trois couches :
1. **Une valeur donnée à l'inconnu** (« la moyenne des lieux connus ») : l'omniscience que MEM-3
   devait retirer, revenue sous la forme d'une ignorance traitée comme un savoir moyen.
2. **Des candidates connues par ouï-dire** : toute source qu'un membre connaît, y compris
   apprise d'autrui dans un pays où personne n'est allé.
3. **Un seuil de 48 h de subsistance** qui rangeait parmi les inconnus les lieux vus en chassant
   ou en explorant.
Signes ignorés : le pré-mortem (point 1 : « presque toutes les candidates reçoivent la valeur
par défaut ») et l'étape 1 (54 % des destinations sans mémoire) — mesurés au lieu d'être
corrigés. Leçon (ajoutée à la méthode du chantier) : quand une valeur par défaut tient lieu de
savoir, demander d'abord si l'agent **peut** savoir ; un pré-mortem qui touche le concept se
corrige avant de mesurer.

## MEM-3 corrigé — on ne compare que ce qu'on a vu (2026-10-20)
- Candidates : les sources hors du pays battu **dont le lieu de 2 km a été parcouru de jour par
  un membre** ; les autres n'ont pas de valeur et ne sont pas candidates.
- Un lieu travaillé vaut ce qu'il a rendu (récent, puis repoussé) ; un lieu **seulement vu** vaut
  le fourrage qu'on y a vu (`Place::seen_forage`, lu de jour sur la tuile où l'on se tient) ×
  ce que rapporte, sur les lieux à la fois vus et travaillés par la bande, une unité de fourrage
  vu (kcal/h) — la comparaison s'apprend.
- Sans lieu comparable hors du pays, la bande reste.
Tests : `on_ne_part_pas_vers_un_pays_ou_personne_n_est_alle` (rouge : l'ancienne règle partait
vers l'inconnu valu à la moyenne), `un_lieu_vu_se_compare_par_ce_qu_on_y_a_vu_pousser` (rouge :
l'ancienne lui donnait la moyenne) ; `une_bande_leve_le_camp_quand_le_pays_s_epuise` passe
désormais de jour au pays de la source.
Prédictions, banc `marche` 90 j × 8 runs, contre le choix 1 (ee90893) :
1. Déplacement net du foyer ≤ 20 km sur 8/8 runs en 90 j (choix 1 : jusqu'à 33).
2. Retours vers un ancien camp ≥ 30 % des levées (choix 1 : 19 %).
3. Levées : moins nombreuses qu'au choix 1 (102) — moins de candidates.
Réfuterait 1 : une run au-delà de 30 km.
Puis `longue` 10 ans × 8 si 1 tient : foyer ≤ 200 km sur 8/8 ; population ≥ 70 % de `main` sur ≥ 6/8.

### MEM-3 corrigé — mesuré (banc `marche` 90 j × 8 runs, 72cae9f) contre le choix 1 (ee90893)
| run | chemin km/an | net 90 j (km) | levées |
|---|---|---|---|
| t42 | 57 | 4,6 | 3 |
| t7 | 435 | 19,6 | 18 |
| t1337 | 132 | 4,8 | 6 |
| t2024 | 692 | **56,5** | 34 |
| f42 | 95 | 6,0 | 7 |
| f7 | 805 | 8,0 | 37 |
| f1337 | 209 | 6,4 | 9 |
| f2024 | 102 | 4,8 | 7 |
121 levées ; candidats connus **99 %** (10,0 sur 10,2) ; destination travaillée 96 % ; vers un
ancien camp **32 %** ; distance médiane 5,4 km.
1. Net ≤ 20 km sur 8/8 : **réfutée** (t2024 : 56,5). 2. Retours ≥ 30 % : **tenue** (32 %).
3. Moins de levées : **réfutée** (121 contre 102).
Lecture : le concept est corrigé, mais **ce n'était pas le moteur de la dérive** : presque toutes
les sources que connaît la bande sont dans des lieux qu'un membre a vus (99 %) ; l'inconnu ne
pesait que sur une minorité de choix. Ce qui reste : **la fréquence des levées** — 3 à 37 en
90 j selon la run (Hadza ~3, Batek ~11) — chacune d'au moins le rayon de résidence (~5 km).
Les runs qui dérivent (t2024, t7) et celle qui fait la navette (f7) sont celles qui lèvent le
camp le plus souvent. La cause est en amont, dans la **règle de départ** (a'') : un camp jugé
sur quelques jours, contre une habitude d'un mois où entrent les grosses prises (60 000 kcal) —
le défaut déjà relevé en octobre (« prises bosselées contre habitude lissée »).
Run longue non engagée (prédiction 1 réfutée).

## Ce qui déclenche une levée, et le camp jugé sur la cueillette (choix 1 de l'utilisateur, 2026-10-20)
Instrument (observation, sans effet) : chaque levée relève ce que rendait le camp et
l'habitude (kcal nettes par jour d'adulte), la part de la chasse dans chacun, et les jours
passés au camp. Puis **un changement** (décision de l'utilisateur : « idée 1 ») : le camp et
l'habitude se jugent **hors chasse** — ce qui s'épuise autour d'un camp, ce sont les plantes ;
le gibier bouge et ses prises (60 000 kcal d'un coup) sont trop irrégulières pour juger un lieu
en quelques jours.
Prédictions, banc `marche` 90 j × 8 runs, règle actuelle (instrument seul) et règle hors chasse
lancées ensemble :
1. Règle actuelle : la chasse fait plus de la moitié de l'habitude dans ≥ 50 % des levées, et
   dans ≥ 40 % des levées le camp rendait **hors chasse** autant ou plus que l'habitude hors
   chasse (levées déclenchées par l'absence d'une prise, pas par l'épuisement).
2. Règle hors chasse : levées divisées par ≥ 2 (121) ; aucune run au-delà de 20 km nets en 90 j.
3. Jours au camp (médiane) : ≥ 8 (Batek 8,2).
Réfuterait 1 : la chasse < 25 % de l'habitude dans la plupart des levées (alors elle ne
déclenche pas).

### Mesuré (banc `marche` 90 j × 8 runs, départ j135 : printemps-été) — règle actuelle (6c970db, instrument seul) et hors chasse (820a3b7)
**Ce qui déclenche, sous la règle actuelle** (121 levées, médianes ; p10-p90) :
- camp 4 093 kcal nettes/jour d'adulte (2 601-5 779), dont chasse **−95** (coût sans prise) ;
  habitude 5 496 (4 445-8 294), dont chasse **1 517** (596-5 709) ;
- hors chasse : camp 3 926 contre habitude 3 367 — **dans 88 % des levées, la cueillette du camp
  rendait autant ou plus que d'habitude** ; la chasse fait plus de la moitié de l'habitude dans
  31 % des levées ; **2 jours au camp** en médiane (p90 9).
1. Chasse > ½ de l'habitude dans ≥ 50 % des levées : **réfutée** (31 %) ; camp hors chasse ≥
   habitude hors chasse dans ≥ 40 % : **tenue, et de loin** (88 %). Les levées ne viennent pas de
   l'épuisement du pays mais du **creux de chasse** : quelques jours sans prise au camp, contre
   une habitude qui contient des prises.
**Règle hors chasse : aucune levée en 90 jours, sur 8/8 runs** (marche vers l'eau revenue à
1,4 h/j). 2. Levées divisées par ≥ 2 : **réfutée par l'autre bout** (÷ ∞). 3. ≥ 8 jours au camp :
sans objet.
Lecture, aux deux bouts : avec la chasse, la bande part pour un bruit ; sans elle, la cueillette
d'un camp ne baisse jamais sous l'habitude **au printemps et en été** — la saison où les plantes
repoussent (r = 0,08/j) et où une maille ne s'épuise pas en 90 j (déjà vu en MAR-6c). Le banc ne
couvre pas l'hiver : c'est le régime où le pays devrait s'épuiser (règle 2). Mesure suivante :
la règle hors chasse sur une année entière.

### Règle hors chasse sur deux ans (`longue` 2 ans × 8, 820a3b7, départ j135) — contre `main` au jour 720 (38dca99)
| run | levées (retours) | foyer à l'an 2 ; max (km) | pop main → hors chasse | soif ; faim ; froid (hors chasse) |
|---|---|---|---|---|
| t42 | 89 (42) | 12 ; 17 | 56 → 53 | 2 ; 0 ; 0 |
| t7 | 85 (32) | 63 ; 68 | 50 → 51 | 0 ; 0 ; 0 |
| t1337 | 92 (44) | 27 ; 31 | 58 → 57 | 0 ; 0 ; 0 |
| t2024 | 90 (38) | 67 ; 67 | 59 → 53 | 2 ; 0 ; 0 |
| f42 | 114 (84) | 8 ; 17 | 66 → **27** | 15 ; 6 ; 20 |
| f7 | 119 (75) | 22 ; 30 | 33 → 56 | 8 ; 7 ; 4 |
| f1337 | 110 (91) | 1 ; 9 | 85 → 72 | 0 ; 0 ; 1 |
| f2024 | 116 (80) | 11 ; 21 | 75 → 75 | 1 ; 0 ; 0 |
**Une mobilité saisonnière émerge, sans être écrite** : aucune levée de mai à septembre (la
saison où les plantes repoussent), puis les levées d'octobre à février, et de nouveau la
seconde année (levées cumulées par mois, t42 : 0 pendant 5 mois, 28 à l'hiver, plat jusqu'à
l'été suivant, 89 à la fin du second hiver). ~45 levées par an (Batek ~44, Hadza ~12), mais
**concentrées sur 4-5 mois** : un camp tous les 3 jours environ en saison maigre (trop).
Foyers à 9-68 km au plus en 2 ans (règle de la chasse : 300-550 km en 10 ans) ; 36 à 83 % des
levées reviennent à un ancien camp. Population au niveau de `main` sur 6/8 (f42 : 27 contre 66,
froid et soif ; f7 : 56 contre 33). Feu vu sur 3/8 (1/8 sur `main`).
Ce que j'ai pu manquer : 2 ans seulement (la dérive sur 10 ans n'est pas jugée ; t7 et t2024
avancent de ~30 km/an) ; la fréquence en saison maigre n'est pas comparée aux séjours d'hiver
réels ; f42 s'effondre de froid : les huttes restent derrière.

### Règle hors chasse sur dix ans (`longue` 10 ans × 8, 820a3b7) contre `main` (38dca99)
| run | pop moy. ans 5-10 main → hc | foyer, max (km) | levées/an (retours) | soif ; faim ; froid main → hc | feu vu |
|---|---|---|---|---|---|
| t42 | 62 → 63 | 66 | 56 (68 %) | 4→4 ; 0→0 ; 0→0 | 0→0 |
| t7 | 49 → 40 | 180 | 61 (57 %) | 9→15 ; 1→2 ; 0→0 | 42→35 |
| t1337 | 65 → 64 | 156 | 63 (58 %) | 4→5 ; 0→0 ; 0→0 | 0→29 |
| t2024 | 57 → 56 | 202 | 60 (53 %) | 19→7 ; 3→0 ; 0→0 | 5→4 |
| f42 | 75 → **5** | 54 | 41 (75 %) | 4→24 ; 0→9 ; 6→36 | 0→2 |
| f7 | 23 → 31 | 83 | 66 (70 %) | 7→21 ; 6→18 ; 3→9 | 0→22 |
| f1337 | 94 → **35** | 28 | 56 (79 %) | 3→16 ; 0→11 ; 2→15 | 0→0 |
| f2024 | 94 → 62 | 58 | 64 (73 %) | 4→17 ; 0→7 ; 0→3 | 0→12 |
1. Foyer ≤ 200 km sur 8/8 : **tenue sur 7/8** (t2024 : 202) — règle de la chasse : 324-550 km.
2. Population ≥ 70 % de `main` sur ≥ 6/8 : **réfutée de peu (5/8)** : **tempéré 4/4** (82-102 %),
   froid 1/4 (f42 s'éteint presque, f1337 à 37 %).
3. Froid en hausse en scène froide : **tenue 4/4** ; soif et faim aussi.
Feu vu sur 5/8 (2/8 sur `main`). Levées 41-66 par an, retours 53-79 %.
**Lecture** : en pays tempéré, le mécanisme fait ce qu'il doit (mobilité bornée, population
tenue, le monde s'ouvre). En pays froid, **lever le camp en hiver tue** : froid, soif et faim
montent ensemble. La cause est dans la règle de départ : elle compare le camp à l'**habitude** du
mois écoulé, qui contient l'automne ; en hiver, tout le pays rend peu, et la bande part quand
même — vers un lieu qui ne rendra pas mieux, en laissant ses huttes. Le théorème de la valeur
marginale compare le lieu à ce qu'on peut attendre **ailleurs**, trajet compris : la mémoire
datée des lieux (MEM-3) le permet. Les huttes laissées derrière (MAR-6b (b)) aggravent le même
hiver.

## MAR-6b c — partir seulement si un lieu connu promet mieux (solution 3 de l'utilisateur, étape 1 ; 2026-10-22)
Changement (`camp::daily`) : la bande lève le camp si la meilleure destination connue (MEM-3)
promet, trajet compris, plus que ce que la bande attend du lieu de son camp (sa mémoire datée :
le rendement récent) — plus contre l'habitude du mois. En hiver, si tous les lieux connus rendent
peu, on reste. Le rapport « kcal/h par unité de fourrage vu » s'apprend désormais sur le
rendement **de long terme** des lieux travaillés (sinon un camp qu'on vient d'épuiser faisait
paraître épuisé tout pays vu). Les deux pursuits comptent de part et d'autre (même biais de la
maille d'abattage des deux côtés). Test `une_bande_leve_le_camp_quand_le_pays_s_epuise` revu
(dates du vécu alignées sur l'horloge du test, derniers jours épuisés ou non) ; **pas de test
rouge dédié à l'hiver** (dette, à écrire si la mesure garde la règle). Huttes : étape 2, après.
Prédictions, `longue` 10 ans × 8 contre `main` (38dca99) et la règle hors chasse (820a3b7) :
1. Scène froide : population moyenne des ans 5-10 ≥ 70 % de `main` sur ≥ 3/4 (hors chasse : 1/4).
2. Tempéré : ≥ 70 % de `main` sur 4/4 (tenu par hors chasse) ; foyer ≤ 200 km sur 8/8.
3. Levées : moins qu'hors chasse (41-66/an), surtout l'hiver.

### MAR-6b c — mesuré (`longue` 10 ans × 8, b7ce1ec)
| run | pop moy. ans 5-10 main ; hors chasse ; c | foyer max (km) | levées/an (retours) | soif ; faim ; froid (c) | feu vu (c) |
|---|---|---|---|---|---|
| t42 | 62 ; 63 ; **66** | 98 | 28 (31 %) | 4 ; 0 ; 0 | 57 |
| t7 | 49 ; 40 ; **51** | 133 | 33 (45 %) | 8 ; 2 ; 0 | 50 |
| t1337 | 65 ; 64 ; **67** | 73 | 21 (44 %) | 4 ; 0 ; 0 | 0 |
| t2024 | 57 ; 56 ; **68** | 112 | 34 (30 %) | 4 ; 0 ; 0 | 30 |
| f42 | 75 ; 5 ; **9** | 117 | 23 (31 %) | 30 ; 5 ; 41 | 0 |
| f7 | 23 ; 31 ; **46** | 280 | 63 (44 %) | 24 ; 12 ; 5 | 30 |
| f1337 | 94 ; 35 ; **32** | 32 | 31 (70 %) | 30 ; 20 ; 6 | 0 |
| f2024 | 94 ; 62 ; **61** | 330 | 53 (30 %) | 11 ; 8 ; 0 | 36 |
1. Froid ≥ 70 % de `main` sur ≥ 3/4 : **réfutée** (1/4 : f7). 2. Tempéré ≥ 70 % sur 4/4 :
**tenue, au-dessus de `main` sur 4/4** (103-119 %) ; foyer ≤ 200 km : **6/8** (f7 280, f2024 330).
3. Moins de levées : **tenue** (21-34/an en tempéré contre 56-63 hors chasse ; Hadza ~12, Batek ~44).
Lecture : en tempéré, la règle « un lieu connu promet mieux » donne enfin des bandes mobiles,
bornées, **plus nombreuses que les bandes immobiles**, qui voient le feu (3/4 runs). En froid, la
mort vient du froid et de la soif (f42 : 41 et 30 ; f1337 : soif 30, faim 20) : l'étape 2 (les
huttes rebâties au nouveau camp) vise ce bout-là.

## MAR-6b c + b — les huttes rebâties au nouveau camp (solution 3, étape 2 ; 2026-10-23)
Reprise de l'essai (b) (0009b85, cherry-pick a4faad9) sur MAR-6b c : une structure à plus du
rayon de résidence du camp de son clan est laissée derrière (abandonnée, reprise seulement par
d'autres occupants) ; le clan, s'il a froid, en désire une nouvelle au camp. Test
`un_clan_qui_leve_le_camp_laisse_sa_hutte_et_en_desire_une_autre` (vu rouge à l'époque) ; le test
de surchasse, rouge sur l'ancienne branche, passe ici.
Prédictions, `longue` 10 ans × 8 contre MAR-6b c (b7ce1ec) :
1. Scène froide : morts de froid en baisse sur ≥ 3/4 ; population moyenne des ans 5-10 ≥ 70 %
   de `main` sur ≥ 2/4 (c : 1/4).
2. Tempéré : inchangé à ±10 % (pas de huttes sans froid).
Réfuterait 1 : froid en hausse ou égal sur ≥ 2/4.

### MAR-6b c + b — mesuré (`longue` 10 ans × 8) contre c (b7ce1ec)
| run | pop 5-10 main ; c ; c+b | froid c → c+b | soif c → c+b | foyer max c+b (km) | levées c → c+b |
|---|---|---|---|---|---|
| t42 | 62 ; 66 ; 68 | 0 → 0 | 4 → 3 | 319 | 277 → 380 |
| t7 | 49 ; 51 ; 46 | 0 → 0 | 8 → 5 | 307 | 327 → 428 |
| t1337 | 65 ; 67 ; **48** | 0 → 0 | 4 → 11 | 218 | 210 → 604 |
| t2024 | 57 ; 68 ; 65 | 0 → 0 | 4 → 16 | **622** | 340 → 502 |
| f42 | 75 ; 9 ; 18 | 41 → 17 | 30 → 18 | 70 | 229 → 210 |
| f7 | 23 ; 46 ; 39 | 5 → 1 | 24 → 29 | 243 | 626 → 486 |
| f1337 | 94 ; 32 ; 34 | 6 → 12 | 30 → 20 | 25 | 310 → 323 |
| f2024 | 94 ; 61 ; 57 | 0 → 4 | 11 → 24 | 179 | 528 → 410 |
1. Froid en baisse sur ≥ 3/4 : **réfutée** (2/4 ; f1337 et f2024 en hausse) ; population ≥ 70 %
   de `main` sur ≥ 2/4 : **réfutée** (1/4). 2. Tempéré inchangé à ±10 % : **réfutée** (t1337 −28 %),
   et les levées montent de 30 à 190 % en tempéré, les foyers partent à 218-622 km.
Lecture : les huttes rebâties ne sauvent pas le froid, et **dérèglent le tempéré** : laisser derrière
toutes les structures (la hutte du chef comprise) multiplie les levées — mécanisme non lu. **Non
retenu** : l'étape 2 est défaite (revert), MAR-6b c reste. Ce que le froid garde : **la soif**
(18-30 morts en 10 ans contre 3-4 sur `main`), avant le froid lui-même.

## La soif des bandes mobiles en pays froid (instrument, 2026-10-23 ; décision de l'utilisateur : suite 1)
Relevé (`longue`, fichier `<csv>.soif.csv`) : à l'heure d'avant chaque mort de soif, âge, distance
à la source connue la plus proche, à la vraie source la plus proche (2 km), au camp, jours depuis
la dernière levée, en marche ou non. Réserve : chercher la vraie source charge des chunks (D7 :
l'éviction peut changer le monde) — l'instrument n'est pas garanti neutre.
Scène froide, 4 seeds × 5 ans, MAR-6b c (b7ce1ec + instrument). Prédictions :
1. ≥ 50 % des morts de soif à moins de 3 jours d'une levée (la bande vient d'arriver).
2. ≥ 50 % avec une vraie source à moins de 2 km mais aucune source connue plus proche que 2 km
   (on ne sait pas encore où boire au nouveau camp).
3. Enfants (< 5 ans) ≥ 50 % des morts.

### Soif des bandes mobiles en froid — mesuré (`longue` froid 4 seeds × 5 ans, d05a5cf)
57 morts de soif (f42 25, f1337 17, f7 8, f2024 7). À l'heure d'avant la mort :
- **à la source** : source connue la plus proche à 10 m en médiane (p90 1,7 km), vraie source à
  10 m ; une source réelle à moins de 2 km dans 100 % des cas, inconnue de lui dans 9 % seulement ;
- **au camp** (0,48 km en médiane), **pas en marche** (5 %) ;
- **en plein hiver** : 41 des 57 entre mi-décembre et mi-février ;
- 56 % d'enfants de moins de 5 ans, 26 % de moins d'un an ; 6,1 jours depuis la dernière levée
  en médiane.
1. ≥ 50 % à moins de 3 jours d'une levée : **réfutée de peu** (44 %). 2. Source réelle proche mais
   inconnue : **réfutée** (9 %). 3. Enfants ≥ 50 % : **tenue** (56 %).
Lecture : ce n'est pas un problème de mémoire ni de trajet — on meurt de soif **assis à côté de
l'eau, en hiver**. Rien dans le code ne gèle l'eau (`has_fresh_water` est un drapeau fixe). Deux
explications que ces chiffres ne départagent pas : (a) en hiver, une autre urgence (s'abriter du
froid, dormir) l'emporte sur boire jusqu'à la mort — le choix ne bascule pas ; (b) l'instrument de
cause : la mort est attribuée à « la plus forte morsure », et la soif peut masquer un corps qui
meurt de froid et de faim ensemble (défaut d'instrument déjà relevé au chantier de l'eau). Pour
les petits : ils ne boivent qu'avec leur porteur (MOR-4). Mesure suivante : l'activité et la tâche
de l'heure d'avant, la soif, la faim, le froid et la santé, l'heure de la dernière gorgée.

### Soif en froid — tranché (journal détaillé, 4 seeds × 5 ans, 57 morts « de soif »)
Deux populations distinctes :
- **Les petits (< 3 ans), ~20 morts** : dernière gorgée **44 à 124 h** avant (ou jamais : ils ne
  boivent qu'avec leur porteur) ; **porteur mort dans 18 cas**. Ce sont des **orphelins** : leur mort
  par la soif est réelle, et c'est le chantier des orphelins (MOR-4b), pas la mobilité.
- **Les plus grands (3,5 à 42 ans), ~37 morts** : dernière gorgée **20 à 25 h** avant seulement ;
  soif ~1, mais **faim 1,00 dans la plupart des cas**, froid 0,2-0,8, santé ~0 ; endormis, au repos
  ou à l'abri (Sleeping 20, Idle 12, Sheltering 15 sur 57). Personne ne meurt de soif en 22 h :
  ce sont des corps **qui meurent de faim et de froid**, et que l'instrument de cause range en
  « soif » parce que la soif est la plus forte morsure de la dernière heure.
Verdict : explication (b) — **la cause de mort est mal attribuée** pour les grands ; la vraie
cause en pays froid mobile est **la faim d'hiver** (et le froid), plus les orphelins. L'explication
(a) (une autre urgence empêche de boire) ne tient pas : on avait bu la veille.
Conséquences : (1) l'instrument de cause doit compter ce qui a **usé la santé** sur les derniers
jours, pas la morsure de la dernière heure — sans quoi toutes les mesures de « soif » depuis le
chantier de l'eau mêlent faim et froid ; (2) la mobilité d'hiver en pays froid affame : à lire
(réserves laissées au camp ? cueillette nulle partout en hiver ?).

## L'instrument de cause de mort corrigé (2026-10-23, validé par l'utilisateur)
`Physiology::worn` : l'usure de la santé par cause lente (soif, faim, froid, maladie), avec une
demi-vie d'une semaine ; une mort lente revient à la cause qui a le plus usé le corps
(`Physiology::cause_of_death`), une mort soudaine (violence, prédation, foudre, âge) reste
elle-même. Test `la_cause_d_une_mort_lente_est_ce_qui_a_use_le_corps` (dix jours de jeûne puis une
journée sans eau : `last_damage` dit soif, la nouvelle règle dit faim). Sans effet sur la dynamique
(la cause ne sert qu'à la Chronique) : reporté aussi sur `main`. **Toutes les causes de mort
mesurées avant ce commit mêlent la faim et le froid à la soif.**
Remesure : `longue` froid 4 seeds × 10 ans, MAR-6b c contre `main`, tous deux avec la bonne cause.
Prédiction : sur la branche, les morts « de soif » des plus de 3 ans passent en grande partie à la
faim (≥ 50 %) ; sur `main`, la soif baisse aussi.

### Froid mobile remesuré avec la bonne cause (`longue` froid 4 × 10 ans ; `main` f0bdec5, branche 22c985d)
Dynamique identique à MAR-6b c (populations des ans 5-10 : 9, 46, 32, 61 — mêmes qu'en b7ce1ec) ;
seules les causes changent.
| run | soif ; faim ; froid ; maladie — `main` | — branche (avant correction de l'instrument) |
|---|---|---|
| f42 | 3 ; 4 ; 3 ; 29 | **13 ; 47 ; 14** ; 15 (30 ; 5 ; 41) |
| f7 | 4 ; 9 ; 1 ; 20 (violence 32) | 14 ; 27 ; 1 ; 41 (24 ; 12 ; 5) |
| f1337 | 4 ; 0 ; 1 ; 41 | 17 ; 33 ; 4 ; 44 (30 ; 20 ; 6) |
| f2024 | 4 ; 0 ; 0 ; 30 | 10 ; 8 ; 0 ; 44 (11 ; 8 ; 0) |
Prédiction (≥ 50 % des « soifs » des plus grands passent à la faim) : **tenue sur l'ensemble** —
soif 95 → 54, froid 52 → 19, faim 45 → **115** (contre 13 sur `main`).
**La bande mobile en pays froid meurt de faim**, d'abord : 115 morts de faim en 4 × 10 ans, contre
13 pour la bande immobile. Reste à lire pourquoi (cueillette nulle partout l'hiver ? réserve du
clan ? chasse d'hiver hors du domaine des troupeaux ? marche plus coûteuse ?).

### La faim d'hiver des bandes mobiles en froid — lu (deux ans, 4 seeds, f184cb6 contre b99981d ; et les runs de 10 ans de 22c985d)
- **Sur deux ans, l'hiver ne décroche pas** : cueillette et chasse du jour, réserve des clans et faim
  des adultes de la bande mobile valent celles de la bande immobile (faim d'hiver 0,66 contre 0,66 ;
  réserve plus haute : 4 400 contre 2 555) ; morts de faim 6 contre 1.
- **Le décrochage vient ensuite, avec le gibier** (10 ans, f42 et f1337) : la bande immobile garde
  ses troupeaux près d'elle (troupeaux à 10 km : 9 → 105 en f42, 7 → 32 en f1337) ; la bande mobile
  **les quitte** (f42 : 3-15 puis **0** dès l'an 5, foyer à 50-115 km ; f1337 : 8-15 puis 0 aux ans
  8-10). Sa faim et ses morts de faim suivent (f42 : 11 et 15 aux ans 2-3 ; f1337 : 12 à l'an 5,
  4-7 aux ans 7-9).
Lecture : en pays froid, l'hiver se passe sur la chasse ; la bande mobile choisit ses camps sur ce
que les lieux ont rendu, **toutes saisons confondues** — un lieu riche en plantes l'été vaut autant
en hiver dans sa mémoire — et le gibier vu ne pèse plus (choix 1). Elle part loin de son pays de
gibier vers des pays de cueillette d'été. Deux mécanismes candidats : (a) **une mémoire des lieux
sans saison** (le sourçage de MEM l'appuie : on connaît les ressources de son pays saison par
saison) ; (b) le gibier absent du choix.

## MEM-3 par saison (décision de l'utilisateur, 2026-10-23 : « 1 »)
Changement : chaque lieu retient, par **quart d'année vécu** (`memory::quarter_of`, le moment de
l'année et non l'étiquette `Season` du climat, que la simulation ne lit pas), les kcal acquises,
les heures de subsistance et le fourrage vu. Le camp compare les lieux sur ce qu'ils rendent **à
ce moment de l'année** (`Place::expected_in_season`, `season_seen`) ; un lieu jamais travaillé ni
vu à ce moment de l'année n'est pas comparable. Test `un_lieu_se_juge_sur_ce_qu_il_rend_a_ce_moment_de_l_annee`
(l'été A rend 1 000 kcal/h et B 200, l'hiver A 50 et B 400 : toute l'année, A paraît meilleur ;
l'hiver, B l'emporte).
Prédictions, `longue` 10 ans × 8 contre MAR-6b c (22c985d en froid, b7ce1ec en tempéré) et `main` :
1. Froid : troupeaux à 10 km du foyer non nuls à l'an 10 sur ≥ 3/4 (c : 0 sur f42 et f1337) ;
   morts de faim divisées par ≥ 2 (115) ; population des ans 5-10 ≥ 70 % de `main` sur ≥ 2/4.
2. Tempéré : population ≥ 90 % de c sur 4/4.

### MEM-3 par saison — mesuré (`longue` 10 ans × 8, aa51683)
| run | pop moy. ans 5-10 main ; c ; saison | faim c → saison | soif ; froid (saison) | foyer max (km) | levées |
|---|---|---|---|---|---|
| t42 | 62 ; 66 ; 68 | 0 → 0 | 3 ; 0 | 105 | 329 |
| t7 | 49 ; 51 ; 42 | 2 → 9 | 7 ; 0 | 216 | 431 |
| t1337 | 65 ; 67 ; 62 | 0 → 0 | 3 ; 0 | 210 | 463 |
| t2024 | 57 ; 68 ; 68 | 0 → 0 | 5 ; 0 | 144 | 428 |
| f42 | 75 ; 9 ; 7 | 47 → 27 | 11 ; 32 | 55 | 184 |
| f7 | 23 ; 46 ; **54** | 27 → 15 | 15 ; 0 | 307 | 576 |
| f1337 | 94 ; 32 ; 36 | 33 → 17 | 12 ; 3 | 32 | 300 |
| f2024 | 94 ; 61 ; **74** | 8 → 3 | 5 ; 0 | 69 | 677 |
1. Froid : morts de faim ÷ 2 — **manqué de peu** (115 → 62, ÷ 1,85) ; population ≥ 70 % de `main` sur
   ≥ 2/4 — **tenue** (f7, f2024) ; troupeaux à 10 km — **instrument faux** : la colonne
   `troupeaux_10km` se mesure autour du **point de départ**, pas du camp. Pour une bande immobile
   c'est le même lieu ; pour une bande mobile, non. La lecture « la bande mobile quitte son pays de
   gibier » (plus haut) repose en partie sur ce chiffre : **à reprendre** avec les troupeaux autour du
   camp courant.
2. Tempéré ≥ 90 % de c sur 4/4 : **réfutée** (3/4 ; t7 à 82 %).
Lecture : la saison aide en froid (faim presque ÷ 2, deux runs au niveau de `main` ou au-dessus),
sans coût net en tempéré (une run en baisse). f42 et f1337 restent effondrées (f42 : froid 32, faim 27).

## Instrument de `longue` relu et corrigé (2026-10-24, demande de l'utilisateur)
Relu colonne par colonne pour ce qui supposait une bande immobile :
- `troupeaux_10km` : autour du **point de départ** → autour du **camp du plus grand clan** (ou du
  centre de la population sans clan), relevé de la veille ;
- la nuit (`noir_dormi`) : à la latitude du départ → à celle du camp courant ;
- `kcal_cueillette_j`, `kcal_chasse_j` : différence des sommes (un mort faisait baisser la somme) →
  différence par humain ;
- gardés : `foyer_depart_km`, `foyer_max_km` (le départ est voulu), dispersion (centre du jour),
  morts cumulées, faim, techniques.
Journal des morts lentes (`<csv>.morts.csv`, remplace `.soif.csv`) : soif, faim et froid, avec la
cause, l'état de l'heure d'avant (relevé dès que la santé passe sous 0,2).
Remesure : `longue` froid 4 × 10 ans, MEM-3 par saison contre `main` (mêmes corrections).

### Remesure avec l'instrument corrigé (`longue` froid 4 × 10 ans ; `main` 117d0ed, branche 53362a3)
Dynamique inchangée (populations des ans 5-10 de la branche : 7, 54, 36, 74, comme aa51683).
- **Troupeaux autour du camp courant** (an 1/4/7/10) : `main` f42 10/46/96/105, f7 19/63/76/85,
  f1337 7/18/16/32, f2024 7/70/75/88 ; branche f42 8/40/0/0 (bande presque éteinte), f7 4/7/11/14,
  f1337 6/18/20/37, f2024 6/4/11/6. La bande mobile a moins de troupeaux autour d'elle (sauf
  f1337), mais **ne les a pas « quittés »** : la lecture d'avant est corrigée.
- **Nourriture d'hiver par personne** (kcal/j, cueillette + chasse, ~janvier-février) : la bande
  mobile en a **autant ou plus** que l'immobile (f1337 : 2 000-3 100 contre 1 300-1 500 ; f2024 :
  2 400-4 000 contre 1 800-3 300).
- **Qui meurt de faim** (62 morts sur la branche, journal des morts lentes) : **des adultes** (54),
  **loin du camp** (38 à plus de 2 km, souvent 3-5 km), **en hiver**, **quelques jours après une
  levée** (0,2 à 7 j), en train de cueillir ou de chasser, faim 1, santé 0. Froid : 35 morts, soif 43.
**Verdict (à confirmer par l'utilisateur)** : ce n'est pas la dureté du pays — dans le même pays,
la bande immobile ne meurt pas de faim (0 à 9 en 10 ans), et la bande mobile a en moyenne autant à
manger l'hiver. Ce qui tue, c'est **la dynamique d'après une levée d'hiver** : des adultes partis
cueillir ou chasser à 3-5 km d'un camp neuf, sans revenir manger ce que la bande a. Les famines
réelles (Kivalliq, 1916-1926 ; Keewatin, 1950) suivent l'effondrement d'une ressource, pas une
abondance moyenne : **problème de modèle**.

## Pourquoi des adultes meurent de faim loin d'un camp neuf (2026-10-24)
Lecture du code (`brain`, « puiser dans le stock du clan ») : la réserve n'est proposée que si
`stock > 0,1` **et** (fourrage alentour < 5 **ou** part de la maille < 0,3) **et aucun troupeau en
vue**. Porte tout ou rien sans impossibilité physique (règle 10) : un adulte affamé qui voit un
troupeau, ou qui se tient dans un pays « riche » sur le papier (un camp neuf), **ne pense pas à la
réserve**, chasse ou cueille sans succès jusqu'à la mort. Une bande immobile vit dans un pays
épuisé : la porte s'ouvre ; une bande qui vient d'arriver dans un pays neuf : elle se ferme.
Hypothèse, prédictions (journal des morts lentes enrichi : réserve du clan, troupeau le plus proche,
fourrage de la tuile ; froid 4 × 5 ans, même dynamique que 53362a3) :
1. ≥ 60 % des morts de faim avec une réserve > 0,1 **et** (un troupeau à moins de 2 km ou un
   fourrage ≥ 5) — la porte fermée sur une réserve pleine.
Réfuterait : réserve vide (≤ 0,1) dans la plupart des morts (alors la bande n'a rien à partager).

### La porte de la réserve — mesuré (froid 4 × 5 ans, 9c77968, 48 morts de faim)
- **41 sur 48 (85 %)** meurent avec une réserve du clan > 0,1 **et** la porte fermée ; 7 avec une
  réserve vide. Prédiction (≥ 60 %) : **tenue**.
- Le fourrage de la tuile vaut **98 à 150** à chaque mort (seuil de la porte : 5) : la porte se ferme
  sur la **biomasse** (le fourrage du gibier), qui reste haute l'hiver, alors que ce qu'un humain en
  tire l'hiver est presque nul — on meurt « en train de manger » (Eating/Forage) sur une tuile riche.
- Des réserves pleines au même moment : 1 990 à 3 735 points dans plusieurs cas.
**Pourquoi** : la réserve n'est proposée qu'à un affamé dont le pays « ne répond plus », jugé sur la
biomasse et sur l'absence de troupeau en vue — deux signes qui disent « il y a à manger ici » alors
qu'en hiver ni la biomasse ni un troupeau aperçu ne nourrissent. Une bande immobile vit dans un pays
épuisé (porte ouverte) ; une bande qui vient d'arriver ailleurs, non.

## La porte de la réserve retirée (décision de l'utilisateur, 2026-10-24)
Changement (`brain`) : la réserve du clan est une option pour tout membre quand elle n'est pas vide,
pesée par la faim et la distance au camp ; elle se compare à la cueillette et à la chasse, chacune
pesée par ce qu'elle rapporte à l'expérience (l'hiver, la cueillette ne rend plus et la réserve
l'emporte d'elle-même). Test `un_affame_pense_a_la_reserve_meme_sur_une_tuile_qui_parait_riche`
(midi, un troupeau à 200 m, réserve pleine), vu rouge. Touche aussi les bandes immobiles de `main`.
Prédictions, `longue` 10 ans × 8, contre MEM-3 par saison (aa51683 / 53362a3) :
1. Froid : morts de faim ÷ ≥ 2 (62) ; population des ans 5-10 ≥ 70 % de `main` sur ≥ 3/4 (saison : 2/4).
2. Tempéré : population à ±10 % de la saison.
3. Réserve des clans plus basse en fin d'hiver (on y puise).

### Porte de la réserve retirée — mesuré (`longue` 10 ans × 8, 50c84ba) contre MEM-3 par saison (aa51683)
| run | pop 5-10 main ; saison ; réserve | faim saison → réserve | soif ; froid (réserve) | foyer max (km) |
|---|---|---|---|---|
| t42 | 62 ; 68 ; 65 | 0 → 0 | 1 ; 0 | 356 |
| t7 | 49 ; 42 ; **59** | 9 → 0 | 3 ; 0 | 385 |
| t1337 | 65 ; 62 ; 62 | 0 → 0 | 5 ; 0 | 163 |
| t2024 | 57 ; 68 ; 65 | 0 → 10 | 8 ; 0 | **670** |
| f42 | 75 ; 7 ; **29** | 27 → 40 | 3 ; 0 | 136 |
| f7 | 23 ; 54 ; 56 | 15 → 4 | 5 ; 0 | 198 |
| f1337 | 94 ; 36 ; **49** | 17 → 9 | 7 ; 7 | 35 |
| f2024 | 94 ; 74 ; 76 | 3 → 16 | 7 ; 1 | 275 |
1. Froid : faim ÷ 2 — **réfutée** (62 → 69 ; en baisse sur 2/4, en hausse sur 2/4) ; population
   ≥ 70 % de `main` sur ≥ 3/4 — **réfutée** (2/4), mais en hausse sur 3/4 (f42 7 → 29, f1337 36 → 49).
2. Tempéré à ±10 % : **réfutée par le haut** sur t7 (+40 %), les trois autres à ±5 %.
Ce qui change le plus : **la soif et le froid** en pays froid (43 → 22 et 35 → 8 sur les 4 runs) —
des corps qui mangent mieux boivent et se chauffent mieux. Morts lentes en froid : 140 → 99.
**Voisin** : les foyers dérivent plus loin (tempéré : 163-670 km, contre 105-216) — mécanisme non lu.
Lecture : la réserve tenait derrière la porte une grande part des morts lentes ; ce qui reste, en
froid, est une faim qui ne baisse pas partout (f42, f2024 en hausse) — à relire au journal.

### 1. La faim qui reste en froid (journaux de 50c84ba, f42 et f2024)
f42 : 40 morts de faim, **toutes réserve vide**, en décembre-janvier, au camp ou près (16 au camp),
**28 à 49 jours après la dernière levée** (la bande ne bouge plus : aucun lieu connu ne promet mieux
l'hiver), aucun troupeau à moins de 2 km (souvent aucun dans le rayon relevé). f2024 : 16 morts,
15 réserve vide, surtout des enfants de 3-15 ans, en hiver. **C'est une vraie disette d'hiver** :
plus de réserve, plus de gibier proche, plus de plantes. La bande immobile du même pays vit au
milieu des troupeaux (96-105 à 10 km en f42), la bande mobile, partie à 130 km, non. Ce qui manque
est en amont : arriver à l'hiver avec une réserve, ou là où est le gibier d'hiver.

### 2. La dérive (comparaison saison / réserve)
Chemin du foyer sur 10 ans : 1 060-3 860 km (106-386 km/an ; réel ~160-175), du même ordre avec
et sans la porte ; retours vers un ancien camp 23-55 %. La distance finale au départ (25-660 km)
varie dans les deux sens d'une variante à l'autre (plus loin sur 5/8, moins sur 3/8) : c'est
l'issue d'une **marche presque aléatoire** (~300-500 levées de ~5,5 km : √400 × 5,5 ≈ 110 km
attendus), pas un effet de la porte. Le défaut de fond : **aucun attachement à un domaine** — la
bande compare les lieux connus à ~5 km, sans pays à elle vers lequel revenir.

### 3. Synthèse du chantier camp + mémoire (2026-10-24)
Ce qui marche, mesuré : la carte vécue (MEM-1, neutre, coût nul) ; le camp qui se lève sur la
mémoire des lieux (MEM-3), **sans comparer l'inconnu**, jugé **hors chasse**, **par saison**, et
**seulement si un lieu connu promet mieux** ; le gibier compté par troupeau ; **l'instrument de
cause de mort** (déjà sur `main`) ; **la réserve sans porte**. En tempéré : bandes mobiles bornées en
fréquence (21-48 levées/an), population au niveau de `main` ou au-dessus, le feu vu bien plus souvent.
Ce qui ne tient pas encore : (a) **pas de domaine vital** — le foyer marche presque au hasard, 25 à
670 km en 10 ans ; (b) **la disette d'hiver en froid** quand la bande arrive sans réserve loin du
gibier ; (c) des runs froides sous `main` (f42, f1337).

## MAR-6d — le domaine vital (ouvert le 2026-10-24)

**Lecture** (code de `main` 8347586, `camp.rs`, `memory.rs`). Écart à la réalité : les
chasseurs-cueilleurs tournent sur un pays connu (Hadza, Ju/'hoansi : des points d'eau
nommés, revisités d'année en année) ; ici le foyer fait une marche presque aléatoire
(25-670 km en 10 ans, ~300-500 levées de ~5,5 km, retours vers un ancien camp 23-55 %).

Ce que fait le code : le camp ne peut se poser que sur une **source connue d'un membre**
(`choose_destination` parcourt `band.springs`) ; or chaque agent ne retient que
**`MAX_KNOWN_SPRINGS` = 16 sources, et oublie la plus éloignée de l'endroit où il se trouve**
quand il en apprend une nouvelle (`Memory::remember_spring`). Les candidats sont donc
toujours les sources autour de la position courante : le pays qu'on a quitté sort de la
mémoire à mesure qu'on avance, et rien ne peut ramener la bande vers son ancien pays. La
carte vécue (`places`, 5 000 lieux de 2 km) garde, elle, le souvenir des lieux — mais sans
source retenue, un lieu n'est plus candidat.

Classement : **modèle** (l'oubli par la distance n'est pas l'oubli réel, qui est par
désusage), en amont du choix de destination.

Deuxième écart, en aval : un lieu seulement vu une heure vaut autant qu'un lieu travaillé
des centaines d'heures (aucune incertitude) — la lisière, vue au meilleur moment, peut
l'emporter sur le cœur connu.

**Question à l'utilisateur** (avant toute prédiction) : quel mécanisme du réel doit
porter le domaine vital ?
**Décision de l'utilisateur (2026-10-24)** : l'oubli par désusage. Une source s'oublie faute
d'avoir été revue, pas parce qu'on s'en est éloigné.

**Le changement (un seul)** : `Memory::spring_seen` garde quand chaque source a été vue pour la
dernière fois ; la parole transmet cet âge (on garde le plus récent) ; au-delà de 16 sources,
on oublie **la moins récemment vue** au lieu de la plus éloignée. La capacité (16, choix non
sourcé) **ne change pas** : si elle borne le domaine, ce sera le changement suivant, mesuré seul.
Test rouge : `le_pays_quitte_ne_s_oublie_pas_parce_qu_on_s_en_eloigne` échoue sur l'ancien
code (l'ancien camp, à 40 km, est oublié).

**Témoin** : `out/camp/longue_r` (`longue` 10 ans, binaire `camp-memoire-50c84ba`, même code de
simulation que `main`), 4 seeds × 2 scènes, départ jour 135 :

| run | levées | retours % | foyer à 5 ans (km) | foyer à 10 ans | max |
|---|---|---|---|---|---|
| f1337 | 212 | 52 | 19 | 25 | 35 |
| f2024 | 507 | 26 | 151 | 269 | 275 |
| f42 | 305 | 43 | 93 | 129 | 136 |
| f7 | 292 | 34 | 127 | 192 | 198 |
| t1337 | 444 | 41 | 147 | 156 | 163 |
| t2024 | 481 | 24 | 473 | 663 | 670 |
| t42 | 447 | 31 | 254 | 354 | 356 |
| t7 | 359 | 26 | 214 | 384 | 385 |

Signature de la dérive : distance finale ≈ maximum (on ne revient jamais).

**Prédictions (écrites avant), même banc, même régime :**
- P1 retours vers un ancien camp : en hausse sur ≥ 6/8 runs, médiane +10 points au moins
  (témoin médiane 32 %). Réfutée si < 5/8 en hausse.
- P2 distance du foyer au départ à 10 ans : en baisse sur ≥ 6/8, médiane −30 % au moins
  (témoin médiane 230 km). Réfutée si < 5/8 en baisse.
- P3 (ce que je crois) : **pas encore de domaine borné** — la capacité de 16 sources, mise en
  commun chaque jour par la bande, couvre le pays de quelques camps ; distance finale / max
  reste ≥ 0,8 sur la plupart des runs. Si le foyer finit au contraire sous 0,6 × son max sur
  ≥ 5/8, le désusage suffit.
- P4 voisins : morts de soif non accrues de plus de 20 % au total (les sources proches restent
  revues à chaque délibération) ; population moyenne des ans 5-10 dans ±15 % du témoin sur ≥ 6/8.
- Seuil de décision : P1 et P2 tenues → garder ; puis, si P3 tenue (pas borné), proposer la
  capacité comme changement suivant. P2 réfutée → le désusage ne porte pas le domaine, relire.

### Débat (2026-10-24, pré-mortem)
1. Pour une bande qui dérive, « la moins récemment vue » et « la plus éloignée » sont presque
   la même source : le changement peut ne rien faire. → **Retenu** : instrument d'abord
   (`longue` : sources connues de la bande la plus nombreuse, leur portée, **anciens camps
   encore candidats** — une source connue dans le lieu de 2 km d'un camp quitté), run d'un an
   sur t42 et f42, témoin contre variante, **avant** la run longue. Si les anciens camps
   candidats ne montent pas, la run de 10 ans n'est pas lancée.
2. Le témoin vient d'un autre binaire. → **Écarté** : le code de simulation de 50c84ba et de
   `main` est le même (seul `scenario.rs` a changé, sortie de `marche` identique) ; de plus le
   binaire témoin + instrument sera reconstruit, et sa première année comparée à `longue_r`.
3. Un va-et-vient A→B→A compte comme retour. → **Retenu** : journal des levées
   (`<out>.camps.csv`), d'où l'on compte les retours vers un camp quitté depuis plus de 30 jours.
4. Distance finale et rapport final/max = un tirage d'une marche aléatoire. → **Retenu** : la
   statistique suit la forme (règle 8) — **déplacement quadratique moyen selon le délai**
   (croît linéairement si diffusion, plafonne si domaine) et **rayon de giration des camps par
   an**, depuis le journal des levées. Prédiction ajoutée : P5 ci-dessous.
5. On se souvient du pays dans l'état où on l'a quitté (épuisé) ; le trajet pèse peu (×0,7 à
   40 km). → **Retenu comme voisin**, non traité ici (un changement, une chose) ; le journal des
   levées garde `attendu` et `trajet` du lieu choisi.
6. Un lieu travaillé seulement dans une autre saison n'est pas candidat. → **Retenu comme
   voisin** ; l'instrument « anciens camps candidats » compte la source, pas la saison : à lire
   avec cette réserve.
7. P4 sous le bruit. → **Retenu** : P4 ne juge que le **sens commun** — dégradation seulement si
   population et morts de soif empirent sur ≥ 7/8.
8. Densité des sources selon le climat. → **Retenu** : `sources_bande` par scène.

**P5 (ajoutée avant la mesure)** : rayon de giration des camps de l'année, ans 6-10 : témoin à
relever dans la même passe ; variante en baisse sur ≥ 6/8. Déplacement quadratique moyen entre
deux camps séparés de 1, 2, 4 ans : si le désusage suffit, il cesse de croître entre 2 et 4 ans
sur ≥ 5/8 (sinon P3 tenue).

### Mesure de contrôle, 2 ans × 8 (binaires `mar6d-temoin` = `main` + instrument, `mar6d-desusage`)
Instrument vérifié : première année du témoin identique à `longue_r` (0 cellule, t42 et f42).

| run | pop T → D | levées T → D | retours % T → D | retours > 30 j % | sources bande (méd.) | portée km | anciens camps candidats méd/max | foyer à 2 ans km | giration an 2 km |
|---|---|---|---|---|---|---|---|---|---|
| t42 | 55 → 56 | 90 → 141 | 23 → 16 | 3 → 2 | 33 → 52 | 6,6 → 11,3 | 1/6 → 1/12 | 93 → 57 | 23 → 31 |
| t7 | 55 → 37 | 85 → 137 | 18 → 43 | 1 → 18 | 23 → 22 | 6,2 → 7,6 | 1/10 → 3/12 | 112 → 31 | 15 → 7 |
| t1337 | 59 → 58 | 113 → 143 | 20 → 21 | 4 → 1 | 46 → 47 | 8,0 → 10,5 | 1/7 → 1/7 | 65 → 113 | 19 → 29 |
| t2024 | 57 → 52 | 142 → 164 | 20 → 18 | 1 → 1 | 31 → 56 | 7,8 → 11,5 | 1/6 → 1/7 | 205 → 214 | 38 → 29 |
| f42 | 38 → 51 | 85 → 138 | 32 → 22 | 5 → 3 | 25 → 35 | 6,6 → 9,7 | 1/9 → 2/12 | 69 → 102 | 4 → 15 |
| f7 | 52 → 80 | 98 → 175 | 24 → 23 | 4 → 5 | 25 → 32 | 6,9 → 8,9 | 1/9 → 2/12 | 66 → 165 | 9 → 38 |
| f1337 | 63 → 72 | 72 → 109 | 31 → 37 | 18 → 15 | 28 → 33 | 5,1 → 7,3 | 1/13 → 3/21 | 30 → 20 | 6 → 5 |
| f2024 | 76 → 82 | 139 → 127 | 22 → 17 | 9 → 2 | 29 → 40 | 7,7 → 10,3 | 2/10 → 2/9 | 99 → 151 | 11 → 38 |

Lecture (seuil du pré-mortem : la run de 10 ans n'est lancée que si les anciens camps restent
candidats) :
- **Le changement agit sur la mémoire** : la portée des sources connues monte sur 8/8 (+25 à
  +70 %), leur nombre sur 7/8.
- **Mais pas sur le retour** : anciens camps candidats en médiane 1-3, comme le témoin ;
  retours en hausse sur 3/8 seulement, retours de plus de 30 jours sur 2/8. P1 réfutée en
  avant-première.
- **Il fait bouger davantage** : levées en hausse sur 7/8 (+15 à +79 %), foyer plus loin sur
  5/8, giration plus grande sur 5/8. P2 réfutée en avant-première. Plus de lieux connus, c'est
  plus de lieux où partir — vers la lisière, pas vers l'ancien pays.
- La population, elle, monte en froid (4/4 : 38 → 51, 52 → 80…) et baisse en tempéré sur 2/4
  (t7 55 → 37) : à ne pas lire sur 2 ans.
- **La run de 10 ans n'est pas lancée.** Le témoin le disait : « sources retenues 16/16/16 »
  (acceptation 5 ans) — chacun est toujours au plafond ; un camp sur cinq jours apporte assez
  de sources neuves pour pousser les anciennes dehors, quel que soit l'ordre d'oubli.

### Sensibilité à la capacité (exploration, binaire `mar6d-cap256` : désusage + 256 sources, non committé)
Même banc, 2 ans × 8.

| run | levées T / D / 256 | retours % | retours > 30 j % | anciens camps candidats méd/max | foyer à 2 ans km | pop |
|---|---|---|---|---|---|---|
| t42 | 90 / 141 / **208** | 23 / 16 / **35** | 3 / 2 / 9 | 1 / 1 / **16**/36 | 93 / 57 / **34** | 55 / 56 / 53 |
| t7 | 85 / 137 / **262** | 18 / 43 / **52** | 1 / 18 / 11 | 1 / 3 / **25**/51 | 112 / 31 / **9** | 55 / 37 / 46 |
| t1337 | 113 / 143 / **190** | 20 / 21 / **28** | 4 / 1 / 4 | 1 / 1 / **6**/23 | 65 / 113 / 115 | 59 / 58 / 56 |
| t2024 | 142 / 164 / **296** | 20 / 18 / **34** | 1 / 1 / 4 | 1 / 1 / **10**/27 | 205 / 214 / **32** | 57 / 52 / 52 |
| f42 | 85 / 138 / **234** | 32 / 22 / **44** | 5 / 3 / 12 | 1 / 2 / **20**/46 | 69 / 102 / — | 38 / 51 / **4** |
| f7 | 98 / 175 / **459** | 24 / 23 / **41** | 4 / 5 / 7 | 1 / 2 / **19**/37 | 66 / 165 / 109 | 52 / 80 / 69 |
| f1337 | 72 / 109 / **181** | 31 / 37 / **58** | 18 / 15 / 23 | 1 / 3 / **21**/35 | 30 / 20 / 18 | 63 / 72 / 64 |
| f2024 | 139 / 127 / **263** | 22 / 17 / **30** | 9 / 2 / 5 | 2 / 2 / **14**/38 | 99 / 151 / 78 | 76 / 82 / 63 |

Lecture :
- **Avec une mémoire assez grande, le domaine commence à se voir** : les anciens camps restent
  candidats (médiane 6-25 contre 1-3), les retours montent sur 8/8 (28-58 %), le foyer finit
  plus près du départ sur 5/7 (t7 112 → 9 km, t2024 205 → 32).
- **Mais la bande lève le camp sans cesse** : 90-230 levées par an (réel : Batek ~44, Hadza ~12),
  et f42 s'effondre (60 → 4 : 31 morts de faim, 15 de froid, la hutte toujours laissée
  derrière). Avec beaucoup de lieux connus, il s'en trouve presque toujours un qui « promet
  mieux » : la règle de départ (MAR-6b c) ne compte ni la journée de déménagement ni
  l'installation (rebâtir l'abri), seulement un facteur de trajet.
- Débit : 25 ticks/s contre 41 en t42 (la parole échange 256 sources).
- Conclusion : le désusage est nécessaire mais pas suffisant ; la capacité borne le domaine ;
  et dès qu'elle ne le borne plus, c'est le **coût du départ** qui manque. **Question à
  l'utilisateur.**

**Décision de l'utilisateur (2026-10-24, soir)** : d'abord la capacité d'après le réel, puis le
coût du départ, chacun mesuré seul. Capacité retenue : **256** (sources : > 50 points d'eau
utilisés par un homme, > 1 200 connus d'un groupe, retrouvés après 50-60 ans — cognition,
SOURCES). Le binaire `mar6d-cap256` est exactement ce code : la mesure de 2 ans × 8 ci-dessus
vaut pour lui. L'oubli au fil du temps n'est pas ajouté : à 50-60 ans de souvenir, il serait
sans effet sur l'horizon des bancs.

## MAR-6d, étape 2 — le coût du départ (2026-10-25)

**Lecture** (`camp.rs`, `choose_destination` et `daily`) : on part si `attendu là-bas ×
1/(1 + d / 96 km) > rendu ici`. Ce facteur est celui de la valeur marginale (Charnov) avec un
séjour d'**un jour** : il ne compte ni l'installation, ni la durée pendant laquelle le
nouveau lieu rendra mieux. L'abri, lui, se bâtit en un tick sur la réserve (6 unités).

**Sources** (sous-agent `sourceur`, 2026-10-25, tout de seconde main) : installer un camp
hadza, « moins de deux heures » (les femmes) ; tente évenk 10-15 min ; igloo 1-6 h. Levées par
an : Hadza 7, Ju/'hoansi 5,5, moyenne de la base de Marlowe 7,4, Aché 75-100 (Marlowe, via
résumé) ; Hadza : un camp toutes les 2 à 5 semaines (Marlowe 2010). Rien sur la journée de
déménagement elle-même.

**Le changement (un seul)** : la valeur marginale avec le séjour attendu et le coût du départ.
Pour chaque lieu connu : gain = (attendu là-bas − rendu ici) × heures de subsistance par jour
× séjour attendu ; coût = rendu ici × (heures de trajet + **2 h** d'installation). On part vers
le lieu au meilleur gain net, s'il est positif. Le **séjour attendu** est la moyenne des
séjours de la bande elle-même (jours depuis sa fondation / camps) ; les **heures de subsistance
par jour** sont celles de ses adultes (`Yields`, le mois). Le facteur de trajet disparaît (le
coût du trajet le remplace). Choix non sourcé : 2 h (borne haute hadza).

**Prédictions (écrites avant)**, 2 ans × 8, témoin `mar6d-cap256` (ca81a78) :
- P6 levées en baisse sur ≥ 6/8, de −20 à −50 % (témoin 181-459 en 2 ans). Réfutée si < 5/8.
- P7 les anciens camps restent candidats (médiane ≥ 5 sur ≥ 6/8) et les retours ne baissent
  pas de plus de 10 points sur ≥ 6/8.
- P8 (ce que je crois) : les levées restent bien au-dessus du réel (> 40 par an) : la
  rétroaction va dans le mauvais sens — plus les séjours s'allongent, plus le gain attendu
  d'un départ grandit. Si les levées tombent sous 40 par an sur ≥ 5/8, je me trompe.
- Seuil : P6 tenue → garder ; P6 réfutée → le coût du départ n'est pas ce qui manque, relire
  (candidat : la malédiction du gagnant — avec 256 lieux estimés, le meilleur l'emporte
  presque toujours sur ici par le seul bruit des estimations).

**Mesure** (2 ans × 8, `mar6d-cout` contre `mar6d-cap256`) :

| run | levées 256 → coût | retours % | anciens camps cand. méd | foyer à 2 ans km | pop | morts faim ; froid (256 → coût) |
|---|---|---|---|---|---|---|
| t42 | 208 → 152 | 35 → 45 | 16 → 28 | 34 → 47 | 53 → 57 | 0 ; 0 → 0 ; 0 |
| t7 | 262 → 334 | 52 → 46 | 25 → 13 | 9 → 88 | 46 → 54 | — |
| t1337 | 190 → 194 | 28 → 34 | 6 → 13 | 115 → 53 | 56 → 54 | — |
| t2024 | 296 → 344 | 34 → 29 | 10 → 14 | 32 → 77 | 52 → 61 | — |
| f42 | 234 → 306 | 44 → 33 | 20 → 20 | — → 61 | 4 → 30 | 31 ; 15 → 18 ; 0 |
| f7 | 459 → 312 | 41 → 41 | 19 → 27 | 109 → 40 | 69 → 65 | 2 ; 0 → 2 ; 0 |
| f1337 | 181 → 147 | 58 → 38 | 21 → 22 | 18 → 34 | 64 → 45 | 5 ; 0 → 8 ; 0 |
| f2024 | 263 → 219 | 30 → 17 | 14 → 15 | 78 → 88 | 63 → 85 | 3 ; 1 → 0 ; 0 |

- **P6 réfutée** : levées en baisse sur 4/8 seulement (73-172 par an, réel 5-100).
- P7 tenue : anciens camps candidats ≥ 13 en médiane sur 7/8 ; retours en baisse de plus de
  10 points sur 2/8 seulement.
- P8 tenue : bien au-dessus de 40 levées par an partout.
- Le froid ne tue plus en f42 (15 → 0), mais sur une seule seed et 2 ans.
- **Verdict** : le coût du départ, à ce qu'il coûte réellement (trajet et 2 h), n'est pas ce qui
  fait partir la bande. Changement **annulé** (commit puis retour, pour l'histoire) selon le
  seuil écrit avant. Deux réfutations de suite (étape 1 : P1, P2 ; étape 2 : P6) → contre-lecture.

### Débat (2026-10-25, contre-lecture après deux réfutations)
1. La porte de départ est saturée : on part dès que la preuve le permet. → **Retenu, vérifié**
   sur les journaux existants : 67 % des levées à 1-2 jours de la précédente (cap256 ; témoin
   42 %). La question est pourquoi le meilleur lieu gagne dès la 2ᵉ nuit.
2. « Ici » et « là-bas » ne sont pas jugés sur la même grandeur (ici : rendement récent d'une
   maille qu'on épuise ; là-bas : repoussé vers la saison ; ici : la seule maille du foyer).
   → **Retenu** : à mesurer (contrefactuels ci-dessous).
3. La queue de la chasse : une prise sur un lieu peu travaillé donne un attendu énorme. →
   **Retenu, vérifié** : attendu du lieu retenu, médiane ~1 300 kcal/h, p90 5 700-8 200, max
   178 771.
4. La malédiction du gagnant (ma candidate) : signature à vérifier (rendu réalisé < attendu,
   écart qui décroît avec les heures vécues). → **Retenu** dans la même mesure.
5. Lieux seulement vus : **vérifié**, 4-5 % des levées seulement — écarté comme cause.
6. L'étape 2 ne pouvait pas tester le coût réel (le départ ne coûte presque rien dans le
   modèle : l'abri en un tick). → **Retenu** : noté ; rendre le départ physiquement coûteux
   reste un axe (question à l'utilisateur plus tard).
7. « < 40 levées par an » frôle le réglage vers un critère. → **Retenu** : on juge la cause par
   les contrefactuels, pas par un nombre de levées visé.
8. Changement de quart d'année : à vérifier dans la même mesure.

### Contrefactuels (instrument d'observation, `Sim::camp_nights`, `<out>.nuits.csv`)
Chaque nuit où la bande peut juger : le camp jugé comme aujourd'hui, sur sa saison, sur le
rayon de résidence ; le meilleur et le deuxième score ; pour le meilleur lieu, ses heures
vécues et la part de la chasse ; et si l'on partirait encore sous chaque règle : ici sur sa
saison (`alt_saison`), ici sur le rayon (`alt_rayon`), hors chasse des deux côtés
(`alt_hors_chasse`), lieux travaillés seuls (`alt_travailles`), attendu moins ~attendu/√h
(`alt_marge`).

**Prédictions (avant de lire)**, 2 ans × 8, comparées aux nuits où l'on part aujourd'hui :
- C1 lieux travaillés seuls : quasi pas de baisse (< 10 %) — 5 % des levées vont vers un lieu vu.
- C2 hors chasse : la plus forte baisse, −40 à −70 % des départs, dans le même sens sur 8/8.
- C3 ici sur sa saison : baisse de −20 à −50 %.
- C4 ici sur le rayon : baisse plus faible que C3.
- C5 marge d'incertitude : baisse de −10 à −40 % ; et le rendu réalisé au camp suivant
  (le « ici » de la première nuit jugée après une levée) inférieur à l'attendu de plus de 30 %
  en médiane.
- La cause est celle dont la règle fait le plus tomber les départs, dans le même sens sur
  4 seeds × 2 scènes ; ensuite, un seul changement, à proposer à l'utilisateur.

**Mesure** (2 ans × 8, binaire `mar6d-nuits` = ca81a78 + instrument ; sorties identiques à
`mar6d-cap256` sur 8/8, hors débit et mémoire). Variation des départs sous chaque règle :

| run | nuits | départs | travaillés seuls | hors chasse | ici sur sa saison | ici sur le rayon | marge 1/√h | réalisé / attendu (méd.) |
|---|---|---|---|---|---|---|---|---|
| t42 | 537 | 208 | −1 % | +22 % | +1 % | +156 % | **−48 %** | 0,87 |
| t7 | 894 | 262 | −1 % | +82 % | −31 % | +197 % | **−39 %** | 0,85 |
| t1337 | 565 | 190 | −1 % | +44 % | −6 % | +172 % | **−52 %** | 0,87 |
| t2024 | 644 | 296 | −4 % | +46 % | −17 % | +98 % | **−45 %** | 0,72 |
| f42 | 491 | 234 | −2 % | +28 % | +1 % | +94 % | **−38 %** | 0,81 |
| f7 | 1290 | 459 | −5 % | +71 % | −15 % | +136 % | **−42 %** | 0,81 |
| f1337 | 781 | 181 | −2 % | +112 % | +11 % | +269 % | **−33 %** | 0,89 |
| f2024 | 512 | 263 | −0 % | +32 % | −13 % | +81 % | **−35 %** | 0,70 |

Rendu réalisé au camp suivant / attendu, selon les heures vécues du lieu retenu (8 runs) :
< 20 h 0,76 (788 levées) ; 20-100 h 0,73 (481) ; 100-500 h 0,89 (645) ; > 500 h 0,95 (173).

- C1 tenue (−0 à −5 %).
- **C2 réfutée, à l'envers** : sans la chasse, on partirait **plus** (+22 à +112 %) — la chasse
  gonfle « ici » plus que « là-bas ». La queue de la chasse (point 3) n'est pas la cause.
- C3 réfutée (−31 à +11 %, sens variable).
- C4 réfutée à l'envers : la maille du foyer rend **mieux** que le rayon (+81 à +269 % de départs
  si l'on jugeait ici sur le rayon) — pas de biais d'« ici » trop bas.
- **C5 tenue en sens, 8/8** : la marge d'incertitude fait tomber les départs de 33 à 52 %, la
  plus forte baisse partout. Le rendu réalisé est sous l'attendu (0,70-0,89, prédiction
  « > 30 % » tenue sur 2/8 seulement), et **l'écart disparaît quand le lieu est bien connu**
  (0,76 sous 20 h vécues, 0,95 au-delà de 500 h) : c'est la signature de la malédiction du
  gagnant — le meilleur de ~250 estimations est d'abord celui qu'on surestime.
- **Cause retenue : un lieu peu vécu est jugé comme s'il était connu.** Le changement à faire
  est l'incertitude dans la valeur d'un lieu — l'option « désusage + incertitude » proposée
  le 2026-10-24. **Question à l'utilisateur** (la forme).

## MAR-6d, étape 3 — juger un lieu selon ce qu'on en sait (2026-10-25)

**Décision de l'utilisateur (2026-10-25)** : option 1, l'estimation tirée vers la moyenne ;
« si ce n'est pas suffisant, on refait une analyse et on met l'option 2 (la prudence) en face
d'une autre possibilité ».

**Le changement (un seul)** : la valeur d'un lieu travaillé (et celle du camp, même règle) est
`(kcal vécues + moyenne de la bande × 100 h) / (heures vécues + 100 h)`. La moyenne : ce que
rendent les lieux travaillés de la bande, à ce moment de l'année. Les lieux seulement vus ne
changent pas (5 % des levées). Choix non sourcé : 100 h, lu sur la courbe d'avant (biais fort
sous 100 h, presque nul au-delà de 500 h), pas réglé sur un nombre de levées.

**Prédictions (avant)**, 2 ans × 8, contre `mar6d-nuits` (= cap256) :
- P9 levées en baisse sur ≥ 6/8, de −25 à −50 %. Réfutée si < 5/8 en baisse.
- P10 rendu réalisé / attendu plus proche de 1 : ≥ 0,85 en médiane sur ≥ 6/8 (avant 0,70-0,89).
- P11 anciens camps candidats et retours : pas de baisse de plus de 10 points sur ≥ 6/8.
- P12 voisins : dégradation seulement si population et morts de faim empirent ensemble sur ≥ 7/8.
- Seuil : P9 tenue → garder et passer à 10 ans ; P9 réfutée → nouvelle analyse.

**Mesure** (2 ans × 8, `mar6d-moyenne` contre `mar6d-nuits` = cap256) :

| run | levées | retours % | retours > 30 j % | anc. camps cand. | foyer 2 ans km | giration an 2 km | réalisé/attendu | pop | faim ; froid |
|---|---|---|---|---|---|---|---|---|---|
| t42 | 208 → 176 | 35 → 68 | 9 → 29 | 16 → 24 | 34 → 11 | 9,4 → 6,5 | 0,82 → 0,90 | 53 → 53 | 0 ; 0 → 0 ; 0 |
| t7 | 262 → 176 | 52 → 57 | 11 → 17 | 25 → 18 | 9 → 24 | 12,1 → 9,5 | 0,79 → 0,81 | 46 → 49 | 0 ; 0 → 1 ; 0 |
| t1337 | 190 → 100 | 28 → 53 | 4 → 12 | 6 → 8 | 115 → 37 | 25,9 → 10,3 | 0,82 → 0,94 | 56 → 56 | 0 ; 0 → 0 ; 0 |
| t2024 | 296 → 256 | 34 → 36 | 4 → 11 | 10 → 14 | 32 → 45 | 26,3 → 5,0 | 0,68 → 0,88 | 52 → 59 | 0 ; 0 → 0 ; 0 |
| f42 | 234 → 154 | 44 → 52 | 12 → 8 | 20 → 16 | — → 70 | 10,4 → 5,7 | 0,76 → 0,83 | 4 → 28 | 31 ; 15 → 31 ; 0 |
| f7 | 459 → 235 | 41 → 57 | 7 → 17 | 19 → 29 | 109 → 37 | 23,5 → 8,4 | 0,76 → 0,79 | 69 → 50 | 2 ; 0 → 5 ; 0 |
| f1337 | 181 → 91 | 58 → 52 | 23 → 12 | 21 → 5 | 18 → 38 | 6,1 → 5,0 | 0,85 → 0,95 | 64 → 51 | 5 ; 0 → 12 ; 1 |
| f2024 | 263 → 108 | 30 → 44 | 5 → 19 | 14 → 20 | 78 → 12 | 20,4 → 6,5 | 0,65 → 0,92 | 63 → 71 | 3 ; 1 → 0 ; 0 |

(réalisé/attendu : le « ici » de la première nuit jugée au nouveau camp sur l'attendu du
journal des levées, les deux estimés par la même règle.)
- **P9 tenue** : levées en baisse sur 8/8 (−14 à −59 %), soit 45-128 par an (Aché 75-100).
- P10 à moitié : réalisé/attendu en hausse sur 8/8, ≥ 0,85 sur 5/8 (prédit 6/8).
- P11 tenue : retours en hausse sur 7/8 ; anciens camps candidats en baisse de plus de 10 sur
  f1337 seulement.
- P12 : pas de dégradation commune (population en baisse sur 3/8, en hausse sur 4/8 ; faim en
  hausse sur f7 et f1337 ; le froid ne tue plus en f42).
- **Le domaine se dessine** : giration de l'an 2 en baisse sur 7/8 (26 → 5 km en t2024,
  24 → 8 en f7), retours de plus de 30 jours en hausse sur 6/8.
- Seuil : P9 tenue → gardé ; mesure à 10 ans × 8 contre le témoin `main` (`longue_r`).

**Prédictions à 10 ans × 8 (écrites avant)**, `mar6d-moyenne` contre `longue_r` (`main`) :
- P13 distance finale du foyer au départ en baisse sur ≥ 6/8, médiane ≤ 100 km (témoin 230).
- P14 un domaine borné : distance finale / distance max ≤ 0,6 sur ≥ 5/8 (témoin 0,7-1,0, presque
  toujours ≥ 0,95) — la bande revient.
- P15 voisins : population moyenne des ans 5-10 et morts de faim ne se dégradent pas ensemble
  sur ≥ 7/8.

**Mesure à 10 ans × 8** (`mar6d-moyenne`, 44f92b1, contre `longue_r` = `main`, départ jour 135) :

| run | foyer fin / max km, témoin | variante | pop moy. ans 5-10 | morts de faim | levées (10 ans) | giration ans 6-10 km |
|---|---|---|---|---|---|---|
| t42 | 354 / 356 | **32 / 39** | 65 → 58 | 0 → 0 | 447 → 990 | 5,1 |
| t7 | 384 / 385 | **42 / 65** | 59 → 47 | 0 → 1 | 359 → 782 | 12,6 |
| t1337 | 156 / 163 | 126 / 132 | 62 → 69 | 0 → 0 | 444 → 602 | 10,0 |
| t2024 | 663 / 670 | **20 / 55** | 65 → 56 | 10 → 0 | 481 → 590 | 4,9 |
| f42 | 129 / 136 | 88 / 108 | 29 → 26 | 40 → 34 | 305 → 700 | 9,6 |
| f7 | 192 / 198 | 122 / 127 | 56 → 36 | 4 → 10 | 292 → 746 | 7,4 |
| f1337 | 25 / 35 | 32 / 41 | 49 → 46 | 9 → 15 | 212 → 517 | 4,2 |
| f2024 | 269 / 275 | **34 / 36** | 76 → 72 | 16 → 0 | 507 → 401 | 6,9 |

- **P13 tenue** : distance finale en baisse sur 7/8, médiane 38 km (témoin 230).
- **P14 réfutée telle qu'écrite** : fin/max ≤ 0,6 sur 2/8 seulement. Mais le maximum lui-même
  est borné (36-132 km contre 35-670) : la bande ne revient pas forcément à son point de départ,
  elle **cesse de s'en éloigner**. Le rapport fin/max était une mauvaise statistique pour un
  domaine qui se pose ailleurs que le départ ; la giration des camps (ans 6-10 : **4 à 13 km**,
  soit ~50-500 km² — Baka 205 ± 109) dit le domaine.
- **P15 tenue au sens écrit** (population en baisse **et** faim en hausse ensemble sur 3/8), **mais
  la population des ans 5-10 baisse sur 6/8** (−5 à −36 %, f7 56 → 36) : à expliquer avant tout
  report.
- **Les levées ont doublé par rapport à `main`** (40-99 par an contre 21-51) : la bande bouge
  plus, mais dans un pays à elle. Le réel : Hadza 7, Ju/'hoansi 5,5, Aché 75-100.
- D'où vient la baisse de population (10 ans, témoin → variante) : **moins de naissances** sur
  6/8 (57 → 47, 51 → 40, 74 → 60…) et **plus de morts violentes** sur 3/8 (t7 0 → 7, t2024 2 → 11,
  f2024 0 → 8) ; la soif et la faim varient dans les deux sens. Une bande qui reste dans son pays
  y croise plus souvent ses voisins (D13) ; la natalité plus basse n'est pas lue (deux fois plus
  de levées : effort, nourrissons portés ?). Voisins à lire avant le report.

### Débat (2026-10-25, contre-lecture du jalon)
1. La mesure à 10 ans mêle trois changements (désusage, 256, moyenne) ; le doublement des levées
   vient sans doute du 256, et la baisse de population n'est attribuée à aucun. → **Retenu** :
   10 ans × 8 sur `mar6d-cap256`.
2. Pas de giration ni de déplacement quadratique du témoin ; un maximum de 108-132 km peut être
   une diffusion plus lente ; « 50-500 km² » mélange dispersion des camps et aire parcourue.
   → **Retenu** : 10 ans × 8 du témoin avec instrument (`mar6d-temoin`), déplacement quadratique
   à 1, 2, 4, 8 ans sur les journaux des levées des trois bras ; la comparaison aux Baka est
   retirée tant qu'elle n'est pas faite sur la même grandeur.
3. P14 réinterprétée après lecture. → **Retenu** : « la bande cesse de s'éloigner » devient une
   prédiction (P16 ci-dessous), jugée sur le déplacement quadratique.
4. Moins de naissances et plus de violence : un décompte, pas une cause ; autre explication :
   l'estimation tire toutes les bandes vers les mêmes lieux réputés. → **Retenu** : instrument
   (naissances par femme fertile et par an, rencontres entre bandes) avant le report.
5. `PRIOR_HOURS` ajusté sur son propre symptôme ; moyenne pondérée par les heures (dominée par
   les lieux épuisés) ; la queue de la chasse reste (1 h vécue à 178 771 → ~3 000). → **Retenu** :
   sensibilité à 50 et 200 h (2 ans × 8) ; l'estimation séparée cueillette / chasse et le poids
   déduit des variances sont des axes, à proposer à l'utilisateur.
6. Tests rouges : la surchasse mesure une zone fixe que des chasseurs plus mobiles quittent ; les
   curieux se noient dans le partage des sources en 14 jours. → **Retenu** : à lire (positions du
   camp, écart curieux − ternes sur 4 seeds, deux bras), jamais en baissant les seuils.
7. P10 circulaire (l'attendu est rétréci). → **Retenu** : P10 retirée ; à remplacer par le
   rendement brut au nouveau camp contre les derniers jours de l'ancien.
8. Régime : jour 135, 10 ans, f42 trop petit pour renseigner. → Noté.
**Le report sur `main` attend les points 1, 2, 4 et 6.**

**P16 (écrite avant la mesure du témoin)** : déplacement quadratique moyen des camps entre deux
dates séparées de 1, 2, 4, 8 ans. Témoin : croît à peu près linéairement (diffusion). Variante
`moyenne` : plateau — le rapport MSD(8 ans) / MSD(4 ans) ≤ 1,3 sur ≥ 5/8 (≈ 2 pour une diffusion).

**Témoin avec instrument, 10 ans × 8** (`mar6d-temoin`) — déplacement quadratique des camps
(√MSD, km, à 1 / 2 / 4 / 8 ans ; rapport MSD 8 ans / 4 ans) :

| run | témoin | variante `moyenne` |
|---|---|---|
| t42 | 55 / 100 / 196 / 321 ; 2,69 | 8 / 9 / 7 / — (clan recomposé) |
| t7 | 43 / 80 / 152 / 303 ; 4,00 | 23 / 21 / 28 / 27 ; **0,97** |
| t1337 | 48 / 81 / 132 / 159 ; 1,44 | 24 / 32 / 38 / 75 ; 3,89 |
| t2024 | 77 / 147 / 284 / 537 ; 3,59 | 16 / 20 / 28 / 29 ; **1,05** |
| f42 | 19 / 28 / 34 / 71 ; 4,45 | 19 / 21 / 24 / 35 ; 2,11 |
| f7 | 26 / 43 / 86 / 148 ; 2,94 | 19 / 33 / 60 / 91 ; 2,25 |
| f1337 | 16 / 19 / 19 / 14 ; 0,57 | 10 / 11 / 11 / 16 ; 1,96 |
| f2024 | 31 / 55 / 108 / 199 ; 3,42 | 14 / 14 / 14 / 14 ; **1,03** |

- Le témoin diffuse (et plus vite qu'une marche aléatoire : rapports 2,7-4,5, sauf f1337).
- **P16 réfutée** : plateau sur 3/7 seulement (t7, t2024, f2024). Mais l'échelle tombe de 3 à
  20 fois à 4 ans sur 7/8 (t2024 : 284 → 28 km) : **la bande diffuse beaucoup plus lentement, et
  se pose dans un pays sur 3 runs** ; elle ne se pose pas encore partout.

**Acceptation 5 ans × 8 de la branche** (35010e8, `out/mar6d/acc/`) contre `main` (832a803) :
- Population finale plus basse sur 6/8 (61 → 48, 61 → 51, 56 → 36…), **naissances en baisse sur
  7/8** (31 → 28, 30 → 26, 37 → 31…).
- **Débit plancher sous 20 ticks/s sur 4/8** (17,4-19,7 ; `main` 24-73) : la parole échange 256
  sources par paire et par jour (`exchange_knowledge`, `remember_spring` en O(n)).
- Curiosité ↔ territoire connu : r positif sur 8/8 (0,13-0,76 ; `main` NaN-0,47) — le critère
  CUR se lit mieux à l'échelle du banc, alors que le test court tombe sur 1337.
- Marche 21-28 km/j, inchangée ; feu en froid 2/4 (`main` 3/4).
- **Report bloqué** par la natalité (non expliquée) et le débit.

**Coût de la parole à 256 sources** : `remember_spring` cherchait chaque source entendue dans la
liste (O(n)) ; elle regarde d'abord l'index des âges. Sorties de `longue` 2 ans identiques
(t42, f42, hors débit et mémoire) ; débit sur 30 j, minimum 15,5 → 18,2 et 14,5 → 18,6,
médiane 19,9 → 24,8 et 48,3 → 65,1 (un autre banc tournait en parallèle dans les deux cas).
Le minimum reste sous 20 : le reste du coût n'est pas lu (à profiler, PERF).

**Profil par phase** (feature `profile`, `longue` 1 an t42 ; `main` contre la branche e3bf7e8) :
`3t echanges` 2,3 s → **80 s**, `5 ecologie` 50 → 128 s, `2 execution` 69 → 44 s,
`1 deliberation` 44 → 36 s. La parole coûte ~37 ms par passe (toutes les 4 h) : chaque paire à
portée se redit ses 256 sources, ~600 000 rappels par passe. L'écologie ne lit pas la mémoire :
son doublement vient de la trajectoire (à lire dans PERF).
**Essai annulé** : sauter les paires qui savent exactement la même chose. Ni gain (75 s), ni
résultat identique (t42 diffère au jour 253) : une source apprise d'un premier interlocuteur peut
en évincer une autre, qu'un second réapprend — l'ordre des paroles compte. Le seul remède serait
de changer la règle de l'échange (union des savoirs entendus, âge le plus récent, puis oubli des
plus anciennes) : un changement de comportement, à mesurer comme tel.

**Décision de l'utilisateur (2026-10-25, soir)** : tout consigner ; tester la nouvelle règle
d'échange des sources ; si elle ne suffit pas, le débit va au chantier PERF, suivi comme problème
vu. D'accord pour le reste (natalité à instrumenter, report bloqué jusque-là).

## MAR-6d, étape 4 — l'échange des sources : on réunit ce qu'on entend (2026-10-25)

**Le changement (un seul)** : à chaque passe de parole, chacun réunit les sources de tous ses
interlocuteurs (en gardant pour chacune l'âge le plus récent), les ajoute à ce qu'il sait, puis
oublie les moins récemment vues au-delà de 256. Avant : il les apprenait une à une, dans l'ordre
des paires, chaque nouvelle pouvant en évincer une autre — ~600 000 rappels par passe.
**Prédictions (avant)**, 2 ans × 8 contre `mar6d-moyenne` (= e3bf7e8, sorties identiques) :
- P17 la phase `3t echanges` baisse de 60 à 85 % (profil, t42, 1 an).
- P18 débit plancher sur 30 j ≥ 20 ticks/s sur ≥ 6/8 (avant : 14,5-15,5 sur t42/f42).
- P19 comportement : pas de décalage de même sens sur ≥ 6/8 pour les levées, les retours, la
  population et les sources connues de la bande (la règle ne change que l'ordre des oublis).

**Mesure** (2 ans × 8, `mar6d-echange` contre `mar6d-moyenne` ; **régime : trois runs sur deux
cœurs**, les débits ne sont pas comparables) :
- P19 tenue : levées, retours, population et sources de la bande **identiques sur 7/8** ; f2024
  s'écarte (levées 108 → 132, population 71 → 82) — la règle ne change que l'ordre des oublis.
- P17 : `3t echanges` 80 → 48 s par an (profil t42, 1 an) sous une charge plus forte ; rapportée
  à `2 execution` (même charge), la parole coûte −55 %. À refaire seul sur la machine (P18 aussi).
- Gardé (comportement inchangé, règle plus simple : ce qu'on retient ne dépend plus de l'ordre
  des paroles). Le débit reste un **problème ouvert** si la mesure seule ne passe pas 20.

### La natalité, porte par porte (instrument, 2026-10-26)
`longue` relève chaque jour, pour les femmes en âge de concevoir (15-45 ans), la première porte
de `demography::daily` qui les retient : enceinte, allaite un nourrisson, santé ≤ 0,6, faim ≥ 0,85,
soif ≥ 0,9, aucun homme fécond à 400 m ; sinon éligible (lecture seule). Mesure : 5 ans × 8,
`main` (832a803 + instruments) contre la branche (étape 4), lancée quand les runs de 10 ans
libèrent la machine (`out/mar6d/natal/`).
**Prédiction (avant)** : la porte qui se ferme plus souvent sur la branche est « aucun homme à
400 m » (une bande qui lève le camp deux fois plus souvent se disperse davantage à minuit) ;
écart de jours-femmes éligibles ≥ 10 % sur ≥ 6/8. Autre candidate : la santé (plus de malades).

**Les trois bras à 10 ans × 8** (témoin `main` · 256 seule `mar6d-cap256` · étape 3 `mar6d-moyenne`) :

| run | foyer fin/max km | pop moy. ans 5-10 | naissances | levées | morts violentes |
|---|---|---|---|---|---|
| t42 | 354/356 · 253/282 · **32/39** | 65 · 68 · 58 | 57 · 59 · 47 | 447 · 984 · 990 | 0 · 0 · 0 |
| t7 | 384/385 · 117/124 · **42/65** | 59 · 45 · 47 | 45 · 38 · 40 | 359 · 1053 · 782 | 0 · 5 · 7 |
| t1337 | 156/163 · 234/260 · 126/132 | 62 · 63 · 69 | 54 · 54 · 58 | 444 · 882 · 602 | 0 · 0 · 0 |
| t2024 | 663/670 · 373/391 · **20/55** | 65 · 60 · 56 | 60 · 60 · 63 | 481 · 1104 · 590 | 2 · 5 · 11 |
| f42 | 129/136 · éteinte à 4,6 ans · 88/108 | 29 · 0 · 26 | 32 · 13 · 27 | 305 · 234 · 700 | 0 · 0 · 0 |
| f7 | 192/198 · 280/284 · 122/127 | 56 · 65 · 36 | 51 · 56 · 40 | 292 · 1818 · 746 | 11 · 6 · 7 |
| f1337 | 25/35 · 22/44 · 32/41 | 49 · 42 · 46 | 52 · 46 · 52 | 212 · 880 · 517 | 0 · 10 · 0 |
| f2024 | 269/275 · 115/157 · **34/36** | 76 · 52 · 72 | 74 · 64 · 60 | 507 · 1046 · 401 | 0 · 1 · 8 |

Attribution (point 1 de la contre-lecture) :
- **Le doublement des levées vient de la capacité 256** (882-1818 levées en 10 ans, 2 à 6 fois
  le témoin) ; l'étape 3 les ramène à 401-990 (en baisse sur 7/8 contre 256 seule).
- **La capacité seule ne fait pas de domaine** (foyer à 115-373 km) ; c'est l'étape 3 qui pose la
  bande.
- **La baisse de population n'a pas de sens commun entre les étapes** : 256 seule la baisse sur
  5/8 (et éteint f42), l'étape 3 la remonte sur 5/8 par rapport à 256 seule. Contre le témoin,
  la variante finale reste plus basse sur 6/8 ; naissances plus basses sur 5/8.
- Morts violentes : en hausse avec 256 seule sur 4/8, avec l'étape 3 sur 3/8 — pas d'effet propre
  de l'étape 3 lisible ici.

**Mesure** (5 ans × 8, `natal-main` = 832a803 + instruments contre `natal-branche` = 7167e75) — part
des jours-femmes (15-45 ans) retenus par chaque porte, 8 runs réunis :

| | enceinte | allaite | santé | faim | soif | pas d'homme à 400 m | éligible | naissances |
|---|---|---|---|---|---|---|---|---|
| `main` | 29,6 % | 59,4 % | 0,0 % | 0,3 % | 0,1 % | 2,4 % | **8,2 %** | 267 |
| branche | 29,8 % | 57,3 % | 0,0 % | 0,4 % | 0,1 % | 3,0 % | **9,4 %** | 245 |

- **Prédiction réfutée** : « pas d'homme à 400 m » monte sur 6/8 mais de 0,6 point seulement, et
  les jours éligibles sont **plus nombreux** sur la branche (6/8).
- Deux compteurs : conceptions attendues (jours éligibles × 1,2 %, jusqu'à 270 j avant la fin)
  contre naissances. `main` 245 attendues → 267 nées (×1,09) ; branche 261 → 245 (×0,94). L'écart
  (~15 % sur ~250, ~1,7 écart type de Poisson) est **dans le bruit du tirage** ; par run, le
  rapport va de 0,78 à 1,44 des deux côtés.
- Jours-femmes en âge : −3 % sur la branche.
- **Conclusion** : la natalité plus basse n'est pas un effet des portes ; elle tient au tirage et
  à un peu moins de femmes en âge. Ce qui reste de la baisse de population est à chercher dans
  les **morts** (acceptation : 18 contre 9 en t7, 29 contre 14 en t2024, 55 contre 41 en f7 ; morts
  violentes en hausse sur 3/8 à 10 ans). Le blocage « natalité » est levé ; reste le débit.
