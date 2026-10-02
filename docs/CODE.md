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

**Cerveau** (`brain::decide`). Un agent re-délibère **tous les 4 ticks** (décalé selon son identifiant) ou quand sa tâche est finie. Il score jusqu'à **22 tâches** candidates par des courbes de réponse sur ses besoins, les pondère par le coût du trajet, et tire au softmax (τ = 0,12). Les tâches : boire, cueillir, chasser, pister, partir en quête, dormir (sur place ou au campement), s'abriter, errer, suivre un parent, rejoindre un congénère, explorer, pèlerinage, revenir au clan, manger au stock, rapporter la chasse, bâtir, expédition, cultiver, attaquer une meute, garder le cheptel, razzier. Une tâche en cours reçoit un petit bonus.

**Ce qui tient un humain près des autres**, lu dans les scores :
- `ReturnToClan` : un membre à plus de 4,5 km du foyer de son clan, ou sur le territoire d'un autre clan, y revient.
- `Socialize` : un adulte à plus de 500 m de **l'humain le plus proche** marche vers lui (score 0,4 × sociabilité). Pour un humain sans clan, c'est la seule force de rappel, et elle vise un individu, pas un groupe.
- `Explore` n'est proposé qu'au confort et vise une cellule inconnue à moins de 2 km d'une source connue ; `Wander` (bruit de fond 0,06, plus le désespoir) n'a aucun ancrage.

**Alimentation** (D10, 2026-10-01). La nourriture végétale vient d'un **stock comestible par maille de 2 km** (`gathering`, tenu par `World`) : part comestible de la NPP du biome (0,3 à 1 %, 4 kcal/g), née les seuls jours de croissance, perdue avec une durée de vie de 60 jours, avancée jour par jour à la consultation ; la cueillette en retire, la tuile n'est plus que foulée. Une prise de chasse vaut l'énergie de la bête (`Species::edible_kcal`, un cerf ≈ 72 000 kcal, 14 points de faim) ; à portée, la mise à mort est un tirage à 2 %/h × (0,5 + compétence) (~10 % par journée de chasse, Hadza et Ju/'hoansi). Le chasseur mange sa ration, **offre** une part du surplus aux affamés à 500 m selon sa sociabilité, **garde** le reste (porté, mangé au-delà de 0,25 de faim, gâté en 3 j) et le rapporte au clan s'il en a un. La réserve du clan **pourrit** (3 j, ×2 sous grenier, jusqu'à 180 j selon la part des membres qui savent conserver — technique `preservation`). L'envie de chasser suit le **besoin de viande** : faim moins ce qu'on porte, ou manque de la réserve du clan. Un affamé se souvient du dernier troupeau vu et peut le **pister** ; un membre de clan sans gibier en vue part **en quête** sur le territoire et peut **rentrer dormir au campement**. Le lait maternel nourrit le nourrisson.

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

## 4. Défauts et hypothèses tirés de la lecture

