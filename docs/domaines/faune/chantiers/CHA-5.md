# CHA-5 — le gibier perçu suit la nature, pas l'histoire de la boîte

Ouvert le 2026-11-01 depuis la lecture de « suivre le gibier » (MAR.md, section CHA-5 : la
bande ne vide pas son pays ; le gibier n'existe que dans le périmètre de 8 km autour des
humains, et ce périmètre reste coincé en bas sur 5 runs sur 8).

**Décision de l'utilisateur (2026-11-01)** : la faune d'abord. Le gibier qu'un humain perçoit
doit suivre le climat et la surface (règle 10, « la nature reste la nature »), pas l'histoire
de la zone simulée autour de lui. Voisin : D9 (CODE.md §2, immigration placée près d'un humain).

## Lecture du code (`main` 3c42b52)

- Densité naturelle du modèle : chaque maille de 2 km produit un fourrage fixé par son biome
  (`Rangeland::production`, kg/jour) ; la natalité équilibre la mortalité quand la demande
  (têtes × ration) égale la production (`zone_satiation`). La capacité d'une maille est donc
  production / ration : c'est la référence « nature » interne au modèle.
- Le périmètre : un troupeau ne peut pas faire un pas qui l'éloigne au-delà de 8 km de tout
  humain (bord à flux nul, `crosses_perimeter`) ; abandonné au-delà de 16 km ; immigration
  d'un troupeau de 25 têtes avec une chance de 0,15 par jour, autour d'un humain tiré au
  hasard, si aucun troupeau à 3 km et biomasse ≥ 40 ; meutes de même, autour d'un troupeau
  (0,06 par jour).
- Le bord à flux nul suppose « dehors, la même densité qu'ici » : un effondrement de la
  boîte vaut effondrement du pays.

## Instrument (observation seule)

`Sim::fauna_flows` (`fauna::FaunaFlows`) : croît (naissances − mortalité de fond, fissions
remises), prises demandées par les meutes et par les humains, têtes réellement ôtées, restes
dissous sous `HERD_MIN`, abandonnés, immigrés, prédateurs immigrés. Banc `longue` :
`<out>.faune.csv` par jour, avec les mailles du périmètre, la production et la demande de
fourrage qui s'y trouvent (occupation = demande / production, 1 = capacité), la satiété
moyenne pondérée par les têtes, les têtes à 4,5 et 10 km du camp principal.
Identité de la trajectoire : `longue` seed 42 tempéré, 1 an, binaire `main` contre
instrumenté (`out/cha5/id_*`).

## Témoin — prédictions (écrites avant la mesure)

Banc : `longue` 5 ans, 4 seeds × 2 scènes, départ jour 135, `main` + instrument. Régime : une
bande de départ, la scène d'acceptation ; statistiques des ans 3 à 5.

- **P1 bilan** : tetes(j) − tetes(j−1) = croît − ôté − dissous − abandonnés + immigrés, à
  1 % près des flux cumulés, sur 8/8. Sinon il manque un flux : on le trouve avant de lire
  quoi que ce soit.
- **P2 deux régimes** : sur ≥ 3/8 runs, occupation < 0,1 (« en bas ») ; sur les autres > 0,4.
  Réfutée si toutes les occupations sont dans une même bande.
- **P3 la cause du bas** : sur les runs en bas, la prédation pèse ≥ 3 fois la chasse humaine,
  et l'immigration fait ≥ 50 % des entrées brutes (croît + immigrés) : la boîte vit
  d'immigration, pas de reproduction. Réfutée si l'occupation basse va avec une satiété basse
  (< 0,5 : le pays est pauvre, pas la boîte piégée).
- **P4** : la chasse humaine fait < 5 % des sorties de têtes sur 8/8 (l'épuisement par la
  chasse est exclu).
- **Seuil de décision** : P2 et P3 tenues → le défaut est le piège de la boîte fermée ; on
  propose à l'utilisateur la forme de l'échange au bord (avant tout code). P3 réfutée par la
  satiété → le pays est pauvre là où vit la bande, et la question revient côté humains (suivre
  le gibier, chasser sans faim).

**Amendement de P2, avant le témoin** (vu au contrôle d'identité, seed 42 tempéré, jour 1) :
le périmètre initial (82 mailles, 328 km²) produit 52 000 kg de fourrage par jour pour une
demande de 1 663 kg : **l'état initial occupe 3 % de la capacité** (554 têtes pour ~17 000 cerfs
possibles, ~52 par km²). Les seuils de P2 étaient posés sans cette échelle. P2 révisée : en bas
= occupation < 0,05 (sous l'état initial), en haut > 0,15. Bilan de l'instrument sur 74 jours :
résidu 2,7 têtes pour 646 têtes de flux (0,4 %), P1 tient en vue.

## Témoin — mesuré (`longue` 5 ans × 8, binaire `longue-instr` = f255a73, départ j135 ; ans 3 à 5)

Sorties : `out/cha5/t5_<scène>_<seed>.csv.faune.csv`. Têtes : médiane ; /km² sur l'aire du
périmètre ; p/P = têtes par prédateur ; flux cumulés sur les trois ans ; imm % = immigrés /
(immigrés + croît) ; ch % = chasse humaine / sorties (ôté + dissous + abandonnés).

| run | résidu % | occupation | têtes | /km² | prédateurs | p/P | satiété | croît | immigrés | imm % | prédation | chasse | ch % | abandonnés |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| f1337 | 0,6 | 0,209 | 1 317 | 4,57 | 16,3 | 96 | 0,62 | 3 433 | 625 | 15 | 871 | 486 | 25,6 | 520 |
| f2024 | 0,3 | 0,001 | 78 | 0,08 | 14,6 | 6 | 0,81 | 746 | 3 525 | 83 | 630 | 200 | 4,3 | 3 793 |
| f42 | 0,2 | 0,051 | 855 | 3,09 | 19,4 | 45 | 0,87 | 6 776 | 2 225 | 25 | 1 011 | 58 | 0,6 | 8 320 |
| f7 | 0,3 | 0,004 | 55 | 0,14 | 8,4 | 12 | 0,78 | 1 023 | 3 225 | 76 | 361 | 57 | 1,3 | 3 819 |
| t1337 | 0,4 | 0,024 | 288 | 0,83 | 8,4 | 43 | 0,87 | 2 676 | 1 625 | 38 | 435 | 1 608 | 37,5 | 2 150 |
| t2024 | 0,1 | 0,085 | 4 410 | 9,07 | 30,2 | 143 | 0,77 | 22 565 | 1 200 | 5 | 1 621 | 55 | 0,2 | 21 816 |
| t42 | 0,2 | 0,040 | 1 189 | 2,02 | 19,6 | 72 | 0,81 | 8 655 | 1 250 | 13 | 981 | 1 198 | 18,0 | 4 460 |
| t7 | 0,2 | 0,028 | 119 | 0,33 | 11,3 | 42 | 0,78 | 2 945 | 2 600 | 47 | 524 | 80 | 1,3 | 5 457 |

**Verdicts :**
- **P1 tenue** : résidu du bilan ≤ 0,6 % sur 8/8.
- **P2 réfutée dans sa forme** : pas deux modes, un continuum — 5/8 sous 0,05, une seule run
  au-dessus de 0,15 (f1337), deux entre (f42 0,051, t2024 0,085). Toutes très au-dessous de la
  capacité de fourrage.
- **P3 réfutée** : la prédation ne domine pas (≥ 3 fois la chasse sur 3/5 runs basses
  seulement) ; l'immigration fait ≥ 50 % des entrées sur 2/5. La satiété est haute partout
  (0,62-0,87) : le pays n'est pas pauvre, et les meutes ne piègent pas la boîte.
- **P4 réfutée** : la chasse humaine fait 18 à 38 % des sorties sur 3/8 (t1337, f1337, t42).
  Sur `main`, la chasse pèse donc dans certains pays (≠ la lecture faite sur l'ancien binaire
  `moyenne`).

**Ce qui domine, et que je n'avais pas prédit : l'abandon.** La première sortie de têtes, sur
7/8 runs, est le retrait des troupeaux laissés à plus de 16 km de tout humain
(`FAUNA_ABANDON_TILES`) : 2 à 13 fois la prédation (t2024 21 816 têtes abandonnées pour 1 621
prises par les meutes ; f42 8 320 pour 1 011). Sur t2024, l'abandon avale chaque année ce que
le gibier a fait naître (an 4 : 10 661 abandonnés, 7 147 de croît). t7 : un abandon tous les
dix jours environ, 53 têtes en moyenne. La bande se déplace dans son domaine vital (camp à
1,1 km/jour en moyenne) ; le gibier qu'elle laisse derrière elle **sort de la simulation**, et
celui du pays où elle revient n'est plus que ce que l'immigration (25 têtes, une chance de 0,15
par jour) a remis.

C'est l'écart à la règle 10 sous sa forme la plus nette : le périmètre de simulation décide du
gibier qu'un humain perçoit. Et c'est exactement la plainte de départ de CHA-5 (« la chasse
baisse dans un pays fixe »), avec une autre cause : le pays n'est pas épuisé, il est **effacé**
quand on s'en éloigne.

**Ordres de grandeur réels** (SOURCES.md, de seconde main) : cerf élaphe 12,7 /km² avec loups
(Białowieża) ; wapiti 2-9 ; orignal 2,5 ; renne 0,8-3 ; rapport orignaux / loup 15:1 (anormal)
à 40-50:1. Ici : 0,08 à 9 têtes /km², p/P de 6 à 143. Les runs basses (f2024, f7, t7, t1337)
sont sous toute densité réelle d'ongulés trouvée, sauf le bison et le saïga.

**Ce que j'ai pu manquer** : (1) le régime — une bande de départ, 5 ans ; à deux foyers ou plus
le périmètre est plus large et l'abandon plus rare ; (2) l'abandon peut aussi venir de la
dispersion (un isolé qui part loin emmène le bord et le ramène), pas seulement des levées du
camp — non séparé ; (3) la capacité de fourrage (~52 cerfs /km²) est au-dessus des densités
réelles sans prédateurs (10-35) : si l'on rend au pays sa densité « naturelle », celle du
modèle sans humains n'est pas encore connue.

## Débat (2026-11-01, contre-lecture du témoin, contradicteur Opus)

1. Être la première sortie ne veut pas dire fixer le niveau ; il faut le taux par tête et une
   variante sans abandon. → **Retenu** : taux par tête calculé (abandonnés / an / tête
   médiane : f2024 ~16, f7 ~23, t7 ~15 ; t42 ~1,3, f1337 ~0,1) ; variante de diagnostic
   `FAUNA_ABANDON_TILES` = ∞ (jamais un correctif), prédiction ci-dessous.
2. Deuxième fuite : l'immigration a un plafond par jour, pas par surface (D9). → **Retenu,
   vérifié** sur les CSV (ans 3-5) : arrivées par jour f2024 0,130, f7 0,119, t7 0,096 pour un
   plafond de 0,15 ; f1337 0,023, t2024 0,044. Les runs basses vivent au plafond ; f2024 a le
   plus grand périmètre (1 023 km² contre ~280-590). Écart à la règle 10 distinct de l'abandon.
3. L'abandon peut venir d'isolés, de chasseurs, de fissions qui emmènent le bord puis
   rentrent ; immigrés éphémères ? → **Retenu** : trace de chaque abandon (âge, origine, humain
   le plus proche en clan ou non, distance au foyer le plus proche), `<out>.abandons.csv`.
4. Porte tout ou rien sans délai. → Noté ; à juger avec la trace (âges courts = aller-retour).
5. Occupation et /km² dépendent d'un périmètre qui gonfle. → **Retenu** : on juge sur les têtes
   à 10 km du camp (anneau fixe, 314 km²) : f2024 0,08 /km², f7 0,17, t1337 0,78, t7 1,0, f42
   2,0, t42 2,4, f1337 4,3, t2024 6,9 — même ordre.
6. Régime d'une seule bande. → Noté : la scène à deux foyers avant de généraliser.
- Pièges des options (sommeil + rattrapage : forme close sur un système qui oscille, meutes à
  rattraper aussi, mémoire, réveil par un isolé ; repeuplement à l'arrivée : calibration cachée,
  pays neuf à chaque levée, l'épuisement local disparaît) → à porter dans la question à
  l'utilisateur. Axes proposés : une référence sans humains (densité et p/P par biome) ; un
  champ grossier par maille hors périmètre, intégré pas à pas, qui garde l'histoire ;
  l'immigration proportionnelle à la surface.

## Diagnostics — prédictions (écrites avant)

`longue` 2 ans × 8, départ j135. Binaires : `longue-trace` (instrument de trace, même
trajectoire que `main` — vérifié en comparant aux deux premières années des runs `t5`) et
`longue-sansabandon` (le même + `FAUNA_ABANDON_TILES` = ∞, diagnostic non committé).
- **T1** : sur les runs basses (f2024, f7, t7), ≥ 50 % des têtes abandonnées viennent de
  troupeaux immigrés, âge médian à l'abandon < 90 jours.
- **T2** : l'humain le plus proche au moment de l'abandon est membre d'un clan dans ≥ 70 % des
  abandons (la bande qui se déplace, pas les isolés).
- **S1** : sans abandon, têtes à 10 km du camp (moyenne de l'an 2) ×3 au moins sur les runs
  basses (f2024, f7, t7, t1337) ; dans ±50 % sur f1337 et t2024. Réfutée si < ×1,5 sur 3 des
  4 basses : l'abandon ne fixe pas le niveau.
- Seuil : S1 tenue → l'effacement du gibier hors périmètre est le défaut principal, et la
  question à l'utilisateur porte sur ce que devient le gibier hors de vue. S1 réfutée → c'est
  l'immigration (point 2) qu'on lit d'abord.

## Diagnostics — mesurés (`longue` 2 ans × 8, `longue-trace` = bca63c3 et `longue-sansabandon`)

Identité : `longue-trace` reproduit à l'octet près les deux premières années des runs `t5`
(t42, f7, t2024 comparées).

**S1 tenue sur les runs basses** — têtes à 10 km du camp, moyenne de l'an 2 :

| run | avec abandon | sans abandon | rapport | têtes simulées en fin (avec / sans) | troupeaux | tps 30 j |
|---|---|---|---|---|---|---|
| f2024 | 51 | 1 748 | ×34 | 388 / 17 516 | 10 / 282 | 25,6 / 14,1 |
| f7 | 116 | 1 175 | ×10 | 74 / 44 914 | 3 / 742 | 40,7 / 21,4 |
| t7 | 84 | 3 591 | ×43 | 519 / 43 594 | 12 / 716 | 44,1 / 16,9 |
| t1337 | 54 | 2 357 | ×44 | 154 / 20 399 | 5 / 329 | 55,7 / 24,1 |
| f42 | 367 | 3 869 | ×10,5 | 443 / 17 930 | 8 / 299 | 82,7 / 27,0 |
| f1337 | 223 | 981 | ×4,4 | 528 / 5 027 | 10 / 84 | 36,6 / 31,9 |
| t2024 | 379 | 613 | ×1,6 | 1 449 / 24 810 | 27 / 420 | 36,2 / 29,8 |
| t42 | 1 549 | 452 | ×0,3 | 1 174 / 21 526 | 19 / 353 | 41,5 / 24,8 |

×3 au moins sur les 4 runs basses (×10 à ×44) : **l'effacement du gibier hors périmètre fixe le
niveau du gibier perçu**. La partie « ±50 % sur les runs hautes » est réfutée (f1337 ×4,4,
t2024 ×1,6, t42 ×0,3 : sans abandon, les meutes montent aussi, 10 → 48 sur t42). Sans abandon, la
faune croît sans borne le long du chemin de la bande (5 000 à 45 000 têtes en 2 ans) et le débit
tombe de moitié : l'abandon est une garde de coût, et c'est elle qui décide de ce qu'on voit.

**T1 tenue en partie** : immigrés 55 % (f2024), 60 % (f7), 43 % (t7) des têtes abandonnées ; âge
médian à l'abandon 42, 45, 63 jours. Les fissions font l'essentiel du reste (23-84 %) : le gibier
qui naît près de la bande est effacé à son tour.
**T2 tenue** : l'humain le plus proche est membre d'un clan dans 94-100 % des abandons sur 7/8
(f7 63 %) ; le foyer le plus proche est à moins de 20 km dans 50-93 % — c'est la bande qui
s'éloigne de son gibier, pas des isolés.

**Conclusion du jalon** : le gibier ne s'épuise pas, il est effacé quand la bande s'éloigne de
plus de 16 km, puis remplacé au compte-gouttes par une immigration plafonnée par jour. Deux
écarts à la règle 10, le premier dominant. Question de vision à l'utilisateur : ce que devient
le gibier hors de vue.
