# CAP — ce qui borne vraiment les ongulés

Ouvert le 2026-11-01 depuis CHA-5 (décision de l'utilisateur : la capacité avant le champ par
maille). Lire d'abord `CHA-5.md`, sections « Diagnostics — mesurés » et « Avant le champ ».

**Décision de l'utilisateur (2026-11-01, soir)** : ce n'est **pas un chantier de calibration**.
On regarde d'abord comment le gibier et les prédateurs naissent, vivent et sortent de la
simulation ; on corrige des règles, jamais des constantes réglées vers une densité (règle 10).

## Le constat (mesuré dans CHA-5)

Sans le périmètre qui efface le gibier, le modèle monte à 30-90 têtes /km² en 2 ans, satiété
encore > 0,5 (croissance continue) ; capacité calculée de fourrage : 23-740 têtes /km² selon le
biome. Réel (`../SOURCES.md`, section CHA-5, seconde main) : cerf 10-35 sans prédateurs, 12,7
avec loups ; wapiti 2-9 ; renne 0,8-3 ; bison 0,3-0,5 ; saïga 0,5-0,8. Les meutes sont déjà au
plafond réel (0,05 /km²) et ne freinent pas. Avec le périmètre, ce que les humains perçoivent est
fixé par l'effacement (abandon à 16 km) et l'immigration (plafonnée par jour), pas par la nature.

## Pourquoi pas une calibration

Le gibier observé dépend d'abord des règles qui **créent et retirent** la faune, dont plusieurs
sont des gardes de coût, pas de la nature. Régler la natalité ou les prises pour retrouver les
densités réelles calibrerait un symptôme pendant que ces règles continuent de décider. Exemples
déjà vus : placement initial d'un troupeau de 25 têtes tous les 1,5 km sur toute tuile de
biomasse > 40, quel que soit le biome (~11 têtes /km², renne compris) ; même taille de départ
(25) et même seuil de fission (90) pour toutes les espèces (un groupe de cerfs : 5-20 ; une
harde de rennes : des centaines) ; immigration de 25 têtes autour d'un humain tiré au hasard
(D9) ; effacement à 16 km ; meutes réensemencées autour d'un troupeau.

## Étape 1 — lire le cycle de vie de la faune (faite le 2026-11-01, lecture de `88bd914`)

Lire dans le code (`fauna.rs`, `scenario.rs`, `sim.rs` passes 4 à 4 bis, `pastoral.rs`) chaque
règle qui fait naître, vivre ou sortir une bête, et la confronter au réel. Pour chacune : ce que
fait le code (fichier:ligne), **nature ou garde de coût**, la référence réelle, l'écart, et ce
qu'elle fixe (niveau, répartition, espèce, taille de groupe).

- **Naissance** : placement initial (`scenario::populate`), choix de l'espèce selon le biome
  (`Species::herbivore_for_biome`, `predator_for_biome`), taille de départ (`HERD_START`,
  `PACK_START`), fission (`HERD_FISSION`, `PACK_FISSION`), immigration du gibier et des meutes.
- **Vie** : natalité et satiété (pâture de la tuile × habitat, maille de 2 km sur une production
  annuelle moyenne), fourrage accessible par biome, saisons et hiver, mortalité de fond fixe,
  migration (renne), fuite et paysage de la peur, domaine vital, prédation (prises par
  prédateur, territoire, conversion), chasse humaine.
- **Sortie** : mort démographique, dissolution sous `HERD_MIN`, fusion, abandon à 16 km, bord à
  flux nul (8 km).

