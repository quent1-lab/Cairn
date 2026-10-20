# MEM — la mémoire des lieux (chantier ouvert le 2026-10-16)

Décision de l'utilisateur (2026-10-16) : « il manque un concept dans le modèle, ou on mesure
mal ou pas assez longtemps. La mémoire est importante, et pour tous finalement : un humain
sans mémoire ne peut rien faire. » Né de MAR-6b : le camp qui se lève dérive sans fin
((a) 200-585 km, (a'') jusqu'à 1 110 km en 10 ans, populations effondrées), faute de savoir
ce que rendaient les lieux.

## Lecture du code (2026-10-16)

Ce qu'un humain retient (`memory::Memory`) :
- **16 sources d'eau**, les plus proches de là où il est : en s'éloignant, il oublie les
  anciennes (porte par la distance, pas par l'âge ni l'usage). Une bande partie à 20 km ne
  sait plus revenir à ses eaux.
- **Les cellules de 512 m où il a mis les pieds** (4 096 au plus), **sans date ni ce
  qu'elles ont rendu**, jamais transmises.
- **Un troupeau**, le dernier vu (3 jours), et le gîte de la dernière nuit.
- Ce que rapporte **chaque activité** (MAR-6a), mais **pas chaque lieu**.

Ce qu'il sait **sans l'avoir appris** (écart à la réalité : on ne connaît un lieu qu'en y
étant allé ou en l'entendant raconter) :
- `brain` : pour « changer de pays », il lit le stock de plantes **réel** des 8 mailles de
  2 km voisines (`edible_share` → `World::edible_kcal`), même jamais vues.
- `camp` (MAR-6b) : la destination lit le stock réel de la maille de chaque source.

## Plan proposé (à valider)

- **MEM-1 — la carte vécue** (observation seule d'abord) : par maille de 2 km (la maille
  des plantes et des domaines du gibier), la date du dernier passage et ce que le lieu a
  rendu (kcal nettes et heures, par activité), le sien.
- **MEM-2 — ce qu'on se raconte** : aux rencontres, on échange ses lieux comme ses sources
  (avec leur date) ; la carte d'une bande devient commune sans être totale.
- **MEM-3 — les choix lisent la mémoire, plus le monde** : changer de pays, lever le camp,
  partir en quête se décident sur ce qu'on sait des lieux (et sur le temps écoulé : un lieu
  quitté repousse), l'inconnu gardant une valeur d'exploration. C'est là qu'un domaine vital
  peut émerger : on revient où c'était bon.
- **MEM-4 — l'oubli** : les sources s'oublient par l'âge et le non-usage, plus par la
  distance.

Questions de fonctionnement :
1. Maille de 2 km (proposé) ou cellule de 512 m ?
2. Combien de temps on se souvient de ce qu'un lieu a rendu : une saison, un an, la vie ?
   (Les chasseurs-cueilleurs connaissent les ressources saisonnières de leur pays sur des
   années ; faut-il retenir **par saison** ?)
3. MEM-1 et MEM-2 avant de reprendre MAR-6b, puis la cueillette rapportée (MAR-6c) ?

## Dimensionnement et forme (proposition, 2026-10-17, après sourçage)

Décision de l'utilisateur : maille « à dimensionner pour être utile sans trop de
stockage » ; mémoire non binaire — fréquence, impact et intérêt d'un lieu en font durer le
souvenir, parfois à vie (camp, chasse, eau, cueillette, danger).

**Maille : 2 km** (celle des plantes et des domaines du gibier). Un territoire hadza
(~3 600-4 000 km²) fait ~1 000 mailles ; aché (~20 000 km²) ~5 000. À ~32 octets par lieu
retenu, 5 000 lieux = 160 Ko par personne, 16 Mo pour 100 personnes — supportable (une run
de 10 ans occupe ~1,1 Go). En cellules de 512 m, ×16 : trop. Les cellules de 512 m visitées
(`Memory::known`, 4 096 au plus) restent pour l'exploration fine.

**Ce qu'on retient d'un lieu** : la date du dernier passage ; ce qu'il a rendu, par activité
(cueillir, chasser) ; l'eau qu'on y a bue ; le danger qu'on y a vécu (blessure, mort d'un
proche, meute) ; et une **force du souvenir**.

