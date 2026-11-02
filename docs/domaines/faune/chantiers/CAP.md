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

## Étape 3 — corriger règle par règle

Dans l'ordre de l'amont à l'aval, un changement à la fois, chacun mesuré contre la référence de
l'étape 2 et jugé sur les sources. Aucune constante réglée vers une densité visée. Le champ par
maille de CHA-5 vient ensuite.
