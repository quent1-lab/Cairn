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
