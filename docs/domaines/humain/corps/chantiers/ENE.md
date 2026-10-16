# Chantier : bilan énergétique — conception et prédictions

Décision de l'utilisateur (2026-10-05) : le bilan énergétique passe **avant le
sevrage** ; **tous** les coûts y entrent (masse, âge, sexe, activité, froid,
grossesse, allaitement, fièvre). Sources : `out/mortalite/predictions.md`,
section « Sources — bilan énergétique ».

## Lecture du code (adf01e8)

- Apports en kcal depuis D10 ; monnaie de la nourriture = **point de faim**
  (`KCAL_PER_HUNGER` = 5 000 kcal) : stock de clan, viande portée, parts.
- Dépense **unique** : `HUNGER_PER_TICK` = 1/48 → 2 500 kcal/j pour tous,
  quel que soit l'âge (un nourrisson de 1 an dépense comme un chasseur),
  l'activité ou le froid. Grossesse +20 %, allaitement +30 % (`demography`).
- Aucune réserve : faim saturée à 48 h, puis santé −1/336 par heure → mort
  de faim en ~16 j (réel : 30-70 j avec eau).
- Lait : `NURSE_RELIEF` 0,1 point/h × santé de la mère ; la mère paie 0,3 ×
  la dépense standard.
- Sites qui mangent (à convertir) : `resolve_shares` (sim.rs:126), viande
  portée (1602), prise de chasse (1657), cueillette (1818-1835), stock du clan
  (1862), lait (`demography::nurse_infants`).

## Modèle proposé

