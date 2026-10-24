# SRV — le serveur

## SRV-1 — voir le monde dans le navigateur (ouvert le 2026-10-24)

**Lecture.** Écart au BRIEF §8 : la simulation tourne dans l'onglet (client Phase 2) ;
`protocol` porte déjà `Hello`/`WorldSnapshot`, mais aucun processus ne les émet.

**Ce qui est fait (mode léger, sans affirmation chiffrée sur le monde).**
- Fil de simulation propriétaire du `Sim` (aucun verrou sur la simulation), pas fixe visé
  20 ticks/s, publication à 5 Hz ; un pas trop long ralentit le monde, ne saute aucun tick.
- Scène = celle du banc d'acceptation (seed, tempéré/froid, jour 135, 16 384 chunks).
- Client `?live=1` : terrain régénéré depuis la seed **avec la configuration par défaut**
  (celle de `Sim::new`), alors que le mode local garde l'humidité rapide — sinon on peindrait
  un autre monde sous les pas des humains du serveur.
- Abonnement : tout le monde au premier instantané (pour trouver le peuple), puis l'écran
  plus 25 % de marge ; réabonnement quand l'écran sort de la fenêtre ou qu'elle devient
  trop large.

**Prédictions (écrites avant la première mesure), scène tempérée seed 42, début de run :**
- cadence tenue : 20 ticks/s (pas < 50 ms) avec 40 humains ; réfuté si < 15 ticks/s.
- publication < 2 ms par abonné ; réfuté si > 10 ms (alors `from_sim` coûte trop).
- image de 2 à 8 Ko pour la fenêtre centrée sur le peuple (40 humains, ~50 groupes de faune) ;
  réfuté si > 30 Ko.
- mémoire du processus ≈ celle du banc (~1 Go à terme, store de 16 384 chunks).

**Non couvert par SRV-1** : Chronique, inspection, gestes divins (SRV-1b) ; persistance
(SRV-2) ; le coût du terrain en configuration complète dans le wasm (à mesurer à l'œil au
premier affichage).

**Mesure** (2026-10-24, binaire de l'arbre de travail sur 93af7c5, seed 42 tempérée, départ
jour 135, tick 3240 → 4384 ; **régime : le témoin `acceptation` tournait sur l'autre cœur**) :

| | prédit | mesuré |
|---|---|---|
| cadence | 20 ticks/s | 20,0 (un observateur) ; 18,1 avec trois sondes Node en plus sur 2 vCPU |
| pas | < 50 ms | 23-24 ms (40 humains) |
| publication | < 2 ms / abonné | 0,1 ms (1 abonné), 0,8 ms (3-4 abonnés) |
| image (monde entier) | 2-8 Ko | 1,5-1,6 Ko |
| images reçues | 5 Hz | 5,1 Hz seul ; 4,4-4,5 Hz à trois |
| mémoire | ~1 Go à terme | 483 Mo après 1 100 ticks |

Toutes tenues. Trois observateurs simultanés servis (critère BRIEF de la phase 6, ici sur
la même région : la sonde s'abonne au monde entier) ; le test `chaque_observateur_recoit_sa_fenetre`
vérifie les régions différentes (rouge quand on neutralise le filtre : 3 humains reçus au
lieu de 1). Le repli à 4,5 Hz et 18 ticks/s à trois sondes : compté avec la concurrence de
quatre processus sur deux cœurs, non isolé.

**Ce que j'ai pu manquer.**
- Le rendu n'a été vu par **aucun navigateur** : pas de navigateur sur la machine. Le wasm
  compile, la sonde Node valide le fil ; le dessin, le recadrage et le réabonnement restent
  à voir à l'œil.
- Le coût du terrain en configuration complète dans le wasm (humidité non rapide) : peut
  rendre le premier affichage et chaque pan lents.
- L'horizon : 1 100 ticks. La cadence chutera quand la population et la faune croîtront
  (PERF : ~10 ticks/s à 170 humains) ; une semaine sans fuite (critère BRIEF) n'est pas
  mesurée.
- `scene.rs` recopie la scène du banc : deux copies qui peuvent diverger.
