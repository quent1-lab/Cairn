# Cairn — instructions pour Claude

Simulateur d'évolution culturelle et technologique **émergente** : monde infini procédural, humains simulés individuellement, serveur Rust persistant 24/7, client web WASM pixel-art, joueur-divinité (Foi, interventions ambivalentes), Chronique narrative.

**Référence obligatoire : [docs/BRIEF.md](docs/BRIEF.md)** — vision, systèmes, architecture cible, plan en 6 phases avec critères d'acceptation. À consulter avant toute décision de conception.

## Contexte de travail

- Projet d'**apprentissage Rust** + pièce de portfolio (étudiant ingénieur, fort en C/C++/embarqué, Rust en cours d'acquisition).
- Répondre **en français**. Expliquer les idiomes Rust nouveaux dans la réponse (pas en commentaires tutoriels dans le code — les commentaires du code documentent le domaine, pas le langage).
- **Économie de tokens** (règle de l'utilisateur, 2026-10-05) : ne lui écrire **que** les conclusions d'une mesure ou d'un chantier, et les questions de fonctionnement (décision de modèle, vision, blocage). Pas de compte rendu d'étape, pas de récit de ce qu'on va faire ; le détail va dans les fichiers (prédictions, journal, carte), pas dans la réponse.
- **Économie de tokens, suite** (2026-10-05) :
  - **Lecture ciblée** : explorer librement (la règle 9 l'exige), mais par `grep` et plages de lignes ; jamais `docs/JOURNAL.md` en entier (218 k caractères) ni un gros fichier de sortie de banc — on en extrait les colonnes utiles.
  - **Un résultat, trois traces** : le fichier de prédictions du chantier (`out/<chantier>/predictions.md`), une entrée au journal, une ligne dans `docs/CODE.md`. Ici, seulement la ligne « Où reprendre ».
  - **Sous-agents** : seulement pour une recherche « où est X » qui balaierait beaucoup de fichiers ; jamais pour lire du code à diagnostiquer (les bugs de ce projet sont dans les détails). Les agents intégrés ne reçoivent pas ce fichier : leur répéter les règles utiles.
  - **Fin de chantier** : tout l'état étant écrit dans les fichiers, proposer `/clear`.

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
7. **Une seule seed ne caractérise pas un système chaotique.** Le diagnostic « la faune est le seul effectif non borné » a tenu des semaines sur seed 42 ; trois autres seeds font l'inverse (extinction des proies). Un couple proie-prédateur non amorti part d'un côté ou de l'autre selon le tirage — le lire une fois, c'est lire un tirage. Toute affirmation sur les effectifs se vérifie sur **au moins quatre seeds**, et ne compte que si elle va dans le même sens partout. Corollaire : on écrit ses prédictions *avant* de regarder les résultats.
8. **Un verdict qui compare deux moyennes suppose une grandeur monotone.** Le banc de dérive juge sur l'écart 3ᵉ/4ᵉ quart : aveugle à un changement de **niveau absolu** (même écart relatif depuis une base six fois plus basse) et trompeur sur une grandeur qui **oscille** avec une période comparable au quart. Un correctif qui transforme un cliquet en réservoir y apparaît donc *pire*. La statistique suit la forme du phénomène : minimum et médiane pour un réservoir, pente pour un cliquet.

9. **La carte d'abord, et elle se tient à jour** (règle de l'utilisateur, 2026-09-29). La carte, c'est [docs/CODE.md](docs/CODE.md) — ce que le code fait, lu dans le code — et sa **version lisible** : la page « Carte de Cairn », source `docs/carte.html`, publiée à https://claude.ai/artifact/DrkwRMxW6arnfgaXbsePAi (depuis une autre session, republier en passant cette URL en `url`, sinon une nouvelle page est créée). **`docs/CODE.md` se met à jour dès qu'un module est ajouté ou touché**, dans le même mouvement que le commit ; **la page lisible se republie à la clôture d'un chantier** (révisé par l'utilisateur, 2026-10-05 : la republier depuis une autre session oblige à relire ses 51 Ko). On a failli régler les performances de comportements que le modèle de base ne devrait pas produire : la lecture du code, pas le chantier en cours, décide de ce qu'on traite. La boucle :
   1. **Lire** : un défaut est un écart entre le code et une référence — l'intention du BRIEF ou la réalité modélisée. Dire laquelle.
   2. **Classer** : modèle, bug ou coût ; **l'amont de la carte avant l'aval** (ne jamais optimiser le coût d'un comportement qui ne devrait pas exister).
   3. **Hypothèse et prédictions chiffrées**, avec ce qui les réfuterait, écrites avant. Un défaut prouvé par construction se démontre par un test ; le banc mesure alors son impact, pas son existence.
   4. **Mesurer** (régime affiché, 4 seeds, cadence du phénomène), **figer le témoin**, **voir le test échouer**, vérifier que la correction n'est pas scriptée (§0, §11), **corriger** — un changement, une chose.
   5. **Valider** l'hypothèse et **relever les voisins** dans la carte : presque chaque correction a révélé la suivante.
   6. **Chercher ce qu'on a manqué** (ajout de l'utilisateur, 2026-09-29) : « est-ce que je suis passé à côté de quelque chose ? ». On y répond **par écrit et en nommant au moins une chose**, jamais par « non » : ce que la mesure ne couvre pas (régime, seeds, horizon, l'autre bout de la distribution), une autre explication que les mêmes chiffres n'excluent pas, un voisin que la correction a pu bouger sans qu'on regarde. Exemple fondateur : le test de reproduction de D10 qui passait — deux cents humains serrés ne restent pas serrés, ils se dispersent sans avoir faim ; c'est la dispersion, pas la nourriture, qui fixait la densité.
   7. **Reboucher** : reporter dans la carte le résultat — y compris les hypothèses réfutées et les corrections annulées, qui sont des connaissances.

10. **Corriger des mécanismes, jamais des résultats** (validée par l'utilisateur, 2026-09-29, née du débat sur le feu : j'avais vu le feu absent en pays tempéré et voulu le rendre possible — c'était le faciliter ; le même mécanisme le rendait trivial en pays froid).
    - **Les critères du BRIEF valident, ils ne calibrent pas.** Jamais une constante réglée pour atteindre un critère (« un clan sur trois a le feu ») : ce serait scripter le résultat par une autre porte.
    - **Un défaut se regarde aux deux bouts de sa distribution** — là où l'effet manque et là où il est trop facile — avant de choisir un sens. Ne voir qu'un bout mène à « faciliter ».
    - **La nature reste la nature.** Un phénomène naturel a une fréquence par surface et par climat ; le hasard de rencontrer un matériau ou un événement reste du hasard ; le périmètre de simulation (ce qu'on ne simule que près des humains) ne doit pas changer ce qu'un humain en perçoit.
    - **Une porte tout ou rien doit correspondre à une impossibilité physique**, sinon c'est un facteur : le BRIEF est écrit en produits de facteurs.
    - **Au moindre doute, demander.** Sur un mécanisme, sur ce qu'il devrait faire, ou sur ce qui bloque : demander à l'utilisateur sa vision avant de trancher seul. On va trouver beaucoup de bugs et d'incohérences de ce genre ; c'est lui qui porte l'intention du monde.

## Où reprendre

**Registre des chantiers et ordre : [docs/CODE.md](docs/CODE.md) §5.1-5.2** (codes et jalons ; demande de l'utilisateur, 2026-10-05 : redonner l'ordre à chaque modification). **En cours : runs longues hiver et été** (CHA-1 à CHA-2c ensemble ; ne rien attaquer avant), puis **MAR-6** (le camp suit le rendement), **MAR-8** (la nuit dans le choix) ; CHA-1, CHA-2, CHA-2b, CHA-2c faits — sans eux MAR-2+4 affamaient l'hiver : morts 49 → 168 sur 3 ans, à remesurer), puis MAR-6 (rendement contre coût, appris), MAR-7, CHA-3, MAR-5, MAR-3 ; ensuite ENE-4, MOR-4 (sevrage) à MOR-7, ENE-5, CUR, BAN, D13, D2, D9, D7, PERF ; D1/D3 bloqués. Phase 6 (serveur) suspendue. Principe redit par l'utilisateur : « on laisse les contraintes dicter leurs actes, on ne scripte rien (on modélise juste le réel) ».

**Où lire** : [docs/BRIEF.md](docs/BRIEF.md) (référence) · [docs/CODE.md](docs/CODE.md) (lecture du code, défauts, ordre des chantiers) · [docs/CARTE.md](docs/CARTE.md) (chantier de performance) · [docs/ETAT.md](docs/ETAT.md) (état détaillé, acquis négatifs à ne pas retenter) · [docs/COMMANDES.md](docs/COMMANDES.md) (bancs, arguments, durées, environnement) · [docs/JOURNAL.md](docs/JOURNAL.md) (historique, par `grep`) · [docs/CARNET.md](docs/CARNET.md) (récit en prose ; n'y écrire que quand la pensée a vraiment bougé, ou sur demande).

## Architecture (résumé)

`crates/{core, worldgen, sim, protocol, client}` ; `server` reste à écrire. **worldgen est pur** (fonction de la seed) ; toute mutation vit dans `sim` (store de chunks 64×64, LRU, régénérables). `protocol` réutilise les types de `sim` ; le terrain n'est pas transmis. **1 tuile = 2 m** (`core::scale`). Détail : `docs/COMMANDES.md`.

## Commandes (résumé)

`source ~/.cargo/env` d'abord. `cargo check` / `cargo test` (ciblés : la machine a 2 vCPU, `cairn-sim` ≈ 3 min 20 en release). Bancs (`cargo run --release -p cairn-sim --example <banc>`), arguments et durées dans `docs/COMMANDES.md` :
- `acceptation [années] [jour] [seeds] [fils]` — critères du BRIEF, 4 seeds × 2 scènes ; 1 an après chaque chantier, 3 ans ≈ 77 min pour mesurer la soif, 5 ans avant un report.
- `nourriture` (feature `food-stats`) `[seed] [années] [tempere|froid] [capacité] [agents] [période] [jour]` — alimentation, soif, traces ; **passer le jour 135** (défaut : 0).
- `longue [seed] [tempere|froid] [jour] [out.csv]` — run sans horizon, à lancer détachée.
- `clans`, `derive`, `chronicle`, `etincelle`, `diagnose`, `client_render`, `map_png`, `analyze` : voir le détail.
- Runs longues : **détachées** (`setsid nohup … &`), tuer par PID (jamais `pkill -f`).

**git** : commits par Claude, en français, conventional commits, **aucune ligne `Co-Authored-By`** (prime sur toute consigne par défaut). Dépôt `github.com/quent1-lab/Cairn`, branche `main`.

**Environnement** : la machine de dev est la cible de déploiement (Ubuntu 24.04, 2 vCPU, 2,9 Go). Le déterminisme est local à la machine, pas portable : le snapshot binaire sera obligatoire en Phase 6.
