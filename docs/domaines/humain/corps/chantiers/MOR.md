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

### MOR-4c (anticorps) — mesuré (acceptation 5 ans, 8 runs, `exp/anticorps`, contre `main` MOR-4 + correctif)

| | `main` | anticorps |
|---|---|---|
| mortalité < 1 an ; 1-4 ans (‰/an) | 290 ; 144 | **394 ; 220** |
| 1-4 ans par run | 45, 188, 128, 161, 240, 202, 64, 154 | 188, 246, 163, 216, 310, 315, 213, 157 (**8/8 en hausse**) |
| morts de maladie < 1 an ; 1-4 ans | 44 ; 37 | 74 ; 47 |
| mourir avant 15 ans | 58 % | **72 %** |
1. Maladie des 1-4 ans 37 → 47 (prédit 42-60) : **tenu**. 2. Mortalité 1-4 ans en hausse sur
≥ 5/8 : **tenu (8/8)**.
Lecture : le mécanisme fait ce qu'on attendait — sans le plancher tout ou rien, les enfants
sevrés se défendent moins. Mais le modèle s'éloigne du réel : mourir avant 15 ans passe de
58 à 72 % (Hadza ~46 %, à sourcer précisément). Le plancher masquait une **maladie trop
létale** pour les enfants : c'est **MOR-5** (« relire la maladie des adultes, 14-24 ‰
contre ~5-7 réels »), en amont. Branche gardée, non reportée : à reprendre après MOR-5.

### MOR-4b' (on boit pour l'enfant porté) — mesuré : **réfuté**

| | `main` | MOR-4b | MOR-4b' |
|---|---|---|---|
| heures-enfant en soif saturée (3 ans) | 348 | 618 | **505** |
| morts de soif < 1 an ; 1-4 ans | 14 ; 16 | 20 ; 16 | **19 ; 24** |
| mortalité < 1 an ; 1-4 ans | 290 ; 144 | 346 ; 142 | 354 ; 178 |
| part « mère présente, en bonne santé » des heures de soif | ~0 % | 8,3 % | **19,6 %** |
Prédiction 1 (≤ 200 heures) : **réfutée**. Fait nouveau, non expliqué : depuis MOR-4b, des
enfants **dont la mère vit et se porte bien** atteignent une soif saturée (8 puis 20 % des
heures), ce qui n'arrivait pas sur `main`. Hypothèses à instrumenter avant toute correction :
une mère qui porte aussi un orphelin ; un porteur désigné alors que la mère vit (relevé du
porteur réel dans le banc `soif`) ; une mère qui part boire loin et s'arrête faute
d'atteindre la source. Branche gardée, non reportée.

## Décisions de l'utilisateur (2026-10-17, soir)

- **MOR-5 d'abord** (la maladie trop létale, en amont de MOR-4c) : d'accord.
- **Orphelins** : d'accord pour instrumenter les trois hypothèses de la soif à mère présente
  (mère qui porte aussi un orphelin ; porteur désigné alors que la mère vit ; trajet d'eau
  inabouti) avant toute correction. Branche `exp/orphelins` gardée.
- **Mémoire** : forme validée (MEM.md) ; chercher et sourcer ce qu'il serait utile de retenir.

## MOR-5 — la maladie trop létale (ouvert le 2026-10-17)

**Référence** : Gurven et Kaplan 2007 (ci-dessus) — survie à 15 ans ~55-65 %, adultes
15-40 ans ~1 %/an toutes causes, dont maladie ~70 % (~5-7 ‰). **Écart** : adultes 15-39 ans
14 ‰/an de maladie seule (adf01e8) ; mourir avant 15 ans 58 % sur `main` (MOR-4), 72 % sous
MOR-4c ; < 1 an 290 ‰ (réel ~200).

