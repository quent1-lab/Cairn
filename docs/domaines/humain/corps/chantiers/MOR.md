# Chantier : mortalité de fond — prédictions et décisions

## Décision de l'utilisateur (2026-10-05)

Mécanismes **causés**, pas un taux de fond par âge : chaque élément de la vie
qui peut causer une maladie ou un accident porte un risque, qui tue **ou non**.

## Lecture du code (état au 2026-10-05, a0e4864)

- Causes de mort existantes (`agent::DeathCause`) : faim, soif, froid (érosion
  de `Physiology::health` par un besoin saturé), sénescence (Gompertz pure,
  `demography::annual_mortality` = 0,006·e^{0,11(a−40)}, tirage quotidien),
  prédation (assaillant blessé par une meute, `combat`), violence (raid),
  maladie (**seulement** l'intervention divine, `divine.rs:310`), foudre.
- `Wound(f32)` existe : plaie de combat, mortelle à 1, cicatrise en 14 j
  (`WOUND_HEAL_PER_TICK`), pénalise le travail ; **ne s'infecte pas**
  (commentaire `sim.rs:1207`).
- Santé : régénère de 1/240 par heure quand faim, soif, froid < 0,8.
- Aucune maladie naturelle, aucun accident, aucune mort à l'accouchement,
  aucune vulnérabilité propre au nourrisson autre que le lait.
- Gompertz sans terme de Makeham : la mortalité indépendante de l'âge (le terme
  qu'on remplace par des causes) est absente — c'est ce que mesure le
  « 0,5 %/an hors soif ».

## Référence (ordres de grandeur, à vérifier avant de s'en servir pour valider)

- Gurven et Kaplan 2007 (« Longevity among hunter-gatherers ») : mortalité de
  la première année ~20 % ; survie à 15 ans ~55-65 % ; adulte 15-40 ans
  ~1 %/an ; causes : maladies ~70 % (respiratoires et digestives en tête),
  violence + accidents ~20 %, dégénératives ~10 %.
- Mortalité maternelle en fécondité naturelle : ordre de 1 % par naissance.
- Ces chiffres **valident**, ils ne calibrent pas (règle 10).

## Inventaire proposé : chaque élément de la vie et son risque

| Élément de la vie (code) | Ce qui peut arriver | Ce qui module le risque | Ce qui décide de la mort |
|---|---|---|---|
| Boire (`Drink`, sources) | infection digestive | eau stagnante vs courante ; humains qui vivent autour de la source (souillure) | gravité × vulnérabilité |
| Vivre ensemble (nuit, campement, hutte) | infection respiratoire, contagion d'un malade | contacts rapprochés, malades présents, froid | gravité × vulnérabilité |
| Manger la viande (chasse, réserve qui pourrit) | infection digestive, zoonose | âge de la viande, dépeçage | gravité × vulnérabilité |
| Chasser (`Hunting`) | blessure (corne, chute) | espèce (aurochs ≫ gazelle), compétence | plaie : mortelle à 1, sinon guérit |
| Marcher (`Walking`) | chute, entorse | relief, neige/glace, nuit | plaie |
| Plaie (combat, chasse, chute) | infection de la plaie | gravité de la plaie, santé | gravité × vulnérabilité |
| Accoucher (naissance) | mort de la mère, du nouveau-né | santé et faim de la mère, âge, rang de naissance | tirage à la naissance |
| Faim, froid sous-létaux | (pas une cause en soi) | **affaiblissent** : multiplient la vulnérabilité | — |

**Vulnérabilité** = f(âge : nourrisson et vieillard ; santé ; faim ; froid).
C'est elle qui fait émerger la mortalité infantile et la synergie
malnutrition-infection (un enfant affamé ne meurt pas de faim, il meurt de
diarrhée), sans écrire « les enfants meurent ».

**Proposition de forme** : une maladie est un **état** de l'agent (voie,
gravité, jours restants) qui **ronge la santé** comme un besoin saturé ; elle
tue si la santé tombe à zéro, sinon elle passe. La mort reste donc
l'aboutissement des mêmes rouages que la faim — et la faim, le froid, la plaie
s'additionnent à elle sur la même barre.

Hors périmètre de ce chantier : prédateurs qui attaquent d'eux-mêmes (voir la
note de `combat`), feux de forêt qui brûlent des humains, noyade.

## Décisions de l'utilisateur (2026-10-05, suite)

1. Trois voies (digestive, respiratoire, plaie) — elles permettront l'évolution
   (densité, sédentarité → maladies de foule).
