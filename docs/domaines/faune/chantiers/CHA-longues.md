# Runs longues CHA-1 → CHA-2c contre témoin (2026-10-09)

Demande de l'utilisateur : voir tous les derniers changements dans la durée, avec témoin.
Régime : scène **froide** d'`acceptation` (60 agents, foyer 1-5 °C ; c'est là que l'hiver
tuait : morts 49 → 168 sur 3 ans après MAR-2+4), **seed 42 seule**, 10 ans, départ jour
315 (hiver) et 135 (été). Témoin : commit `6ff8294` (MOR, ENE, MAR-1/2/4 faits, avant
CHA-1), même banc instrumenté (`249a623`). Version : `def25be` (CHA-1a, 1b, 2, 2b, 2c).
**Limite** : une seule seed — on lit ce qui se passe sur ce tirage, on n'en tire pas de
sens général (règle 7). Les 3-5 ans d'`acceptation` (4 seeds × 2 scènes) le diraient.

Prédictions (version contre témoin, sur 10 ans) :
1. Têtes chassées par an : plus haut en été les premières années (domaine vital, pistes),
   puis l'écart se réduit ou s'inverse (CHA-2c écarte le gibier des camps fixes).
2. Troupeaux à 10 km du foyer : plus haut au début (domaine vital), plus bas ensuite.
3. Morts de faim d'hiver : moins nombreuses que le témoin les 2 premiers hivers ; incertain
   au-delà (CHA-2c pèse −33 % de prises l'hiver).
4. Marche par adulte : un peu plus basse (pistage au lieu de quête au hasard), < 10 %.
5. Part du noir dormie : inchangée à ±2 points (CHA-2b ne libère pas la nuit — MAR-8).
Réfuteraient : une population éteinte ou effondrée (> 50 %) dans la version et pas dans le
témoin ; des morts de faim plus nombreuses dès le premier hiver.

## Mesuré — départ en hiver (jour 315), 10 ans

| | témoin | CHA-2c |
|---|---|---|
| pop an 1 → an 10 | 69 → **39** | 69 → **63** |
| morts sur 10 ans (soif / froid / maladie / faim) | 80 (23 / 4 / 52 / 0) | 60 (6 / 1 / 52 / 0) |
| morts < 3 ans / < 15 ans | 24 / 44 | 20 / 31 |
| têtes chassées par an (an 1 ; ans 8-10) | 19 ; 15 | 84 ; 37-72 |
| troupeaux à 10 km (an 1 → an 10) | 2,9 → 3,2 | 8,1 → 39,1 |
| faune totale en fin de run | 11 troupeaux, 643 têtes | 81 troupeaux, 4 996 têtes |
| marche par adulte | 7,7-8,3 h | 7,6-7,8 h |
| part du noir dormie | 0,51-0,56 | 0,48-0,49 |
1. Plus de prises au début : **tenu** (×4) ; l'écart ne se réduit pas : **réfuté** (×2 à ×5 sur 10 ans).
2. Troupeaux proches : plus haut au début **tenu** ; plus bas ensuite **réfuté** — ils
   montent (8 → 39) : la faune résidente croît (CHA-1b) plus vite que la peur ne l'écarte.
3. Morts de faim : **0 des deux côtés** — la prédiction ne départage rien. L'écart de morts
   vient de la **soif** (23 contre 6) ; non prédit, à expliquer.
4. Marche un peu plus basse : tenu (−2 à −5 %).
5. Noir dormi à ±2 points : **réfuté de peu** (−3 à −6 points) ; sens non prédit.
Témoin : la faune s'effondre l'hiver (643 têtes, migrations de toutes les espèces avant CHA-1b).

## Mesuré — départ en été (jour 135), 10 ans

| | témoin | CHA-2c |
|---|---|---|
| pop an 1 → 2 → 10 | 62 → **34** → 25 | 71 → 73 → **47** |
| an 2 (premier hiver plein) : faim / soif / froid / maladie | **7 / 10 / 5 / 14** | 0 / 1 / 0 / 6 |
| morts sur 10 ans ; naissances | 71 ; 36 | 67 ; 54 |
| têtes chassées par an (an 1 ; ans 8-10) | 44 ; 7-11 | 169 ; 46-85 |
| troupeaux à 10 km (an 1 → an 10) | 0,8 → 5,1 | 4,3 → 32,4 |
| faune fin de run | 29 troupeaux, 1 680 têtes | 55 troupeaux, 3 415 têtes (plateau ~3 000-3 700 dès l'an 5) |
| marche ; noir dormi | 7,5-8,1 h ; 0,50-0,56 | 7,4-8,2 h ; 0,49-0,54 |
Prédictions : 1 (plus de prises) tenue, sans rapprochement ; 2 troupeaux proches en hausse
continue (réfuté comme en hiver) ; 3 morts de faim du premier hiver : **tenue** (7 → 0,
et le témoin perd 45 % de sa population cette année-là) ; 4 marche : égale à ±3 % ;
5 noir dormi : égal (± 1 point).

## Conclusion (une seed, scène froide)

- Les chantiers CHA font ce pour quoi ils ont été faits : le gibier reste (faune ×2 à ×8
  en fin de run, plateau dès l'an 5 — bornée), la chasse rapporte 3 à 5 fois plus, et le
  premier hiver ne fait plus d'hécatombe (témoin été : 36 morts l'an 2, pop −45 %).
- CHA-2c n'affame pas ici : sur 10 ans, les troupeaux proches **augmentent** — la croissance
  de la faune résidente l'emporte sur la peur. Le −33 % de prises du banc `marche` (90 j,
  densité de départ) ne se retrouve pas dans la durée sur cette seed.
- Non expliqué : la soif du témoin (23 morts en hiver, 10 l'an 2 en été) contre 6 et 13
  pour CHA ; la population CHA d'été baisse lentement après l'an 2 (73 → 47, 54
  naissances pour 67 morts, maladie en tête) — à surveiller, une seule seed.
- Ce que ces runs ne couvrent pas : la scène tempérée, les autres seeds, l'au-delà de 10
  ans ; la marche reste à 7,5-8 h/j des deux côtés (Hadza 5,8-11,4 km/j : MAR-6/8 à venir).