**Lecture du code** (`disease.rs`, 19e74e0) : la létalité d'un épisode n'est écrite nulle
part, elle vaut ≈ P(virulence > clairance × défense), virulence log-normale (médiane 0,165,
σ 0,6), défense = maturité × expérience × (1 − 0,5 faim) × (1 − 0,3 froid). Ordres de
grandeur qui en découlent (calcul, pas mesure) :
| hôte | défense | létalité/épisode |
|---|---|---|
| adulte aguerri, nourri, au chaud | 0,99 | ~0,15 % |
| même, faim 0,5 / faim 1 | 0,74 / 0,49 | ~0,6 % / ~3 % |
| nouveau-né sans lait / allaité | 0,24 / 0,48 | ~27 % / ~4 % |
| 1 an sevré, 1 épisode vécu | 0,37 | ~9 % |
| 2 ans sevré, 3 épisodes | 0,55 | ~2 % |
Trois défauts possibles, que les chiffres d'aujourd'hui ne départagent pas :
- **H1 — trop d'épisodes** : l'eau souille à 0,2 % + 0,02 %/humain proche par gorgée (0,8 %
  près d'un camp de 30), la contagion à 5-10 % par nuit et par malade à 20 m ; seule une
  convalescence de 30 j borne la fréquence. **Le nombre d'épisodes par an n'est pas mesuré.**
- **H2 — trop létal par épisode chez l'enfant** : l'hôte naïf part à 0,6 d'expérience et la
  maturité à 0,4.
- **H3 — la faim de l'heure, pas les réserves** : la défense lit `hunger` (qui monte en
  quelques heures, retombe au repas) ; la malnutrition réelle est un état des réserves. Une
  virulence fixée à l'infection, jugée heure par heure contre une défense qui oscille avec
  les repas, tue sur un creux.

**MOR-5a — instrumenter (observation seule)** : par classe d'âge et par voie, épisodes par
personne-année, létalité observée par épisode, défense moyenne à l'infection et au décès ;
deux compteurs qui doivent concorder : morts de maladie (cause de décès) et épisodes × létalité.
Référence à sourcer avant la mesure (`sourceur`) : épisodes de diarrhée et d'infection
respiratoire par an chez l'enfant et l'adulte sans assainissement, létalité par épisode.
Références sourcées (`SOURCES.md`, 2026-10-17) : diarrhée < 5 ans 3-8 épisodes/an (pic
6-11 mois), adulte ~2-3/an ; respiratoire basse 0,2-4/an chez l'enfant ; létalité d'une
diarrhée sans réhydratation ~2,4 % (enfants) ; non allaité ×10 (partiel ×4,6) ; la
malnutrition pèse surtout **légère à modérée**, sans seuil (Pelletier) — appuie H3.
Prédictions, acceptation 1 an × 8 runs (instrument seul, rien ne change) :
1. Adultes 15-39 : létalité par épisode ≥ 0,5 % (le calcul donne 0,15 % nourri) — la faim
   de l'heure tue sur un creux. Réfuterait : ≤ 0,2 %.
2. Épisodes adultes (toutes voies) ≥ 3/an — exposition trop forte. Réfuterait : ≤ 2/an.
3. Enfants 1-4 ans : létalité ≥ 3 % (réel ~2,4 % sans soins, diarrhée).
4. Concordance : infections closes par une mort de maladie = morts de maladie de la table
   à ±5 %.
Seuil de décision :
si les épisodes adultes dépassent le double de la référence → l'exposition (H1) d'abord ; si
leur nombre est réel et la létalité trop haute → la défense (H2, H3).

### MOR-5a — essai de fumée (1 an, seed 42, 2 scènes ; instrument de 2 commits après caba2fa) : l'instrument tient
Concordance : infections closes par une mort de maladie 4 (< 1 an) et 3 (15-39) = morts de
maladie de la table, 4 et 3. Un seul tirage, pas une mesure ; il montre déjà des adultes à
~13 épisodes/an (digestive ~6, respiratoire ~7, contre ~2-3 réels). À 1 an, aucun enfant de
1-4 ans (les fondateurs sont adultes) : la prédiction 3 demande l'horizon de 5 ans.
Mesure lancée (2026-10-17, 19 h 23 Paris) : acceptation 5 ans × 8 runs, 1 fil,
`out/bin/mor5a/acceptation` → `out/mor5/acceptation_mor5a_5ans.txt` (~2-3 h estimées).

### MOR-5a — mesuré (acceptation 5 ans, 8 runs, départ j135, `out/bin/mor5a`, d13238c)
Régime : 5 ans, 4 seeds × 2 scènes ; **aucun 5-14 ans** (les fondateurs sont adultes, les
premiers nés ont < 5 ans) ; adultes 1 668 années-personnes, enfants 547.
| âge | épisodes/p-an (dig. ; resp.) | létalité/épisode (dig. ; resp.) | défense ; faim à l'infection | maladie ‰/an |
|---|---|---|---|---|
| < 1 | 6,8 ; 7,7 | 2,0 % ; 1,3 % | 0,36 ; 0,03 | 231 |
| 1-4 | 6,7 ; 7,5 | 0,8 % ; 1,0 % | 0,61 ; 0,15 | 127 |
| 15-39 | 6,5 ; 7,3 | 0,3 % ; 0,4 % | 0,85 ; 0,30 | **53** |
| 40-59 | 6,7 ; 7,4 | 0,3 % ; 0,3 % | 0,85 ; 0,30 | 43 |
Mourir avant 15 ans : 69 %. Malades : enfants 20-24 % du temps, adultes 12-16 %.
1. Létalité adulte ≥ 0,5 % : **ni tenue ni réfutée** (0,3-0,4 % ; au-dessus du calcul nourri,
   0,15 %, sous le seuil). 2. Épisodes adultes ≥ 3/an : **tenue, et de loin** (~14/an, contre
   ~2-3 réels pour la seule diarrhée). 3. Létalité 1-4 ans ≥ 3 % : **réfutée** (0,8-1,0 %,
   sous les ~2,4 % d'une diarrhée sans soins). 4. Concordance : **exacte** (48, 43, 79, 8 des
   deux côtés).
