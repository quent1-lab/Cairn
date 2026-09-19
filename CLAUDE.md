# Cairn — instructions pour Claude

Simulateur d'évolution culturelle et technologique **émergente** : monde infini procédural, humains simulés individuellement, serveur Rust persistant 24/7, client web WASM pixel-art, joueur-divinité (Foi, interventions ambivalentes), Chronique narrative.

**Référence obligatoire : [docs/BRIEF.md](docs/BRIEF.md)** — vision, systèmes, architecture cible, plan en 6 phases avec critères d'acceptation. À consulter avant toute décision de conception.

## Contexte de travail

- Projet d'**apprentissage Rust** + pièce de portfolio (étudiant ingénieur, fort en C/C++/embarqué, Rust en cours d'acquisition).
- Répondre **en français**. Expliquer les idiomes Rust nouveaux dans la réponse (pas en commentaires tutoriels dans le code — les commentaires du code documentent le domaine, pas le langage).

## Règles non négociables (BRIEF §0 et §11)

1. **Émergence stricte** : aucun comportement narratif scripté. Les « âges » sont des étiquettes dérivées, jamais des conditions de déblocage.
2. **Aucune dépendance sans justification** face à une implémentation maison de ~50 lignes.
3. Simple et lisible d'abord ; **optimiser seulement après mesure**.
4. **Déterminisme bit-à-bit** : RNG dérivé de la seed globale (`cairn-core`), jamais d'itération sur `HashMap` dans la simulation.
5. Chaque incrément produit quelque chose de **visible et testable**.

## Méthode — les règles payées cher

Chacune vient d'une erreur réelle, pas d'un principe. Elles priment sur l'intuition.

1. **Mesurer, jamais déduire.** Une formule d'état stationnaire appliquée à un système qui oscille donne un chiffre faux et convaincant. Si une grandeur compte, on l'instrumente à la source.
2. **Un banc doit dire dans quel régime il mesure.** Sept hypothèses ont été « réfutées » par des bancs qui testaient tous le même coin de l'espace des paramètres sans le dire. Le scénario réel vivait trente fois plus clairsemé que le test le plus clairsemé. Toute mesure affiche ses conditions.
3. **La cadence d'échantillonnage suit le phénomène, pas l'impatience.** Relever toutes les dix minutes un cycle dont la période se compte en années de jeu fait lire du bruit comme une tendance — quatre revirements en une heure.
4. **Un changement, une chose.** Une correction glissée dans un refactor rend les deux invalidables ensemble. On sépare, quitte à refaire la mesure.
5. **Deux compteurs indépendants qui doivent concorder.** Les bugs de ce projet ne crashent pas : ils produisent un monde *plausible* (des plaies qui ne tuent pas, une végétation qui ne repousse plus). Seule la confrontation de deux mesures les attrape.
6. **Un test qui ne casse pas avant le correctif ne prouve rien.** On écrit la reproduction d'abord, on la voit échouer, puis on corrige.

## État courant

**Phase 6 — partie serveur.** Fait : `crates/protocol` (types partagés, terrain non transmis car fonction pure de la seed). Reste : serveur détaché, WebSocket, client réseau, `inspect`, persistance, déploiement. **Suspendu** le temps du chantier de dérive.

**Chantier de dérive (en cours).** Le débit s'effondre sur les runs longs. Trois bugs trouvés et corrigés (génération de chunk à 216 tuiles par tuile lue ; repousse gelée sur toute terre broutée puis évincée ; `pack.last_kills` non remis à zéro), puis un profileur (feature `profile`) qui a mis fin à trois jours d'hypothèses. Le coût était l'**écologie** — que personne n'avait suspectée : `daily_regrowth` balayait les 4 096 tuiles de chaque chunk sale pour n'en faire pousser qu'une poignée. Balayage épars sur `Chunk::touched` (92ef903) : **×5,4 sur 600 jours, trajectoire bit-identique**.

**Le résultat qui oriente la suite** : la machine est 5,4× plus rapide et **la dérive n'a pas bougé d'un jour** — mêmes effectifs, même effondrement. L'écologie était un *coût fixe* ; la faune est une *croissance*. Elle pèse désormais **84,8 %** du temps de tick et reste le seul effectif non borné (+138 % de têtes entre le 3ᵉ et le 4ᵉ quart). Aucune optimisation de parcours ne rattrapera ça : il faut soit borner l'effectif par le modèle, soit rendre les requêtes de voisinage sous-quadratiques (la **grille spatiale** du §8.2, jamais écrite).

