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