**Lecture** : ce n'est pas la létalité par épisode qui est trop forte, c'est **le nombre
d'épisodes** — ~7 par voie et par an **à tout âge**, ce qui est presque le plafond que permet
la convalescence de 30 j (≈ 9-10 par voie et par an) : l'exposition sature, chacun se
réinfecte dès que sa voie se rouvre. Seuil de décision écrit : **H1 (l'exposition) d'abord**.
H2 réfutée pour l'instant ; H3 non jugée (la défense a peu de marge à perdre quand on tombe
malade 14 fois par an).
**Ce que j'ai pu manquer** : l'âge 5-14 n'est pas vu ; la létalité par épisode des enfants
paraît basse parce que les plus faibles meurent aux premiers épisodes (sélection) ; la source
de chaque infection (eau, contagion, portage réveillé par le froid) n'est pas mesurée — c'est
elle qui dira quelle exposition sature.
**Suite, MOR-5b** : instrumenter la source de chaque infection, puis corriger l'exposition
qui sature, une à la fois.

### MOR-5b — d'où viennent les infections (instrument, prédictions avant le banc)
Relevé : chaque infection note si elle vient d'un malade (contagion de nuit, à 10 m) ; la
digestive sinon vient de l'eau, la respiratoire du portage réveillé par le froid. Sans effet
sur la simulation (même tirage) : contrôle, la seed 42 à 1 an doit redonner la fumée de MOR-5a.
Acceptation **1 an** × 8 runs (régime : adultes seulement, ~13 épisodes/an déjà à 1 an).
1. Contagion ≥ 70 % des épisodes, sur les deux voies : 2 à 4 malades à 10 m chaque nuit
   dans un camp de 30 (12-16 % du temps malade) font ~10-20 % de chances par nuit, de quoi se
   réinfecter quelques jours après la convalescence. Réfuterait : ≤ 40 %.
2. Respiratoire hors contagion ≈ 1-2 par an (portage : 1/an au chaud, 5/an transi).
Si 1 tient : la contagion est un **cercle qui s'entretient** (le malade d'hier infecte qui
guérira demain) — le défaut est dans la contagion (portée, durée de la contagiosité,
contagiosité égale du premier au dernier jour), à lire avant de choisir.

### MOR-5b — mesuré (acceptation 1 an, 8 runs, e46cd5a)
Contrôle : seed 42 identique à la fumée de MOR-5a (morts de maladie 1 et 6) — l'instrument
ne touche pas la simulation.
| âge | voie | épisodes/p-an | part venue d'un malade | hors contagion /p-an |
|---|---|---|---|---|
| < 1 | dig. ; resp. | 6,4 ; 9,1 | 100 % ; 97 % | ~0 ; 0,3 |
| 15-39 | dig. ; resp. | 5,4 ; 6,6 | **69 % ; 94 %** | 1,7 (eau) ; 0,4 (portage) |
| 40-59 | dig. ; resp. | 6,2 ; 6,8 | 71 % ; 91 % | 1,8 ; 0,6 |
1. Contagion ≥ 70 % sur les deux voies : **tenue** (respiratoire 94 % ; digestive 69 %, à la
   limite ; réfutation à ≤ 40 % loin). 2. Portage 1-2 par an : **réfuté vers le bas** (0,4).