Chaque ligne suit la boucle de la règle 9 de `CLAUDE.md`. Statut : **hypothèse** (posée, pas encore mesurée), **mesuré** (une mesure l'a établi), **corrigé**, **réfuté**. L'ordre suit la carte, de l'amont vers l'aval.

| # | Défaut lu dans le code | Référence violée | Nature | Hypothèse | Ce qui la réfuterait | Statut |
|---|---|---|---|---|---|---|
| D1 | La découverte du feu passe par une porte de froid **tout ou rien** (probabilité exactement nulle sans gel), et par des incendies dont la fréquence ne suit ni la surface ni le climat (D9) | BRIEF §5.2 : la pression est un **facteur** (« un clan repu n'invente *presque* rien ») ; §5.3 : « foudre → feu observé → maîtrise du feu » ; critère §9 : au moins un clan sur trois en 500 ans — un plancher sur un long horizon | modèle | Les deux mécanismes, non naturels, produisent les deux extrêmes observés : feu en un ou deux ans en pays froid (etincelle, an 1), jamais en pays tempéré (`chronicle`, 50 ans). **Une correction ne viserait pas à faciliter le feu** mais des fréquences naturelles — plus rare là où il est trivial, possible mais rare ailleurs ; la découverte des matériaux reste du hasard. | Des découvertes du feu étalées dans le temps et entre climats, sans porte de froid | hypothèse — **deux mesures d'abord, aucune correction** : (a) incendies vus par clan et par an selon le climat ; (b) par quelle porte passe chaque découverte, et quand. Le critère des 500 ans ne se juge pas sur des runs de 50 ans. Garder le froid comme porte ou en faire un facteur est une **décision de conception** (utilisateur). |
| D2 | Un humain sans clan n'a ni pression ni insight, et n'est rappelé que vers l'humain le plus proche | BRIEF §5.1 : le clan est une conséquence ; rien n'y interdit à un isolé d'errer, mais rien ne le ramène vers un peuple | modèle | La dispersion humaine (et la faune qu'elle entraîne) naît des non-affiliés, qui forment des paires ou des petits groupes loin de tout | Une dispersion portée par des membres de clans | hypothèse |
| D3 | Voir un gisement exige de poser le pied sur sa tuile (2 m) | BRIEF §5.2 : « a déjà VU » un affleurement | modèle | Le cuivre est quasi inatteignable par la marche ordinaire | Des expositions au cuivre fréquentes là où un gisement est à portée | hypothèse |
| D4 | La famine n'agit qu'au-delà de 0,5 de faim moyenne | BRIEF §5.2 : la famine est une pression | modèle (calibrage) | La composante famine ne s'active presque jamais | Une composante famine non nulle sur une part notable des jours-clan | hypothèse (mesure partielle : faim moyenne max 0,19 sur `chronicle`, 2026-09-22) |
| D5 | `ClanPressure::total` n'est lue que par les tests | — | code mort | Aucun effet sur la simulation | — | à nettoyer |
| D6 | L'hydrologie n'est pas simulée ; l'eau douce = sources aléatoires | BRIEF §2.2 (rivières, lacs, deltas) et §5.3 (« feu + argile + rivière → poterie ») | modèle (choix) | Pas de rivière pour structurer l'habitat humain | — | à décider |
| D7 | Un chunk évincé rattrape sa repousse d'un coup, à minuit | Déterminisme : le monde ne doit pas dépendre de la capacité du store | bug | — | — | mesuré (70 % des tuiles broutées diffèrent à 30 j entre deux capacités) |
| D8 | Documentation d'`exposure.rs` périmée ; `springs_for` contourne le cache d'humidité | — | documentation / coût | — | — | à nettoyer |
| D9 | Incendies, météo et immigration de la faune : **une tentative par jour pour tout le monde**, placée près d'un humain tiré au hasard (le lieu tiré filtre ensuite par climat et combustible) | La nature : un phénomène naturel a une fréquence par surface et par climat | modèle (artefact du périmètre de simulation) | Ce qu'un peuple voit (incendies, averses, gibier qui arrive) dépend du nombre et de la répartition des humains : un peuple seul reçoit tout, dix peuples se partagent la même quantité | Une fréquence par peuple indépendante du nombre de peuples | hypothèse |
| D10 | La nourriture ne contraint jamais les humains : faim +1/48 par heure, 0,02 de faim par unité de biomasse, soit ~25 unités par jour ; une tuile de forêt en repousse ~4,2 : ~6 tuiles (24 m²) nourrissent un humain, une capacité implicite de ~40 000 humains/km² — le même défaut de calibrage que les 44 000 cerfs/km² | BRIEF §9 phase 3 : « 50 → 300 en 100 ans sans explosion malthusienne (l'écologie doit la borner) » ; §1 : « la nécessité est la mère de l'invention » | modèle (calibrage) | La faim ne régule ni la population ni l'invention : D4 (famine muette) en est un symptôme, et la croissance humaine n'a d'autre frein que la soif et la vieillesse | Des morts de faim, ou une famine active, dans une population qui croît | **corrigé (2026-10-01)** — neuf mécanismes (journal, « D10 ») ; prouvés : garder sa prise (survivants ×1,8), partage, mise à mort incertaine ; justes mais non prouvés : besoin de viande, piste, campement. Validation 4 seeds × 2 scènes × 5 ans, départ d'été : 8/8 survivent, densité de l'an 5 à 0,46-0,49 hab./km² sur 5 runs, famines par épisodes en froid, gibier vivant. La population ne croît pourtant pas : D11 et D12 la bornent. |
| D11 | La soif tue plus que la faim : 2 à 31 morts de soif en 5 ans par run (validation D10), presque toutes **à 250-330 m d'une source connue** | La réalité : un groupe de chasseurs-cueilleurs vit près de l'eau et n'en meurt pas | bug (mécanisme) | « Boire » visait toujours la source connue la plus proche à vol d'oiseau ; derrière une étendue d'eau, la marche butait, la tâche s'effaçait, l'assoiffé revisait la même source jusqu'à en mourir, avec quinze autres en mémoire | Des morts de soif rares | **corrigé (be7e3da)** : une source où la marche bute est mise de côté deux jours. 8 runs × 2 ans : 28 → 11 morts de soif, population 497 → 517 ; tempéré 42 : 20 → 0. Restent des **nourrissons** morts de soif (ils ne boivent que le lait d'une mère qui peut en manquer) |
| D12 | ~~La natalité est très basse~~ — **réfuté** : la natalité tourne au maximum biologique du modèle (73 % des jours-femme enceinte ou à allaiter ; 157 conceptions attendues, 155 naissances) ; la « baisse » était un effet de cohorte des fondatrices (baby-boom puis trois ans d'allaitement) | — | — | — | — | **réfuté par la mesure (3cc6227)** ; la stagnation vient des morts, voir D13 |
| D13 | La violence pèse 29 % des morts sur 3 ans (63 % sur 2 ans), concentrée en épisodes très meurtriers (froid 1337 : 22 tués en 5 affrontements) | Bowles 2009 : ~14 % des morts chez les chasseurs-cueilleurs nomades ; Fry & Söderberg 2013 : ~9 % corrigé | modèle | Premier frein à la croissance là où deux clans se touchent | Une violence rare, peu meurtrière par épisode | **mesuré (f0d8b68)** : le critère de rareté (gibier < 3 têtes par bouche) est rempli ~100 % du temps dès que deux clans se touchent — il ne départage plus rien ; la violence est épisodique (4 périodes de 30 j sur 80) mais un blessé repart au combat à chaque délibération. **Prototype R1** (branche `d13-blesses`, non fusionné) : on ne razzie pas blessé — affrontements 10 → 3, violence 29 % → 10 % des morts, sur 2 runs seulement (règle 7 non satisfaite). Ouvert : ce qui déclenche la tension (critère de rareté), la fuite des victimes. **Voisin** : depuis quête + campement (c95b10e), les clans d'un petit groupe se forment et se dissolvent sans cesse (9 formés, 6 dissous en ~100 jours dans la scène de 24) ; le test `la_chronique_se_remplit_en_jouant` ne passait plus que grâce à une razzia |
| D14 | **Une technologie implicite.** La réussite de la chasse est calée sur des chasseurs à l'**arc** (Hadza, Ju/'hoansi), la létalité des combats sur la force et la compétence seules, et l'arbre technologique commence au feu : aucune arme, aucun outil n'existe dans le modèle, et pourtant leur effet est supposé | La réalité : des humains anatomiquement modernes ont toujours un outillage (éclats de pierre, épieu) ; l'arc, le propulseur, les pièges, la pierre polie, puis le métal sont des innovations | modèle | Le calage de la chasse vaut pour une technologie que les humains n'ont pas inventée ; la violence (D13) ne dépend d'aucune arme ; le bronze ne sert à rien de concret | — | **décision de conception actée (utilisateur, 2026-10-01)** : les fondateurs arrivent avec **l'outillage de base de leur espèce** (éclats de pierre, épieu, savoir dépecer) — ce n'est pas une innovation, c'est le point de départ ; les innovations commencent au-delà (arc, propulseur, pièges, filets, pierre polie, métal). Chantier futur, non urgent : un outil a un matériau (efficacité, durabilité), une fabrication et une usure, un savoir-faire transmis ou perdu, et un effet par **facteur**, jamais une porte. Conséquence à reporter alors : **recaler la chasse sur des chasseurs à l'épieu** (le calage actuel sur l'arc deviendra l'effet de l'arc), et faire dépendre la létalité des armes |

Le chantier de dérive (performance), ses mesures et ses corrections sont tenus dans [CARTE.md](CARTE.md). Il a porté presque entièrement sur l'aval de la carte (faune, store) ; D1 à D4 sont en amont.

## 5. L'ordre des chantiers (règles 9 et 10, 2026-09-29)

Un chantier ne passe que si l'objectif qui le **valide** est atteignable aujourd'hui ; sinon on prend le suivant. L'amont de la carte passe avant l'aval.

| Chantier | Ce qui le validerait | Atteignable aujourd'hui ? | Verdict |
|---|---|---|---|
| ~~**D10**~~ — la nourriture ne contraint pas les humains | BRIEF phase 3 : 50 → 300 en 100 ans, bornée par l'écologie | — | **fait (2026-10-01)** pour sa part : la nourriture contraint ; la population est désormais bornée par D11 et D12 |
| ~~**D11**, **D12**~~ — soif, natalité | Des morts de soif rares ; une natalité réaliste | — | **fait (2026-10-01)** : D11 corrigé (bug), D12 réfuté (effet de cohorte) |
| **D13** — la violence | Une violence rare (~10 % des morts), par épisodes | Oui, mais elle n'apparaît que sur 2 runs sur 8 : il faut plus de runs ou de clans | **en cours** — mesuré ; prototype R1 en attente de décision ; le déclencheur (rareté) reste à repenser |
| **Clans instables** (voisin de D13, cœur de D2) | Un clan qui garde son identité tant que son groupe reste le même | Oui : banc `nourriture`, 1 an | **mesuré (2026-10-02)** : 65 dissolutions en 8 runs × 1 an avant quête et campement, 233 après ; mais à la dissolution 91 % des membres sont dans le rayon de résidence — l'hypothèse de l'absence est réfutée. Pas un simple saut d'identité non plus : un clan qui se forme ne reprend en moyenne que 41 % de ses membres à un clan dissous la veille (31-60 %) — les groupes se **recombinent** presque chaque jour (scissions, fusions). À lire dans `social::detect_clans` (seuils de modularité, hystérésis) |
| **D2** — les humains sans clan se dispersent | BRIEF phase 4 : « un clan affamé s'effondre, ses survivants rejoignent d'autres clans ou en fondent un nouveau » | Oui : 50 ans suffisent (seed 2024 : 84 % hors clan à l'an 15, dispersés 18 ans ; seed 42 : 100 % hors clan de l'an 25 à 35) | **après D13** — dépend de D10 (un clan qui ne peut pas avoir faim ne s'effondre pas pour la raison que dit le BRIEF) |
| **D9** — fréquences naturelles rattachées aux humains | Fréquence par peuple indépendante du nombre de peuples | Oui : bancs courts | en parallèle possible — amont de D1, peu d'effet sur les échecs actuels |
| **D7** — l'éviction change le monde | Trajectoire identique entre deux capacités | Oui : banc `eviction` | avant la phase 6 (persistance), pas avant |
| **D1**, **D3** — feu, cuivre | BRIEF phase 5 : feu chez un clan sur trois en **500 ans** ; bronze ⇒ route | **Non** : 500 ans et beaucoup de clans, hors de portée du débit actuel ; D9 en amont | bloqués — mesures d'observation seulement |
| Performance (objectif de dérive) | ≥ 20 tps chaque année, 50 ans, 4 seeds, 2 scènes | Mesurable, mais ses échecs viennent de D10 (populations sans borne) et de D2 (dispersion) | **après D13 et D2** — ne pas optimiser le coût d'un comportement que le modèle ne devrait pas produire |
| D6 (rivières), D5/D8 (nettoyage) | — | — | à décider / à nettoyer au passage |

## 6. Ce que la lecture laisse en question


Ces points ne se tranchent pas en lisant ; ils demandent de confronter le code aux mesures (étape suivante).

- Quelle part de la population vit hors clan au fil d'une run, et comment évolue-t-elle ? C'est elle qui porte la dispersion, et elle ne peut pas inventer.
- Combien de tuiles de cuivre un peuple a-t-il une chance de fouler ? La taille réelle des gisements n'est pas lue ici.
- Le froid ressenti atteint-il 0 °C dans les foyers où l'on a observé le feu, et jamais dans les autres ?
- La composante famine peut-elle jamais mordre (seuil 0,5 de faim moyenne) ?

## 7. Registre des hypothèses (constantes choisies, 2026-10-02)

Une constante **sourcée** vient d'une mesure réelle ; **ordre de grandeur**, d'une estimation défendable mais non mesurée ; **choix**, d'une décision de modélisation sans source — à surveiller en premier quand un résultat surprend. Les critères du BRIEF ne servent jamais à en caler une (règle 10).

| Constante | Valeur | Statut | Source ou raison | Ce qui la réfuterait |
|---|---|---|---|---|
| `KCAL_PER_HUNGER` | 5 000 kcal (2 500/j) | sourcée | FAO/OMS, dépense d'un adulte actif | — |
| `Species::edible_kcal` | masse × 50 % × 1 200 kcal/kg | sourcée | White 1953 (part comestible), USDA (gibier cru) | — |
| `gathering::edible_fraction` | 0,3 à 1 % de la NPP | ordre de grandeur | fruits, noix, racines accessibles, moins la part des bêtes | des densités humaines hors de 0,04-3 hab./km² à long terme |
| `gathering::EDIBLE_KCAL_PER_G` | 4 kcal/g | ordre de grandeur | noix ~6, graines et racines ~3,5, baies ~3 | — |
| `gathering::PERSISTENCE_DAYS` | 60 j | choix | baies en semaines, noix en mois | une soudure d'hiver absente ou totale partout |
| `HUNT_SUCCESS_PER_HOUR` | 2 %/h à portée × (0,5 + compétence) | sourcée, **pour l'arc** | Hadza (~1 gros animal / 29 jours), Ju/'hoansi (< 27 % des jours) ; mesuré ~10 %/jour dans le modèle | à recaler sur l'épieu quand les outils existeront (D14) |
| rendement d'une prise | 0,75 + 0,25 × compétence | choix | un novice gâche un quart | — |
| `SHARE_RADIUS_TILES` | 500 m | choix | portée de la voix autour d'un dépeçage | — |
| part offerte | surplus × sociabilité | choix | une disposition, pas une règle (vision de l'utilisateur) | — |
| `EAT_CARRIED_HUNGER` | 0,25 | choix | une demi-journée sans repas | — |
| `FRESH_KEEP_DAYS` | 3 j | ordre de grandeur | viande crue à l'air | — |
| `PRESERVED_KEEP_DAYS` | 180 j | ordre de grandeur | viande séchée, gelée, cachée | — |
| `GRANARY_KEEP_FACTOR` | × 2 | choix | au sec, hors d'atteinte | — |
| technique `preservation` | pression Famine, sans matériau | choix, **trop facile** | découverte en un mois dans 5 runs froids sur 8 | à remplacer par cache au froid / séchage (vision de l'utilisateur) |
| `GAME_MEMORY_DAYS` ; arrivée sur la piste | 3 j ; 100 m | choix | un troupeau bouge de quelques km par jour | — (la piste n'a pas d'effet mesuré) |
| quête de gibier | jusqu'à 0,9 × rayon de résidence, score 0,55 × urgence | choix | le territoire du clan | elle a triplé les dissolutions de clans (à lire avec D2) |
| `CAMP_RADIUS_TILES` ; retour au camp | 500 m ; × (0,5 + 0,5 sociabilité), chemin jugé sur 6 h | choix | — | — (pas d'effet mesuré) |
| perception de la maille ; `LEAN_SHARE` | une semaine de nourriture par tête ; 0,3 | choix | — | — |
| `BLOCKED_SPRING_TICKS` ; mémoire | 2 j ; 4 sources | choix | l'échec peut venir d'un budget de calcul épuisé | — |
| `RAID_WOUND_LIMIT` | mi-plaie | choix | instinct de conservation | — |
| `SCARCITY_PER_CAPITA` (ancienne) | 3 têtes de gibier par bouche | **cassée** | calée sur l'ancien gibier ; rare ~100 % du temps en contact (D13) | à remplacer (vision : rareté ressentie, territoire, rancunes) |
| `MATE_RADIUS_TILES` (ancienne) | 400 m, tout ou rien | porte | — | ferme 16 % des jours-femme (mesuré) ; à transformer en couple si D2 le demande |