**La force du souvenir** (forme non sourcée en chiffres, sens sourcé) : chaque passage
l'augmente d'autant plus que le lieu a compté (impact : kcal obtenues, soif étanchée,
blessure ; intérêt : rareté de ce qu'il offre). Elle s'efface vite puis lentement, vers un
**plancher qui monte avec les répétitions** : la rétention d'itinéraires reste à ~72-75 %
au bout de 6-12 mois (Nottingham, VR), et les Inuit gardent trois générations de caribou en
mémoire orale. Un lieu vu une fois s'oublie ; un lieu fréquenté ou marquant se garde à vie.
On oublie le lieu quand sa force tombe sous un seuil, ou quand la mémoire est pleine
(le plus faible part le premier : par l'usage, plus par la distance).

**Ce qui se raconte** (MEM-2) : les lieux forts, avec leur âge — la carte d'une bande devient
commune sans être totale (transmission par récits et chants, aborigènes).

Questions pour valider avant MEM-1 : (1) la maille de 2 km ; (2) une forme
« passage = renfort selon l'impact, effacement vers un plancher qui monte avec les
répétitions », à mesurer au banc plutôt qu'à chiffrer d'avance ; (3) le danger dans la
même mémoire.

## Décision de l'utilisateur (2026-10-17, soir)

Validé : maille de 2 km ; forme « passage = renfort selon l'impact, effacement vers un
plancher qui monte avec les répétitions », mesurée au banc ; le danger dans la même mémoire.
« Il faudra chercher et sourcer ce qui pourrait être intéressant de mémoriser » : sourçage
lancé (catégories de lieux, mémoire saisonnière, durée du souvenir, transmission, taille du
territoire connu, refuges de disette) → `SOURCES.md` du domaine.

**Ordre des jalons revu** (relecture des chantiers, 2026-10-17) : MEM-1 (carte vécue), puis
**MEM-4 avant MEM-3** — les sources d'eau passent dans la même force du souvenir (oubli par
l'usage, plus par la distance), ce qui touche la soif et les trajets d'eau (MAR-5) ; puis
MEM-3 (les choix lisent la mémoire : retire l'omniscience de `brain` et de `camp`, donne enfin
un gain à l'exploration — CUR), puis MEM-2 (ce qu'on se raconte).

**Ce que le sourçage apporte (2026-10-17)** : on retient des **ressources par espèce et par
saison** (centaines d'espèces connues ; saisons nommées d'après ce qui est mûr), des
**toponymes** par centaines sur des territoires de 10³-10⁵ km² (même ordre que ~1 000-5 000
mailles de 2 km), et la connaissance s'étend avec l'âge et diffère selon le sexe (les hommes
parcourent ~4 fois plus). Rien de chiffré sur la durée du souvenir d'un lieu ni sur des
lieux-refuges : la forme validée reste à mesurer, pas à régler. Conséquence pour MEM-1 :
retenir le rendu **par saison** (la maille qui rend en été n'est pas celle de l'hiver) est
appuyé par les calendriers ; à proposer comme variante, pas d'emblée.

## MEM-1 — la carte vécue (2026-10-20 ; ordre revu : la mobilité avant MOR-5c3)
Changement (observation seule, aucune décision ne le lit) : `Memory::places`, par maille de
2 km, ce que le lieu a été pour soi — premier et dernier passage, heures, heures à cueillir et
à chasser, kcal acquises par activité, heures à boire, nuits. Noté chaque heure
(`Memory::live_hour`, après `note_visit`) ; plafond de 5 000 lieux (le plus ancien s'oublie,
en attendant la force du souvenir de MEM-4). Non encore : le danger (blessure, mort d'un
proche), la saison. Test `la_carte_vecue_retient_ce_que_chaque_lieu_a_rendu`.
Prédictions, acceptation 1 an × 8 runs (régime : camps immobiles, dispersion ≤ 6 km), contre
MOR-5c5 lancé ensemble :
1. Lieux retenus par adulte : médiane 15-40, p90 ≤ 80.
2. Part des heures dans les 10 lieux les plus vécus : médiane ≥ 85 %.
3. Lieux qui ont nourri : médiane ≤ 15 ; lieux où l'on a dormi : médiane ≤ 3.
4. Contrôle : morts par run identiques à MOR-5c5 (pure observation) ; débit à ±5 %.

### MEM-1 — mesuré (acceptation 1 an, 8 runs, 5316d3b, contre MOR-5c5 lancé ensemble)
Contrôle : survie et morts par run **identiques** (pure observation) ; débit plancher à ±2 %.
| par adulte, en fin d'année (347 adultes) | p10 | médiane | p90 |
|---|---|---|---|
| lieux de 2 km retenus | 25 | **52** | 99 |
| part des heures dans les 10 premiers | 74 % | **92 %** | 98 % |
| lieux qui ont nourri | 17 | 27 | 43 |
| lieux où l'on a bu | 17 | 30 | 46 |
| lieux où l'on a dormi | 17 | **30** | 53 |
1. 15-40 lieux, p90 ≤ 80 : **réfutée vers le haut** (52 ; 99). 2. ≥ 85 % des heures dans 10
lieux : **tenue** (92 %). 3. Nourri ≤ 15, dormi ≤ 3 : **réfutées** (27 ; 30).
Lecture : le foyer du clan ne bouge pas, mais chacun **dort à 30 endroits** en un an : les
nuits se passent là où la journée a mené, dans un rayon de ~6 km du foyer (≈ 28 mailles,
cohérent avec la dispersion de 1,5-6 km des runs longues). Le pays connu (~200 km²) est de
l'ordre d'un territoire de chasse baka (205 km²) ; il est vécu, pas exploité : 92 % du temps
dans 10 mailles. Mémoire : +0,6 Mo sur 1,09 Go.
Ce que j'ai pu manquer : 1 an seulement (le pays connu d'une vie n'est pas vu) ; les enfants ne
sont pas comptés ; « dormi » compte toute heure de sommeil, sieste comprise.

## MEM-4a — les sources s'oublient par l'usage (branche `exp/mem4a`, eba022b, 2026-10-20)
Ordre de MEM : MEM-4 avant MEM-3 pour les sources (la soif, les trajets d'eau). Changement
(`memory`) : chaque source connue porte une force de souvenir — renfort de 0,2 vue, 0,1
entendue, 0,5 bue (une fois par jour : un jour d'usage) ; effacement de constante 60 j vers un
plancher usage / (usage + 10) ; oubli sous 0,02 ; plafond porté de 16 à 64 (Nyae Nyae : ~40
points d'eau), la plus faible part la première, **plus la plus lointaine**. Constantes non
sourcées (forme validée par l'utilisateur). Test `une_source_s_oublie_par_l_usage_pas_par_la_distance`
(remplace `les_sources_lointaines_s_oublient`, qui fixait l'ancien comportement).
**Voisin** : `les_curieux_explorent_plus_loin` (CUR) échoue sur la seed 1337 (64,9 cellules
contre 66,3 ; il passait à 1,03-1,30 selon les seeds) — critère déjà fragile, lu dans CUR ; la
branche n'est pas reportée tant que la mesure n'a pas tranché.
Prédictions, banc `marche` 90 j × 8 runs puis acceptation 1 an × 8, contre `main` (MEM-1) :
1. Heures de marche vers l'eau en baisse de 10-30 % (on ne perd plus les sources du pays en
   s'éloignant). Réfuterait : baisse < 5 % ou hausse.
2. Sources retenues par adulte : médiane 20-64 (16 avant, plafond).
3. Morts de soif à 1 an : égales ou en baisse (8 runs).

### MEM-4a — mesuré (banc `marche` 90 j × 8 runs ; acceptation 1 an × 8 ; contre `main` 1e960f8)
| | `main` | MEM-4a |
|---|---|---|
| marche vers l'eau (h/j, jour ; noir) | 1,39 ; 0,47 | 1,38 ; 0,47 |
| marche totale (h/j) | 7,6 | 7,5 |
| sources retenues par adulte (p10 / méd. / p90) | 16 / 16 / 16 | **64 / 64 / 64** |
| morts de soif (8 runs, 1 an) | 3 | 2 |
| débit plancher annuel, moyenne des 8 runs (tps) | 70,1 | **60,8** (−13 %) |
1. Marche vers l'eau −10 à −30 % : **réfutée** (−0,7 %). 2. Sources 20-64 : tenue, mais **au
plafond pour tous** : en un an, aucune source ne passe sous le seuil d'oubli. 3. Soif : égale.
Lecture : dans le régime d'aujourd'hui (camp immobile, pays de ~6 km de rayon), l'oubli par
la distance ne mordait pas — on ne quitte jamais assez ses sources pour les perdre. Les 1,4 h/j
vers l'eau ne viennent pas de sources oubliées. Et le changement coûte : 64 sources à parcourir
à chaque soif, et un tri du plus faible à chaque source vue (le plafond est toujours plein).
**Non reporté** ; la branche reste pour quand le camp bougera (MAR-6b), où la distance comptera.
Le coût serait à reprendre (une structure qui ne rescanne pas, ou un plafond plus bas).
Conséquence pour l'ordre : MEM-3 (les choix lisent la mémoire) directement.