**Lecture** : l'eau seule donne ~1,7 diarrhée par adulte et par an — dans l'ordre réel (~2-3) ;
le portage reste rare. **L'excès est la contagion** : ~4 diarrhées et ~6 infections
respiratoires de plus par an, et la quasi-totalité chez les nourrissons. Dans le code, chaque
malade, **quelle que soit sa gravité** (de 0,1 au début à 0,01 en guérison), infecte chaque
nuit 5 % (dig.) ou 10 % (resp.) de ses voisins à 10 m. Un épisode dure ~3-4 jours : un voisin
a ~19 % (dig.) et ~34 % (resp.) de chances d'être infecté **par épisode**, et tout un camp
dort à moins de 10 m ; après 30 j de convalescence on retombe sous l'onde suivante.
Trois mécanismes possibles, non départagés : la contagiosité ne suit pas la gravité ; la
portée ne distingue pas le foyer (même hutte) du camp ; l'immunité de 30 j est la même pour
tous les germes d'une voie (une voie = un seul germe, alors que le réel en fait circuler
plusieurs, chacun immunisant plus longtemps). Référence à sourcer : taux d'attaque secondaire
en foyer (diarrhée, infection respiratoire) et durée de l'immunité après un épisode.
Question à l'utilisateur avant de corriger : laquelle de ces pistes correspond à sa vision.