Le résultat est un tableau dans ce fichier, classé de l'amont à l'aval, avec les chiffres réels
manquants listés pour le `sourceur` (part du fourrage réellement broutée par biome, taille des
groupes par espèce, mortalité hivernale densité-dépendante des cervidés, capacité de charge
d'hiver, prises annuelles par loup).

### Lecture

Calendrier : 360 jours par an, 24 ticks par jour. Classes : **N** nature (la règle prétend
modéliser le réel), **C** garde de coût ou de bord (elle remplace ce qu'on ne simule pas),
**S** condition de scène, **B** bug probable. « Réel » : sans lien, c'est un ordre de grandeur
de ma part, à sourcer (liste en bas) ; avec renvoi, c'est `../SOURCES.md`.

**Ce que dit la lecture, avant le détail.** Trois règles décident de la densité, et aucune
n'est la nature d'un pays réel :

1. **Les taux vitaux des herbivores sont environ dix fois trop rapides.** Natalité 0,020 /j au
   plus, mortalité de fond 0,010 /j (`fauna.rs:50-55`) : r max = 3,6 /an (×36 en un an,
   doublement en 69 jours), survie annuelle sans faim e^−3,6 ≈ 3 %, espérance de vie 100 jours,
   7,2 petits par tête et par an. Réel (`../SOURCES.md`, section CAP) : bison r = 0,145-0,16 /an
   à faible densité (×22 à ×25) ; cerf de Rùm, mortalité adulte ~3 %/an et 28 % des faons la
   première année (le modèle : 97 %/an pour tous) ; seul indice d'un cerf rapide, λ = 1,63 en
   enclos (non vérifié), reste ×2 sous le modèle en r. Conséquence par construction : à l'équilibre (satiété 0,5), le troupeau renouvelle toutes ses
   têtes en ~100 jours, si bien que **la prédation et la chasse ne peuvent rien réguler** — des
   loups au plafond réel (0,05 /km² × 18 prises /an ≈ 0,9 tête /km² /an) prélèvent ~1 % de ce
   qu'un troupeau à 30 /km² peut produire. Białowieża : les loups prennent 9-13 % de la densité
   de cerfs par an, et cela compte parce que le cerf ne se refait que de ~30 %. La densité du
   modèle n'est donc bornée que par le fourrage, et l'atteint en quelques mois.
2. **La capacité est la production annuelle moyenne, sans saison.** La maille de 2 km lit une
   production constante toute l'année (`fauna.rs:184-198`) ; la tuile ne repousse pas l'hiver
   mais ne dépérit pas non plus (`ecology.rs:255-260`, le commentaire le dit : « il donnerait à
   la faune le régulateur par le haut qui lui manque »). Réel : la capacité des ongulés du nord
   se fixe à la saison maigre (hiver, neige, saison sèche), et la mortalité qui borne est celle
   des jeunes en hiver, densité-dépendante. Dans le modèle, la faim ne tue pas : elle n'abaisse
   que la natalité ; la mortalité est fixe (`fauna.rs:691`).
3. **Une espèce prend tout le fourrage « accessible ».** À l'équilibre, la demande égale 100 %
   de la production accessible (`zone_satiation`, `fauna.rs:203`), elle-même posée « comme point
   de départ, pas comme mesure » (5 % de la PPN en forêt, 30 % en herbacé, `fauna.rs:175`). Une
   seule espèce par biome (aurochs en prairie, gazelle en savane) mange la part de tout le
   cortège réel. Capacités qui en découlent (ration réelle) : cerf en forêt tempérée 55 /km²,
   renne en toundra 26, aurochs en prairie 41 (bison réel 0,3-0,5 : ×100), gazelle en savane 740.

Le reste fixe la **répartition** et la **taille des groupes**, pas le niveau — sauf l'abandon
et l'immigration, déjà mesurés dans CHA-5.

#### Naissance (amont)

| Règle | Code | Classe | Réel | Écart | Fixe |
|---|---|---|---|---|---|
| Placement initial : un troupeau de 25 têtes tous les 1,5 km, sur toute tuile de biomasse > 40 | `scenario.rs:101-110` | S | densité par espèce et biome (`../SOURCES.md`) | 11 têtes /km² partout, renne compris ; **la toundra (biomasse de base 25, `tile.rs:119`) et les déserts n'en reçoivent jamais** | niveau et répartition de départ |
| Espèce selon le biome du point de naissance (tirage) | `fauna.rs:474` | N | — | une espèce par biome ; aurochs seul en prairie (l'aurochs réel est plutôt de forêt claire et de plaine alluviale ; la prairie ouverte est au bison et au cheval) ; aurochs par défaut dans les déserts | espèce |
| Prédateur selon le biome : loup partout, lion des cavernes 40 % en milieu ouvert | `fauna.rs:490` | N | — | non sourcé | espèce |
| Taille de départ 25 (troupeau), 5 (meute), toutes espèces | `fauna.rs:43, 66` | N déguisée en C | biches 5-40, bison ~20 l'hiver et ~200 l'été, caribou des bois ≤ 15 (SOURCES, CAP) | une taille pour toutes | taille des groupes |
| **Meutes de la scène de référence posées deux fois** : `populate` en pose 4 à 4 km, `reference_packs` 4 autres aux mêmes points | `scenario.rs:112-121` et `236-249`, appelés ensemble `scenario.rs:257-258` (aussi `longue`, `contact`) | **B** | — | 40 prédateurs au départ pour un territoire qui en porte ~10 : la famine en retire ~30 dans les premiers mois. Hérité de l'ancien banc d'acceptation, reproduit à l'identique par 74e21b7 | transitoire de départ des prédateurs, donc du gibier |
| Fission : au-dessus de 90 têtes, la moitié part à 1 km | `fauna.rs:1169` | C (coût par entité) et N (taille de groupe) mêlés | taille de groupe par espèce et saison | un seuil pour toutes ; sans effet sur le nombre de têtes | taille des groupes, nombre d'entités |
| Fission de meute au-dessus de 8 | `fauna.rs:607, 1752` | N | meute de loups 5-10 | cohérent | couverture spatiale |
| Immigration de gibier : chaque jour, 15 % de chance, 25 têtes à 1-4 km d'un humain au hasard, si aucun troupeau à 3 km et biomasse ≥ 40 | `fauna.rs:1902-1987`, `sim.rs:1675` | C (le code le dit : garde de performance) | remplace le flux entrant d'une population extérieure | plancher par jour, pas par surface (CHA-5, D9) ; jamais en toundra (biomasse 25 < 40) | plancher de densité près des humains |
| Immigration de meutes : 6 %/j, 5 loups à 0,5-2 km d'un troupeau au hasard, si aucune meute à 3 km | `fauna.rs:1989-2035`, `sim.rs:1692` | C | — | la garde de 3 km est plus courte que le territoire (8 km) : une meute immigrée peut arriver dans un territoire plein et y mourir de faim (part K/P) ; ancrée sur n'importe quel troupeau, même hors du périmètre | niveau des prédateurs (apport ~1,8 loup/jour au plus) |

#### Vie

| Règle | Code | Classe | Réel | Écart | Fixe |
|---|---|---|---|---|---|
| **Natalité 0,020 /j × satiété, mortalité de fond 0,010 /j, continues toute l'année** | `fauna.rs:50-55, 691` | N | bison r 0,145-0,16 /an ; cerf : mortalité adulte ~3 %/an, faons 28 % ; renne : 80-90 % des naissances en 10 jours (SOURCES, CAP) | r max ×22 (bison) ; mortalité 97 %/an contre 3 % ; pas de saison des naissances ; la faim ne tue pas (réel : jusqu'à 25-78 % de pertes un hiver sévère, faons d'abord) | **vitesse de retour au fourrage ; insensibilité à la prédation et à la chasse** |
| Satiété de maille : 0,5 × production / demande, production = moyenne annuelle du biome | `fauna.rs:184-203, 1153` | N | capacité fixée par la saison maigre | pas de goulot d'hiver ou de saison sèche | **niveau** |
| Fourrage accessible par biome (9 à 270 g/m²/an) | `fauna.rs:175` | N, non mesuré | part réellement broutée par biome (à sourcer) | posé comme point de départ ; une espèce prend la part de tout le cortège | **niveau** |
| Ration : 3 kg MS/j (cerf, renne), 12 (aurochs), 1 (gazelle) | `fauna.rs:383` | N | ~2-3 % de la masse | cohérent | conversion fourrage → têtes |
| Satiété de tuile : biomasse trouvée / capacité × habitat ; broutage 1 unité/tête/tick | `fauna.rs:682, 1264` | N | — | mesuré non contraignant (M1-faune) ; la tuile reste pleine l'hiver | quasi rien |
| Facteur d'habitat 1 / 0,6 / 0,35 | `fauna.rs:355` | N, non sourcé | — | hors habitat, l'équilibre demande une tuile à ≥ 83 % : la niche tient | répartition |
| Hiver : seul le renne migre, vers le plus chaud, 3,6 km/j | `fauna.rs:412, 827, 1085` | N | caribou 19-55 km/j en migration ; cerf migrateur partiel | lent ; les résidents ne paient rien de l'hiver | répartition saisonnière |
| Domaine vital : retour au centre au-delà de r (cerf 3 km², aurochs et renne 30, gazelle 10) | `fauna.rs:421` | N, partiellement sourcé | — | gazelle et renne à sourcer | répartition, densité locale |
| Fuite devant humains et meutes (0,4-1 km), souffle 3 h, paysage de la peur | `fauna.rs:999` | N | — | — | répartition |
| Prises des meutes : 0,05 /loup/j au contact (≈ 18 /an), un tick par rencontre sauf proie à bout de souffle | `fauna.rs:71, 712` | N | 15-25 ongulés /loup /an | cohérent ; pas de réponse fonctionnelle autre que la présence à 500 m | prélèvement |
| Croissance des meutes : 0,42 × prises − 0,020 /j ; territoire K ≈ 10 par disque de 8 km | `fauna.rs:82-129, 702, 1748` | N | r loup ~0,3-0,4 /an ; densité max 0,05 /km² | la mortalité de fond tient lieu de famine : 7,2 /an, renouvellement complet en 50 jours ; une meute sans proie meurt au lieu de se disperser | niveau des prédateurs |
| Choix de la proie : la proie gardée à moins de 6 km, sinon le troupeau le plus proche, **où qu'il soit** | `fauna.rs:1698` | N | — | sans borne de territoire | répartition des meutes |
| Chasse humaine : 1 tête par prise ; récolte du cheptel 0,3 tête/j | `sim.rs:168, 235` | N | — | hors CAP (CHA-5 : la chasse n'est tirée que par la faim) | prélèvement |
| **Apprivoisement automatique** : tout troupeau d'aurochs ou de rennes à moins de 2 km d'un foyer, avec moins de 8 prédateurs à 3 km, s'apprivoise de 0,02 /j et s'ancre, sans geste humain | `pastoral.rs:49` | N | domestication : siècles de sélection et de garde | seuil de cheptel en 25 jours sans éleveur ; l'ancré échappe au bord, à l'abandon, à la fusion et à la migration | sortie (voisin, domaine pastoral) |

#### Sortie (aval)

| Règle | Code | Classe | Réel | Écart | Fixe |
|---|---|---|---|---|---|
| Mort démographique (taux de fond), seule mort hors prises | `fauna.rs:691` | N | mortalité d'hiver densité-dépendante, jeunes surtout | constante, indépendante de la nourriture et de la saison | voir Vie |
| Dissolution sous 4 têtes : les têtes disparaissent (`dissolved`) | `fauna.rs:46, 1167` | C | 3 cerfs vivent et rejoignent un groupe | puits de têtes, faible | — |
| Fusion de deux troupeaux de même espèce qui se voient, si total ≤ 90 | `fauna.rs:1228`, `sim.rs:1587` | C (coût par entité, le code le dit) | — | sans effet sur les têtes | taille des groupes |
| Bord à flux nul à 8 km de tout humain | `fauna.rs:1875, 1131` | C | — | CHA-5 : coincé en bas sur 5/8 runs | densité perçue |
| Abandon au-delà de 16 km de tout humain (troupeaux seulement ; les meutes ne sont jamais abandonnées) | `fauna.rs:1881`, `sim.rs:1628` | C | — | CHA-5 : fixe le gibier perçu ; les meutes restent simulées loin de tout humain | densité perçue |
| Meute sous 1 prédateur : disparue | `fauna.rs:67` | N | — | — | — |

**Dette de documentation relevée** (sans effet) : le champ `fauna_lod_period` (`sim.rs:392-405`)
décrit un LOD des troupeaux qui n'existe plus (il ne sert qu'à l'écologie) ; `update_herds`
parle encore de « `steps` fois » (`fauna.rs:1163`).

#### Ce que l'étape 1 change à l'étape 2

Le banc « nature sans humains » mesurera une nature dont les taux vitaux et la saison sont
eux-mêmes en écart : il dira la capacité du fourrage, pas celle d'un pays. Il garde son
utilité (témoin), mais l'ordre amont → aval place **avant** tout banc la question de vision :
les taux vitaux (règle Vie 1) et le goulot saisonnier (Vie 2) sont-ils à corriger ici, et le
bug des meutes posées deux fois (S) à corriger d'abord, en mode léger, puisqu'il change le
témoin ?

#### Chiffres réels manquants (pour le `sourceur`)

Rendu le 2026-11-01 (`../SOURCES.md`, section CAP) : r max du bison, survie du cerf, saison des
naissances du renne, pertes d'hiver (renne, wapiti), tailles de groupe. Restent : r max du cerf
lu de première main (Forsyth et al.), du renne et de la gazelle ; survie du renne ; fécondité ; part de la PPN
réellement broutée par biome ; capacité d'hiver (neige) ; domaine vital de la gazelle et du
renne hors migration ; dispersion d'une meute sans proie contre famine ; biomasse totale
d'ongulés par biome (tout le cortège).

## Étape 2 — ce que la nature du modèle produit sans humains

Après la lecture seulement : un banc de référence **sans humains**, par biome, avec les seules
règles classées « nature » (boîte fixe, sans périmètre mobile ni immigration ; appel direct à
`fauna::update_herds` / `update_packs`, comme les tests de `sim.rs`), 4 seeds, plusieurs années.
On relève la densité (minimum et médiane : c'est un réservoir), le rapport proies / prédateurs
et la satiété par saison. Prédictions écrites avant.

### Banc et prédictions (écrites le 2026-11-01, avant la série)

Banc `cap_nature` (`crates/sim/examples/cap_nature.rs`, observation seule) : `Sim::step` sans
agent, `allow_fauna_immigration = false` (ni immigration, ni bord, ni abandon) ; carré de 12 km
(6 × 6 mailles) où le biome visé couvre ≥ 75 % des centres de maille, le plus près du point de
référence ; un troupeau de 25 têtes tous les 1,5 km sur toute tuile praticable portant de l'herbe
(toundra comprise) ; 4 meutes à 4 km du centre, posées **une seule fois** (le bug de la scène n'y
entre pas). Monde ouvert : les troupeaux peuvent sortir du carré. Relevé tous les 5 jours :
densité du carré, **charge** de ses mailles (demande ÷ production, 1 = équilibre du fourrage),
satiété, densité sur les mailles occupées, prédateurs, flux (croissance, prédation). Départ au
jour 135. Régime : seeds 42, 7, 1337, 2024 × forêt tempérée, taïga, toundra, prairie, steppe,
savane ; 4 ans.

Vu avant d'écrire : un essai d'un an, seed 42, forêt (chronométrage, 67 s) — 9,5 → 42 têtes /km²,
charge 0,18 → 0,80, prédation 3,5 têtes par 5 jours pour une croissance de ~110. Les prédictions
valent pour les 23 autres cases et pour les années 2 à 4 de celle-ci.

- **P1, le fourrage seul borne** : charge médiane de l'an 4 entre 0,85 et 1,15 dans chaque biome
  sans migration (forêt, prairie, steppe, savane), sur 4 seeds ; densité du carré à ±25 % de la
  capacité calculée (forêt 55 /km², prairie 41, steppe 23 à 274 selon le mélange aurochs /
  gazelle, savane 740). Réfutée si la charge médiane reste < 0,7 sur au moins 3 seeds d'un biome.
- **P2, les prédateurs ne pèsent pas** : prédation annuelle < 5 % des têtes moyennes du carré
  (réel, Białowieża : 9-13 %). Réfutée au-dessus de 10 % sur un biome et 3 seeds.
- **P3, pas de goulot d'hiver** : en forêt, prairie, steppe, savane, le minimum de l'an 4 reste
  ≥ 85 % de la médiane. Toundra et taïga (renne) : le carré se vide l'hiver (migration) — ce n'est
  pas une mort, le minimum y ment ; on lira la densité sur les mailles occupées. Réfutée si le
  minimum tombe sous 85 % de la médiane dans un biome sans migration, sur 3 seeds.
- **P4, la croissance ne dépend pas de la seed** : les 4 seeds vont dans le même sens pour P1-P3.

**Seuil de décision** : si P1 et P2 tiennent, la lecture de l'étape 1 est confirmée (taux vitaux
et saison d'abord, en amont de tout le reste) ; si P2 tombe, la prédation borne déjà quelque
part, et l'ordre des corrections se rediscute avec l'utilisateur avant d'y toucher.

### Coût du banc

La série complète prenait ~8 h par seed (steppe 4 ans : 8 000-12 800 s ; savane : 2 200-3 600
troupeaux à l'an 2) : horizons réduits pour 7 et 2024 (`out/cap/queue2.sh`). Un banc à taux
vitaux réels (r ~0,15 /an) demandera des décennies de jeu : il faudra un monde plus petit ou
moins d'entités.

### Témoin complet (4 seeds, binaire `9b31e57`, 2026-11-02)

Régime : sans humains ni immigration, départ jour 135 ; forêt, taïga, toundra, prairie 4 ans ;
steppe 4 ans (42, 1337) ou 3 (7, 2024) ; savane 2 ans. Toundra : carré trouvé sur 42 et 2024
seulement. Dernière année complète (`out/cap/analyse.py`) :

| biome | seeds | charge méd | carré méd (têtes/km²) | min ÷ méd | prédation /an ÷ têtes | prédateurs en fin |
|---|---|---|---|---|---|---|
| forêt | 4 | 0,87-0,95 | 42-50 (capacité 55) | 0,92-0,95 | 0,2-1,3 % | 1-16 |
| taïga | 4 | 0,81-0,92 | 30-75 (75 : carré mêlé de forêt) | 0,90-0,92 | 0-0,9 % | 0, 0, 3, 4 |
| toundra | 2 | **0,19-0,30** | 5-8 | — | 0-0,5 % | 0, 11 |
| prairie | 4 | 0,80-0,88 | 35-37 (capacité 41) | 0,88-0,93 | 0,2-1,0 % | 3-11 |
| steppe | 4 | 0,92-0,99 | 196-254 | 0,84-0,96 | 0-0,3 % | 0-14 |
| savane (an 2) | 4 | 0,69-0,83, en hausse | 330-479 | — | 0,1-0,3 % | 6-18 |

Verdict sur les prédictions :
- **P1 tenue** (forêt, steppe ; taïga) ; prairie à 0,80-0,88, sous la fourchette sur 3 seeds mais
  au-dessus du seuil de réfutation (0,7) et à −10/−15 % de la capacité : tenue. Savane non jugée
  (horizon trop court, encore en croissance).
- **P2 tenue, et au-delà** : prédation ≤ 1,3 % par an sur 22 cases (réel 9-13 %) ; **les
  prédateurs s'éteignent** sans immigration sur 4 cases sur 22, malgré 30-250 têtes /km².
- **P3 tenue** : min ÷ médiane ≥ 0,84 partout où elle se juge.
- **P4 tenue** : les 4 seeds vont dans le même sens.
- **Non prédit, 2/2** : en toundra, la charge du carré plafonne à 0,19-0,30 ; satiété ~0,5 pour
  une maille qui offrirait 1,7-2,6 : l'herbe de la tuile borne (capacité 25), pas la maille.

**Seuil de décision atteint**, avec la portée que lui laisse le débat ci-dessous : P1 et P3
vérifient surtout que **le code fait ce que dit sa formule** (aucun terme saisonnier, équilibre
écrit à charge 1) — aucune autre règle ne la contredit. Ce qui pouvait échouer et a parlé : les
prédateurs éteints, la toundra bloquée.

## Débat (2026-11-02, contre-lecture des étapes 1-2)

1. P1 et P3 étaient acquis par construction (`zone_satiation`, aucun terme saisonnier) : le banc
   vérifie le code, pas la régulation. → **Retenu** : verdict reformulé ci-dessus ; les prochaines
   prédictions distingueront vérification de code et prédiction réfutable.
2. Prédation ≤ 1,3 % : arithmétique des plafonds (0,05 loup /km² × 18 = 0,9 tête /km² /an),
   pas une preuve sur les taux vitaux ; avec r réel = 0,15 /an et K = 55, le prélèvement
   soutenable maximal (r·K/4) vaut ~2 /km² /an. → **Retenu en partie** : le niveau (×100 pour le
   bison) vient du fourrage accessible, pas des taux vitaux. Mais, en logistique, 0,9 /km² /an
   ôté à r = 0,15 abaisse l'équilibre d'environ 12 % (N/K = ½ + √(¼ − H/rK) ≈ 0,875), contre
   < 1 % à r = 3,6 : les taux vitaux décident bien de la **sensibilité** au prélèvement —
   prédateurs et chasseurs humains. Deux défauts distincts, pas un.
3. Extinction des prédateurs : une meute sur le fil (≥ 0,048 prise par jour sur 0,05 possibles),
   pas un manque de proie ; les prédateurs sont comptés sur tout le monde. → **Retenu** : à
   instrumenter (prises par loup et par jour contre le seuil, temps à portée) avant de corriger.
4. Toundra : peut-être l'hiver (la tuile ne repousse pas) ou la taille des groupes, pas la seule
   capacité de la tuile ; la scène de jeu n'y place jamais de troupeau. → **Retenu** : satiété
   décomposée tuile / maille par saison, biomasse des tuiles broutées ; ma lecture « la tuile
   borne » reste une hypothèse.
5. Monde ouvert : l'extérieur vide est un puits, la prairie à 0,80-0,88 peut en venir. →
   **Retenu comme réserve** : flux sortant du carré à mesurer si la prairie compte.
6. Charge mêlée (rennes hivernant en taïga, aurochs et gazelles en steppe). → **Retenu** :
   ventiler par espèce, coût nul.
7. `G_M2_YR_TO_KG_KM2_DAY` divise par 365 quand l'année en compte 360 (~1,4 %). → **Retenu**
   comme bug mineur, mode léger, plus tard.
- **Ordre proposé** : « la faim tue » et la production saisonnière d'abord, sur un **banc
  démographique réduit** (une maille, équations de flux, des décennies en secondes) ; puis les
  taux vitaux ; le fourrage accessible en dernier, jugé sur la biomasse d'ongulés par biome
  (à sourcer). Raison : un goulot d'hiver mesuré à r = 3,6 se refait en un été, et des taux
  réels sans saison ni faim mortelle ne sont bornés par rien sous K. → **Retenu comme
  proposition**, à trancher par l'utilisateur.

**Décision de l'utilisateur (2026-11-02)** : ordre validé (faim qui tue et saison, puis taux
vitaux, puis fourrage accessible) ; banc démographique réduit validé ; un banc avec humains après
validation. Taux vitaux propres à chaque espèce : oui, « éviter de scripter dans tous les cas, le
modèle doit être réaliste par rapport au réel ». Rappel de l'utilisateur : la question de base
du chantier — **ce qui borne les ongulés dans le réel** — n'a pas encore de réponse sourcée ; les
étapes 1-2 ont dit ce qui les borne **dans le modèle**. À sourcer avant de figer l'ordre
(régulation par le bas ou par le haut selon la taille, l'hiver, la prédation).

## La question de base : ce qui borne les ongulés dans le réel (2026-11-02, `../SOURCES.md`, seconde main)

- **Cela dépend de la taille** (Sinclair 2003, Serengeti) : sous ~150 kg, la prédation borne ;
  au-dessus, la nourriture. Dans le modèle : cerf (120 kg), renne (110), gazelle (25) sont du
  côté des prédateurs ; seul l'aurochs (700) est du côté du fourrage.
- **Sans prédateurs, c'est la saison maigre qui borne, par les jeunes** : faons de Rùm qui meurent
  l'hiver d'autant plus que la densité est forte, fécondité qui baisse ; veaux de gnou en
  saison sèche. La nourriture borne donc **par la faim qui tue en saison maigre**, pas par une
  natalité qui suit une moyenne annuelle.
- **Avec prédateurs, sous 150 kg, la densité reste bien sous ce plafond** : caribou sédentaire
  régulé par les loups (lichens non limitants) ; orignal tenu bas par loups et ours en Alaska ;
  wapiti divisé par 2 à 4 après le retour des loups (mais sécheresse et chasse y ont pris part).
- **La part de végétation mangée** : 1,5 % de la PPN en forêt, 23 % en prairie, **tous herbivores
  confondus, insectes compris** (Cebrian 1999) ; 66 % au Serengeti. Le modèle donne aux seuls
  grands ongulés, et à une seule espèce, 5 % de la PPN en forêt (×3 au moins) et 30 % en prairie.

**Ce que le modèle a de faux, rapporté à cette réponse** : il n'a que le mécanisme de
l'aurochs (le fourrage), et sous une forme sans saison ni faim qui tue ; le mécanisme des
trois autres espèces (la prédation) n'y peut rien, ni par ses taux (meute sur le fil, éteinte
sans immigration), ni par la vitesse de la proie (r ×20).

**Conséquence sur l'ordre validé** : les prédateurs n'y figuraient pas comme étape. Ils doivent
y entrer **avant** le fourrage accessible : pour trois espèces sur quatre, c'est eux qui fixent
la densité réelle, et juger le fourrage accessible sur la densité d'un cerf sans loups capables
de le réguler reviendrait à lui faire porter la borne de la prédation (calibration déguisée).
Ordre proposé : (1) faim qui tue et saison ; (2) taux vitaux par espèce ; (3) prédateurs qui
persistent et régulent (réponse aux proies, dispersion au lieu de la mort) ; (4) fourrage
accessible. **Validé par l'utilisateur (2026-11-02).**

## Bug : les meutes de la scène de référence posées deux fois (mode léger, 2026-11-02)

`scenario::reference` (et `longue`, `contact`) appelait `populate`, qui pose 4 meutes à 4 km du
foyer, puis `reference_packs`, qui en posait 4 autres aux mêmes points. Test rouge
`scenario::tests::la_scene_de_reference_pose_quatre_meutes` (seed 42, tempéré : 6 meutes, 30
prédateurs) ; correction : `reference_packs` supprimée. Témoin `4dd43d8` (`out/bin/main/`),
variante `out/bin/meutes/`.

Prédictions (écrites avant) — régime : `acceptation 1 135 42,7,1337,2024 1` et `longue` 1 an,
jour 135, 4 seeds × 2 scènes :
- prédateurs au départ divisés par 2 (30-40 → 15-20) ; écart à 90 jours < 30 %, à 1 an < 20 %,
  sans sens constant (la famine retire l'excédent en 2-3 mois, demi-vie ~35 jours) ;
- têtes de gibier à 1 an : écart < 15 % (la prédation pèse ≤ 1,3 %/an, témoin nature) ;
- critères d'acceptation : mêmes verdicts sur les 8 runs.
Réfuté si le gibier ou la population humaine s'écarte de plus de 15 % dans le même sens sur
au moins 6 runs sur 8.

### Mesure (2026-11-02, `out/cap/meutes/`)

| run | prédateurs j1 / j90 / an 1 (témoin → variante) | têtes moyennes sur l'an | têtes à 10 km (moy.) | têtes au jour 360 | pop an 1 |
|---|---|---|---|---|---|
| t42 | 30/14/13 → 15/14/22 | 1 329 → 711 | 918 → 484 | 2 141 → 710 | 49 → 54 |
| t7 | 39/32/13 → 20/25/12 | 890 → 954 | 216 → 506 | 1 011 → 952 | 44 → 46 |
| t1337 | 30/14/5 → 15/18/5 | 131 → 98 | 88 → 53 | 286 → 159 | 49 → 49 |
| t2024 | 39/32/19 → 20/31/12 | 1 075 → 633 | 189 → 324 | 1 378 → 191 | 47 → 38 |
| f42 | 39/27/19 → 20/22/19 | 285 → 572 | 154 → 228 | 386 → 86 | 22 → 47 |
| f7 | 39/35/11 → 20/33/4 | 514 → 500 | 236 → 266 | 281 → 26 | 60 → 51 |
| f1337 | 10/3/2 → 5/5/9 | 205 → 202 | 91 → 165 | 155 → 60 | 49 → 71 |
| f2024 | 39/39/13 → 20/36/16 | 553 → 277 | 125 → 98 | 308 → 157 | 68 → 65 |

- **Prédateurs : tenue.** Départ divisé par 2 ; dès 90 jours, écarts sans sens constant.
- **Gibier : réfutée en grandeur** (écarts de −50 à +100 %), **sans sens constant** sur la moyenne
  de l'an (5 en baisse, 2 en hausse, 1 égal) ni à 10 km du camp (5 en hausse, 3 en baisse). Le
  bilan des flux dit pourquoi : l'abandon (126 à 2 588 têtes par an) domine et dépend de la
  marche de la bande, chaotique (CHA-5). Dans ce régime, à 1 an, le bruit du gibier vaut ±50 % :
  ce banc ne peut pas voir un effet de la taille de celui-ci. Seul signe constant : les têtes au
  jour 360, plus basses 8 fois sur 8 — instantané, non expliqué, à ne pas lire seul.
- **Humains : sans sens constant** (population à 1 an : 4 en hausse, 3 en baisse, 1 égal) ; des
  verdicts d'acceptation changent (mortalité nulle ou non) au gré du chaos, aucun dans un sens.
- **Verdict** : la correction tient par construction (test) ; elle change le témoin de tous les
  bancs de la scène de référence. Les chiffres antérieurs à `9802e30` sur cette scène portaient
  20 prédateurs de trop au départ.

## Étape 3.1 — la faim qui tue et la saison : proposition de conception (2026-11-02, à valider)

Écart (étape 1, Vie 1-2 ; sources CAP) : la nourriture borne dans le réel **par la saison maigre**
(production nulle l'hiver ou en saison sèche, l'animal vit sur ce qui reste sur pied et sur ses
réserves) et **par la mort**, des jeunes d'abord ; dans le modèle, par une natalité qui suit une
production annuelle moyenne, la mort restant fixe.

Mécanismes proposés, tous lus dans le réel, aucune densité visée :
1. **Fourrage sur pied par maille**, sur le patron exact du stock de cueillette (`gathering`,
   D10) : `dS/dt = production(jour) − S/τ − broutage`, production les seuls jours de croissance
   (même total annuel qu'aujourd'hui), perte de ce qui reste sur pied (τ à sourcer), avancé jour
   par jour à la consultation. La satiété d'un troupeau devient ce qu'il trouve dans ce stock ÷
   sa ration. Corollaire à trancher : la biomasse de la tuile ne servirait plus qu'à choisir où
   brouter et au rendu (elle bloque aujourd'hui la toundra).
2. **Réserves** : une condition corporelle par troupeau, qui se remplit quand il mange plus que
   sa ration et se vide quand il mange moins (vitesse : perte de masse d'hiver réelle, à
   sourcer) ; **la mortalité monte quand la condition s'épuise** ; la mortalité de fond devient
   celle des adultes réels.
3. **Naissances** : la fécondité dépend de la condition (gestation selon la condition à
   l'automne, Rùm) — la forme saisonnière (mise bas au printemps) appartient à l'étape 3.2
   (taux vitaux par espèce), mais les deux se mesurent ensemble sur le banc réduit.

Question de vision ouverte : faut-il une **classe d'âge** (jeunes de l'année) dans le troupeau ?
Dans le réel, la faim d'hiver tue d'abord les jeunes, et c'est ce qui régule (Rùm, gnou) ; un
effectif sans âge ne peut que faire mourir « tout le monde un peu ». Coût : un nombre de plus par
troupeau.

Banc réduit (validé) : les fonctions de démographie restent pures dans `fauna.rs`, et le banc les
appelle sans espace (une maille, un troupeau agrégé, des décennies en secondes) — le même code que
la simulation, pas une copie.

**Décision de l'utilisateur (2026-11-02)** : oui aux deux — les troupeaux mangent dans un stock
de fourrage par maille (la tuile ne sert plus qu'à choisir où brouter et au rendu) ; une classe
d'âge (jeunes de l'année) dans le troupeau.

Découpage, un mécanisme à la fois : **3.1a** réserves et faim qui tue (offre inchangée) ;
**3.1b** stock saisonnier par maille à la place de la production moyenne et de la tuile ;
**3.1c** jeunes de l'année. Puis 3.2 (taux vitaux par espèce, mise bas au printemps).

### 3.1a — réserves et faim qui tue

Changement : une **condition** (réserves, 0-1) par troupeau. Elle se vide au rythme du manque,
`(nourriture − 1) / 85 j` (endurance au jeûne du renne de Svalbard, 85 ± 17 j), et se remplit
quand la ration est entière, `(1 − c) / 60 j` (choix : l'engraissement d'un été). La natalité
suit la condition (`0,020 × c` ; Rùm : fécondité liée au poids — forme linéaire, choix) ; la
mortalité de fond reste 0,010 /j et s'y ajoute la **mort de faim**, `1/30 j × (1 − c/0,2)` sous
20 % de réserves (choix, aucun seuil létal sourcé). La nourriture devient la **part de la ration
trouvée** : min(production ÷ demande de la maille, satiété de la tuile), bornée à 1 ; le facteur
0,5 de `zone_satiation` disparaît (la condition d'équilibre, 0,5, est désormais celle des
réserves). Taux de 3.2 inchangés ici.

Tests rouges sur l'ancien `herd_tick` (vus échouer) : `la_faim_tue_une_fois_les_reserves_epuisees`
(1,00 %/j au 100e jour de famine, attendu ≥ 3 %) ; `dix_jours_de_jeune_n_arretent_pas_un_troupeau_gras`
(−0,995 %/j, attendu ≥ +0,5 %/j).

Témoin `cap_demo` (offre annuelle constante, sans prédateur ni saison ; déterministe, sans
RNG, donc sans seeds) : densité plate à la capacité dès l'an 10 dans les 6 biomes (forêt 54,8,
prairie 41,1, savane 739,7…).

Prédictions (`cap_demo`, 50 ans) :
- **plus de plateau** : un cycle de dépassement puis d'effondrement, la condition réagissant
  avec ~3 mois de retard ; après l'an 10, max ÷ min ≥ 1,5 dans les 6 biomes ;
- max au-dessus de la capacité de 20 à 100 % ; médiane à ±30 % de la capacité ;
- la même forme dans tous les biomes (la capacité ne fait que changer d'échelle).
Réfutée si max ÷ min < 1,1 après l'an 10 (le retard ne suffit pas à osciller) ou si la
population s'éteint. Risque connu : pas de saison ici, donc pas de goulot d'hiver ; c'est 3.1b.
Voisin à surveiller : hors de son habitat (facteur 0,6), un troupeau ne trouve plus que 60 %
de sa ration et meurt de faim en ~4 mois, là où il se maintenait parfois.

Mesure (`cap_demo` 50 ans, après le changement) — densité min / méd / max de l'année :

| biome (capacité /km²) | an 1 | an 10 | an 50 |
|---|---|---|---|
| forêt (54,8) | 11,1 / 64,4 / 113,8 | 54,6 / 56,9 / 58,6 | 54,8 / 55,3 / 55,4 |
| taïga (36,5) | 11,1 / 42,8 / 75,9 | 36,4 / 38,1 / 39,1 | 36,5 / 36,8 / 36,9 |
| toundra (25,6) | 11,1 / 30,0 / 53,1 | 25,5 / 26,9 / 27,4 | 25,6 / 25,8 / 25,9 |
| prairie (41,1) | 11,1 / 48,2 / 85,4 | 41,0 / 42,7 / 44,0 | 41,1 / 41,4 / 41,5 |
| steppe (274) | 11,1 / 67,2 / 394 | 273 / 288 / 295 | 274 / 277 / 278 |
| savane (740) | 11,1 / 67,2 / 402 | 737 / 774 / 801 | 740 / 746 / 749 |

- **Cycle entretenu : réfuté.** Dépassement la première année (×1,4 à ×2,1 la capacité), puis
  oscillation **amortie** : max ÷ min = 1,05-1,09 à l'an 10, ~1,01 à l'an 50. Sans forçage, le
  troupeau se pose à demande ≈ production, condition ≈ 0,5 (naissances = morts), ration à peine
  entamée : le retard des réserves (85 j) ne suffit pas à entretenir un cycle contre une
  croissance qui s'annule d'elle-même. Un cycle demandera un forçage — la saison (3.1b).
- **Médiane à ±30 % de la capacité : tenue** (+0 à +1 % à l'an 50).
- **Même forme partout : tenue.**
- Tests : 299 verts ; rouges connus inchangés (surchasse, curieux). Deux tests de l'étape 4
  supposaient une natalité instantanée (horizon 20-30 j) : `un_troupeau_qui_fuit_n_echappe_pas…`
  passe à 150 jours (vert) ; `un_pays_surpeuple…` juge désormais la **condition** à 20 jours
  (≤ 0,894 ; vue échouer à 0,999 en neutralisant le frein de la maille) — sur 150 jours, ses
  gazelles passent de 29 600 à 45 778 parce qu'elles s'étalent sur leur domaine vital, une dizaine
  de mailles : le test ne surpeuplait qu'une maille.
- Conséquence : sans saison, 3.1a ne change pas le niveau ; elle change la réponse au manque
  (la faim tue, les naissances suivent les réserves). Le niveau et l'hiver se jouent en 3.1b.

### 3.1a, défaut trouvé par 3.1b : réserves linéaires (2026-11-02)

Le premier banc saisonnier figeait la forêt de seed 42 à 88,8 têtes/km² (capacité 52), réserves
0,498, fourrage 0,6 jour de demande toute l'année. Lecture : la condition ne se remplissait
qu'à ration entière ; avec un stock, les bêtes mangent à satiété ~14 h puis rien ~10 h, et
cette alternance gardait les réserves qu'une ration régulière de même total (58 %) vidait. La
réponse dépendait de la répartition des repas dans la journée — un artefact de pas de temps.
Test rouge `les_reserves_suivent_la_ration_du_jour_pas_sa_repartition` (0,653 contre 0,722 en
30 jours). Correction : réserves **linéaires** dans la nourriture, `(f − m)/T`, vidées en 85 j à
f = 0, remplies en 120 j à f = 1 (choix : un été), d'où une part d'entretien m ≈ 0,59 de la
ration — non sourcée ; une ration réelle (2-3 % de la masse) est l'ingestion volontaire,
au-dessus de l'entretien.

Effet sur `cap_demo` à offre annuelle constante (code de 3.1a) : la densité ne se pose plus à la
capacité ; elle **oscille chaque année** entre ~0,95 et ~3,5 fois la capacité, médiane ~2 fois
(forêt 51,8 / 109,7 / 194,4 à l'an 50 ; savane 700 / 1 520 / 2 624). Le niveau moyen ~1,7 fois
la capacité est celui où les bêtes ne trouvent que leur entretien (1/m) ; l'amplitude vient des
taux vitaux (r = 3,6 /an), à juger en 3.2. La conclusion de 3.1a (« sans saison, le niveau ne
change pas ») ne tient plus : elle venait de la forme non linéaire.

### 3.1b — le fourrage sur pied, saison par saison

Changement : `Rangeland` tient un **stock de fourrage par maille** (kg MS), sur le patron du
stock de cueillette : il croît de la production annuelle accessible (inchangée) répartie sur les
seuls jours de croissance de la maille, se perd avec τ = 300 j (litière et herbe sur pied :
−28 % de l'été à l'automne, −22 à −50 % l'hiver, prairie tempérée ; milieu des fourchettes,
choix), et les bêtes y prélèvent leur ration tant qu'il en reste — la ration manque quand le
stock est vide, une impossibilité physique, pas un seuil. Avancé jour par jour à la
consultation, rejoué sur un an à la première visite (sans broutage). La **tuile ne nourrit
plus** : sa biomasse sert au choix du pas et au rendu (décision de l'utilisateur). Le facteur
d'habitat reste un multiplicateur de la part trouvée. L'année de la conversion passe de 365 à
360 jours (contre-lecture, point 7), dans le même mouvement puisque la production se répartit
désormais sur les jours de l'année du jeu.

Test rouge (vu échouer, ancien comportement derrière `Rangeland::graze`) :
`l_hiver_affame_un_pays_charge_a_sa_production_annuelle` — forêt tempérée à saisons, demande =
production annuelle : part de ration trouvée 1,00 toute l'année ; attendu < 0,5 au creux et 1 l'été.

Prédictions (`cap_demo` repris sur une maille réelle par biome, seeds 42, 7, 1337, 2024,
50 ans ; taux vitaux encore ceux du modèle) :
- **le niveau descend sous la capacité annuelle** dans les biomes à hiver : médiane de l'an 50
  entre 40 et 90 % de la capacité annuelle (forêt, taïga, prairie, steppe), 20-60 % en toundra ;
- **un cycle annuel** : max ÷ min dans l'année ≥ 1,3 (mortalité de fin d'hiver, croissance
  d'été) ; part de ration trouvée < 0,5 à la fin de chaque hiver ;
- biome sans saison (savane près de l'équateur, `season` = None) : comme 3.1a, à ±10 %.
Réfutée si la médiane reste au-dessus de 90 % de la capacité annuelle dans un biome à hiver sur
au moins 3 seeds (l'hiver ne borne pas), ou si un biome s'éteint sur 3 seeds.

Mesure (`cap_demo` saisonnier, 50 ans, une maille dont les 4 quarts portent le biome, cherchée
à 240 km au plus du point de référence ; réserves linéaires ; densité min / méd / max de
l'année ; capacité annuelle = production ÷ ration) :

| biome | seeds trouvées | an 50 (min / méd / max, têtes/km²) | capacité | forme |
|---|---|---|---|---|
| forêt | 42, 7, 1337, 2024 | 48,8/108/184 · 8,6/175/309 · 52/115/197 · 72/160/275 | 52-63 | cycle annuel stable (3 seeds) ; seed 7 : effondrements à 0,3 /km² et rebonds |
| taïga | 7, 2024 | 0,5/1,2/63 (identiques sur les deux seeds) | 37 | effondrements et rebonds, ×100 d'une année à l'autre |
| toundra | 2024 | 0,0/0,1/39 | 27 | quasi-extinction l'hiver, rebond l'été |
| prairie | 42, 7, 1337, 2024 | 33/77/157 · 1,5/3,6/45 · 37/81/138 · 33/79/184 | 39-42 | cycle annuel stable (3 seeds) ; seed 7 chaotique |
| steppe | 7, 2024 | 20/47/785 · 6/14/235 | 278-333 | chaotique |
| savane | 7 | 703/1 554/2 656 | 750 | cycle annuel stable |

- **Test rouge passé au vert** ; part de ration trouvée à 0 chaque fin d'hiver presque partout :
  l'hiver pèse.
- **« Niveau entre 40 et 90 % de la capacité » : réfutée.** Là où le cycle est stable (forêt,
  prairie, savane), la médiane est à ~2 fois la capacité, le minimum à ~1 fois : le niveau est
  celui de l'entretien (1/m ≈ 1,7), et le troupeau triple chaque été (r = 3,6 /an) avant de
  fondre l'hiver.
- **Cycle annuel ≥ 1,3 : tenue**, au-delà (×3 à ×100).
- **Non prédit** : dans les pays froids et la steppe, la population **s'effondre puis rebondit**
  (0,1 à 900 têtes/km² d'une année à l'autre) ; seed 7 fait de même en forêt et en prairie. Un
  troupeau qui croît de ×36 par an dépasse tout stock d'hiver, et la mort de faim le ramène
  près de zéro : c'est la vitesse vitale qui commande, comme la contre-lecture l'avait dit.
- Réserve : les seeds 7 et 2024 donnent la taïga au chiffre près (même maille de climat ?), non
  vérifié ; beaucoup de biomes sans maille trouvée dans le rayon de recherche.
- Tests : 297 verts hors rouges connus. `l_hiver_affame…` cherche désormais une maille de
  **taïga** : la forêt tempérée proche du point de référence pousse toute l'année (aucun jour
  sous 5 °C), le test n'y voyait pas d'hiver. `un_pays_surpeuple…` (simulation entière) est
  **retiré** : avec un an de fourrage sur pied au départ et des gazelles qui s'étalent sur leur
  domaine vital (une dizaine de mailles vierges), il ne surpeuplait plus rien (réserves 0,902 à
  60 jours, attendu ≤ 0,85) ; remplacé au niveau de la maille par
  `dix_fois_trop_de_bouches_vident_la_maille` (≤ 20 % de ration du 40e au 60e jour) — garde de
  régression, **qui passait aussi avant** 3.1b : elle ne prouve pas le changement.
- **Lecture** : le mécanisme (stock, hiver, faim) fait ce qu'il doit ; le niveau ne peut pas se
  juger avant les taux vitaux réels (3.2). Rien n'est réglé ici.

## 3.2 — taux vitaux par espèce, naissances au printemps, jeunes de l'année (conception, 2026-11-02)

3.1c (jeunes de l'année) rejoint 3.2 : une classe de jeunes n'a de sens qu'avec une saison des
naissances, et les deux portent les taux de chaque espèce.

Mécanismes (aucune densité visée ; chaque taux est une entrée physiologique, la densité en sort) :
1. **Deux classes** par troupeau : adultes (yearlings compris) et jeunes de l'année. Prises,
   fission et fusion se partagent à proportion des têtes.
2. **Naissances à la reprise de la pousse** dans la maille du troupeau (premier jour de
   croissance après un jour sans) — la mise bas suit le reverdissement (caribou, cerf), aucune
   date écrite ; étalées sur 30 jours (renne : 80-90 % en 10 j, saison complète 4-5 semaines).
   Nombre : adultes × fécondité de l'espèce × condition au reverdissement (Rùm : fécondité liée
   au poids). Maille sans saison (toujours en pousse) : naissances continues au même total
   annuel. Maille qui ne pousse jamais : aucune.
3. **Les jeunes de l'an passé deviennent adultes** au reverdissement, juste avant les
   naissances (en continu, 1/360 par jour, sans saison).
4. **Mortalités** : de fond par classe et par espèce, plus la faim ; **les jeunes meurent de
   faim les premiers** : sous 50 % de réserves pour eux, 20 % pour les adultes (choix : un
   renne maigre tient ~45 j de moins sur 85, Svalbard).

| espèce | mort. adulte /an | mort. de fond des jeunes /an | jeunes par adulte et par an (pleine condition) | sources (`../SOURCES.md`) |
|---|---|---|---|---|
| cerf | 0,03 | 0,10 | 0,45 | adulte Rùm 3 % ; 10 % des faons meurent en 2 semaines ; 0,55 de femelles × ~0,8 de gestation (choix : pourcentages de Rùm non trouvés) |
| renne | 0,05 | 0,10 | 0,43 | adultes 4-6 % (peu de prédateurs) ; vêlage 79 % × 0,55 de femelles |
| aurochs | 0,05 | 0,10 | 0,23 | survie adulte du bison 0,92-0,95 ; veaux par vache 0,44-0,47 (Yellowstone) × ~0,5 de femelles (choix) ; gestation des adultes 57-74 % (Fuller, lu en entier ; USGS) |
| gazelle | 0,33 | 0,10 | 0,9 | **sources faibles, aucun chiffre primaire trouvé** (deux recherches) : mortalité adulte ~1/3 /an (vulgarisation), 1 petit, jusqu'à 2 mises bas par an, 0,6 de femelles (choix) — décision de l'utilisateur : retenues faute de mieux |

Contrôle à la main (sans faim) : λ ≈ 1 + b·(1 − μj) − μa → cerf 1,37, renne 1,34, aurochs 1,16,
gazelle 1,48 : r max de 0,15 à 0,4 /an, contre 3,6 aujourd'hui.

Fécondité de l'aurochs : la seconde recherche (gestation du bison) donne 0,44-0,47 veau par vache, soit ~0,23 par adulte, la valeur qu'aurait donnée le r max : retenue sur la gestation, plus sur le r max.

Tests rouges prévus : (a) un troupeau de cerfs bien nourri croît au plus de 40 % par an
(aujourd'hui ×36) ; (b) toutes les naissances d'une maille à saisons tombent dans les 30 jours
qui suivent le reverdissement ; (c) à réserves de 35 %, les jeunes meurent de faim et pas les
adultes.

### 3.2 — tests rouges et prédictions (écrites avant le code)

Tests rouges vus échouer (interface `herd_life_tick` posée sur l'ancien comportement) :
`un_troupeau_de_cerfs_repu_croit_d_au_plus_40_pour_cent_par_an` (100 → 3 657 en un an) ;
`les_naissances_suivent_le_reverdissement` (330 jours de croissance hors de la fenêtre) ;
`a_35_pour_cent_de_reserves_les_jeunes_meurent_de_faim_pas_les_adultes` (jeunes −0 %,
adultes −12,3 %).

Prédictions (`cap_demo` saisonnier, 4 seeds, 50 ans, départ 11 têtes/km²) :
- **montée lente** : une dizaine d'années pour approcher l'équilibre (r ~0,15-0,4 /an) ;
- **niveau** : médiane de l'an 50 entre 0,5 et 1,5 fois la capacité annuelle dans les biomes à
  hiver (la mort des jeunes en fin d'hiver borne, plus la natalité) ;
- **cycle annuel modéré** : max ÷ min de l'année entre 1,1 et 1,8 (pulse de naissances, puis
  morts d'hiver) ;
- **plus d'effondrement ×100** : min de l'an 50 ≥ 0,2 fois sa médiane, sur toutes les seeds.
Réfutée si une médiane sort de 0,3-2 fois la capacité, ou si une population s'effondre sous
0,1 fois sa médiane. Ce qui reste choisi et non sourcé : seuils de faim (0,2 adultes, 0,5
jeunes), mortalité de fond des jeunes (0,10), part d'entretien (0,59), τ du fourrage (300 j),
toute la gazelle.

### 3.2 — mesure (`cap_demo`, 50 ans, 4 seeds ; mailles trouvées seulement)

| biome | seed | an 10 (min/méd/max) | an 50 (min/méd/max) | capacité annuelle | méd ÷ capacité | saison |
|---|---|---|---|---|---|---|
| forêt | 42 | 82/87/93 | 89/89/89 | 52 | 1,71 | pousse toute l'année |
| forêt | 7 | 52/64/66 | 54/63/64 | 56 | 1,13 | à hiver |
| forêt | 1337 | 87/90/97 | 95/95/95 | 56 | 1,71 | pousse toute l'année |
| forêt | 2024 | 79/82/83 | 83/86/88 | 63 | 1,38 | à hiver |
| taïga | 7, 2024 | 30/31/37 | 35/36/37 | 37 | 0,98 | à hiver |
| toundra | 2024 | 6/8/8 | 21/21/22 | 27 | 0,80 | à hiver |
| prairie | 42 | 41/49/50 | 68/70/74 | 42 | 1,67 | à hiver |
| prairie | 7 | 36/41/42 | 42/43/44 | 41 | 1,06 | à hiver |
| prairie | 1337 | 39/41/44 | 67/67/67 | 39 | 1,71 | pousse toute l'année |
| prairie | 2024 | 38/45/46 | 47/49/50 | 42 | 1,17 | à hiver |
| steppe | 7 | 267/328/369 | 202/340/378 | 278 | 1,22 (75-340 selon l'année) | à hiver |
| steppe | 2024 | 267/450/500 | 303/367/414 | 333 | 1,10 (208-450) | à hiver |
| savane | 7 | 200/233/272 | 1 280/1 281/1 283 | 750 | 1,71 | pousse toute l'année |

- **Montée lente : tenue** (10 à 20 ans pour approcher l'équilibre ; an 1 : +10 à +40 %).
- **Niveau 0,5-1,5 fois la capacité dans les biomes à hiver : tenue** (0,80-1,38 ; prairie 42 à
  1,67, seule hors fourchette). Là où la maille **pousse toute l'année**, la médiane est à 1,71 :
  c'est 1/m, le niveau où les bêtes ne trouvent que leur entretien (m = 0,59).
- **Cycle annuel 1,1-1,8 : réfuté vers le bas** : max ÷ min de 1,00 à 1,10 hors steppe. À
  l'équilibre, les réserves sont basses (~0,5 ou moins), donc peu de naissances et peu de morts :
  le pulse ne fait que remplacer.
- **Plus d'effondrement ×100 : tenue** (min ≥ 0,6 fois la médiane partout ; la steppe varie de
  ×4 d'une année à l'autre, la gazelle restant la plus rapide, r ~0,4).
- Tests : 30 tests de faune verts, dont les trois rouges de 3.2. Deux anciens tests retirés,
  leur sujet n'existant plus (natalité quotidienne suivant la satiété) : `un_troupeau_bien_nourri…`,
  `la_satiete_d_equilibre…` ; `dix_jours_de_jeune…` remplacé par
  `les_naissances_suivent_les_reserves_au_reverdissement`.

**Lecture** : le niveau sort désormais d'un mécanisme réel dans les pays à hiver (les jeunes
meurent en fin d'hiver, les naissances suivent les réserves) et tombe à 0,8-1,4 fois ce que le
fourrage accessible nourrit. Il reste au-dessus du réel d'autant que ce fourrage l'est (étape 4)
et que les prédateurs n'y sont pas (étape 3.3). Deux voisins à regarder : la part d'entretien
(0,59, non sourcée) fixe le niveau des pays sans hiver ; et la toundra (0,8) n'est plus bloquée.

### Jalon 3.1-3.2 — prédictions avant le banc spatial (2026-11-02)

Régime : `cap_nature` (sans humains ni immigration, monde ouvert), forêt, taïga, prairie,
seeds 42, 7, 1337, 2024, 10 ans, départ jour 135, binaire `d0e6e3e` ; et `acceptation 1 135`
(4 seeds × 2 scènes) contre la variante `9802e30` (après le correctif des meutes).
- croissance du carré ≤ 45 %/an les deux premières années (avant : ×3-4 la première année) ;
- densité du carré à l'an 10 entre 0,5 et 1,7 fois la capacité annuelle ;
- min ÷ médiane de l'an 10 ≥ 0,8 (pas d'effondrement) ;
- prédateurs : toujours éteints sans immigration sur au moins un tiers des cases (rien n'a
  changé chez eux) ; prédation de 1 à 10 % des têtes par an (proie plus lente) ;
- acceptation : la chasse rend moins (gibier moins dense au départ des runs, plus lent à se
  refaire) — verdicts humains dans le bruit déjà mesuré (±50 % sur le gibier à un an).
Réfutée si une densité dépasse 2 fois la capacité, ou si le gibier s'éteint sur une case.

### Jalon 3.1-3.2 — mesure (binaire `d0e6e3e`, `out/cap/j32/`)

`cap_nature`, sans humains, carré de 12 km ; densité du carré, médiane de l'année (têtes/km²) :

| biome | seed | an 1 | an 2 | an 5 | an 10 (min / méd) | capacité | an 10 ÷ cap. | prédateurs an 10 |
|---|---|---|---|---|---|---|---|---|
| forêt | 42 | 11,2 | 14,0 | 31,8 | 82 / 86 | 55 | 1,56 | 0 |
| forêt | 7 | 8,9 | 12,1 | 24,2 | 35 / 39 | 55 | 0,71 | 0 |
| forêt | 1337 | 9,0 | 11,5 | 21,4 | 44 / 46 | 55 | 0,84 | 9 |
| forêt | 2024 | 9,6 | 11,7 | 23,7 | 57 / 60 | 62 | 0,96 | 0 |
| taïga | 42 | 5,7 | 7,8 | 14,1 | 28 / 32 | 37 | 0,88 | 0 |
| taïga | 7 | 7,0 | 7,8 | 13,4 | 21 / 24 | 37 | 0,66 | 0 |
| taïga | 1337 | 7,2 | 9,0 | 10,1 | 12 / 14 | 37 | 0,39 | 9 |
| taïga | 2024 | 5,6 | 7,1 | 13,7 | 23 / 25 | 37 | 0,68 | 0 |
| prairie | 42 | 8,4 | 9,0 | 11,3 | 15 / 18 | 41 | 0,45 | 3 |
| prairie | 7 | 8,7 | 10,5 | 15,8 | 45 / 50 | 41 | 1,22 | 0 |
| prairie | 1337 | 9,3 | 9,3 | 10,8 | 17 / 19 | 41 | 0,46 | 0 |
| prairie | 2024 | 8,5 | 9,6 | 13,7 | 21 / 24 | 41 | 0,58 | 0 |

(Témoin de l'étape 2, ancien modèle : 30-50 têtes/km² dès l'an 2, 200+ en steppe.)

- **Croissance ≤ 45 %/an : tenue** (an 1 → an 2 : +0 à +40 % ; puis ~+15-25 %/an).
- **Densité à l'an 10 entre 0,5 et 1,7 fois la capacité : tenue sur 9/12**, les trois sous 0,5
  (taïga 1337, prairie 42 et 1337) **encore en croissance** — l'horizon de 10 ans ne suffit pas.
- **Min ÷ médiane ≥ 0,8 : tenue** (0,83-0,95).
- **Prédateurs éteints sans immigration sur au moins un tiers : tenue, au-delà** (9 cases sur 12) ;
  prédation 0-7 % des têtes là où ils survivent.
- **Gibier éteint : aucune case.**
- **Acceptation** (8 runs, contre `9802e30`) : population à un an 4 en hausse, 4 en baisse ;
  tempéré 2024 40 → 27 (19 morts, contre 9) ; aucun sens constant. **Débit plancher en baisse
  sur 6 runs sur 8 (−4 à −23 %)** : le stock et le signal de saison par maille se calculent à
  chaque tick — coût à mesurer (voisin PERF), pas un changement de modèle.

## Débat (2026-11-02, contre-lecture du jalon 3.1-3.2)

1. La part d'entretien 0,585 découle de deux durées dont une non sourcée (120 j) ; elle fixe le
   niveau des mailles sans hiver (1/m = 1,71) et pèse sur les autres — paramètre non sourcé le
   plus lourd, devant la fécondité linéaire et τ = 300 j. → **Retenu** : sourceur lancé
   (entretien ÷ ingestion volontaire, hypophagie d'hiver, vitesse d'engraissement) ; puis
   sensibilité de `cap_demo` à m ± 0,1.
2. « Les jeunes meurent en fin d'hiver » n'est pas démontré : cycle 1,00-1,10, réserves ~0,5 —
   c'est peut-être la natalité écrasée qui borne. → **Retenu** : ventiler naissances et morts
   (fond, faim) par classe dans `cap_demo` ; la phrase de lecture est suspendue.
3. **Bug probable** : un troupeau qui passe dans une maille qui reverdit plus tard reçoit un
   second reverdissement : ses jeunes de quelques jours deviennent adultes et une portée
   complète écrase la première. → **Retenu** : test rouge, une portée par an.
4. « Pousse toute l'année » est mal posé : la saison ne lit que la température ; la savane n'a
   pas de saison sèche, qui borne dans le réel (gnou). Les cas à 1,71 sont ces mailles. →
   **Retenu, question de modèle pour l'utilisateur** (l'eau comme saison maigre) ; compter la
   part des mailles habitées « sans saison ».
5. 9/12 « tenue » juge un transitoire ; deux des trois cas bas ont des prédateurs vivants ; le
   carré ouvert laisse l'émigration fixer sa densité. → **Retenu** : verdict suspendu ; banc à
   30 ans avec l'effectif du monde entier, à lancer détaché.
6. Hors de sa niche, un troupeau retire du stock sa ration entière mais n'en profite qu'à 60 %
   (le reste disparaît) ; 0,6 ≈ m, il se maintient de justesse. → **Retenu** : la demande d'un
   troupeau hors niche doit être ce qu'il mange ; test à écrire.
7. La baisse de débit (−4 à −23 %) attribuée au coût est déduite. → **Retenu** : mesure `profile`
   contre `9802e30`, dans PERF.
8. Cas limites : `day.wrapping_sub(1)` au jour 0 (sans effet au départ jour 135) ; le stock de
   départ (un an rejoué sans broutage) surnourrit la première année. → **Retenu** : corrigé en
   léger pour le jour 0 ; la première année se lit comme transitoire.
- **Ordre** : pas 3.3 tout de suite ; d'abord le bug (3), la ventilation des causes (2), m
  sourcé (1). → **Retenu.**

### Suites de la contre-lecture (2026-11-02)

- **Point 3 corrigé** : test rouge `un_second_reverdissement_dans_l_annee_ne_fait_pas_une_seconde_portee`
  (renne reverdi aux jours 0 et 20 : 172,8 têtes au bout d'un an contre 134,2) ; garde d'une
  portée par an (`BIRTH_INTERVAL_DAYS` = 300 j, choix ; gestation du cerf ~8 mois), héritée par
  la fille d'une fission, la plus récente gardée à la fusion.
- **Point 1, m sourcé indirectement** : cerf élaphe, entretien 567 kJ EM/kg^0,75/j contre
  ingestion volontaire d'été 89 g MS/kg^0,75/j ; à 9-11 MJ EM par kg MS de fourrage (hypothèse,
  non sourcée), m = 0,58-0,71 — la valeur retenue (0,585) est au bas de cette fourchette. Les
  120 jours de remplissage s'accordent avec le renne (+11-12 kg du printemps à mi-septembre) ;
  l'hiver, l'ingestion volontaire baisse de 34-37 % (cerf, renne) — l'**hypophagie d'hiver**,
  absente du modèle (ration constante), serait un mécanisme de plus. Sensibilité à m à mesurer.

- **Point 2, ventilation** (`herd_life_tick` rend ses flux, `LifeFlows`, ignorés par la
  simulation ; `cap_demo` les cumule) — an 50, têtes/km² par an :

  | case | nés | adultes : fond / faim | jeunes : fond / faim | ce qui borne |
  |---|---|---|---|---|
  | forêt 42, 1337 (sans hiver) | 13-14 | 2,6-2,8 / 0 | 0,3 / 10,2-10,9 | mort de faim des jeunes |
  | forêt 2024 | 6,6 | 2,4 / 0,9 | 0,5 / 2,8 | les deux |
  | forêt 7, taïga, toundra, prairie 7 et 2024 | 1,7-10 | 1-2,3 / 0 | 0,1-0,8 / 0,1-0,9 | **natalité écrasée** (réserves basses au reverdissement) |
  | prairie 42, 1337 | 5,4-6,7 | 3,2-3,4 / 0 | 0,2-0,3 / 3,2-4,4 | mort de faim des jeunes |
  | steppe 7 | 15 | 52 / 42 | 3,5 / 63 | faim de tous (gazelle) |
  | steppe 2024, savane | 142-425 | 47-318 / 0 | 11-32 / 0-76 | mortalité de fond de la gazelle (33 %) |

  La contre-lecture avait raison : la mort des jeunes ne borne que dans la moitié des cas ;
  ailleurs c'est la natalité, qui suit des réserves basses. Les deux sont des mécanismes réels
  (Rùm : fécondité **et** survie juvénile baissent avec la densité). Densités inchangées par
  l'instrument ; la garde d'une portée par an déplace la steppe (méd. 340 → 178 et 367 → 267) :
  certaines mailles y reverdissaient deux fois par an (redoux).

- **Point 6 corrigé** : test rouge `un_troupeau_hors_de_sa_niche_ne_retire_que_ce_qu_il_mange`
  (cerf en savane, habitat 0,6 : 5,0 kg retirés par tick pour 3,0 mangés) ; la demande d'un
  troupeau à la maille est désormais sa ration × son facteur d'habitat.
- **Point 8 corrigé** (et plus grave que lu) : une maille vierge n'était rejouée que depuis le
  jour 0 — visitée au jour 135, elle portait 135 jours de pousse (75 642 kg contre 171 465 pour
  un an, taïga seed 42). Test rouge `une_maille_vierge_porte_une_annee_de_fourrage_quel_que_soit_le_jour` ;
  le rejeu porte maintenant sur les 360 jours de l'année qui précède (même jour de l'année).
  `cap_demo` : seule la première année change (taïga an 1, méd. 3,6 → 15) ; équilibres
  identiques. **Voisin non corrigé** : le stock de cueillette (`gathering`, D10) a le même
  rejeu depuis le jour 0 — les humains de toutes les scènes partent avec 135 jours de cueillette
  sur pied au lieu d'une année ; à ouvrir dans son domaine.
- Suite complète avant ces deux corrections (`e747c14`) : 301 verts, deux rouges connus.

## Périmètre de CAP (décision de l'utilisateur, 2026-11-02)

« On finit CAP tout en préparant le terrain pour les autres chantiers. » La pluie n'a pas de
saison (**PLU**) et la pousse ne lit que la température (**POU**) : CAP ne juge et ne règle
**rien** dans les biomes où l'eau fait la saison maigre (savane, steppe, déserts) ni dans les
mailles qui poussent toute l'année ; aucune constante n'y compensera l'absence de saison sèche.
CAP se valide là où la température fait la saison : taïga, toundra, forêts et prairies à hiver.
Suite : (a) vérifier si la scène tempérée de référence est dans une maille sans saison ;
(b) la part de végétation mangeable (étape 4), jugée sur ces biomes ; (c) les prédateurs (3.3) ;
clôture. **Hypophagie d'hiver écartée** : l'ingestion volontaire baisse de 34-37 % l'hiver (cerf,
renne), mais l'entretien aussi, de ~22 % (cerf de Virginie, 97 contre 125 kcal/kg^0,75/j) ; sans
qualité du fourrage (plus pauvre l'hiver), les deux ensemble feraient **gagner** des réserves à
un cerf qui trouve sa ration d'hiver, quand le réel en perd ~20 % : le mécanisme ne tient pas
seul, et la qualité du fourrage est hors de CAP.

### (a) Les scènes de référence et la saison (`cap_scene`, 2026-11-03)

| seed | tempéré : foyer, jours de pousse, mailles sans hiver à 8 km | froid : foyer, jours de pousse |
|---|---|---|
| 42 | forêt tempérée, **360**, 49/49 | taïga, 166 |
| 7 | prairie, 268, 0/49 | prairie, 172 |
| 1337 | forêt tempérée, **360**, 29/49 (moyenne 257 j) | taïga, 170 |
| 2024 | prairie, 284, 0/49 | taïga, 154 |

Les 4 scènes froides et 2 tempérées sur 4 sont dans le périmètre de CAP ; la scène tempérée 42
(entière) et 1337 (pour moitié) poussent toute l'année — hors périmètre jusqu'à **POU**.

### (b) Étape 4, la végétation mangeable : pas de correction faute de source (2026-11-03)

Deux recherches (`../SOURCES.md`, sections CHA-5 et « étape 4 ») : rien de solide ni de propre
à un biome pour la forêt tempérée, la taïga ou la toundra (production de brout et de lichen en
g/m²/an, part de la PPN mangée par les grands mammifères). Seul chiffre lu : en prairie de
Yellowstone, wapiti et bison mangent ~45 % de l'ANPP (0-266 g/m² retirés selon la prairie) —
le modèle donne 30 % de la PPN aux herbivores de prairie : **pas d'écart démontré** là où l'on
sait mesurer. Changer les fractions de forêt ou de taïga sans chiffre serait calibrer vers une
densité (règle 10). **Décision : les fractions restent telles quelles, marquées non sourcées**
(`fauna::accessible_forage_g_m2_yr`). L'écart de niveau qui reste (cerf 40-90 /km² en forêt sans
prédateurs, contre 10-35 réels sans prédateurs ; aurochs 35-70 /km² contre bison 0,3-0,5 à
Yellowstone) est porté par ce qui n'est pas mesurable aujourd'hui, et, pour les ongulés de moins
de 150 kg, par les prédateurs absents (Sinclair 2003) : c'est l'étape 3.3.

## 3.3 — les prédateurs : lecture (2026-11-03)

Règles (`fauna.rs`, étape 1, Vie) : prises 0,05 par loup et par jour au contact d'un troupeau
(≤ 500 m), une prise par rencontre sauf proie à bout de souffle ; croissance `0,42 × prises −
0,020 × effectif` par jour, logistique sous le territoire (K ≈ 10 dans un disque de 8 km) ;
mort sous 1 prédateur ; aucune saison, aucune réserve, aucune dispersion ; immigration (garde
de coût) à 0,5-2 km d'un troupeau au hasard.

**Mesuré** (`out/cap/j32/`, sans humains ni immigration, 12 cases) : les loups tuent **12,9 à
17,9 ongulés par loup et par an**, dans la fourchette réelle (15-25, `../SOURCES.md`). Ils
s'éteignent pourtant sur 9 cases sur 12 (derniers loups aux jours 270 à 1 905). Le modèle exige
0,020 ÷ 0,42 = 0,048 prise par jour, soit **17,3 par loup et par an, pour seulement se
maintenir** : les trois cases où ils survivent (seed 1337) tuent 17,8-17,9 /an, les autres 13-17.
Le défaut n'est donc pas la chasse — la rencontre spatiale produit un taux réel — mais la
**conversion démographique** : la « mortalité » de 0,020 /j (7,3 /an, une meute sans proie
fondant de moitié en 35 jours) tient lieu de famine, sans réserves, et un loup réel vit d'un
taux de prise ordinaire, pas de son maximum. Ni saison des naissances, ni dispersion des jeunes.

## 3.3 — conception proposée (2026-11-03, à valider par l'utilisateur)

La chasse spatiale reste telle quelle (elle donne 13-18 prises par loup et par an, le taux
réel) ; on remplace la **conversion démographique** par la même vie que les troupeaux :

1. **Viande sur pied** : une meute garde la viande de ses prises (masse comestible de l'espèce,
   cerf 60 kg), qui se perd en quelques jours (charognards, τ = 5 j, **choix**) ; chaque loup en
   mange jusqu'à sa ration de 4,5 kg/j (wolf.org). **Elle ne chasse que si ses réserves de viande
   tombent sous deux jours de ration** (choix : un loup repu ne chasse pas).
2. **Réserves** des loups, même forme linéaire que les troupeaux, part d'entretien
   **≈ 0,6 = 2,7 / 4,5 kg** (seuil de reproduction réussie 2,3-3,2 kg/j sur 4,5 — la même valeur
   que celle des herbivores, ici sourcée) ; durée de jeûne qui les vide : **30 jours, choix**
   (aucune durée sourcée ; « plusieurs jours » sans dommage).
3. **Deux classes**, adultes et louveteaux ; **une portée par meute et par an** au reverdissement
   de sa maille, 4,8 petits × réserves (Yellowstone) ; mortalité de fond adultes 0,22 /an (survie
   0,78), louveteaux 0,30 /an (choix ; 40-60 % au total la première année, la faim faisant le
   reste) ; faim sous 20 % (adultes) et 50 % (petits) comme les troupeaux.
4. **Dispersion à la place de la fission** : au reverdissement, les yearlings deviennent adultes
   et 75 % d'entre eux partent (8 % des adultes), en groupe, fonder une meute à 16-40 km (choix :
   le réel va à 80-160 km, mais le monde simulé autour des humains est plus petit) ; là où le
   territoire est plein (K ≈ 10 dans un disque de 8 km, 0,05 /km² — Yellowstone 0,054), ils
   n'ont pas accès aux proies et meurent de faim. Plus de mort d'une meute « sans proie en 35
   jours » : elle s'affame à travers ses réserves.
5. Supprimés : `PRED_CONV`, `PRED_DEATH_PER_DAY`, la fission à 8. Gardés : territoire, prises
   au contact, immigration (garde de coût, inchangée).

Prédictions à écrire avant le code (banc `cap_nature` sans humains, 10 ans, 4 seeds × forêt,
taïga, prairie) : prédateurs vivants à l'an 10 sur ≥ 10 cases sur 12 ; 15-25 prises par loup et
par an ; densité ≤ 0,05 /km² ; gibier du carré abaissé de 10 à 40 % par rapport au témoin
`d0e6e3e` (cerf et renne ; aurochs moins).

### 3.3 — implémentation (2026-11-03, validée par l'utilisateur)

Tests rouges vus échouer (interface `pack_tick` posée sur l'ancien comportement) :
`une_meute_qui_tue_au_taux_reel_se_maintient` (18 cerfs par loup et par an : 0,34 loup en
3 ans) ; `une_meute_sans_proie_vit_d_abord_sur_ses_reserves` (5,36 loups sur 8 après 20 jours) ;
`les_jeunes_d_un_an_se_dispersent_au_reverdissement` (0 dispersant). Prémisse du premier test
passée de 15 à 18 cerfs par an en l'écrivant : à 60 kg comestibles, 15 cerfs font 2,5 kg/j,
sous l'entretien (2,7) — le cap de prises du modèle (0,05 /loup/j, 18 /an) donne au mieux
3 kg/j de cerf. **Voisin à mesurer** : ce cap est compté en têtes ; le plateau réel (Messier,
3,36 orignaux/loup/100 j) vaut plutôt ~6 kg/j ; sur une proie de 60 kg, le loup du modèle
vit à la limite de son entretien.

Retirés avec leur sujet (croissance `0,42 × prises − 0,020`, fission à 8) :
`une_meute_qui_prospere_finit_par_se_scinder`, `une_meute_bien_nourrie_croit`,
`une_meute_seule_croit_jusqu_a_ce_que_porte_son_territoire` ; réécrits sur `pack_tick` :
`une_meute_sans_proie_s_eteint`, `une_meute_ne_croit_pas_plus_vite_qu_un_loup` ; ajoutés :
`sans_territoire_une_meute_s_affame`, `une_meute_nourrie_essaime_l_annee_suivante` (placé sur
le foyer de référence : à l'origine, la maille ne pousse jamais, n'ouvre pas de saison, et la
meute n'y met jamais bas). Suite complète : 305 verts, les deux rouges connus.

### 3.3 — mesure (binaire `7b5dd3f`, `out/cap/j33/`) : prédiction réfutée

Sans humains, 10 ans, 12 cases : **loups éteints sur 12 cases sur 12** (avant 3.3 : 9 sur 12) ;
prises 6-16 par loup et par an ; gibier du carré à l'an 10 de −13 à +85 % du témoin `d0e6e3e`
(sans sens). Les quatre prédictions tombent.

Trace (`cap_nature` instrumenté : réserves des loups, viande en jours de ration, louveteaux ;
seed 1337, 2 ans) — deux causes distinctes :
1. **Forêt** : viande toujours à 0, réserves 0,07-0,4, meutes éteintes au jour ~200. Le cap de
   prises est compté **en têtes** (0,05 par loup et par jour) : sur un cerf de 60 kg comestibles,
   au plus 3 kg par loup et par jour, sous l'entretien (2,7) dès que le contact n'est pas
   permanent. Le plateau réel est en **masse** (Messier : 3,36 orignaux par loup et par 100 j,
   ~6-7 kg/j). Le voisin signalé à l'implémentation est la cause.
2. **Prairie** : réserves pleines, deux jours de viande en stock (la meute repue ne chasse
   plus) — mais **aucune portée la deuxième année** : la mortalité de la première année (famine
   des 80 premiers jours, avant la première prise) et la dispersion ont laissé ~1,4 adulte par
   meute, sous le couple exigé ; et les dispersants partent par fractions de moins d'un loup,
   effacées sous `PACK_MIN`. Une population continue de « 1,4 loup » ne se reproduit pas.

Corrections, une à la fois : (a) le cap de prises en masse ; (b) la dispersion d'un groupe
trop petit pour fonder (les fractions effacées).

### 3.3 — corrections (2026-11-03)

- **(a) Plafond de chasse en masse** (`0d6b7ca`) : test rouge
  `le_plafond_de_chasse_d_un_loup_est_une_masse` (5 loups au contact de cerfs : 0,62 kg par
  tick, plateau 1,40) ; `WOLF_MAX_KILL_KG_DAY` = 6,7 kg (Messier 3,36 orignaux/loup/100 j ×
  ~200 kg comestibles, masse de l'orignal choisie) remplace 0,05 tête par loup et par jour.
- **(c) Bug de déplacement trouvé par la trace** (`062eb06`) : après (a), les meutes de forêt
  (seed 1337) mouraient encore — toutes arrêtées au jour 216 sur une rive (altitude 0), la proie
  la plus proche à 11 km : chaque pas de poursuite en ligne droite tombait dans l'eau et était
  refusé. Test rouge `une_meute_contourne_l_eau_qui_la_separe_de_sa_proie` ; la poursuite essaie
  à 45°, 90°, 135° de chaque côté. Trace sur 3 ans ensuite : loups vivants (7-16, réserves
  pleines), cerfs du carré 7 → 5 têtes/km² et en baisse.
- (b) la dispersion par fractions reste à traiter si le banc la montre encore.

**Question de l'utilisateur et décision (2026-11-03)** : « poser la limite en têtes ou en masse,
c'est scripté ou pas ? Si on se base sur ses besoins et ses dépenses, il décidera de lui-même. »
Réponse : le plateau de Messier est un taux **mesuré sur le terrain**, déjà fait de recherche,
poursuite, échecs, mise à mort et digestion ; le poser par-dessus une recherche et une poursuite
simulées impose un résultat que le modèle devrait produire (demi-calibration), et la prise par
**fraction** de troupeau à chaque heure de contact est un artefact. **Décision** : après le banc
en cours, remplacer le plafond par le mécanisme — la meute chasse quand elle a faim (déjà), une
attaque réussit ou échoue (taux de succès réel par proie, à sourcer), une réussite tue une **bête
entière**, la meute mange au rythme de son estomac (~9 kg par loup et par repas), le reste se
perd ; le taux de prise **émerge** et se compare au réel (15-25 /loup/an, Messier). Puis, si cela
tient, **appliquer le même principe au gibier** (besoins et dépenses, aucune limite posée).

## Étape 3 — corriger règle par règle

Dans l'ordre de l'amont à l'aval, un changement à la fois, chacun mesuré contre la référence de
l'étape 2 et jugé sur les sources. Aucune constante réglée vers une densité visée. Le champ par
maille de CHA-5 vient ensuite.