**Acquis négatif à ne pas retenter** : le LOD temporel ne se généralise **pas** à la faune. Six tentatives, six biais mesurés (+59 % à +953 % sur les prédateurs). Un couple proie-prédateur ne se grossit pas temporellement — son cycle de rencontre est plus court que toute fenêtre de réveil utile. Ce qui se grossit est ce qui n'a pas de partenaire couplé : la flore, **champ** et non acteur, au rattrapage exact par forme fermée.

**Dette identifiée, non livrée** : sous la pluie, une tuile intacte pousse au-dessus du baseline sans entrer dans `Chunk::touched` — elle échappe donc à l'instantané d'éviction et repart silencieusement au baseline. Bug **antérieur** au balayage épars ; test de reproduction écrit, en attente de son propre commit.

Historique complet, incrément par incrément : **[docs/JOURNAL.md](docs/JOURNAL.md)**.

## Architecture

Workspace cible (BRIEF §8) : `crates/{core, worldgen, sim, protocol, server, client}` — `protocol` partagé serveur↔client. Existent : `core`, `worldgen`, `sim`, `protocol`, `client`. Reste : `server`.

`protocol` = **ce que le serveur dit du monde, et ce que le joueur lui demande**. Il **dépend de `sim` et réutilise ses types** plutôt que de les recopier (arbitrage acté 2026-09-14) : c'est ce qui rend une désynchronisation serveur↔client structurellement impossible (BRIEF §8.1). Trois natures de données, et ce découpage porte tout le reste — **ce qui ne change jamais** (la seed : le terrain n'est donc *pas* transmis, le client le régénère, fonction pure) ; **ce qui bouge** (les entités, filtrées par la région abonnée) ; **ce qu'on demande** (le détail d'un être, §7.2, à la demande et jamais dans le flux). Règle de tri : entre dans le flux ce qui se **dessine** ou ce qui tient en quelques octets ; les panneaux qui *résument* le monde reçoivent des **agrégats pré-calculés** (pyramide des âges, morts par cause), pas une ligne par habitant.

`sim` = monde **chunké** (store de tuiles) : le pont entre worldgen (baseline pur, immuable) et l'état mutable à venir. `Tile` (16 o) matérialise le baseline + slots mutables (fertilité, biomasse) ; `Chunk` 64×64 généré à la demande (humidité échantillonnée aux 4 coins partagés → aucune couture) ; `World` = LRU BTreeMap, capacité bornée, éviction sûre car régénérable. Le store sert le **local** ; la vue monde lit le worldgen en échantillonnage grossier (mode carte). **Règle : worldgen reste pur ; l'évolution/mutation vit dans `sim`.**

## Échelle

**1 tuile = 2 m** (échelle humaine : une hutte = plusieurs tuiles, un agent sur une, un village se parcourt tuile à tuile). Source unique : `core::scale` (`TILE_METERS`, `km_to_tiles`, `tiles_to_km`). Tout le worldgen exprime ses tailles en km via ces helpers. Le monde est donc énorme en tuiles (continents ~3000 km ≈ 1,5 M tuiles) — sans coût car procédural/chunké. Le zoom monde↔village est un problème de **rendu (LOD)**, jamais de taille de tuile : proche = sprites/structures, loin = mode carte agrégé.

## Commandes