## Décisions de l'utilisateur sur MOR-5c (2026-10-17, nuit)
1. Contagiosité selon la gravité : « je crois que la contagion est plus forte au pic de la
   maladie et diminue avec » — à vérifier (sourcer la courbe d'excrétion).
2. Foyer et camp : « oui et non : le foyer est plus touché, le camp moins ; mais les
   relations sociales et le travail sont vecteurs aussi » — à sourcer.
3. Plusieurs germes par voie : « bonne idée ».
Validé, ordre proposé (2, 1, 3), un changement à la fois, chacun mesuré.

**Ce que disent les sources** (`SOURCES.md`, contagion) : l'attaque secondaire **par épisode en
foyer** est de 15-33 % (diarrhée, par germe), 10-38 % (grippe) — l'ordre de ce que le code
donne (19 % ; 34 %), mais le code l'applique à **tout dormeur à 10 m**, pas au seul foyer ;
hors foyer, la grippe montre qu'une communauté peut peser autant que le foyer (contacts de
travail et de loisir ~37 % des contacts quotidiens, POLYMOD). Piste 1 (l'utilisateur) :
**vraie pour la grippe** (pic aux jours 1-2), **fausse pour la diarrhée** (norovirus, Shigella :
excrétion de 2 à 6 semaines, souvent maximale après les symptômes). Piste 3 appuyée : immunité
de 4-8 ans par génotype de norovirus, ~160 rhinovirus, 6 germes principaux de la diarrhée.

### MOR-5c0 — qui dort à portée de contagion (instrument, prédictions avant le banc)
À minuit, pour chacun : voisins à 10 m, dont parents directs (mère, père, fratrie), affinité
moyenne des autres, voisins malades. Acceptation 1 an × 8 runs (régime : adultes, enfants
nés dans l'année).
1. ≥ 8 voisins à 10 m par personne, dont ≤ 3 parents : la contagion « de foyer » touche le
   camp entier. Réfuterait : ≤ 3 voisins (alors c'est la probabilité par paire, pas le cercle).
2. Affinité moyenne des non-parents ≥ 0,4 (le camp est fait de liens forts).
Seuil : si les non-parents font ≥ 60 % des voisins, MOR-5c1 pondère la contagion par le lien
(parents : plein ; autres : selon l'affinité) ; sinon la portée n'est pas en cause.

### MOR-5c0 — mesuré (acceptation 1 an, 8 runs, 3de9a2b)
À minuit, à 10 m : **7,6 voisins** par personne, dont **0,1 parent direct** ; affinité moyenne des
autres **0,41** ; 1,07 voisin malade. Contrôle : morts de maladie identiques à MOR-5b.
1. ≥ 8 voisins dont ≤ 3 parents : **tenue** à la marge (7,6 ; 0,1). 2. Affinité ≥ 0,4 : **tenue**.
Régime : à 1 an, les fondateurs ne sont pas apparentés ; le foyer n'existe qu'entre une mère et
son nourrisson. Seuil atteint (non-parents ~99 % des voisins) → MOR-5c1.
Ordre de grandeur (calcul) : par épisode de 3-4 nuits, R ≈ 19 % × 7,6 ≈ 1,4 (digestive) et
34 % × 7,6 ≈ 2,6 (respiratoire) — des chaînes qui s'entretiennent.

## MOR-5c1 — la contagion suit le lien (2026-10-17)
Changement (`disease::daily`) : par malade couché à 10 m, chance β × contact ; contact 1 pour
le foyer (parents directs, fratrie, les deux parents d'un même enfant), l'affinité du lien pour
les autres, 0 pour un inconnu. Test `la_contagion_de_nuit_suit_le_lien`, vu rouge (lien 0,5 :
9,7 % par nuit au lieu de ~5 %). Les contacts du jour (travail, partage) ne sont pas encore
simulés : jalon suivant possible si la mesure laisse un manque.
Prédictions, acceptation 5 ans × 8 runs contre MOR-5a (même scène, 5 ans) — R passe à ~0,6
(digestive) et ~1,1 (respiratoire) :
1. Adultes 15-39 : diarrhées ≤ 3/an (6,5 ; eau ~1,7). Réfuterait : ≥ 4,5.
2. Adultes : respiratoires 1,5-5/an (7,3) ; réfuterait ≥ 6.
3. Maladie 15-39 ans : 53 → ≤ 25 ‰/an.
4. < 1 an : épisodes en baisse de ≤ 50 % (le nourrisson reste au contact plein de sa mère).
5. Mortalité 1-4 ans en baisse sur ≥ 6/8 runs ; mourir avant 15 ans < 69 %.
Voisins à surveiller : population plus forte ⇒ faim, soif, camp (rien de prédit en chiffre).

### MOR-5c1 — mesuré (acceptation 5 ans, 8 runs, 8f8d855) contre MOR-5a
| | MOR-5a | MOR-5c1 |
|---|---|---|
| adultes 15-39 : diarrhées ; respiratoires /an | 6,5 ; 7,3 | **4,8 ; 4,6** |
| part venue d'un malade (dig. ; resp.), adultes | 69 % ; 94 % (1 an) | 49 % ; 86 % |
| < 1 an : épisodes /an | 14,5 | 7,5 |
| maladie 15-39 ans (‰/an) | 53 | **24** |
| mortalité < 1 an ; 1-4 ans (‰/an) | 317 ; 213 | 249 ; **77** |
| 1-4 ans par run | 100, 105, 128, 358, 197, 556, 220, 172 | 19, 85, 56, 221, 118, 121, 40, 29 (**8/8 en baisse**) |
| mourir avant 15 ans | 69 % | **43 %** (réel ~40 %) |
| morts de maladie ; de soif (8 runs) | 178 ; 44 | 113 ; 21 |
| population finale (8 runs) | — | 54-100 (départ 40 / 60) |
1. Diarrhées adultes ≤ 3/an : **réfutée** (4,8 ; seuil de réfutation 4,5). La contagion n'en fait
   plus que la moitié ; l'eau, elle, monte (~1,7 → ~2,4/an : des camps plus peuplés autour des
   sources, 12,1 voisins à 10 m contre 7,6 à 1 an).
2. Respiratoires 1,5-5/an : **tenue** (4,6), mais 86 % viennent encore d'un malade.
3. Maladie 15-39 ≤ 25 ‰ : **tenue** (24). 4. < 1 an, baisse ≤ 50 % : **tenue** (−48 %).
5. 1-4 ans en baisse sur ≥ 6/8 : **tenue (8/8)** ; mourir avant 15 ans 43 %.
Reste : la maladie adulte à ~24 ‰ contre ~5-7 réels, faite pour moitié de respiratoire
contagieux. Violence en hausse (9 → 16 morts adultes) — plus de monde ; non lu.
**Ce que j'ai pu manquer** : l'affinité sert ici d'intensité de contact, alors qu'elle naît des
rencontres (ce n'est pas la même grandeur : deux chasseurs qui chassent ensemble ont un lien
fort et un contact de jour qui n'est pas simulé) ; les 5-14 ans restent hors de vue ; la hausse
des diarrhées d'eau vient peut-être de la seule taille des camps (non séparée).
**Suite, MOR-5c2** (piste 1 de l'utilisateur, appuyée pour la seule voie respiratoire) : la
contagiosité respiratoire suit la charge (gravité) ; la digestive non (excrétion de semaines,
maximale après les symptômes).

## MOR-5c2 — la contagiosité respiratoire suit la charge (2026-10-18)
Changement (`disease::daily`) : pour la seule voie respiratoire, la chance par nuit est
multipliée par min(1, gravité / gravité d'apparition) ; la digestive est inchangée. Test
`la_contagion_respiratoire_suit_la_charge`, vu rouge (charge 0,02 : 9,7 % par nuit au lieu
de ≤ 3,5 %). Choix non sourcé en chiffres : la charge pleine dès l'apparition (le pic grippal
est aux jours 1-2, la gravité du modèle part de 0,1 et décroît chez un hôte qui l'emporte).
Calcul : chez un adulte qui guérit (gravité ÷ 10 en ~3,3 j), la contagiosité moyenne de
l'épisode tombe à ~0,4 ; chez un enfant dont la gravité monte, peu de changement.
Prédictions, acceptation 5 ans × 8 runs contre MOR-5c1 :
1. Adultes 15-39 : respiratoires ≤ 2,5/an (4,6) ; part contagieuse ≤ 70 % (86 %).
   Réfuterait : ≥ 3,5/an.
2. Maladie 15-39 ans ≤ 15 ‰ (24).
3. Diarrhées adultes inchangées à ±15 % (4,8) — la digestive n'est pas touchée (à part par
   moins de malades affaiblis).
4. Mourir avant 15 ans : en baisse, mais moins que les adultes (contagion d'enfant à gravité
   montante) ; 1-4 ans en baisse sur ≥ 5/8 runs.

### MOR-5c2 — mesuré (acceptation 5 ans, 8 runs, fa42221) contre MOR-5c1
| | MOR-5c1 | MOR-5c2 |
|---|---|---|
| adultes 15-39 : diarrhées ; respiratoires /an | 4,8 ; 4,6 | 4,8 ; **2,0** |
| part venue d'un malade (dig. ; resp.), adultes | 49 % ; 86 % | 49 % ; 56 % |
| maladie 15-39 ans (‰/an) | 24 | **19** (dig. 19 morts, resp. 11) |
| mortalité 15-39 ans toutes causes (‰/an) | 38 | 39 (maladie 30, violence 16, faim 7, soif 6) |
| mortalité < 1 an ; 1-4 ans (‰/an) | 249 ; 77 | **127** ; 70 |
| 1-4 ans par run | 19, 85, 56, 221, 118, 121, 40, 29 | 18, 48, 18, 111, 86, 122, 52, 98 |
| mourir avant 15 ans | 43 % | **33 %** |
| morts de maladie ; de soif (8 runs) | 113 ; 21 | 84 ; 16 |
1. Respiratoires ≤ 2,5/an, part contagieuse ≤ 70 % : **tenue** (2,0 ; 56 %).
2. Maladie 15-39 ≤ 15 ‰ : **réfutée** (19 ; la digestive en fait désormais les deux tiers).
3. Diarrhées inchangées : **tenue** (4,8). 4. 1-4 ans en baisse sur ≥ 5/8 : **tenue de justesse**
   (5/8 ; f1337 et f2024 en hausse, f7 égal).
**L'autre bout** : la mortalité < 1 an (127 ‰) passe **sous** le réel (~200 ‰, Gurven et Kaplan)
et mourir avant 15 ans (33 %) sous ~40 % — à lire comme un manque d'autres causes (accidents,
accouchement, infanticide : MOR-6, MOR-7), pas comme une raison de rendre la maladie plus
forte (règle 10). Les adultes restent à ~39 ‰ toutes causes contre ~10 réels : maladie
digestive (4,8 épisodes/an contre ~2-3), violence (16 morts), faim.
**Suite** : piste 3 (plusieurs germes par voie, immunité propre à chacun) — vise la diarrhée de
l'adulte, qui se réinfecte dès ses 30 j de convalescence. Choix de forme et de constantes à
valider avec l'utilisateur (ci-dessous).

## MOR-5c3 — plusieurs germes par voie (proposition, 2026-10-18)
Lecture : une voie = un germe ; après un épisode, 30 j d'immunité contre **toute** la voie, puis
plus rien (`IMMUNE_DAYS`) ; l'expérience (`episodes`) aguerrit la défense, pas l'exposition.
Réel : ~6 germes principaux de la diarrhée (GEMS) ; immunité de génotype ~2,5 ans
(norovirus, 4-8 ans homotypique), courte pour le rotavirus (anticorps 100-150 j) mais qui
protège de la forme sévère ; ~160 rhinovirus.
Forme proposée : chaque infection porte un germe tiré parmi K de sa voie (l'eau en tire un au
hasard, la contagion transmet celui du malade) ; l'immunité se garde **par germe**, pour une
durée propre à la voie ; la convalescence de 30 j reste (fatigue, défense). Constantes à
décider : K digestive (6, GEMS) et durée (~2,5 ans ?), K respiratoire (grand : l'immunité par
germe y compte peu) — elles se tirent des sources, ne se règlent pas sur la mortalité.

## Débat (2026-10-18, contre-lecture de MOR-5, contradicteur Opus, un tour)
1. *< 1 an −49 % contre −21 % chez l'adulte : la prédiction 4 de MOR-5c2 (« les enfants
   baissent moins ») est contredite et non relevée.* — **Retenu** : prédiction 4 **réfutée** sur
   sa première moitié. Action : tracer la source de chaque infection (âge, lien du malade).
2. *5/8 arrive une fois sur trois par hasard ; « mourir avant 15 ans 33 % » n'est pas établi.* —
   **Retenu.** Seul MOR-5c1 sur les 1-4 ans (8/8) est solide. Action : 4 seeds de plus.
3. *`water_risk` croît linéairement avec les humains à 100 m, sans plafond : toute baisse de
   mortalité grossit les camps et ramène des diarrhées (boucle compensatrice) ; MOR-5c3 la
   masquerait.* — **Retenu**, en amont de MOR-5c3. Action : diarrhées d'eau par adulte selon la
   taille du camp ; référence physique à sourcer (souillure d'un point d'eau : accumulation,
   lessivage, débit). Question de mécanisme à l'utilisateur.
4. *19 ‰ pour ~6,8 épisodes : ~0,28 % par épisode, le double du calcul nourri ; H3 (faim de
   l'heure) jamais jugée.* — **Retenu.** Action : faim au décès contre faim à l'infection.
5. *L'affinité monte avec l'ancienneté du camp (contagion non stationnaire) ; un inconnu vaut 0 :
   aucune contagion par un arrivant, porte tout ou rien sans impossibilité physique (règle 10).*
   — **Retenu.** Action : affinité des voisins de nuit par année ; question à l'utilisateur (un
   inconnu couché à côté doit pouvoir contaminer).
6. *Témoin non apparié au sens fort : écarts par run énormes, des sommes sur 8 runs ne séparent
   pas 20 %.* — **Retenu en partie** : les seeds et scènes sont les mêmes, mais les trajectoires
   divergent. Action : grandeurs rapportées à la population run par run.
7. *Instrument : `defense(age, false, …)` ignore le lait (défense des < 1 an biaisée vers le
   bas, H2 jugée en partie là-dessus) ; létalité des enfants biaisée par sélection.* —
   **Retenu.** Action : compter le lait (posé avant la physiologie, lisible à l'heure) ;
   létalité par rang d'épisode.
8. *MOR-5c3 : une immunité de ~2,5 ans est de l'ordre de l'horizon (règle 8) ; fondateurs naïfs
   par germe ; risque de passer sous la référence.* — **Retenu** pour MOR-5c3 : prédictions aux
   deux bouts, run longue ≥ 15 ans sur 4 seeds.
Ordre qui en sort : instrument (source de l'infection, faim au décès, lait, rang d'épisode,
eau selon la taille du camp, affinité par an) → mesure sur 8 + 4 seeds → décisions de
l'utilisateur sur l'eau et l'inconnu → MOR-5c3.

### Mesures du débat de MOR-5 — prédictions (2026-10-18, avant le banc)
Instrument : la source de chaque contagion (`Infection::source`, un malade tiré selon sa part
de l'exposition, après l'infection et sans effet sur elle), le lait compté dans la défense
(`Illness::nursed`), la faim au décès, la létalité par rang d'épisode (< 5 ans), les
diarrhées d'eau des adultes selon la taille du camp (humains à 500 m), l'affinité par année.
Acceptation 5 ans, seeds 42, 7, 1337, 2024 (contrôle : morts de maladie par run identiques à
MOR-5c2) et 11, 23, 99, 314 (puissance).
1. < 1 an : ≥ 70 % des contagions viennent du foyer (la mère) ; adultes : ≤ 30 %.
2. Faim au décès de maladie (adultes) ≥ 0,5 en moyenne (0,30 à l'infection) : H3. Réfuterait :
   ≤ 0,35.
3. Enfants : létalité au rang 0 ≥ 2 × celle du rang 3+ (sélection).
4. Diarrhées d'eau des adultes : camp de 60+ ≥ 2 × camp de < 10.
5. Affinité hors foyer des voisins de nuit en hausse de l'an 1 à l'an 5.
6. Défense des < 1 an, lait compté : ≥ 0,45 (0,33 sans).

### Mesures du débat de MOR-5 — résultats (acceptation 5 ans, 12270ce ; a = 42, 7, 1337, 2024 ; b = 11, 23, 99, 314)
Contrôle : série a identique à MOR-5c2 run par run (morts de maladie 3, 6, 4, 12, 14, 18, 11,
16) — l'instrument ne touche pas la simulation.
| | a | b |
|---|---|---|
| contagions venues du foyer : < 1 an ; 1-4 ; adultes | 45-51 % ; 42 % ; 10-14 % | 44-50 % ; 40-44 % ; 11-14 % |
| contagions venues d'un enfant : < 1 an ; adultes | 20-29 % ; 20-34 % | 18-27 % ; 21-36 % |
| faim au décès de maladie, 15-39 (à l'infection) | 0,44, 27 % > 0,5 (0,30) | 0,45, 37 % > 0,5 (0,32) |
| létalité < 5 ans par rang 0 ; 1 ; 2 ; 3+ | 2,9 ; 2,5 ; 0,0 ; 0,9 % | 4,1 ; 2,1 ; 1,6 ; 0,3 % |
| diarrhées d'eau des adultes /an, camp < 10 ; 10-29 ; 30-59 ; 60+ | 1,76 ; 2,38 ; 2,84 ; 3,14 | 1,80 ; 2,18 ; 2,76 ; 3,16 |
| affinité hors foyer des voisins de nuit, an 1 → an 5 | 0,40 → 0,30 | 0,41 → 0,33 |
| défense des < 1 an, lait compté | 0,52-0,57 (0,33 sans) | 0,52-0,56 |
| mourir avant 15 ans ; mortalité 15-39 (‰/an) | 33 % ; 39 | 39 % ; 93 (froid 99 éteint : 50 morts de froid) |
1. < 1 an ≥ 70 % du foyer : **réfutée** (~50 % ; un quart vient d'autres enfants) ; adultes
   ≤ 30 % : **tenue** (10-14 %). Les enfants transmettent un quart à un tiers des contagions.
2. Faim au décès ≥ 0,5 : **ni tenue ni réfutée** (0,44-0,45, contre 0,30 à l'infection) : un
   effet de la faim pendant l'épisode, modéré ; H3 reste ouverte.
3. Rang 0 ≥ 2 × rang 3+ : **tenue** (3-12 fois) — la létalité moyenne des enfants est tirée vers
   le bas par les survivants aguerris.
4. Eau : camp de 60+ ≥ 2 × camp de < 10 : **réfutée de peu** (×1,8), mais la hausse est monotone
   sur les deux séries : la boucle « population → diarrhées » existe.
5. Affinité en hausse : **réfutée** — elle baisse puis se stabilise dès l'an 2 : la contagion
   par le lien est stationnaire sur 5 ans.
6. Défense des < 1 an ≥ 0,45 lait compté : **tenue** (0,52-0,57) ; H2 avait été jugée sur un
   chiffre biaisé (0,33), le jugement « pas trop létal par épisode » n'en est que renforcé.
Série b contre l'ancienne mesure des mêmes seeds (cueillette + MOR-4, avant MOR-5) : 1-4 ans
302 → 86, 24 → 57, 150 → 98, 132 → 77, 244 → 91, 81 → 101, 91 → 107 : **4/7 en baisse** — l'effet
de MOR-5c1 + 5c2 sur les 1-4 ans ne va pas dans le même sens partout ; seul MOR-5c1 sur la série a
(8/8) est établi. Froid 99 s'éteint (50 morts de froid, adultes) : voisin à lire (non lié à la
maladie : 1 mort de maladie).
En attente : décisions de l'utilisateur sur l'eau (souillure comme état du point d'eau) et sur
l'inconnu (contact minimal de qui dort à côté) ; puis MOR-5c3.

## Décisions de l'utilisateur (2026-10-18)
1. Eau : « pour cet ajout de réalisme, à dimensionner pour ne pas impacter les performances ».
2. Inconnus : « une contagion minimale doit être appliquée s'ils dorment proches ».
3. Ensuite, une run longue arrêtée quand le débit tombe à 1 tps, avec deux foyers de départ
   plus ou moins proches et plus d'humains.

## MOR-5c4 — le contact minimal de qui dort à côté (2026-10-18)
Changement : contact = max(affinité, 0,01) hors foyer (norovirus : attaque entre foyers
~1/100 de celle du foyer, Tsang 2018 ; même part faute de mieux pour la voie respiratoire).
Test `la_contagion_de_nuit_suit_le_lien` étendu, vu rouge (inconnu : 0 au lieu de ~0,1 %/nuit).
Prédictions, acceptation 1 an × 8 runs contre MOR-5c2 (même scène, mesures du débat) :
1. Effet presque nul dans une bande (les voisins de nuit ont une affinité moyenne de 0,3) :
   épisodes par âge et morts de maladie à ±5 %. Réfuterait : > 15 %.
Le mécanisme compte pour les rencontres entre bandes (deux foyers de départ, migrants) : c'est
la run longue qui le verra.
Instrument ajouté : gorgées par personne et par jour (dimensionne la souillure de l'eau).
