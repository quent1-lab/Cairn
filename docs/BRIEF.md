# CAIRN — Brief de projet complet

> Document destiné à être fourni comme contexte de départ à un modèle assistant (Claude Fable 5) pour l'accompagnement du développement. Il décrit l'intégralité du projet : vision, univers, systèmes de simulation, architecture technique, direction artistique, et le plan de développement en 6 phases avec critères d'acceptation.

---

## 0. Instructions au modèle

Tu accompagnes le développement de **Cairn**, un simulateur d'évolution culturelle et technologique en monde ouvert infini, écrit **en Rust**, tournant en **serveur persistant 24/7**, observable et influençable via un **client web (WASM)**.

Contexte du développeur :
- Étudiant ingénieur (2ᵉ année, IMT Mines Alès), solide en systèmes embarqués, robotique, ROS2, contrôle et traitement du signal.
- **Rust est en cours d'apprentissage** : le projet est explicitement un projet d'apprentissage doublé d'une pièce de portfolio.
- Familier de C/C++, Python, Docker, Linux, architecture logicielle.

Attentes envers toi :
1. **Expliquer les idiomes Rust au fur et à mesure** (ownership, borrow checker, traits, lifetimes, `Result`/`?`, itérateurs, `Arc`/`Mutex`, async). Ne pas produire de code magique sans explication.
2. Privilégier les solutions **simples et lisibles** avant les solutions performantes ; n'optimiser qu'après mesure.
3. Ne jamais introduire une dépendance sans justifier pourquoi elle est préférable à une implémentation maison de 50 lignes.
4. Rester **fidèle au principe d'émergence** : aucun comportement narratif ne doit être scripté en dur. Tout doit découler de règles locales.
5. Découper en incréments testables. Chaque phase doit produire quelque chose de **visible et jouable**.

---

## 1. Vision

Un monde infini, généré procéduralement, peuplé d'humains simulés individuellement. Ils naissent, ont faim, explorent, se souviennent, se regroupent, inventent, transmettent, oublient, et meurent.

Il n'y a **pas de joueur-personnage**, **pas de quête**, **pas de condition de victoire**.

Le joueur est une **divinité** : elle ne contrôle rien directement. Elle observe, et peut peser sur le monde. La simulation tourne en permanence sur le serveur, que quelqu'un soit connecté ou non. Se connecter, c'est ouvrir une fenêtre sur une civilisation qui a continué sans vous.

**Le principe cardinal** : les âges technologiques (paléolithique, néolithique, âge du bronze…) ne sont **pas des paliers scriptés**. Ce sont des étiquettes calculées *a posteriori* sur un état de connaissances qui a émergé. Une partie peut stagner 3 000 ans au paléolithique parce que le climat local est trop dur pour la sédentarisation. Une autre peut découvrir le bronze en 400 ans parce que le joueur a foudroyé un affleurement de cuivre au bon moment. **Une civilisation peut régresser et oublier ce qu'elle savait.**

Le second principe : **la nécessité est la mère de l'invention**, littéralement encodée dans les équations. Un clan repu et confortable n'invente presque rien.

---

## 2. Le monde

### 2.1 Structure

- Grille de tuiles 2D, vue de dessus.
- Découpage en **chunks de 64 × 64 tuiles**, générés à la demande depuis une **seed globale**, déchargés quand inutilisés.
- Le monde est donc **infini de fait**, avec une empreinte mémoire bornée.
- Coordonnées en `i64` (pas de limite pratique).

### 2.2 Pipeline de génération (physiquement plausible, pas du bruit décoratif)

Chaque étape s'appuie sur la précédente. C'est ce qui produit un monde *crédible* plutôt qu'un patchwork aléatoire.