- `cargo check` / `cargo test` — vérification et tests.
- `cargo run --release -p cairn-sim --example life_demo -- [seed] [années] [agents]` — démo Phase 2 : population lâchée dans un foyer tempéré, rapport mensuel (pop, morts par cause, besoins moyens), verdict face au critère d'acceptation, carte PNG `out/life_<seed>.png` (centrée sur la médiane des survivants).
- `cargo run --release -p cairn-sim --example chronicle -- [seed] [années] [sample_days] [out.csv] [capacité] [agents]` — **banc d'analyse long terme** : foyer tempéré, journalise **tout** (73 colonnes, un échantillon/jour par défaut) dans un CSV — démographie (classes d'âge, sexes, naissances, morts par cause), physiologie (moyennes + pires cas), dérive génétique (6 traits moyens) et culturelle (compétences, savoirs), faune, clans (tailles, stock, formations/dissolutions), tensions, structures, dispersion/migration (boîte englobante, centroïde), perf. CSV vidé à chaque échantillon (une exécution interrompue reste analysable). Défauts : seed 42, 50 ans, /1 j, `out/chronicle_<seed>.csv`, 16384 chunks, 40 agents. ~15 tps → **50 ans ≈ 8 h** (à lancer en tâche de fond).
- `cargo run --release -p cairn-sim --example etincelle -- [seed] [années] [rapport/j] [capacité] [agents]` — **banc de validation de la Phase 5** : foyer **frais à vrais hivers** (`find_home_where` 2–8 °C — c'est le froid qui pousse au feu ; une scène tempérée repue n'inventerait rien), rapport **lisible** périodique (pop, clans, répartition des âges P/N/B, savoirs vivants) + **verdict** final (histoire technologique tech par tech : découvertes/oublis/perdu, âge le plus avancé, repères feu/expéditions/bronze face aux critères §9). Défauts : seed 42, 30 ans, rapport/360 j, 16384 chunks, 60 agents. La validation à 500 ans (feu ≥ 1 clan/3, chaîne complète, bronze⇒route) se lance hors ligne.
- `cargo run --release -p cairn-sim --example derive -- [seed] [jours] [troupeaux] [meutes] [agents] [capacité] [pas_troupeaux] [étalement_humains]` — **banc de dérive** (2026-09-15) : rend visible en minutes ce que le banc long met huit heures à montrer. Il ne mesure **pas** un débit absolu (la scène est dense par construction) mais des **tendances** : pour chaque effectif, la moyenne du 3ᵉ quart du run contre celle du 4ᵉ, verdict `DIVERGE` au-delà de +15 % — robuste au bruit saisonnier sans ajustement de courbe. Colonne **`regen/j`** (chunks régénérés par jour de jeu) : c'est elle qui départage « la simulation a plus de travail » de « le store rame », distinction indevinable sans ce chiffre. Le principe de la scène : le problème ne se déclenche qu'au-delà de la centaine de troupeaux, que la scène de référence met *quatorze années* à atteindre — on paie donc la densité en **condition initiale** plutôt qu'en heures de calcul (même statut que `find_home_where` choisissant où lâcher la population : aucune règle du cœur n'est touchée). **Deux pièges rencontrés en le construisant** : `find_home_where` appelle `nearest_spring` (81 chunks) sur chaque candidat — une bande de climat étroite le fait balayer des milliers de candidats, plus de dix minutes avant le premier tick ; et poser les troupeaux sur le pas de 1,5 km de `populate` les étale sur 18 km alors que 4096 chunks n'en couvrent que 8,2 — on mesure alors du **thrashing**, pas la faune. Sous la feature `chunk-stats`, il imprime en plus la **mesure M1** (taux d'utilisation des chunks). Défauts : seed 42, 240 jours, 150 troupeaux, 12 meutes, 20 agents, 4096 chunks, pas de 400 m, humains groupés.
- `cargo run --release -p cairn-sim --example diagnose -- [seed] [années] [agents]` — **banc de diagnostic social** (Phase 6) : il ne corrige rien, il **mesure**, et il a déjà réfuté quatre hypothèses. Sept sections — pourquoi un groupe n'est *pas* un clan (les trois portes comptées séparément, avec la valeur mesurée en regard de son seuil) ; durée de vie des clans (fusions vs vraies extinctions) ; santé des liens sociaux année par année ; porteurs **minimum** de chaque tech (la vraie mesure du §5.4 « détenu par trop peu de porteurs ») ; redécouvertes ; recouvrement jour-à-jour du plus gros clan (départage « la structure oscille » de « l'identité saute ») ; distance entre peuples au moment d'une fusion vs entre peuples qui coexistent. **Leçon de méthode** : mesurer sur 5 ans a produit une conclusion *fausse* (voir Phase 6 ci-dessous) — la population n'avait pas atteint la taille où le problème se déclenche. Toujours 15 ans minimum. ~2 h 30 pour 15 ans sur l'ancienne machine, **bien plus ici** (2 vCPU) ⇒ lancer **détaché** (`nohup … &` ou `setsid`, pas une tâche de fond de session, qui meurt avec elle).
- Client : `cargo run --release -p cairn-client --example preview -- <couche> <km>` rend le canvas en PNG (vérif sans navigateur). Bundle web : `crates/client/build.sh` (nécessite `wasm-bindgen-cli` — voir note d'env).
- `cargo run --release -p cairn-client --example client_render -- [seed] [ticks] [scale] [log_every_days] [overlays]` — **vérification native du rendu et de la trajectoire, sans navigateur** : PNG dans `out/`, log quotidien optionnel, et en fin de course le **tps** (référence : `docs/perf-baseline.txt`), les **morts par cause** et la **Chronique** rédigée. C'est le contrôle le moins cher du projet : deux compteurs indépendants (morts enregistrées vs récit) qui doivent concorder — c'est ce qui a révélé que les plaies mortelles ne tuaient pas.
- `cargo run --release -p cairn-worldgen --example map_png -- <seed> [tuiles/pixel]` — rend une image PNG par couche du pipeline dans `out/` (`_alt`, `_temp`, `_hum`, `_bio`) + % de terres émergées ; 2ᵉ arg = zoom (petit = gros plan). Toujours en `--release`.
- `cargo run --release -p cairn-worldgen --example analyze -- [nb_seeds] [onde_continent] [sea_bias]` — vérifs statistiques : connexité des masses terrestres + preuve du rain shadow (humidité par barrière au vent).
- **git** : les commits sont gérés par Claude (demande explicite de l'utilisateur), messages en français, style conventional commits.

## Environnement

**Le développement a déménagé sur le serveur de déploiement lui-même (2026-09-14).** Ubuntu 24.04, `x86_64-unknown-linux-gnu`, Rust 1.98.1, édition 2024. `gcc`, `git`, `curl`, `docker` présents ; `sqlite3` (CLI) et `nginx` absents.

- **La machine de dev *est* la cible du BRIEF §8.3** : 2 vCPU, 2,9 Go de RAM, 37 Go libres — « 2 vCPU / 2 Go, un VPS à 5 €/mois ». Le critère « le tout tient dans 2 vCPU / 2 Go » ne se simulera pas : il se vérifiera en tournant. Corollaire de méthode : **une compilation et une suite de tests coûtent bien plus cher ici** (workspace ≈ 2 min en debug, `cairn-sim` ≈ 3 min 20 en release) — grouper les vérifications, préférer les tests ciblés.
- **Le portage Linux n'a demandé aucune modification** : `cargo check --workspace --all-targets` et le test de déterminisme bit-à-bit sont passés du premier coup sur les 16 000 lignes de `sim` (vérifié 2026-09-14).
- **Le déterminisme est local à la machine, pas portable.** Le test compare deux exécutions *ici*. `exp`, `sin`, `powf` passent par la libm de la plateforme : rien ne garantit qu'un monde Windows et un monde Linux issus de la même seed soient identiques. **Conséquence pour la Phase 6 : le « replay depuis la seed » du §8.2 ne peut pas servir de stratégie de persistance de secours** — le snapshot binaire est obligatoire, pas optionnel.
- Cible `wasm32-unknown-unknown` et `wasm-bindgen-cli` **à réinstaller** sur cette machine (`rustup target add wasm32-unknown-unknown` puis `cargo install wasm-bindgen-cli --version 0.2.126`) — nécessaires seulement à l'incrément « client réseau ».
- `.cargo/config.toml` garde le linker MinGW de l'ancienne machine : la section est ciblée `[target.x86_64-pc-windows-gnu]`, donc **inerte sur Linux**. Conservée pour ne pas casser un retour sur Windows.
- **Dette d'environnement restante** : `.claude/settings.local.json` est une liste de ~200 permissions Windows (chemins `C:\Users\<utilisateur>`, PowerShell, Edge headless) — inutilisable ici, à reconstruire au fil de l'eau. Le CLI headless de capture navigateur (Edge) n'existe plus : la vérification du client se fera par `client_render` (natif, PNG) jusqu'à ce qu'un navigateur headless soit installé.

