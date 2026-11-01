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

## Étape 1 — lire le cycle de vie de la faune (en cours de préparation)

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

## Étape 2 — ce que la nature du modèle produit sans humains

Après la lecture seulement : un banc de référence **sans humains**, par biome, avec les seules
règles classées « nature » (boîte fixe, sans périmètre mobile ni immigration ; appel direct à
`fauna::update_herds` / `update_packs`, comme les tests de `sim.rs`), 4 seeds, plusieurs années.
On relève la densité (minimum et médiane : c'est un réservoir), le rapport proies / prédateurs
et la satiété par saison. Prédictions écrites avant.

## Étape 3 — corriger règle par règle

Dans l'ordre de l'amont à l'aval, un changement à la fois, chacun mesuré contre la référence de
l'étape 2 et jugé sur les sources. Aucune constante réglée vers une densité visée. Le champ par
maille de CHA-5 vient ensuite.