2. Sénescence de Gompertz conservée ; les causes remplacent le terme de Makeham.
3. Ordre : maladie, puis accidents, puis accouchement. Périmètre ci-dessus.
4. **Plus tard** : guérison, remèdes, vaccins, médecine — liés aux technologies ;
   ils éviteront ou soigneront. Le modèle doit donc offrir des prises : taux
   d'infection par exposition (hygiène, eau bouillie), immunité (variolisation),
   clairance (remèdes), soin (repos, nourrir le malade).

## Modèle retenu (choix de Claude, à confirmer par la mesure)

**Hôte contre pathogène.** Une infection porte une gravité `s` (départ 0,1) et
une virulence `r` (par jour, tirée à l'infection, log-normale). L'hôte la
combat avec une clairance `c × immunité`. Chaque heure :
`s ← s · exp((r − c·imm)/24)`, plafonnée à 1 ; guérison sous 0,01.
Elle ronge la santé de `s × DISEASE_DAMAGE` par heure (comme un besoin
saturé ; la régénération continue de lutter). **La létalité n'est écrite
nulle part** : c'est la probabilité que la virulence dépasse la défense.

**Immunité** = maturité × expérience × nutrition × chaleur :
- maturité : 0,4 à la naissance → 1 à 5 ans ; un nourrisson allaité reçoit
  les anticorps du lait (plancher 0,8) — l'orphelin ne les a pas ;
- expérience par voie : `1 − 0,4 × 0,7^épisodes` (immunité acquise ; les
  fondateurs ont un passé) ; après guérison, 30 j sans réinfection sur la voie ;
- nutrition `1 − 0,5 × faim` ; froid `1 − 0,3 × froid` (synergie).

**Sources** :
- digestive : à chaque gorgée, `fond + charge × humains à 100 m de l'eau`
  (souillure) ; contagion de nuit à 10 m d'un malade ;
- respiratoire : portage déclenché (fond horaire × (1 + 4 × froid)) ;
  contagion de nuit à 10 m ;
- plaie : chaque heure, probabilité ∝ gravité de la plaie.

Vieillards : pas d'immunosénescence (déjà comptée dans Gompertz).

## Prédictions — voie digestive seule (écrites avant le banc)

Régime : banc `acceptation`, **3 ans** (comme le témoin), 4 seeds × 2 scènes, départ jour 135.
Létalité par épisode mesurée par le test (lutte seule, sans régénération) : adulte aguerri 0,4 % ; nourrisson allaité 7 % ; orphelin 33 % ; enfant de 6 ans affamé 7,8 % contre 0,9 % nourri.
1. Des morts « mal » apparaissent sur **≥ 6/8 runs**.
2. Elles se concentrent chez les **1-14 ans** (sevrés, peu d'épisodes vécus) :
   ≥ 60 % des morts de maladie ont moins de 15 ans.
3. Nourrissons < 1 an : peu touchés (ils ne boivent pas ; contact seul).
4. Adultes 15-39 : mortalité de maladie < 5 ‰/an.
5. Les morts de maladie frappent **plus les affamés** (synergie) : à vérifier
   par la faim la veille de la mort (instrument à ajouter si besoin).
Réfutation : morts de maladie surtout adultes, ou nulles, ou une bande
anéantie par contagion (le modèle serait mal posé, pas mal réglé).

## Prédictions — voie respiratoire ajoutée (adf01e8, écrites avant le banc)

Régime : 3 ans, 4 seeds × 2 scènes, comparé au run « digestive seule ».
1. Les morts de maladie augmentent davantage en **froid** qu'en tempéré
   (portage × (1 + 4·froid)) sur ≥ 3 seeds sur 4.
2. Les **nourrissons < 1 an** sont désormais touchés (contagion de la mère
   malade la nuit) : mortalité < 1 an en hausse sur l'agrégat.
3. Les orphelins (sans lait) en meurent plus : non mesuré directement
   (instrument à ajouter si la hausse < 1 an est nette).
Réfutation : une contagion qui balaie une bande (≥ 20 % d'une bande malade à
la fois) — la part du temps malade le dirait.

## Témoin (1b3e75f, 3 ans, 4 seeds × 2 scènes, départ jour 135) — mesuré

Régime réel : bandes de **40 → 57-64** (tempéré) et **60 → 79-91** (froid),
1 535 années-personnes ; aucun enfant de 5-14 ans (fondateurs adultes, 3 ans) :
la classe 5-14 est **hors de portée** de ce banc (le libellé « quelques
centaines » de la table est faux, à corriger).
- 8 morts en tout : 3 de froid, 4 de violence, 1 de froid chez un 1-4 ans.
  Mortalité nulle sur 6/8 runs. 5 ‰/an toutes classes (réel ~30 ‰).
- < 1 an : 0 mort sur 185 années-p (réel ~200 ‰). 1-4 : 6 ‰. 15-39 : 6 ‰.
- Mourir avant 15 ans (impliqué) : 2 % (réel ~40 %).

## Prédictions — infection des plaies (écrites avant le banc)

Les plaies ne viennent que du combat (rare : 4 morts de violence au témoin).
1. Effet faible : ≤ 3 morts de maladie de plus sur l'agrégat, toutes chez des
   adultes (15-59) — les seuls qui combattent.
2. Elle prendra son poids avec les accidents (chasse, chute), étape suivante.

## Résultat — voie digestive seule (86161c7, 3 ans, 8 runs) — mesuré

Morts : 8 → **53** ; mortalité nulle sur 6/8 → **2/8** (tempéré 42 et 2024).
| âge | années-p | ‰/an témoin → digestive | maladie | soif | froid |
|---|---|---|---|---|---|
| <1 | 177 | 0 → 79 | 4 | 8 | 2 |
| 1-4 | 144 | 6 → 111 | 5 | 9 | 2 |
| 15-39 | 1089 | 6 → 19 | 9 | 1 | 7 |
| 40-59 | 80 | 0 → 25 | 0 | 1 | 1 |
Mourir avant 15 ans (impliqué) : 2 % → 41 %. Malades : 1-8 % du temps (enfants), 2-4 % (adultes).

Prédictions :
1. ≥ 6/8 runs avec morts de maladie : **6/8 — tenue**.
2. ≥ 60 % des morts de maladie < 15 ans : **9/18 = 50 % — réfutée** (de peu).
3. < 1 an peu touchés : **réfutée** — 4 morts de maladie, et surtout 8 de soif.
4. Adultes 15-39 < 5 ‰/an de maladie : **8 ‰ — réfutée** (de peu).
5. Synergie faim-maladie : **0/18 morts de maladie chez des affamés** —
   non observée (régime : la faim dépasse rarement 0,7 dans ces scènes).

**Le voisin, plus gros que l'effet direct** : la soif revient (0 → 19), 17 sur
19 chez des nourrissons (< 3 ans, tout au lait) : 13 « mère sans lait »
(santé de la mère < 0,5), 4 orphelins. Mécanisme : le lait suit la santé de
la mère (correctif G) ; la maladie ronge la santé de la mère ; le lait tarit.
G prenait la santé pour les **réserves** ; une fièvre de quelques jours n'est
pas une famine. Question à l'utilisateur. Deuxième voisin connu : un enfant de
1 à 3 ans ne boit ni ne mange rien d'autre que le lait (sevrage réel : aliments
de complément dès ~6 mois).

## Sources — lait d'une mère malade, sevrage (2026-10-05, à la demande de l'utilisateur)

Lait :
- Prentice et al. (Gambie) : apport de la mère 1 773 → 1 215 kcal/j (saison
  de soudure, −31 %) : lait bu 653 → 580 g/j (−11 %), surtout en fin de
  lactation ; un supplément de +800-900 kcal/j **n'augmente pas** le volume.
  La lactation est tamponnée contre la sous-alimentation modérée.
  https://archive.unu.edu/unupress/food/8F093e/8F093E09.htm
- Mère malade (OMS, CDC) : on continue d'allaiter ; fièvre, infection
  respiratoire, gastro-entérite ne passent pas par le lait ; le lait apporte
  des anticorps ; le volume peut baisser un peu, la tétée l'entretient.
  https://kellymom.com/bf/can-i-breastfeed/illness-surgery/mom-illness/
- Mort de la mère : effet le plus fort tout au début de la vie (Sear et Mace
  2008, revue). https://hraf.yale.edu/ehc/documents/1074

Sevrage :
- OMS : allaitement exclusif jusqu'à ~6 mois ; ensuite le lait couvre ~70 %
  de l'énergie à 6-8 mois, ~55 % à 9-11 mois, ~40 % à 12-23 mois.
  https://www.ncbi.nlm.nih.gov/pmc/articles/PMC5065977/
- Chasseurs-cueilleurs : sevrage complet vers 3 ans (Aché), 3-3,5 (Hadza),
  4e année (!Kung) ; aliments prémâchés tôt ; un Hadza de 3 ans se procure
  ~30 % de ses calories. https://www.melvinkonner.com/wp-content/uploads/2009/02/Hunter-Gatherer-Childhood-2016.pdf
- Diarrhée du sevrage : pic à 6-11 mois, liée aux aliments et à l'eau
  introduits et à la perte des anticorps maternels.
  https://i-base.info/htb/?p=2605

## Décisions (2026-10-05, suite)

- Lait : **(a)** — plein tant que la mère vit et n'est pas en famine ; baisse
  modérée (Prentice) en famine ; la maladie n'agit que par la mort de la mère
  ou la contagion.
- Sevrage : comme défini (exclusif 6 mois ; part du lait 70/55/40 % ; eau bue
  avec la mère ; nourriture donnée par la mère ou une proche ; sevré à 3 ans).
- L'utilisateur soulève le **bilan énergétique** (kcal) : impact à évaluer.

## Sources — bilan énergétique

- Pontzer et al. 2012 (Hadza, eau doublement marquée) : femmes 1 877 kcal/j,
  43,4 kg, 20,9 % de graisse, PAL 1,78, 5,8 km/j ; hommes 2 649 kcal/j,
  50,9 kg, 13,5 % de graisse, PAL 2,26, 11,4 km/j ; à taille égale, même
  dépense que des Occidentaux. https://pmc.ncbi.nlm.nih.gov/articles/PMC3405064
- FAO/OMS/UNU 2004 : allaitement exclusif +505 kcal/j ; grossesse +360
  (2e trimestre), +475 (3e). Besoins de l'enfant : ~950 kcal/j vers 1-2 ans,
  ~1 500 vers 5-6 ans, ~1 900 vers 10 ans (ordre de grandeur, à relire dans
  les tables du ch. 4). https://www.fao.org/4/y5686e/y5686e00.htm
- Jeûne total avec eau : 30 à 60 jours, ~70 au plus ; grévistes de 1981 :
  46 à 73 jours ; 1 kg de graisse ≈ 7 700 kcal.
  https://www.npr.org/sections/goatsandsoda/2016/01/20/463710330/what-happens-to-the-body-and-mind-when-starvation-sets-in
- Ellison (charge métabolique) : la reprise des cycles après une naissance
  suit le bilan énergétique de la mère, sur un continuum, pas un seuil.
  https://pmc.ncbi.nlm.nih.gov/articles/PMC3305908

## Résultat — voie respiratoire ajoutée (adf01e8, 3 ans, 8 runs) — mesuré

Morts : 53 → **68** ; mortalité nulle sur **0/8** runs. Maladie 18 → 46 ;
soif 19 → 10 (nourrissons : 6 orphelins, 3 « mère sans lait »).
| âge | années-p | ‰/an témoin → dig. → +resp. | maladie |
|---|---|---|---|
| <1 | 172 | 0 → 79 → **169** | 23 |
| 1-4 | 129 | 6 → 111 → 101 | 7 |
| 15-39 | 1084 | 6 → 19 → 23 | 15 |
| 40-59 | 80 | 0 → 25 → 12 | 1 |
Mourir avant 15 ans (impliqué) : 44 %. Malades : enfants 5-16 % du temps,
adultes 3-8 %. Aucune mort de maladie chez un affamé (régime : faim rare).

Prédictions :
1. Hausse des morts de maladie plus forte en froid qu'en tempéré, ≥ 3 seeds
   sur 4 : **3/4 (42, 7, 1337 ; pas 2024) — tenue, sur de petits nombres**.
2. < 1 an en hausse : **tenue** (79 → 169 ‰ ; 4 → 23 morts de maladie).
3. Orphelins : non mesuré.
Voisin : la soif des nourrissons baisse (19 → 10) sans changement de son
mécanisme — trajectoires différentes (règle 7 : un tirage) ; le lait (a)
réglera la cause.
À surveiller : maladie chez les 15-39 ans **14 ‰/an** (réel, toutes maladies,
~5-7 ‰) — à relire après le bilan énergétique (la défense lira les réserves).

## Résultat — infection des plaies (3e3c81c, 3 ans, 8 runs) — mesuré

Sortie **identique** au run respiratoire à une case près (froid 2024 : adultes
malades 3 → 4 % du temps) : au moins une plaie infectée, sans mort ni
divergence de trajectoire. Prédiction 1 (≤ 3 morts de plus, adultes) :
**tenue** (0). Les plaies ne viennent que du combat : la voie attend les
accidents. Non vu : le nombre de plaies non mortelles (instrument absent) —
une divergence nulle sur 3 ans dit aussi que la santé pèse peu sur les
décisions.

## MOR-4 — le sevrage (2026-10-16 ; décision de l'utilisateur : avant la cueillette rapportée)

Sources (sous-agent, chiffres non interprétés ; plusieurs de seconde main) :
- OMS/PAHO 2003 (Dewey et Brown 2002) : à 12-23 mois, ~550 g/j de lait couvrent 35-40 %
  des besoins ; 413, 379, 346 kcal/j à 6-8, 9-11, 12-23 mois. https://media.tghn.org/articles/WHO_2003_1.pdf
- Kenya, ~14 mois : lait 328 kcal/j, 32 % de 1 029 kcal/j (Onyango et al. 2002).
  https://pubmed.ncbi.nlm.nih.gov/12075365
- Bangladesh rural (Persson et al. 1998) : 548 g/j à 12-23 mois, **312 g/j à 24-36 mois**.
  https://archive.unu.edu/unupress/food/V194e/ch08.htm
- 113 sociétés non industrielles (Sellen 2001) : autres liquides à 4,5 ± 6,0 mois, solides
  à 5,0 ± 4,0, allaitement 29 ± 10 mois.
- !Kung : aliments prémâchés dès ~6 mois, sevrage à 3-4 ans (Konner) ; Hadza : à 3 ans,
  ~30 % des calories cueillies par l'enfant lui-même (Crittenden 2013, de seconde main) ;
  enfants sevrés nourris par les grands-mères (Hawkes).
- Non trouvé : part du lait à 24-36 mois chez les chasseurs-cueilleurs ; âge et façon de
  boire de l'eau.

Changement (`demography::nurse_infants`, `milk_share`) : jusqu'à 6 mois, lait exclusif à la
demande (inchangé). Passé 6 mois, le lait apporte chaque heure **sa part** du besoin de
l'enfant et de son eau : 70 % (6-8 mois), 55 % (9-11), 40 % (12-23), **20 % (24-35)** —
dérivé de 312 g/j × ~0,69 kcal/g ≈ 215 kcal pour ~1 050 kcal de besoin. Le reste : l'enfant
**boit quand sa mère boit** (à la même eau, donc à sa souillure : la diarrhée du sevrage), et
**mange ce qu'elle lui donne** quand elle mange ou de ce qu'elle porte (pris sur sa charge,
sinon sur sa faim). La mère ne paie plus l'eau que du lait qu'elle donne. À 3 ans, comme
avant, l'enfant délibère seul.
Test : `un_enfant_d_un_an_boit_avec_sa_mere_et_mange_ce_qu_elle_donne`, vu rouge (faim
0,400 : le lait couvrait tout).
Prédictions, acceptation 5 ans (8 runs, départ j135), contre `main` :
1. Morts de soif des moins de 1 an et des 1-4 ans : 15 et 14 → ≤ 6 chacune.
2. Morts de maladie des moins de 1 an en hausse de 0 à +60 % (eau bue dès 6 mois, anticorps
   gardés tant qu'il tète) ; des 1-4 ans de −20 à +40 %.
3. Mortalité des 1-4 ans (150 ‰/an) : en baisse ; des moins de 1 an (292 ‰) : de −30 à +20 %.
4. Mères : kcal/j mangées en hausse de 0 à 5 % (elles nourrissent l'enfant).
Réfuterait 1 : soif des 1-4 ans ≥ 10 (l'enfant boit trop rarement avec sa mère : 3-4 fois
par jour au plus).

### MOR-4 (premier jet, b83… sur `main`) — acceptation 5 ans : **échec, défaut d'implémentation**

| | main | MOR-4 v1 |
|---|---|---|
| mortalité < 1 an ; 1-4 ans (‰/an) | 292 ; 150 | **589 ; 573** |
| morts de faim < 1 an ; 1-4 ans | 0 ; 2 | **32 ; 50** |
| morts de soif < 1 an ; 1-4 ans | 15 ; 14 | 18 ; **5** |
| kcal/j < 3 ans (froid) | 692-719 | 525-571 |
| mourir avant 15 ans | 59 % | 94 % |
Prédiction 1 : tenue pour les 1-4 ans (soif 14 → 5), pas pour les moins de 1 an (15 → 18).
Lecture : l'enfant ne recevait au repas qu'un débit horaire (0,1 × part manquante × échelle),
soit ~0,06 de faim par heure de repas : il lui en fallait cinq par jour. Le test le laissait
passer (seuil 0,45). Correction : au repas, l'enfant mange **à sa faim** ; test durci (< 0,1),
vu rouge (0,425). À remesurer.

### MOR-4 corrigé (repas à la faim) — acceptation 5 ans (8 runs, départ j135), contre `main`

| | main | MOR-4 v1 | **MOR-4 corrigé** |
|---|---|---|---|
| mortalité < 1 an ; 1-4 ans (‰/an) | 292 ; 150 | 589 ; 573 | **289 ; 144** |
| morts de soif < 1 an ; 1-4 ans | 15 ; 14 | 18 ; 5 | **14 ; 13** |
| morts de faim < 1 an ; 1-4 ans | 0 ; 2 | 32 ; 50 | 0 ; 0 |
| morts de maladie < 1 an ; 1-4 ans | 45 ; 37 | 101 ; 50 | 44 ; 40 |
| kcal/j < 3 ans | 633-719 | 525-571 | 635-717 |
| mourir avant 15 ans | 59 % | 94 % | 58 % |
| population finale (8 runs) | 46-80 | — | 36-83 (sans direction commune) |
1. Soif ≤ 6 : **réfuté** (14 et 13). Boire avec la mère ne retire pas les morts de soif : il
   reste les orphelins et les mères en famine (banc `soif` : 35-68 % sans mère, 64 % des
   autres heures avec une mère à réserves vides).
2. Maladie des moins de 1 an +0 à +60 % : tenu bas (44 contre 45) — l'eau bue dès 6 mois
   n'ajoute presque rien de visible.
3. Mortalité 1-4 ans en baisse : 150 → 144, dans le bruit ; < 1 an : inchangée.
4. Kcal des femmes : inchangées (2 569-2 682 contre 2 578-2 678).
Lecture : le sevrage est en place, sans coût ni gain visible à 5 ans sur ces scènes. Les
morts de soif des petits viennent d'**orphelins** et de **mères affamées**, pas du sevrage.
Repère réel (à sourcer) : mortalité infantile hadza ~20 % la première année.

## Débat (2026-10-17, contradicteur Opus, un tour)

1. *L'attribution « orphelins et mères affamées » vient du banc `soif` lancé avant MOR-4 ;
   depuis, l'enfant boit quand sa mère boit, la voie « mère affamée » devrait avoir presque
   disparu.* — **Retenu.** C'est un chiffre de l'ancien binaire mis en conclusion du nouveau.
   Action : banc `soif` sur MOR-4 (main) et sur cueillette + MOR-4, avec l'âge au décès et le
   temps écoulé depuis la dernière gorgée de la mère.
2. *L'orphelin sevré n'a personne : `continue` ; seule une femme qui allaite peut le
   prendre. Passé 6 mois, une grand-mère pourrait le nourrir et le faire boire (Hawkes) :
   porte tout ou rien sans impossibilité physique.* — **Retenu**, défaut de mécanisme :
   jalon proposé **MOR-4b** (soin d'une proche hors lait passé 6 mois). Décision à demander.
3. *14 contre 15 morts : bruit de Poisson ±4, le banc ne voit qu'un effet de ±50 %.* —
   **Retenu.** Lire en heures (soif saturée, faim > 0,8), seed par seed, en signes appariés.
4. *Prédiction 4 (kcal des mères en hausse) : inchangé par construction ; la différence vraie
   est que le lait est tamponné par les réserves, la nourriture donnée non.* — **Retenu** :
   prédiction mal posée. À mesurer : faim des mères d'un 6-36 mois les jours à réserves
   vides, `food_stats` Milk contre Shared.
5. *Le plancher immunitaire (0,8) tient tant que `milk` est vrai, à 100 % comme à 20 % : la
   diarrhée du sevrage ne peut pas apparaître ; et l'enfant étanche sa soif même si la mère
   boit à une eau qui n'est pas douce.* — **Retenu, deux points.** Le second est un **bug**
   (la mère « boit » sans s'étancher à une eau non douce, l'enfant si) : correctif à part,
   avec test rouge. Le premier : proportionner les anticorps à la part du lait — jalon à
   part (MOR-4c), un changement à la fois.
6. *L'aménorrhée vaut 3 ans tout ou rien, même à 20 % de lait ; l'intervalle entre naissances
   n'est pas mesuré.* — **Retenu** comme voisin : à mesurer, à relier à ENE-5.
Report de la cueillette : **pas encore** (accord) — l'hypothèse « le sevrage résorbe la
hausse » n'est pas vérifiée. Ordre : bug de l'eau, banc `soif` (MOR-4, cueillette + MOR-4),
acceptation 5 ans appariée lue seed par seed (froid 7 à part) ; une mortalité 1-4 ans en
hausse sur ≥ 6/8 runs bloque le report.

### Correctif de l'eau (débat, point 5b) et mesures du débat — prédictions (2026-10-17, avant)

Correctif : l'enfant ne boit (et ne s'expose à l'eau) que si sa mère s'est vraiment
désaltérée. Test `l_enfant_ne_boit_pas_quand_sa_mere_ne_s_est_pas_desalteree`, vu rouge.
Mesures (`out/mortalite/queue14.sh`) : banc `soif` 3 ans puis acceptation 5 ans, 8 runs,
départ j135, `main` (MOR-4 + correctif) contre cueillette (idem).
1. Banc `soif`, `main` : heures-enfant en soif saturée ≤ 300 (441 avant MOR-4) ; la part
   « sans mère » monte à ≥ 60 % (la voie « mère affamée » se ferme par l'eau bue avec elle).
2. Cueillette contre `main`, apparié seed par seed (froid 7 lu à part) : mortalité 1-4 ans
   plus haute sur ≤ 4/8 runs si le sevrage a résorbé l'effet ; heures de soif saturée à ±30 %.
Réfuterait 2 (et bloquerait le report) : mortalité 1-4 ans plus haute sur ≥ 6/8 runs.

## MOR-4b — l'orphelin sevré est pris en charge (branche `exp/orphelins`, 2026-10-17)

Décision de l'utilisateur : « dans la réalité, un orphelin est pris en charge par les
autres même après le sevrage, jusqu'à ce qu'il soit autonome. » Débat, point 5.
Changement : un orphelin de 6 à 36 mois sans nourrice peut être pris par **n'importe quel
adulte** à portée de résidence, d'autant plus volontiers qu'il lui est apparenté (même tirage
que l'allomaternage : parenté, puis bande 5 %/j, puis étranger 1 %/j) ; ce porteur
(`Sim::carers`) le porte, le fait boire et le nourrit, **sans lait** (seules la mère et la
nourrice allaitent). Les 3-13 ans orphelins ne suivent encore personne (jalon suivant).
Test `un_orphelin_sevre_est_pris_en_charge_et_boit_avec_son_porteur`, vu rouge.
Prédictions, acceptation 5 ans appariée contre `main` (MOR-4 + correctif de l'eau) :
1. Morts de soif et de faim des 6-36 mois en baisse sur ≥ 5/8 runs ; part « sans mère » des
   heures de soif saturée (banc `soif`) divisée par ≥ 2.
2. Mortalité < 1 an inchangée (les < 6 mois orphelins restent sans lait) ; 1-4 ans en baisse.
3. Porteurs : faim légèrement plus haute (ils nourrissent l'enfant).
Réfuterait 1 : aucune baisse sur ≥ 5/8 (peu d'orphelins sevrés, ou recueil trop lent à 5 %/j).

### Mesures du débat — résultats (2026-10-17)

Banc `soif` (3 ans, 8 runs), `main` (MOR-4 + correctif) : 348 heures-enfant en soif saturée
(441 avant MOR-4 ; prédit ≤ 300 : **réfuté de peu**) ; **99 % sans mère** (prédit ≥ 60 % :
tenu) — la voie « mère affamée » est fermée, il ne reste que les orphelins (→ MOR-4b).
Cueillette + MOR-4 : 354 heures, 100 % sans mère.

Acceptation 5 ans (8 runs, départ j135), mortalité 1-4 ans (‰/an), appariée :
| run | t42 | t7 | t1337 | t2024 | f42 | f7 | f1337 | f2024 |
|---|---|---|---|---|---|---|---|---|
| `main` | 45 | 188 | 128 | 161 | 240 | 202 | 64 | 154 |
| cueillette | 100 | 105 | 128 | 358 | 197 | 556 | 220 | 172 |
En hausse sur **5/8**, en baisse sur 2, égal sur 1 : sous le seuil de blocage écrit (≥ 6/8),
mais l'agrégat monte (144 → 213 ‰ ; morts 1-4 ans 55 → 72, dont froid 2 → 8, soif 16 → 19,
maladie 37 → 43 ; mourir avant 15 ans 58 → 69 %). Marche −4 à −6 km/j et faim des mères en
baisse sur 8/8. **Sans direction commune, et pas assez de puissance** (débat, point 3) :
quatre seeds de plus (11, 23, 99, 314) avant de trancher.

## MOR-4c, étape 1 — les anticorps suivent la part du lait (branche `exp/anticorps`, 2026-10-17)

Décision de l'utilisateur (« bonne idée ») ; débat, point 6. Changement : le plancher
immunitaire de l'allaité (0,8) n'est plus tout ou rien ; la maturité le rejoint **au prorata
de la part du lait** (1 à moins de 6 mois, 0,2 la troisième année). Test
`les_anticorps_du_lait_suivent_sa_part`, vu rouge. Forme non sourcée (linéaire) ; sens
sourcé : l'allaitement partiel protège moins que l'exclusif (à chiffrer par `sourceur`).
Prédictions, acceptation 5 ans (8 runs) contre `main` (MOR-4 + correctif) :
1. Morts de maladie des 1-4 ans en hausse (37 → 42-60) ; des moins de 1 an de 0 à +20 %.
2. Mortalité 1-4 ans en hausse sur ≥ 5/8 runs.
Réfuterait 1 : maladie des 1-4 ans ≤ 40 (le plancher ne pesait pas sur cet âge).
L'aménorrhée (étape 2) viendra seule, après.

### MOR-4b — mesuré (acceptation 5 ans, 8 runs ; banc `soif` 3 ans) : **pire**

| | `main` | MOR-4b |
|---|---|---|
| heures-enfant en soif saturée (3 ans) | 348 | **618** |
| mortalité < 1 an ; 1-4 ans (‰/an) | 290 ; 144 | **346** ; 142 |
| morts de soif < 1 an ; 1-4 ans | 14 ; 16 | **20** ; 16 |
| 1-4 ans par run (t42, t7, t1337, t2024, f42, f7, f1337, f2024) | 45, 188, 128, 161, 240, 202, 64, 154 | 45, 63, 128, 161, 144, 346, 159, 109 |
1. Soif et faim des 6-36 mois en baisse sur ≥ 5/8 : **réfuté** (soif en hausse).
Lecture : l'orphelin sevré n'a **plus d'eau du tout** hors des gorgées de son porteur (pas
de lait), et le porteur boit selon **sa** soif, qui monte bien plus lentement que celle d'un
enfant qui ne boit rien d'autre : entre deux gorgées, l'enfant sature. Avant, l'orphelin
mourait vite ; porté, il souffre plus longtemps. Il manque ce que fait tout porteur réel :
**boire aussi pour l'enfant qu'il porte** (MOR-4b', ci-dessous).

## MOR-4b' — on boit aussi pour l'enfant qu'on porte (branche `exp/orphelins`, 2026-10-17)

Changement : l'envie d'aller boire d'un porteur (mère, nourrice, porteur d'un orphelin sevré)
suit la plus forte des deux soifs, la sienne et celle de l'enfant porté. Test
`on_boit_aussi_pour_l_enfant_qu_on_porte`, vu rouge. Touche aussi les mères (leur enfant
sevré tire une part de son eau du lait, le reste des gorgées de sa mère).
Prédictions, acceptation 5 ans (8 runs) contre `main` (MOR-4 + correctif) :
1. Heures-enfant en soif saturée (banc `soif`) ≤ 200 (`main` 348, MOR-4b 618).
2. Morts de soif < 1 an et 1-4 ans : ≤ 8 chacune (14 et 16).
3. Mortalité < 1 an et 1-4 ans en baisse sur ≥ 5/8 runs.
4. Marche des femmes : +0 à +0,5 km/j (trajets d'eau en plus).
Réfuterait 1 : ≥ 300 heures.

### Cueillette + MOR-4 — quatre seeds de plus (11, 23, 99, 314 ; acceptation 5 ans, départ j135)

| run | t11 | t23 | t99 | t314 | f11 | f23 | f99 | f314 |
|---|---|---|---|---|---|---|---|---|
| 1-4 ans `main` (‰/an) | 168 | 85 | 86 | 99 | 508 | 337 | — | 204 |
| 1-4 ans cueillette | 302 | 24 | 150 | 132 | 244 | 81 | — | 91 |
Agrégat 1-4 ans : `main` 218, cueillette **141** ‰ (première série : 144 et 213) ; mourir
avant 15 ans 68 % contre 56 %. Sur les 15 runs comparables : en hausse 8, en baisse 6, égal 1
— **sans direction commune** ; l'effet de la première série était du bruit. Marche en baisse
sur 15/15 (−2 à −6 km/j). Seuil de blocage écrit (≥ 6/8 en hausse) non atteint dans aucune
série : **cueillette rapportée reportée sur `main`** (décision de l'utilisateur du
2026-10-16, protocole tenu : 5 ans, 16 runs).
