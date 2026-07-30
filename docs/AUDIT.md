# Grille d'audit de Cairn

Vérification **reproductible** de la cohérence du code face au [BRIEF](BRIEF.md) :
émergence stricte, respect des règles, couverture des critères, sobriété des
ressources. À rejouer à chaque incrément.

Deux étages, deux natures :

1. **Le socle mécanique** — `scripts/verify.sh`. Bon marché, à lancer souvent.
   Il ne *juge* pas : il **compile, teste, mesure et pointe des suspects**.
2. **La passe de jugement** — un humain (ou Claude) lit le code avec la grille
   ci-dessous, *confirme* les suspects, et remplit le tableau de couverture.
   Règle d'or du brief : **on ne juge la perf qu'après l'avoir mesurée** (§0.3).

Un suspect remonté par un grep n'est **pas** une violation : c'est un point à
regarder. Beaucoup sont légitimes (une taille lue pour une *probabilité* n'est
pas une taille lue pour un *déclenchement*).

---

## Axe A — Émergence stricte / non-scripté (BRIEF §0.4, §1, §11)

> « Aucun comportement narratif scripté. Tout découle de règles locales. Les
> âges sont des étiquettes dérivées, jamais des conditions de déblocage. »

**Ce qu'on cherche à réfuter :**

- [ ] Un comportement déclenché par une **taille** (`if members.len() > N { … }`),
  un **compteur** ou un **âge** de clan.
  - Signal : `rg '\.len\(\)\s*(>=|>|==)\s*[0-9A-Z]' crates/*/src`
  - Jugement : la valeur alimente-t-elle une **décision** (interdit) ou une
    **mesure/probabilité** (permis) ? Ex. `crowding = effectif / SCALE` est une
    mesure ; `if effectif > 8 { fission }` serait une violation.
- [ ] Un `Age` (ou une étiquette dérivée) lu dans une **condition** de logique.
  - Signal : `rg 'Age::(Paleolithic|Neolithic|BronzeAge)' crates/sim/src`
  - Jugement : l'âge ne doit apparaître que dans `label()`/affichage et
    `age_of()` (dérivation), **jamais** pour gater un comportement.
- [ ] Un événement narratif « en dur » (une date, un tick, un lieu qui
  déclenche un scénario).
  - Signal : `rg 'if .*tick\s*==|== *[0-9]{3,}' crates/sim/src` (dates/ticks en
    dur) — un salt ou une période cyclique n'en est pas un.
- [ ] Une tech « débloquée » au lieu d'être **découverte** par insight
  (probabilité × exposition × pression). Vérifier que `tech::insight` reste la
  seule voie (+ `diffuse` pour la transmission), sans court-circuit.

**Mécaniques à deux faces — vérifier la symétrie** (une leçon récurrente du
projet) : famine ↔ satiété, froid ↔ chaleur, immigration de gibier ↔ de
prédateurs. Un moteur qui ne modélise qu'un côté dérive (ex. la faune qui
s'emballe faute d'immigration de prédateurs).

**Une mécanique dont l'effet n'est jamais observé sur une scène réelle est
suspecte, même avec des tests verts.** Cherché : un système dont les tests
n'exercent que la *cause* (les plaies infligées) sans jamais vérifier l'*effet*
attendu (la mort). Cas réel : le combat mettait `health = 0`, que la
régénération de `Physiology::drift` annulait dans le même tick — aucune plaie
mortelle ne tuait, et les trois tests de combat passaient. Le contrôle qui
l'aurait attrapé : comparer les **morts par cause** (`client_render` les
imprime) au récit de la Chronique. Deux compteurs indépendants qui doivent
concorder valent mieux qu'un test.

---

## Axe B — Règles non-négociables (BRIEF §0.2-3, §8.2)

- [ ] **Déterminisme bit-à-bit.**
  - Aucune `HashMap`/`HashSet` **dans la simulation** (`crates/sim`,
    `crates/worldgen`, `crates/core`) : ordre d'itération non déterministe.
    - Signal : `rg 'HashMap|HashSet' crates/sim/src crates/core/src crates/worldgen/src`
    - (Le **client** a le droit — son état de rendu n'entre pas dans le
      déterminisme.)
  - RNG toujours **dérivé de la seed** (`Pcg32` / `WorldSeed::derive` + salt),
    jamais d'entropie système.
    - Signal : `rg 'thread_rng|rand::random|from_entropy|SmallRng|StdRng' crates/`
  - Test de garde : `cargo test -p cairn-sim --lib bit_a_bit` doit passer.
  - Chaque nouveau flux RNG a un **salt unique** (plage 1000+, voir
    `sim::salt`) — deux consommateurs partageant un salt seraient corrélés.
- [ ] **Deps justifiées** (§0.3, §11) : toute dépendance doit battre une
  implémentation maison de ~50 lignes. Inventaire attendu : `core` = 0 dep ;
  `worldgen` = `noise` (tables de gradients Simplex, subtiles) ; `sim` = `hecs`,
  `serde`, `ron` (arbre tech data-driven) ; `client` = plomberie WASM.
  - Signal : `git diff` sur les `Cargo.toml` ; toute nouvelle ligne se justifie.