1. **Altitude** — bruit fBm (simplex, ~6 octaves) + masque continental. Produit continents, chaînes de montagnes, plateaux, plaines côtières.
2. **Température** — fonction de la latitude (gradient équateur → pôles) **et** de l'altitude (décroissance adiabatique réelle, ≈ −6,5 °C / 1000 m).
3. **Vent** — champ de vent dominant simplifié dépendant de la latitude (alizés, vents d'ouest).
4. **Humidité** — advection de l'humidité depuis les océans le long du champ de vent, avec **effet d'ombre pluviométrique** : l'air qui franchit un relief se déleste de son humidité au vent et arrive sec sous le vent. *C'est cette étape qui crée des déserts crédibles, placés là où la physique les met, et pas au hasard.*
5. **Hydrologie** — calcul de *flow accumulation* (algorithme D8) sur le champ d'altitude, sur une macro-grille grossière puis raffiné localement. Produit rivières, confluences, lacs, deltas, zones marécageuses.
6. **Biomes** — classification de **Whittaker** : le biome est déterminé par le couple (température moyenne, précipitations annuelles). Toundra, taïga, forêt tempérée, prairie, steppe, savane, désert chaud, désert froid, forêt tropicale, mangrove, marais.
7. **Géologie** — couche indépendante des précédentes : type de roche (sédimentaire / ignée / métamorphique) + gisements. Détermine où se trouvent **silex, argile, obsidienne, cuivre, étain, or, fer**.

> **Contrainte de conception majeure** : le cuivre et l'étain sont volontairement générés dans des contextes géologiques **rarement co-localisés**. Conséquence : l'âge du bronze *force* le commerce longue distance. C'est historiquement exact, et cela fournit gratuitement un moteur de gameplay (caravanes, routes, alliances, guerres pour le contrôle des routes).

### 2.3 Données par tuile

Cible : **~16 octets packés**, pour tenir un grand monde en RAM.

| Champ | Bits | Description |
|---|---|---|
| `terrain` | 5 | biome / type de sol |
| `elevation` | 8 | altitude quantifiée |
| `soil_fertility` | 8 | fertilité, dégradable par surexploitation |
| `biomass` | 8 | biomasse végétale actuelle |
| `deposit` | 4 | gisement présent (aucun, silex, argile, cuivre…) |
| `structure` | 8 | bâtiment / aménagement humain |
| `claim` | 8 | ID de clan revendiquant la tuile |
| `flags` | 8 | eau, rivière, côte, brûlé, cultivé, sacré… |

### 2.4 Écologie vivante

- **Végétation** : croissance logistique `dP/dt = r·P·(1 − P/K)`, où `K` (capacité de charge) dépend du biome, de la fertilité du sol et du climat courant.
- **Faune** : troupeaux d'herbivores (proies) et prédateurs. Dynamique proie-prédateur **spatialisée** (type Lotka-Volterra localisé, avec diffusion).
- **Migration saisonnière** des troupeaux → les chasseurs-cueilleurs doivent les suivre → **le nomadisme émerge tout seul** avant l'agriculture. Aucune règle ne dit « les humains sont nomades ».
- **Surexploitation** : la surchasse effondre localement la faune → famine → migration ou guerre. La déforestation autour d'un village dégrade le sol, abaisse `K`, et le village s'effondre ou déménage. **Rien de tout cela n'est scripté** ; tout tombe des équations.

### 2.5 Climat

- Cycle saisonnier annuel (température et précipitations modulées).
- Événements stochastiques **corrélés dans le temps** (chaînes de Markov ou bruit rouge), pas des tirages indépendants : sécheresses pluriannuelles, hivers volcaniques, crues, années fastes.
- **La pression climatique est le principal moteur exogène d'innovation.**

---

## 3. Les entités

### 3.1 Les humains (1 000 à 5 000, simulés individuellement)

| Bloc | Contenu |
|---|---|
| **Physiologie** | âge, sexe, santé, faim, soif, température corporelle, fatigue, grossesse, blessures, maladies |
| **Traits** (hérités) | force, endurance, dextérité, **curiosité**, sociabilité, agressivité — hérités des deux parents (moyenne + mutation gaussienne) |
| **Compétences** (acquises) | chasse, cueillette, taille de pierre, construction, artisanat, agriculture, oratoire, combat — progressent par la pratique (courbe logarithmique, plafond conditionné par les traits) |
| **Mémoire spatiale** | **carte mentale partielle** : l'agent ne connaît que les tuiles qu'il a vues, ou dont on lui a parlé. *L'exploration a donc un sens réel : découvrir n'est pas révéler du brouillard, c'est acquérir une information qui change les décisions.* |
| **Social** | parents, enfants, fratrie, conjoint, affinités et rivalités individuelles, clan d'appartenance, rang, prestige |
| **Savoir** | sous-ensemble des technologies du clan que **cet individu** maîtrise personnellement |
| **Inventaire** | concret et pondéré : 12 kg de viande, 3 silex, une hache — pas un compteur abstrait « +12 food » |

### 3.2 La faune

- Herbivores en troupeaux (cohésion de groupe, fuite, migration saisonnière).
- Prédateurs solitaires ou en meutes.
- Simulés de manière plus légère que les humains (pas d'utility AI complète ; machine à états + steering behaviors).

---

## 4. Le cerveau des agents — Utility AI

**Choix technique justifié** : ni GOAP (planification trop coûteuse à 5 000 agents), ni behavior tree (trop rigide, tue l'émergence). On retient l'**Utility AI**.

### Fonctionnement

- Chaque agent évalue un ensemble d'**actions candidates**. Chacune reçoit un score issu de **courbes de réponse** (linéaire, quadratique, logistique, exponentielle) appliquées à ses besoins et à son contexte.
- Sélection par **softmax** plutôt qu'argmax → deux agents strictement identiques ne prendront pas forcément la même décision. Injecte de la variété sans hasard pur.

### Hiérarchie à trois niveaux

1. **Drive** — motivations fondamentales : *survivre, se reproduire, appartenir, comprendre*.
2. **Tâche** — plan persistant de moyen terme : « suivre ce troupeau vers le sud », « construire cette hutte », « porter ce minerai au campement ». **On ne re-délibère pas à chaque tick**, sinon les agents papillonnent et rien ne s'achève.
3. **Action atomique** — un pas, un coup de hache, une phrase échangée.

### Optimisation : bucketing temporel

L'agent `i` ne re-délibère qu'au tick `t` tel que `t % 10 == i % 10`. Coût de délibération divisé par 10, imperceptible à l'observation.

---

## 5. Clans, culture, innovation

### 5.1 Émergence du clan

**Il n'y a pas de bouton « fonder un village ».** Lorsqu'un groupe d'agents dépasse simultanément un seuil de *cohésion sociale* (densité du graphe d'affinités) et de *co-résidence* (proximité spatiale durable), une entité `Clan` est instanciée :

- **Territoire** : champ d'influence diffusé sur la grille depuis les tuiles occupées, décroissant avec la distance.
- **Stock commun** : ressources mises en commun.
- **Corpus de savoirs** : union des connaissances des membres.
- **Normes** : règles culturelles émergentes (exogamie, partage, hiérarchie, tabous, rites funéraires).
- **Chef** : l'agent maximisant `oratoire × prestige`, recalculé périodiquement. Peut être contesté.

### 5.2 L'innovation — le cœur du jeu

Arbre technologique **stochastique, non linéaire, à prérequis contextuels**. Une technologie n'est pas « débloquée en dépensant des points » : elle est **découverte**, par un individu, dans un contexte donné.

```rust
struct Tech {
    prereq_techs: Vec<TechId>,           // connaissances préalables
    prereq_exposure: Vec<Exposure>,      // a déjà VU : du feu, de l'argile, un affleurement de cuivre
    prereq_environment: Vec<EnvCond>,    // a ACCÈS : à une rivière, à une forêt, à un four
    pressure: Vec<Pressure>,             // famine récente, hiver rude, guerre, surpopulation
}
```

À chaque tick, un agent **oisif**, **curieux**, **exposé au bon contexte** a une probabilité d'**insight** :

```
P(insight) ∝ curiosité × compétence_pertinente × exposition × pression_environnementale
```

Le facteur `pression` est essentiel : **un clan repu n'invente presque rien.** Le facteur `oisif` l'est tout autant : il faut un **surplus** pour dégager du temps de cerveau disponible. Les deux se combinent en une tension : trop de misère → pas de temps pour penser ; trop de confort → pas de raison de penser. Les civilisations décollent dans l'entre-deux.

### 5.3 Chaînes d'innovation — exemples de conception

**Chaîne du feu :**
```
foudre → feu observé → maîtrise du feu → cuisson (+30 % kcal nettes extraites)
       → SURPLUS → temps libre → art, mythes, rites funéraires
```

**Chaîne de la sédentarisation :**
```
feu + argile + rivière → poterie → stockage étanche → survie hivernale
       → sédentarisation possible → observation des graminées → agriculture
       → surplus alimentaire → spécialisation des métiers → hiérarchie sociale
```

**Chaîne du bronze (le clou du système) :**
```
poterie → four à haute température ────┐
cuivre exposé + four ──────────────────┴→ métallurgie du cuivre
métallurgie du cuivre + ÉTAIN (à 300 km !) → BRONZE
       → nécessité de caravanes → routes commerciales
       → nécessité de suivre les stocks → COMPTABILITÉ → ÉCRITURE
```

L'écriture n'apparaît pas parce qu'elle est « la case suivante de l'arbre ». Elle apparaît parce que quelqu'un a besoin de compter des sacs de grain.

### 5.4 Diffusion culturelle

Les technologies se propagent **par contact** :
- commerce
- mariage exogame
- guerre (capture de savoir-faire)
- réfugiés

Avec une probabilité **strictement inférieure à 1** — et une **perte possible**. Un clan qui s'effondre **oublie** ses technologies. Un savoir-faire détenu par trop peu de porteurs peut disparaître avec eux.

**L'humanité peut régresser.** C'est réaliste, et c'est dramatiquement puissant.

### 5.5 Les Âges

Simple étiquette calculée : `age = f(ensemble des techs possédées)`. **Affichée dans l'UI, jamais utilisée comme condition de déblocage.**

---

## 6. Le joueur : la divinité

Le joueur n'a **aucun contrôle direct** sur un agent. Il agit sur **le monde**, et observe le résultat.

### 6.1 Ressource : la Foi

Générée par les croyants. **Aucun croyant au départ → la divinité est quasi impuissante au paléolithique.** Ses premiers miracles créent ses premiers fidèles, qui génèrent la Foi qui permet les miracles suivants. Boucle de rétroaction propre et auto-limitante.

### 6.2 Interventions (coût croissant)

| Intervention | Effet | Ambivalence |
|---|---|---|
| **Pluie / Sécheresse** | modifie l'humidité locale | peut sauver ou noyer les cultures |
| **Foudre** | frappe une tuile | peut **allumer un feu de forêt** → peut *donner le feu* à un clan… ou le brûler vif |
| **Fertilité / Maladie** | modifie la santé, la natalité | — |
| **Révélation** | souffle un insight à un agent précis : booste massivement sa probabilité de découverte | très coûteux |
| **Signe** | marque un lieu comme sacré | les clans y bâtissent, s'y rassemblent… et s'y battent |

**Toutes les interventions sont volontairement ambivalentes.** Aucune n'est un « bouton bien ».

### 6.3 Le culte émergent

Les clans **construisent une religion à partir de ce que la divinité fait réellement** :
- Si les miracles arrivent systématiquement après une famine → dieu nourricier → rites d'offrande.
- Si la divinité foudroie → dieu de colère → crainte, sacrifices.
- Si elle n'intervient jamais → dieu absent, ou athéisme, ou schisme.

Ces mythes deviennent leur **culture** → la culture modifie leurs **normes** → les normes modifient leurs **innovations**. La boucle se referme sur le système technologique.

### 6.4 La Chronique

Le serveur génère en continu un **journal narratif** des événements notables, à partir des faits de simulation :

> *An 342 — Après trois hivers de disette, les Ashkar quittent la vallée de Karn. Sur le chemin, Vela, fille de Toru, découvre qu'on peut durcir la pointe d'un épieu en la passant au feu.*

> *An 389 — Les Ashkar et les Orum se disputent le gué de Nherem. Douze morts. Les Orum se replient vers l'est en emportant la technique de l'épieu durci.*

C'est ce qu'on lit en se reconnectant après trois jours d'absence. **C'est probablement le meilleur « produit » du jeu** et il faut le traiter comme un système de première classe, pas comme un log.

---

## 7. Le client : observation et inspection

### 7.1 Direction artistique

**Style pixel art, contrainte de performance assumée.**

- Tuiles **16 × 16 px**. Sprites d'agents **16 × 16** ou **16 × 24**.
- **Palette globale limitée** (32 à 48 couleurs), unifiée pour tout le jeu → cohérence visuelle immédiate, et rendu très léger. Référence : les premiers *RollerCoaster Tycoon*, *Dwarf Fortress* en tileset, *Rimworld*, *Songs of Syx*.
- **Un seul atlas de textures** → un unique draw call par couche.
- Couches de rendu : `terrain → eau/rivières → structures → objets au sol → faune → agents → overlays → UI`.
- **Beauté par le système, pas par le pixel** : ce qui rend le jeu beau, c'est la **variation** (transitions douces entre biomes via autotiling/bitmasking à 47 tuiles, variantes aléatoires de tuiles pour casser la répétition), l'**éclairage** (cycle jour/nuit teintant la palette, lueur orangée des feux de camp la nuit), les **saisons** (le même terrain en vert, en or, en blanc), et la **météo** (pluie, brume, neige en overlay). Un tileset modeste + un bon système de variation bat un tileset riche appliqué platement.
- **Overlays de données** commutables : territoires des clans (aplats colorés semi-transparents), densité de biomasse (heatmap), routes commerciales, zones connues/inconnues, propagation d'une technologie. C'est là que le jeu devient *lisible* — et honnêtement, c'est là qu'il devient beau, parce qu'on voit la civilisation respirer.

### 7.2 Inspection — exigence de première classe

**Tout est cliquable, tout est interrogeable.**

**Clic sur un agent → panneau d'inspection :**
- Identité : nom, âge, sexe, clan, rang.
- **Portrait** généré : composé procéduralement à partir des traits (silhouette pixel + variations de couleur de peau/cheveux/vêtement selon l'âge, le clan et le statut).
- **Barres physiologiques** : santé, faim, soif, fatigue, thermie.
- **Traits** (radar ou barres) et **compétences** (barres avec progression visible).
- **Tâche courante** en clair : « transporte 8 kg de viande vers le campement des Ashkar » — avec la **destination surlignée sur la carte** et le chemin prévu tracé.
- **Pile de motivations** : les 3-4 actions les mieux scorées par l'utility AI, **avec leur score**. C'est un outil de debug *et* une fenêtre fascinante sur le « pourquoi » — l'un des rares jeux où l'on peut littéralement lire les hésitations d'un personnage.
- **Arbre généalogique** interactif : parents, fratrie, conjoint, enfants — chaque nœud cliquable pour naviguer vers cet individu.
- **Relations sociales** : liste des affinités et rivalités, avec intensité.
- **Savoirs personnels** : quelles techs du clan cet individu maîtrise.
- **Inventaire**.
- **Biographie** : les événements marquants de sa vie, extraits de la Chronique.
- Bouton **« suivre »** : la caméra le suit en continu.

**Clic sur un clan / village → panneau d'inspection :**
- Nom, âge (fondé en l'an X), population (avec **pyramide des âges**).
- **Chef** actuel + historique des chefs.
- **Territoire** surligné sur la carte.
- **Stocks** : nourriture (avec autonomie estimée en jours !), matériaux, outils.
- **Technologies possédées** : l'arbre technologique du clan, avec les techs découvertes, celles *en cours d'incubation* (contexte réuni, insight non encore survenu), et celles hors de portée.
- **Normes culturelles** et **panthéon** (ce qu'ils croient de vous).
- **Relations inter-clans** : alliances, rivalités, routes commerciales actives.
- **Courbes historiques** : population, nourriture, technologies dans le temps.
- **Chronique filtrée** sur ce clan.

**Contrainte technique associée** : le serveur doit exposer un endpoint `inspect(entity_id)` renvoyant un **snapshot détaillé** — ces données sont trop volumineuses pour être diffusées en continu à tous les clients. C'est une requête à la demande, pas un flux.

---

## 8. Architecture technique

### 8.1 Workspace Rust

```
cairn/
├─ Cargo.toml                 # workspace
├─ crates/
│  ├─ core/                   # types fondamentaux, IDs, config, RNG déterministe
│  ├─ worldgen/               # bruit, climat, hydrologie, biomes, géologie
│  ├─ sim/                    # ECS, systèmes, utility AI, tech tree, écologie
│  ├─ protocol/               # messages réseau — PARTAGÉ serveur ↔ client
│  ├─ server/                 # tokio + axum, WebSocket, persistance, chronique
│  └─ client/                 # WASM, rendu pixel, UI d'inspection
└─ assets/
   ├─ atlas.png               # tileset 16×16, palette unique
   └─ techs.ron               # arbre technologique en données, pas en code
```

> Le crate `protocol` est partagé entre serveur et client. **Il est donc structurellement impossible de désynchroniser les types entre les deux.** C'est l'avantage décisif du full-Rust ici.

### 8.2 Moteur de simulation

- **ECS : `bevy_ecs` seul** (la bibliothèque ECS de Bevy, utilisable indépendamment du moteur de jeu). Headless, scheduler parallèle, mature.
  - *Alternative pédagogique* : `hecs`, plus simple, mais l'ordonnancement des systèmes est à écrire soi-même. À arbitrer.
- **Tick fixe.** Proposition : **1 tick = 1 heure de jeu**, **20 ticks/s réels** → 1 jour ≈ 1,2 s réelle, **1 an ≈ 7 min réelles**, 1 000 ans ≈ 5 jours. Vitesse ajustable côté serveur ; à calibrer selon ce qu'on veut voir se produire pendant une session de 20 minutes.
- **Déterminisme strict.**
  - RNG PCG seedé par `hash(world_seed, tick, entity_id)`.
  - **Aucune itération sur `HashMap`** (ordre non déterministe) → `BTreeMap` ou `Vec` indexés.
  - Bénéfice majeur : **replay complet du monde depuis la seed + le journal des interventions divines**. Debug reproductible, et une fonctionnalité « rejouer l'histoire depuis le début » obtenue gratuitement.
- **Parallélisme** : `rayon` ou le scheduler de `bevy_ecs`. **Spatial hash grid** pour les requêtes de voisinage → O(1) au lieu de O(n²).
- **Pathfinding** :
  - A* sur grille, mais **budgété** : maximum N requêtes par tick, file d'attente prioritaire.
  - **Flow fields** pour les destinations partagées (le puits du village) : 200 agents allant au même endroit = **un seul calcul**.
- **LOD temporel** — *clé de la viabilité du monde infini* : un chunk sans agent n'est pas simulé tick par tick. Sa végétation est **rattrapée analytiquement** au moment de l'accès (la croissance logistique admet une solution en forme fermée). Coût **O(1)** au lieu de **O(ticks écoulés)**.

### 8.3 Performance visée

5 000 agents × 1 délibération / 10 ticks = 500 délibérations/tick. À 20 tps → 10 000 évaluations/s. **Trivial pour du Rust.** Le vrai coût est le pathfinding, d'où le budget par tick.

**Cible matérielle : 2 vCPU / 2 Go de RAM — un VPS à 5 €/mois.**

### 8.4 Persistance

- **Snapshots binaires** (`postcard` ou `bincode`) toutes les N minutes : chunks + entités.
- **Journal d'événements append-only** (interventions divines + événements notables) → **SQLite**, qui sert la Chronique et les requêtes analytiques (« liste tous les clans ayant découvert le feu »).
- Redémarrage à chaud sans perte d'historique.

### 8.5 Réseau

- **WebSocket** (`axum` + `tokio-tungstenite`).
- Le client **s'abonne à une région** (son viewport), jamais au monde entier. **L'interest management est obligatoire** — c'est ce qui rend le multi-observateur scalable.
- Snapshot initial de la région : tuiles compressées **RLE** (très efficace sur du terrain).
- **Deltas à 5–10 Hz** : uniquement les agents visibles, les tuiles modifiées, les événements.
- Le client **interpole** entre deux snapshots → rendu fluide même à 5 Hz.
- Canal séparé pour les requêtes `inspect(entity_id)` (requête/réponse, pas de flux).

### 8.6 Client

- **Rust compilé en WASM**, réutilisant le crate `protocol`.
- Rendu : **`wgpu`** (WebGL2 / WebGPU). Un canvas 2D suffit pour démarrer.
- Un seul atlas, batching par couche → tourne sur un GPU intégré de 2012.
- **UI en HTML/CSS en overlay** par-dessus le canvas — bien plus simple et plus riche qu'une UI immediate-mode en WASM, et parfaitement adapté aux panneaux d'inspection denses (arbres généalogiques, graphiques, listes).

---

## 9. Plan de développement — 6 phases

> Chaque phase produit un artefact **visible et testable**. On ne passe pas à la suivante tant que les critères d'acceptation ne sont pas remplis.

---

### Phase 1 — LE SOL

**Rôle** : poser toute la fondation spatiale et prouver que le monde infini est viable. Aucun être vivant. C'est la phase la plus « géographie physique » et la moins « jeu » — mais tout le reste en dépend, et un monde mal généré condamne la simulation à être ennuyeuse.

**Objectifs**
- Workspace Rust en place (`core`, `worldgen`, `client`).
- Pipeline de génération complet : altitude → température → vent → humidité (avec ombre pluviométrique) → hydrologie → biomes → géologie.
- Chunking, génération à la demande, déchargement.
- Client WASM : affichage du tileset, caméra scrollable et zoomable, monde infini.
- Overlays de debug : altitude, température, humidité, biomes, gisements.

**Compétences Rust travaillées** : modules et workspace, ownership et emprunts, traits, itérateurs, `Result` et `?`, `rayon` pour paralléliser la génération, compilation WASM et interop `wasm-bindgen`.

**Critères d'acceptation**
- On scrolle indéfiniment dans un monde cohérent, sans couture visible entre chunks.
- Les déserts sont **derrière les montagnes**, pas au hasard.
- Les rivières coulent **vers le bas** et se rejoignent.
- Le cuivre et l'étain ne sont **pas** au même endroit.
- Même seed → monde strictement identique, bit pour bit.
- 60 FPS en scroll sur une machine modeste.

---

### Phase 2 — LA VIE

**Rôle** : introduire l'ECS et la boucle de simulation. Faire naître des créatures qui **subissent** le monde. Ici, on ne cherche pas encore l'intelligence : on cherche la **pression de sélection**. Si les agents meurent tous, le monde est trop dur ; s'ils survivent tous sans effort, il est trop mou. Cette phase est un **calibrage**.

**Objectifs**
- Intégration de `bevy_ecs`, boucle à tick fixe, RNG déterministe.
- Agents avec physiologie : faim, soif, fatigue, thermie, santé, mort.
- Utility AI **minimale** : manger, boire, dormir, se réchauffer, fuir.
- Écologie : biomasse végétale à croissance logistique, consommée par les agents.
- Faune : troupeaux d'herbivores, prédateurs, dynamique proie-prédateur.
- Pathfinding A* budgété.
- Rendu des agents et de la faune dans le client, temps réel.

**Compétences Rust travaillées** : ECS et pensée data-oriented, systèmes et ordonnancement, gestion d'état mutable partagé, structures de données spatiales.

**Critères d'acceptation**
- Une population lâchée sur la carte **survit un an** sans intervention, avec une mortalité non nulle mais non totale.
- Les agents vont boire quand ils ont soif, chassent quand ils ont faim, s'abritent quand il fait froid.
- La surchasse d'une zone provoque un **effondrement local** observable de la faune.
- La simulation est déterministe : deux exécutions avec la même seed donnent le même résultat au tick près.

---

### Phase 3 — LE NOMBRE

**Rôle** : passer d'une population qui *survit* à une population qui *persiste et se disperse*. C'est ici qu'apparaissent l'hérédité, l'apprentissage et — crucialement — la **mémoire spatiale partielle**, qui transforme l'exploration en véritable acquisition d'information plutôt qu'en simple révélation de brouillard.

**Objectifs**
- Reproduction : appariement, gestation, naissance, enfance (les enfants sont improductifs et coûteux — c'est ce qui rend le surplus nécessaire).
- **Hérédité des traits** : moyenne parentale + mutation gaussienne.
- **Compétences acquises** par la pratique (courbe log, plafond lié aux traits).
- **Mémoire spatiale individuelle** : chaque agent maintient sa propre carte connue.
- **Exploration** : le drive « comprendre » pousse les curieux à sortir du connu.
- **Transmission d'information** entre agents qui se rencontrent (fusion partielle de cartes mentales).
- Pyramide des âges, courbes démographiques dans le client.

**Compétences Rust travaillées** : gestion de graphes de relations (parenté), structures partagées `Arc`, sérialisation, algorithmes sur données creuses.

**Critères d'acceptation**
- Une population de 50 individus atteint 300 en 100 ans **sans intervention** et sans explosion malthusienne (l'écologie doit la borner).
- Les agents curieux explorent **plus loin** que les autres, mesurablement.
- Deux groupes qui se rencontrent échangent de l'information géographique.
- La carte mentale d'un agent est **strictement incluse** dans ce qu'il a pu percevoir.

---

### Phase 4 — LE CLAN

**Rôle** : faire émerger la structure sociale. C'est la phase la plus délicate en conception, parce qu'il faut résister à la tentation de scripter. **Le clan ne doit être qu'une conséquence détectée**, jamais une cause. C'est aussi la phase qui rend le jeu *lisible* : sans clans, on regarde des fourmis ; avec clans, on regarde une histoire.

**Objectifs**
- Graphe d'affinités et de rivalités entre individus.
- **Détection d'émergence de clan** : seuil sur cohésion sociale × co-résidence.
- Territoire : champ d'influence diffusé sur la grille.
- Stock commun, partage, normes de répartition.
- Chef : `max(oratoire × prestige)`, contestable.
- Structures : campement, huttes, foyer, palissade, greniers.
- Relations inter-clans : contact, échange, tension, conflit, guerre.
- Fission d'un clan trop gros ; disparition d'un clan qui s'effondre.
- **Client : panneau d'inspection du clan** (population, stocks, territoire, chef, relations).

**Compétences Rust travaillées** : algorithmes de graphes (détection de communautés), gestion de hiérarchies d'entités dans l'ECS, cycle de vie d'entités agrégées.

**Critères d'acceptation**
- Des clans se forment **sans qu'aucune règle ne dise « former un clan ici »**.
- Un clan trop nombreux **fissionne**.
- Un clan affamé **s'effondre**, et ses survivants rejoignent d'autres clans ou en fondent un nouveau.
- Deux clans voisins sur une ressource rare **entrent en tension** de manière observable.
- On peut cliquer sur un village et lire son état complet.

---

### Phase 5 — L'ÉTINCELLE

**Rôle** : **c'est ici que le jeu devient le jeu.** Tout ce qui précède n'était que le substrat. Cette phase introduit l'innovation, la transmission, l'oubli — et donc l'Histoire. C'est la phase la plus riche conceptuellement et celle qui mérite le plus d'itérations de tuning.

**Objectifs**
- **Arbre technologique en données** (`techs.ron`), pas en code — pour pouvoir itérer sans recompiler.
- Système d'**exposition** : l'agent enregistre ce qu'il a vu (feu, argile, cuivre, graminées…).
- Système de **pression** : famine, froid, guerre, surpopulation — mesurés, agrégés au clan.
- **Insight** : `P ∝ curiosité × compétence × exposition × pression`, évalué pour les agents oisifs.
- **Diffusion culturelle** : propagation par commerce, mariage, guerre, réfugiés, avec `p < 1`.
- **Oubli** : une tech portée par trop peu d'individus peut disparaître.
- **Commerce longue distance** : caravanes, routes émergentes (le bronze force la route de l'étain).
- Calcul de l'**Âge** d'un clan comme étiquette dérivée.
- **Client : panneau d'inspection complet de l'agent** — traits, compétences, tâche courante, **pile de motivations avec scores**, arbre généalogique navigable, savoirs, biographie.
- **Client : arbre technologique du clan**, avec les techs en incubation.
- **Overlays** : diffusion d'une technologie dans le temps, routes commerciales.

**Compétences Rust travaillées** : conception dirigée par les données (`serde` + RON), systèmes probabilistes, modélisation d'arbres de dépendances, tuning de systèmes couplés.

**Critères d'acceptation**
- Le feu est découvert par **au moins un clan sur trois** en 500 ans, sans intervention.
- Une chaîne complète feu → poterie → sédentarisation → agriculture est **observée** dans au moins une partie sur cinq.
- Le bronze **n'apparaît jamais** sans qu'une route commerciale cuivre↔étain existe.
- Un clan qui s'effondre **perd** des technologies, de manière visible dans la Chronique.
- **Deux parties avec des seeds différentes racontent des histoires différentes.** C'est le vrai test.
- On peut cliquer sur n'importe quel individu et comprendre *pourquoi* il fait ce qu'il fait.

---

### Phase 6 — LE DIEU

**Rôle** : transformer un simulateur qui tourne en local en un **monde persistant partagé**, et donner au joueur son rôle. C'est la phase la moins « simulation » et la plus « ingénierie logicielle » : async, réseau, persistance, déploiement. C'est aussi celle qui rend le projet démontrable à un tiers.

**Objectifs**
- **Serveur persistant** : `tokio` + `axum`, simulation en tâche de fond 24/7, indépendante des connexions.
- **Persistance** : snapshots binaires + journal d'événements SQLite. Redémarrage sans perte.
- **WebSocket** + **interest management** : abonnement par région, deltas à 5–10 Hz, compression RLE.
- Endpoint `inspect(entity_id)` en requête/réponse.
- **Interventions divines** : pluie, sécheresse, foudre, fertilité, maladie, révélation, signe. Toutes ambivalentes.
- **Foi** : ressource générée par les croyants, dépensée par les miracles.
- **Culte émergent** : les clans construisent leur théologie à partir des interventions observées ; celle-ci rétroagit sur leurs normes et donc sur leurs innovations.
- **La Chronique** : génération narrative continue à partir des événements de simulation. Consultable, filtrable par clan, par individu, par période.
- Déploiement : Docker, VPS, reverse proxy, TLS.

**Compétences Rust travaillées** : `async`/`await` et `tokio`, concurrence et `Arc<RwLock>`, protocoles réseau et sérialisation, architecture client-serveur, containerisation et déploiement.

**Critères d'acceptation**
- Le serveur tourne **une semaine d'affilée** sans fuite mémoire ni dérive.
- Un client peut se déconnecter, revenir 3 jours plus tard, et **lire dans la Chronique ce qui s'est passé**.
- Trois clients peuvent observer **des régions différentes** simultanément sans surcoût notable.
- Une foudre bien placée **peut** donner le feu à un clan — et **peut** aussi le tuer.
- Un clan développe une théologie **cohérente avec le comportement réel** de la divinité.
- Le tout tient dans **2 vCPU / 2 Go**.

---

## 10. Points ouverts à arbitrer

1. **Tick rate** : 7 min réelles par année de jeu est peut-être trop lent pour qu'une session de 20 minutes soit satisfaisante. Envisager 60 tps, ou un tick = 1 jour avec sous-résolution horaire uniquement pour les agents actifs.
2. **`bevy_ecs` vs `hecs`** : le premier est plus rapide à mettre en œuvre ; le second enseigne davantage. Arbitrage apprentissage / vélocité.
3. **Densité d'agents** : 5 000 agents sur un monde infini, c'est très épars. Faut-il concentrer la simulation dans une « zone de départ » et n'ouvrir le monde qu'à mesure que les populations se dispersent ?
4. **Portraits d'agents** : génération purement procédurale (composition de couches pixel) vs banque de sprites variés. La première est plus élégante et plus légère.
5. **Multi-divinités** : plusieurs joueurs = plusieurs dieux concurrents ? Cela ouvre un axe de jeu passionnant (guerres de religion), mais complexifie fortement le modèle de Foi et de culte. À garder en tête sans l'implémenter avant la phase 6.

---

## 11. Anti-patterns à refuser explicitement

- ❌ Scripter un comportement narratif (« quand la population atteint 20, fonder un village »).
- ❌ Un arbre technologique linéaire à points de recherche.
- ❌ Des agents omniscients (ils doivent avoir une connaissance partielle du monde).
- ❌ Des interventions divines univoquement bénéfiques.
- ❌ Optimiser avant d'avoir mesuré.
- ❌ Introduire une dépendance lourde pour un besoin de 50 lignes.
- ❌ Produire du code Rust sans expliquer les idiomes qu'il emploie.