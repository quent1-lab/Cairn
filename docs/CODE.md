# Cairn lu dans le code

Lecture du dépôt au commit `10cd5b8` (2026-09-29), faite **à partir du code et de sa documentation**, sans s'appuyer sur les conclusions du chantier de dérive. Ce document dit ce que le programme fait ; il ne dit pas ce qu'il devrait faire, ni ce que les mesures en ont tiré. La confrontation avec les mesures est l'étape suivante.

Sources lues : `Sim::step` en entier (`crates/sim/src/sim.rs`), l'en-tête et les fonctions publiques des 31 modules de `cairn-sim`, les constantes de chaque module, `assets/techs.ron`, et les en-têtes de `cairn-worldgen`, `cairn-protocol` et `cairn-client`. La section 3 liste ce que cette lecture corrige dans [CARTE.md](CARTE.md), qui résumait la session plutôt que le code.

---

## 1. Ce que le code fait, domaine par domaine

### 1.1 Le monde de base (`cairn-worldgen`)

Une fonction pure de la seed et des coordonnées, jamais modifiée. Couches point à point, chacune bâtie sur les précédentes : altitude (bruit fractal sous masque continental), température (latitude et altitude), vent dominant, humidité (advection le long du vent, avec ombre pluviométrique), biome (Whittaker), géologie (roche et gisements : silex, argile, obsidienne, cuivre, étain, or, fer). Le cuivre et l'étain sont tirés dans des champs distincts et ne coïncident pas.

L'**hydrologie** (écoulement D8, rivières, lacs) est calculée sur une région bornée et **n'est utilisée par aucun module de la simulation** : seules les cartes PNG s'en servent.

L'**eau douce** que boivent les agents vient de **sources ponctuelles tirées au hasard** par chunk (`chunk::springs_for`) : densité `1,6·10⁻⁵ × humidité²` par tuile, soit au plus une source pour ~15 chunks en pays très humide. Une tuile de source porte le drapeau `FRESH_WATER`.

### 1.2 Le temps, le climat, la météo

Un tick = une heure ; 24 ticks par jour, 360 jours par an. La température ressentie (`climate::instant`) = moyenne annuelle de la tuile + saison (amplitude 18 °C, hémisphères opposés) + cycle jour/nuit (plus ample en air sec). La végétation ne pousse qu'au-dessus de 5 °C.

La météo (`weather`) fait naître, 6 % des jours, une averse ou une sécheresse près d'un humain (0,5 à 4 km), pour 5 jours ; elle déplace la capacité de la végétation de ±40 % et l'inflammabilité. Elle ne touche jamais les tuiles : c'est un registre à part.

### 1.3 La végétation

Chaque tuile porte une biomasse (0-255) et une fertilité. À minuit, `ecology::daily_regrowth` fait repousser, par une logistique en forme fermée (`r = 0,08/j`, vers la capacité du biome modulée par la météo), **les seules tuiles marquées comme touchées** des chunks sales résidents. Une tuile revenue exactement à son état d'origine sort de la liste. Un chunk évincé garde un instantané de ses tuiles modifiées ; sa repousse reprend au rechargement, en rattrapant les jours manqués d'un coup, à la passe de minuit suivante.

Les feux (`fire`) naissent 10 % des jours, à 0,5-3 km d'un humain, sur une tuile de biomasse ≥ 60, sèche (humidité ≤ 120) et ≥ 5 °C ; ils brûlent un disque (≤ 80 m) pendant 4 jours et exposent au feu les humains à moins de ~1 km.

### 1.4 La faune

**Troupeaux** (`fauna::update_herds`, chaque tick). Un troupeau porte un effectif continu, une espèce (cerf, aurochs, gazelle, renne), une satiété, un état. À chaque tick, dans l'ordre : fuite s'il voit une menace (humain ou meute, rayon d'alerte 0,4-1 km selon l'espèce), sinon pâture (9 sondes à 20 tuiles, broutage de 5 tuiles) ou migration l'hiver ; refus d'un pas qui l'éloigne au-delà de 8 km de tout humain ; puis démographie : `natalité 0,02 × satiété − mortalité 0,01` par jour, où la satiété est le minimum de la satiété de pâture et de celle de sa **maille de 2 km** (production fourragère du biome ÷ rations des têtes présentes). Fission au-delà de 90 têtes ; disparition sous 4. Une fois par jour, deux troupeaux sauvages de même espèce qui se voient fusionnent si leur total reste ≤ 90.