- [ ] **Simple d'abord** (§0.2) : pas d'optimisation spéculative non mesurée
  (voir Axe D — on n'optimise **qu'après** mesure).

---

## Axe C — Couverture du brief / points non réalisés (§9, phases)

À chaque passe, remplir l'état : ✅ fait+testé · 🟡 partiel/différé · ⬜ non fait.
Un différé **assumé** (noté dans `CLAUDE.md`) n'est pas un manque — c'est une
dette tracée.

| Phase | Critère d'acceptation (§9) | État | Preuve (test / banc) |
|---|---|---|---|
| 1 | monde cohérent sans couture ; déserts derrière montagnes ; rivières descendantes ; Cu≠Sn ; seed→identique ; 60 FPS | | `worldgen` tests, `example analyze` |
| 2 | survie 1 an (mortalité partielle) ; boit/chasse/s'abrite ; surchasse→effondrement local ; déterminisme | | `example calibrate`, tests sim |
| 3 | 50→300 en 100 ans sans explosion ; curieux explorent plus loin ; échange d'info ; carte mentale ⊆ perçu | | banc démographique long, tests memory |
| 4 | clans sans règle « former un clan » ; fission ; effondrement ; tension observable ; inspection clan | | tests social, panneau client |
| 5 | feu ≥ 1 clan/3 en 500 ans ; chaîne feu→…→agriculture 1/5 ; bronze⇒route ; oubli visible ; seeds→histoires ≠ ; « pourquoi » cliquable | | `example etincelle`, run 500 ans |
| 6 | serveur 1 semaine sans fuite ; reconnexion+Chronique ; 3 régions ; foudre ambivalente ; théologie cohérente ; 2 vCPU/2 Go | 🟡 | Chronique ✅ (`sim::chronicle`, panneau client) ; serveur/réseau/persistance **reportés** (choix utilisateur) ; foudre + Foi + théologie ⬜ |

**Écarts d'architecture assumés à retracer** (pas des manques — des choix) :
- `Tile::structure` / `Tile::claim` (§2.3) **non stockés dans la tuile** :
  registres clairsemés côté `Sim` + `claim_at` pur (raison d'échelle : 1 tuile
  = 2 m, un territoire couvre des millions de tuiles).
- `bevy_ecs` → **`hecs`** (arbitrage §8.2/§10.2 acté : ordonnancement écrit à la
  main = déterminisme explicite).
- `protocol` / `server` : crates non créés (Phase 6).

**Reste explicitement différé** (chercher les notes « reporté » de `CLAUDE.md`) :
combat/guerre comme *résolution* d'une tension ; commerce/échange inter-clans
autre que le troc de matières ; agriculture ; inventaire pondéré concret (§3.1) ;
normes de clan (exogamie, tabous, rites) ; compétences dédiées (taille de
pierre…). Chacun doit rester une **dette tracée**, pas un oubli.

---

## Axe D — Optimisation / ressources (BRIEF §0.2, §8.2-3, §11)

> Cible matérielle : **2 vCPU / 2 Go** (§8.3). Règle : **mesurer d'abord**, ne
> jamais optimiser à l'aveugle (§11).

- [ ] **Débit (tps)** mesuré et comparé à une **référence versionnée**
  (`docs/perf-baseline.txt`). Une chute nette entre deux passes = régression à
  investiguer.
  - Mesure : `example chronicle` / `client_render` impriment le tps.
- [ ] **RAM** : capacité de chunks × ~1 Mo/chunk vs budget 2 Go (§8.3). Le LRU
  borne l'empreinte ; vérifier que la capacité par défaut reste raisonnable.
- [ ] **Goulots connus** (à re-mesurer, pas à supposer) :
  - génération de chunk (bruit fBm du worldgen) — le coût dominant mesuré ;
  - churn LRU quand la population se disperse (visite de nombreux chunks) ;
  - scan de fourrage (~289 tuiles/délibération, `best_forage`) ;
  - allocations par tick (`herd_views`/`human_views`/`clan_views` reconstruits).
- [ ] **Lints de perf** : `cargo clippy --workspace -- -D warnings` propre
  (clone superflu, allocation en boucle, `collect` inutile…).
- [ ] **LOD temporel** (§8.2) respecté : un chunk sans agent n'est pas simulé
  tick par tick (repousse en forme fermée). Vérifier qu'aucun système ne balaie
  tous les chunks résidents chaque tick sans borne.

Toute optimisation proposée doit citer **le chiffre avant/après**. Sans mesure,
on ne touche pas.

---

## Modèle de verdict (à produire à chaque passe)

```
### Audit du <date> — commit <hash>

A. Émergence      : ✅ / ⚠️  <violations confirmées, ou "aucune">
B. Règles         : ✅ / ⚠️  <déterminisme, deps, simple-first>
C. Couverture     : <tableau §9 rempli> — non réalisé priorisé : <liste>
D. Optimisation   : tps <X> (réf <Y>) ; goulot <…> ; wins mesurés : <…>

Actions proposées (priorisées) :
1. …
```
