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