**Meutes** (`fauna::update_packs`, chaque tick). Une meute poursuit sa proie (la garde tant qu'elle est à moins de 6 km), prélève au plus 0,05 tête par prédateur et par jour à portée (0,5 km), se nourrit à 0,42 prédateur par tête prise, meurt de faim à 0,02/j ; sa croissance nette est freinée par la place qui reste sur son **territoire** (≤ ~10 prédateurs dans 8 km), et au-delà seule la part qui a un territoire mange. Fission à 8.

**Domestication** (`pastoral`, quotidien). Un troupeau domesticable (aurochs, renne) près d'un foyer de clan et peu harcelé s'apprivoise, et s'ancre au foyer ; délaissé, il redevient sauvage.

**Périmètre**. Chaque jour : un troupeau naît peut-être (15 %) à 1-4 km d'un humain tiré au hasard, à plus de 3 km de tout autre ; une meute naît peut-être (6 %) près d'un troupeau, loin des autres meutes ; un troupeau sauvage à plus de 16 km de tout humain quitte la simulation. **La faune n'existe que là où il y a des humains.**

### 1.5 Les humains

**Physiologie** (`agent`, chaque tick). Faim +1/48 h, soif +1/24 h, fatigue +1/16 h éveillé. Le froid monte sous 0 °C ressenti (+4 °C en forêt, +8 °C abrité, chaleur d'une hutte de son clan). Les besoins saturés entament la santé ; la mort survient à santé nulle, avec sa cause.

**Cerveau** (`brain::decide`). Un agent re-délibère **tous les 4 ticks** (décalé selon son identifiant) ou quand sa tâche est finie. Il score jusqu'à **20 tâches** candidates par des courbes de réponse sur ses besoins, les pondère par le coût du trajet, et tire au softmax (τ = 0,12). Les tâches : boire, cueillir, chasser, dormir, s'abriter, errer, suivre un parent, rejoindre un congénère, explorer, pèlerinage, revenir au clan, manger au stock, rapporter la chasse, bâtir, expédition, cultiver, attaquer une meute, garder le cheptel, razzier. Une tâche en cours reçoit un petit bonus.

**Ce qui tient un humain près des autres**, lu dans les scores :
- `ReturnToClan` : un membre à plus de 4,5 km du foyer de son clan, ou sur le territoire d'un autre clan, y revient.
- `Socialize` : un adulte à plus de 500 m de **l'humain le plus proche** marche vers lui (score 0,4 × sociabilité). Pour un humain sans clan, c'est la seule force de rappel, et elle vise un individu, pas un groupe.
- `Explore` n'est proposé qu'au confort et vise une cellule inconnue à moins de 2 km d'une source connue ; `Wander` (bruit de fond 0,06, plus le désespoir) n'a aucun ancrage.

**Mémoire** (`memory`). Au plus 16 sources connues (les plus lointaines s'oublient), et les cellules de 512 m visitées (au plus 4 096). Deux agents à moins de 120 m échangent leurs sources toutes les 4 heures.

**Démographie** (`demography`, minuit). Une femme féconde (15-45 ans) conçoit à 1,2 % par jour si un homme est à moins de 400 m et si elle est en état ; gestation 270 jours ; pas de conception pendant l'allaitement (3 ans) ; traits hérités (moyenne des parents + mutation) ; sénescence de Gompertz. Adulte à 14 ans.

**Compétences** (`skills`). Cueillette, chasse, oratoire, combat : elles montent par la pratique vers un plafond fixé par les traits.

### 1.6 La société

**Clans** (`social`). Chaque rencontre à portée renforce un lien (plus vite entre parents), qui s'érode sans entretien ; au plus 15 liens par agent. Chaque jour, les groupes qui sont à la fois cohésifs et co-résidents (70 % à moins de 4,5 km du foyer), d'au moins 8 membres, sans ligne de faille (modularité sous 0,3 pour un groupe nouveau, sous 0,42 pour un clan existant : hystérésis), deviennent des clans ; un groupe avec une ligne de faille se scinde le long de celle-ci. L'identité d'un clan persiste d'un jour à l'autre par son noyau.

**Vie de clan.** Un foyer (centroïde, ancré à la hutte du chef une fois bâtie), un territoire, un stock commun (plafond 3 par membre, plus les greniers), un chef (prestige), des tensions avec les voisins — nées de la **rareté du gibier** (têtes de troupeau par bouche à nourrir autour du point médian des deux foyers) — qui mènent aux **razzias** (`combat::resolve_clashes`). Les structures (hutte, grenier, palissade) se bâtissent sur le stock quand la pression correspondante (froid, remplissage, menace) dépasse son seuil ; abandonnées, elles tombent en ruine.

**Combat** (`combat`). Des humains agressifs attaquent une meute proche du foyer ; la meute encaisse la somme des coups et riposte en plaies, mortelles à 1. Les morts « par prédation » du code sont ces assaillants blessés : les meutes n'attaquent jamais d'elles-mêmes.

### 1.7 L'innovation

**Exposition** (`exposure`). Un agent « voit » le gisement et la matière **de la tuile sous ses pieds** (lue une fois par tick) ; il peut aussi voir une matière par le troc avec un voisin (`tech::diffuse`), ou l'étain au bout d'une expédition. Le feu s'ajoute en voyant brûler.

**Pression** (`pressure`, quotidienne, **membres d'un clan seulement**). Quatre composantes : famine (faim moyenne au-delà de 0,5), froid (froid moyen des membres), menace (tension maximale avec un voisin), surpopulation (membres / 40). Froid, famine et menace gardent une mémoire décroissante (1 %/j). Le total combiné (`ClanPressure::total`) **n'est utilisé que par les tests**.

**Découverte** (`tech::insight`, quotidienne). Un adulte au confort (faim, soif < 0,6 ; froid < 0,4 ; fatigue < 0,7), **membre d'un clan**, peut découvrir une technologie dont il tient les savoirs préalables, les expositions et l'accès (eau douce à 2 km, forêt sous les pieds), avec une probabilité quotidienne `0,004 × curiosité × compétence × pression` — où la pression est **la plus forte des composantes que la technologie déclare** :

| Technologie | Préalables | Composantes de pression |
|---|---|---|
| Maîtrise du feu | vu du bois, et du silex ou du feu | **froid** |
| Cuisson | feu | famine |
| Poterie | feu, vu de l'argile, eau douce | famine, froid |
| Agriculture | poterie, vu des graminées, eau douce | famine |
| Four | poterie | surpopulation |
| Métallurgie du cuivre | four, vu du cuivre | surpopulation, menace |
| Bronze | cuivre, vu de l'étain | menace, surpopulation |

**Diffusion et oubli.** Une technologie passe à un voisin à 5 % par rencontre (toutes les 4 h) s'il en tient les préalables ; une technologie sans porteur vivant est perdue. **Commerce** : un clan qui maîtrise le cuivre sans avoir vu d'étain envoie son meilleur marcheur vers l'étain le plus proche.

### 1.8 La divinité et le récit

**Foi** (`faith`) : individuelle, née d'avoir été témoin d'une intervention, transmise par la parole, décroissante. **Interventions** (`divine::invoke`) : foudre, fertilité, peste, pluie, sécheresse, révélation (à un individu), signe — chacune ambivalente par le lieu choisi. **Culte** (`cult`) : l'agrégat des croyances des membres d'un clan, recalculé à la demande. **Chronique** (`chronicle`) : le journal de ce qui fait date, raconté avec des noms dérivés de la seed (`names`).

---

## 2. Les fonctions : ce qu'exécute un tick

`Sim::step`, dans l'ordre. Les fréquences sont celles du code.

| Étape | Fréquence | Fonctions | Lit | Écrit |
|---|---|---|---|---|
| 0 · instantanés | chaque tick | `herd_views`, `pack_views`, `human_views` | ECS | vues figées du début de tick |
| 1 · délibération | agent tous les 4 ticks | `brain::decide` | vues, mémoire, tuiles voisines (sources, fourrage, abri) | tâche |
| 2 · exécution | chaque tick | `execute` (marche, A* budgété à 8 requêtes, cueillette, chasse, bâtir…) puis `combat::resolve`, `resolve_clashes` | tâche, tuiles | position, besoins, stock de clan, structures, prises |
| 2t · expéditions | chaque tick | `commerce::advance` | positions | exposition à l'étain |
| 2b · nourrissons | chaque tick | `demography::nurse_infants` | mères | besoins des nourrissons |
| 3 · physiologie | chaque tick | `Physiology::drift`, `note_tile` | tuile sous l'agent, climat, huttes | besoins, santé, exposition, morts |
| 3 bis · passes du jour | minuit | `demography::daily`, `social::daily`, `pressure::measure`, `tech::insight`, `tech::forget`, `commerce::dispatch`, `structures::maintain` / `plan` / `anchor_homes`, `pastoral::daily`, `faith::daily`, `weather::daily`, `fire::daily` | tout | naissances, clans, pression, savoirs, expéditions, structures, cheptel, foi, météo, feux |
| 3t · rencontres | toutes les 4 h | `memory::exchange_knowledge`, `social::encounter`, `tech::diffuse`, `faith::preach` | paires à portée | sources connues, liens, savoirs, foi |
| 4a · meutes | chaque tick | `fauna::update_packs` | vue des troupeaux | prises, croissance, fissions |
| 4b · prises | chaque tick | `fauna::apply_kills` | prises (meutes et chasse humaine) | effectifs des troupeaux |
| 4c-4d · troupeaux | chaque tick | `fauna::update_herds`, puis fission, puis `merge_plan` à minuit | menaces, tuiles, mailles | positions, satiété, effectifs |
| 4 bis · immigration | minuit | abandon au-delà de 16 km, `daily_immigration`, `daily_predator_immigration` | humains, troupeaux | nouvelles entités |
| 5 · écologie | minuit | `ecology::daily_regrowth` | tuiles touchées des chunks sales | biomasse |

Le **store** (`World`) est traversé par toutes ces étapes : chaque lecture de tuile passe par `ensure_resident`, qui génère le chunk s'il est absent (humidité des coins et sources en cache, tuiles calculées à la demande), le marque comme accédé et évince le plus ancien si la capacité est dépassée, avec un instantané de ses tuiles modifiées.

---

## 3. Ce que cette lecture corrige dans la carte de session

1. **La délibération revient tous les 4 ticks**, pas « un agent sur 10 » (10 est la valeur du BRIEF).
2. **La pression totale saturée ne bloque rien** : `ClanPressure::total` n'est lu par aucun système. Le moteur d'invention lit la composante que chaque technologie déclare. Le diagnostic « la saturation de la surpopulation rend les trois autres invisibles » (consigné le 2026-09-23) est faux dans son mécanisme.
3. **Le feu exige une pression de froid.** Dans un foyer où le ressenti ne descend jamais sous 0 °C, la maîtrise du feu est **impossible par construction**, quelle que soit l'exposition — et tout l'arbre dépend du feu. C'est le blocage de `chronicle` lu dans le code ; la carte l'attribuait à l'exposition.
4. **Un humain sans clan ne peut rien inventer** (ni pression ni insight hors clan) et n'est rappelé que vers **l'humain le plus proche**. La carte parlait de dispersion sans ces deux faits.
5. **Voir un gisement exige de marcher sur sa tuile** (2 m), en dehors du troc et des expéditions.
6. **L'hydrologie n'est pas simulée** ; l'eau douce est faite de sources aléatoires.
7. La carte omettait presque toute la vie humaine : combat et razzias, structures, stock commun, agriculture, domestication, foi, culte, interventions, Chronique, pathfinding.
8. Deux défauts de code relevés en lisant : la documentation d'`exposure.rs` dit encore que rien ne déclenche l'exposition au feu, alors que `fire.rs` la déclenche ; et `springs_for` recalcule l'humidité de ses coins sans passer par le cache d'humidité.

---

## 4. Ce que la lecture laisse en question

Ces points ne se tranchent pas en lisant ; ils demandent de confronter le code aux mesures (étape suivante).

- Quelle part de la population vit hors clan au fil d'une run, et comment évolue-t-elle ? C'est elle qui porte la dispersion, et elle ne peut pas inventer.
- Combien de tuiles de cuivre un peuple a-t-il une chance de fouler ? La taille réelle des gisements n'est pas lue ici.
- Le froid ressenti atteint-il 0 °C dans les foyers où l'on a observé le feu, et jamais dans les autres ?
- La composante famine peut-elle jamais mordre (seuil 0,5 de faim moyenne) ?