**Corps** (dérivé de l'âge et du sexe, pas stocké) : masse par une courbe de
croissance (repères Hadza adultes : femmes 43 kg, hommes 51 kg — Pontzer 2012 ;
enfants, ordre de grandeur : 3 kg à la naissance, 8,5 à 1 an, 10,5 à 2 ans,
15 à 5 ans, 24 à 10 ans, 38-40 à 15 ans). Métabolisme de base : équations de
Schofield (FAO/OMS/UNU) selon masse, âge et sexe.

**Dépense horaire** = MB/24 × (coefficient de l'activité + froid + fièvre)
+ grossesse + lait produit :
- coefficients d'activité (multiples du MB, ordre de grandeur FAO 2004,
  annexe) : sommeil 1,0 ; abri 1,2 ; repos, boire 1,4 ; marcher 3,0 (+ 0,5
  en portant un nourrisson) ; cueillir 2,5 ; chasser 3,5 ; cultiver 3,5 ;
  combattre 6 ;
- froid : thermogenèse proportionnelle au déficit ressenti (choix de forme ;
  plafond de l'ordre de 2 × MB en frisson) ;
- fièvre : +0,2 × MB × gravité de l'infection (~+10-13 %/°C) ;
- grossesse : +85 / +360 / +475 kcal/j par trimestre (FAO 2004) ;
- lait : la mère paie l'énergie du lait qu'elle produit (~520 kcal/j en
  exclusif : 780 g × 0,67 kcal/g ; FAO compte +505).

**Faim et réserves** : la faim reste la jauge [0, 1] que lit le cerveau,
mais à l'échelle de chacun : 1 = deux jours de **son** besoin de référence
(MB × 1,75, PAL des femmes hadza). Au-delà, la dépense puise dans les
**réserves** (graisse au-dessus de la graisse essentielle : femmes 21 % → 10 %,
hommes 13,5 % → 3 %, × 7 700 kcal/kg). Réserves vides → la santé s'érode
comme aujourd'hui (phase protéique, ~2 semaines). Manger comble d'abord la
faim puis les réserves (on mange davantage après une disette).
La nourriture reste comptée en points de 5 000 kcal ; chaque mangeur les
convertit à son échelle.

**Lait (a)** : plein tant que la mère vit ; réduit en proportion de
l'épuisement de ses réserves (Prentice : −11 % pour −31 % d'apport ; tari
seulement réserves vides). La maladie n'y touche plus directement.

**Fécondité** : la porte « santé > 0,6, faim < 0,85 » reste, mais lit les
réserves au lieu de la santé (changement séparé, si la mesure le demande).

## Prédictions (avant mesure ; banc `nourriture` + `acceptation` 3 ans)

1. PAL émergent (dépense ÷ MB, moyenne sur l'année) chez les adultes : 1,6 à
   2,4 ; si > 3, les agents marchent bien plus que des Hadza (5,8-11,4 km/j) —
   un défaut d'aval à mesurer (km/j par adulte).
2. Dépense moyenne d'un adulte 1 800-2 800 kcal/j (contre 2 500 fixe) ;
   enfants de 1-4 ans 600-1 000 kcal/j.
3. Morts de faim : **plus tardives** (aucune avant 25 jours de faim saturée)
   et **moins nombreuses** sur ≥ 3 seeds sur 4 par scène.
4. La population croît plus vite qu'au chantier de la maladie (enfants moins
   coûteux) — à vérifier sur 4 seeds × 2 scènes.
5. Voisin attendu : les densités et famines validées par D10 bougent ; D10
   devra être remesuré (banc `nourriture`).
Réfutation : PAL > 3 ou < 1,4 en moyenne ; morts de faim en hausse.

## Étapes codées

1. **Dépense** (5e5671d) — banc `depense_3ans` (lancé 13 h 34).
2. **Réserves** (6b0a35d) — plancher de faim après disette (0,5 × part perdue,
   choix : sans lui, manger refaisait les réserves avant d'apaiser la faim et
   une femme qui se refait restait « affamée » donc inféconde). Voisin vu au
   test : conceptions 1/1/3/3 → 0/1/2/1 sur 4 seeds × 45 j — on endure la faim
   saturée sur ses réserves, et la porte de fécondité lit la faim.
3. **Lait (a)** (commit suivant) — `milk_flow` suit les réserves.

## Prédictions — réserves (avant le banc)

1. Morts de faim : aucune, ou plus tardives (pas de mort de faim sous ~25 j de
   faim saturée).
2. Part du temps à faim saturée **en hausse** (on vit sur ses réserves) ;
   naissances **en baisse** sur ≥ 3 seeds sur 4 par scène (porte de
   fécondité sur la faim) — le voisin à corriger ensuite (Ellison).
3. Morts de soif des nourrissons : inchangées (le lait suit encore la santé).

## Prédictions — lait (a) (avant le banc)

1. Nourrissons morts de soif « mère sans lait » : → ~0 sur 8/8 runs ; les
   orphelins restent.
2. Mortalité < 1 an et 1-4 ans en baisse sur l'agrégat.

## Résultats (3 ans, 4 seeds × 2 scènes, départ jour 135) — mesurés

Témoin : run « plaies » du chantier mortalité (3e3c81c).
| | témoin | dépense | + réserves | + lait (a) |
|---|---|---|---|---|
| morts | 68 | 62 | 42 | 49 |
| dont maladie | 46 | 45 | 31 | 31 |
| dont soif (nourrissons « mère sans lait ») | 10 (3) | 12 | 7 (0) | 10 (0) |
| naissances | 200 | 203 | 191 | 190 |
| < 1 an ‰/an | 169 | 157 | 106 | 94 |
| 1-4 ans ‰/an | 101 | 111 | 40 | 38 |
| 15-39 ‰/an | 23 | 18 | 17 | 24 |
| mourir avant 15 ans | 44 % | 45 % | 23 % | 22 % |
Morts de faim : 0 partout.

Dépense mesurée (identique aux trois étapes) : femmes 2 620-2 800 kcal/j
(Hadza 1 877), hommes 3 000-3 240 (2 649) ; PAL femmes 2,25-2,40 (1,78),
hommes 2,06-2,22 (2,26) ; < 3 ans 570-660 kcal/j. **Marche : 33-40 km/j
pour les deux sexes (Hadza : 5,8 et 11,4)** — 8 à 10 h de marche par jour.

Prédictions :
- PAL adultes 1,6-2,4 : tenue de justesse ; mais la marche est 3 à 7 fois
  trop longue : **défaut d'aval révélé** (pas d'énergie, de comportement),
  et il gonfle la dépense des femmes de ~45 %.
- Dépense adulte 1 800-2 800 : femmes oui, hommes au-dessus (3 000-3 240).
  Enfants 1-4 ans 600-1 000 : < 3 ans 570-660, juste sous la fourchette.
- Réserves, morts de faim plus tardives/moins nombreuses : 0 → 0, non
  testable dans ce régime (aucune famine).
- Réserves, naissances en baisse ≥ 3/4 seeds par scène : **réfutée** (2/4
  en tempéré, 2/4 en froid ; total 203 → 191).
- Réserves, soif des nourrissons inchangée : tenue (aucune « mère sans lait »).
- Effet non prédit des réserves : la maladie tue moins (45 → 31) et les
  1-4 ans meurent 3 fois moins — la faim saturée ne ronge plus la santé que
  la maladie ronge aussi (une seule barre).
- Lait (a), « mère sans lait » → 0 : tenue, mais déjà 0 après les réserves —
  effet propre non distinguable à cette taille ; morts 42 → 49 (froid
  30 → 40) : tirages, à lire sur plus de runs.
Non couvert : enfants de 3-13 ans (aucun à 3 ans de run), famines (régime
repu), synergie faim-maladie toujours invisible (0-1 mort de maladie chez un
affamé).
