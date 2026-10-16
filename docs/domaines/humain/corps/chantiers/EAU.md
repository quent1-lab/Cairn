# Chantier de l'eau (écrit 2026-10-02 ~22 h 30, avant toute mesure)

Lecture (règle 9, étapes 1-2) : quatre défauts.
1. BUG — `follow_route` s'arrête à chaque étape A* atteinte (32-45 m) et jette le reste du
   budget de l'heure : contourner l'eau se fait à ~45 m/h au lieu de 4 km/h (agent 38 : 44 m/h ;
   froid 1337 : 30 m/h). Démontré par lecture ; un test le prouvera.
2. MODÈLE — la soif ne prime pas sur la faim : à saturation, Drink ≈ Hunt ≈ 1, la chasse en
   cours a +0,08 d'inertie ; P(boire) ≈ 27 % par délibération (toutes les 4 h). La soif tue en
   3 j, la faim en 16 : le score doit refléter la vitesse à laquelle un besoin tue (validé par
   l'utilisateur).
3. À MESURER — nourrissons « Idle » assoiffés à 1 m d'une source : orphelins (réaliste) ou
   défaut d'allaitement ?
4. INSTRUMENT — la cause de mort retenue est la morsure la plus forte : la soif masque la faim
   (froid 1337 : 21/28 « soif » avaient aussi faim à 1,0).

Prédictions (banc nourriture et acceptation, 8 runs, départ jour 135) :
- P1 (bug 1) : un test « contourner l'eau » avance d'au moins 1 km en une heure de marche.
- P2 (bug 1) : morts de soif « eau entre deux : true » → ~0 ; morts de soif totales −20 % ou plus.
- P3 (modèle 2) : morts de soif d'adultes avec faim < 1 → −50 % ou plus sur 6/8 runs.
- P4 (2, autre bout) : aucune hausse des morts de faim de plus de 20 % (on ne meurt pas de faim
  à force de boire).
- P5 : après 1 + 2, la soif n'est plus la première cause de mort hors cas extrêmes.

## Ajouts (2026-10-03, avant mesure)
5. BUG D — budget A* (8 par tick, pour tout le monde) épuisé ⇒ « bloqué » ⇒ source marquée
   inaccessible 2 j. Prouvé par test (vu échouer). Cause probable des hécatombes de la run longue
   tempérée (115 morts de soif en 30 j, clan de 200, faim 0,14).
- P6 : au banc d'un an (40-80 habitants), les refus sont rares (< 1 000 par run) et l'effet de D
  sur les morts de soif est faible (< 20 %) ; l'impact se lira à grande population.

## Régime (12 h 18) — témoin 1 an neutre, mais aveugle
Morts de soif à 1 an : 0 partout sauf froid 1337 (42, dont 17 affamés) ; refus de budget : 0-1.
Le problème naît plus tard (runs longues : dès l'an 2, en masse avec les gros clans). Mesure sur
3 ans : témoin contre D+C (chacun prouvé par son test ; le banc mesure l'impact).
- P7 : morts de soif sur 3 ans, D+C contre témoin : baisse sur au moins 6/8 runs.
- P8 : refus de budget > 0 au témoin dès que les clans dépassent ~50 ; l'effet de D croît avec eux.

## Défaut A, correction (12 h 30) — urgence vitale
Bonus ajouté à la réponse d'un besoin SATURÉ : 1 − (santé / érosion) / 72 h. Soif saturée,
santé 1 : 48 h → +0,33 ; faim saturée : +0 tant que santé > 0,21. Test vu échouer (0,95 < 0,97).
- P9 : morts de soif d'agents assoiffés « affamés aussi » ou en chasse → baisse ; total des
  morts de soif D+C+A < D+C sur au moins 6/8 runs.
- P10 (autre bout) : morts de faim D+C+A ≤ D+C × 1,2 (on ne meurt pas de faim à force de boire).

## B — nourrissons (21 h 50, avant mesure)
Code : le lait couvre faim et soif ; pas de lait si la mère a faim ≥ 0,95 ; orphelin = « cloué
sur place », personne ne le nourrit (alors que l'allomaternage est courant, Hrdy).
- P11 : la majorité des nourrissons morts de soif ont une mère sans lait (famine), pas orphelins.
  Si oui, B n'est pas un défaut mais la famine ; sinon, l'orphelin sans allomaternage l'est.

## Résultat B (22 h 22) — neutre ; P11 tenue
Enfants morts de soif = 11 nourrissons : 8 mère sans lait (famine, froid 42 et 7), 3 orphelins
(tempéré 42). Pas un défaut de l'eau : la famine d'hiver. Écart réel mais petit : pas
d'allomaternage. Reste inexpliqué : 7 adultes morts de soif en tempéré 42 (dont affamés).

## Nuit du 3 au 4 (01 h 05) — à reprendre
Traces tempéré 42 (3 ans, code D+C+A) : nouvelle signature — « Walking » SANS tâche pendant des
heures, soif 1, source à 331 m « eau entre deux : true ». Hypothèse E : l'A* (NODE_BUDGET 2 500
cellules) échoue sur un contournement plus long que sa recherche → « vraiment cerné » → source
marquée inaccessible : encore une limite de calcul devenue croyance. À vérifier (test).
Runs longues v2 (binaire 0582118, sans allomaternage) :
- tempéré 42 : an 25, pop 28, 52 morts de soif ; deux bandes à > 900 km l'une de l'autre
  (dispersion médiane 458 km, max 1 101 km) — dérive d'une bande de ~40 km/an : anomalie.
- tempéré 7 : an 29, pop 155 (+5 %/an), une bande regroupée à 250 m ; morts : 13 vieillesse,
  11 soif (dont 11 nourrissons).

## G et H (2026-10-04, 12 h, avant mesure)
H RÉFUTÉE PAR LECTURE : `memory::exchange_knowledge` inclut les nourrissons (à la position de
leur mère) — un sevré n'a pas une mémoire vide. Les enfants de tempéré 42 meurent par vagues
pendant les déplacements de bandes (ans 80-85 : 98 morts de soif dont 47 enfants) → pistes E/F.
G corrigé : le lait suit la santé (réserves) de la mère au lieu d'une porte à 0,95 de faim.
Régime : la mortalité des nourrissons ne naît qu'au-delà de ~300 par bande → banc nourriture,
300 agents au départ, tempéré seed 7, 2 ans, témoin (9675c7f) contre G.
- P12 : morts de nourrissons (soif) G ≤ 30 % du témoin.
- P13 (autre bout) : la faim des mères ne monte pas de plus de 0,05 en moyenne (le lait coûte
  toujours) ; aucune hausse des morts d'adultes.

## Résultat G (14 h 16) : P12, P13 tenues — morts de soif 26 → 3 (nourrissons), pop 298 → 343.
## E et F (14 h 22, avant mesure) — runs longues tempéré 42, G+E contre G+E+F
Référence v2 (sans G, E, F) : bandes à 976 km l'une de l'autre à l'an 20, 52 morts de soif à l'an 25.
- P14 (F) : dispersion maximale à l'an 20 : G+E+F < 50 km ; G+E nettement plus (> 200 km).
- P15 (E) : morts de soif sur 20 ans, G+E < v2 (52 à l'an 25).
- P16 (autre bout de F) : la bande ne se fige pas — la dispersion médiane reste > 0,2 km et
  la population ne chute pas sous G+E.

## Résultat E et F (16 h 22, an 20)
Dispersion max : v2 976 km, G+E 773, G+E+F 181 → F divise la dérive par 4, P14 (< 50 km) réfutée.
Morts de soif : v2 51, G+E 41, G+E+F 51 → E faible, P15 non tenue avec F.
P16 tenue (bandes non figées). Fait nouveau : dans les trois versions, la population est déjà
étalée sur ~300 km dès l'an 5, avec 12-18 morts de soif : le problème de tempéré 42 naît dans
les premières années (individus qui partent au loin et meurent de soif), hors de E et F.

## Dispersion précoce de tempéré 42 (16 h 40, avant mesure)
GEF : disp max 19 km au 3e mois → 80 km à 15 mois → 210 km à 3 ans ; médiane 4 → 20 km : le groupe
entier s'étale. Banc nourriture avec relevé des éloignés (> 10 km du point médian), 3 ans.
- P17 : les éloignés sont surtout en clan (> 70 %) et en marche vers Wander (changement de pays ou
  errance) ou SeekGame — la faim les déplace, pas l'exploration.
- P18 : les éloignés sont plus souvent assoiffés que la moyenne.
Réfutation : les éloignés sont hors clan et en errance de curiosité.

## Diagnostic des morts de soif de tempéré 42 (17 h)
Éloignés : 1-13 % en clan, presque jamais assoiffés → ce sont quelques isolés installés loin (P17
réfutée). Morts de soif : 15 en clan, 1 hors clan. E2 (grille qui voit l'agent cerné) réfutée :
6/9 cellules terre. Candidats : boire gagne chaque heure (1,1-1,7) ; l'A* rend un chemin COMPLET
(52-67 étapes) ; l'agent ne bouge pas. E3 : le premier tronçon (tuile exacte → centre de la
première cellule) n'était pas vérifié et traversait l'eau. Test synthétique vu échouer.
- P19 : morts de soif de tempéré 42 sur 3 ans : 16 → ≤ 5.

## Résultat E3 (2026-10-04, 22 h 20) — P19 tenue
Tempéré 42, 3 ans, départ jour 135 : morts de soif 16 → 0 (aucune mort d'aucune cause), pop finale
40 → 57. Gel des naissances de 18 mois identique au témoin (89 % des femmes fécondes allaitent la
cohorte du jour ~450) : effet de cohorte connu (D12), pas E3.
## Voisins (avant mesure) — acceptation 1 an, 8 runs, E3 contre témoin 54f3fe5
- P20 : morts de soif E3 ≤ témoin sur 8/8 ; aucune autre cause de mort ne monte de plus de 20 %.
- P21 (coût) : débit E3 ≥ 0,9 × témoin (le contrôle du premier tronçon ne coûte que 8 segments).

## Résultat des voisins (2026-10-05, nuit) — acceptation 8 runs × 3 ans, E3 contre 54f3fe5
Morts de soif : 18 (tempéré 42 : 16, froid 1337 : 2) → 0 sur 8/8. Aucune autre cause ne monte. P20 tenue.
Trajectoire identique au témoin sur 5/8 runs (E3 ne s'y déclenche jamais) ; changée sur tempéré 42,
tempéré 1337, froid 1337. P21 (débit ≥ 0,9 × témoin) : tenue sur 7/8, réfutée sur tempéré 1337
(63,3 → 52,4 tps, 0,83) — trajectoire différente, coût et trajectoire non départagés ; plancher très
au-dessus de 20. Refus de budget froid 1337 : 3 → 6.
Conséquence : la mortalité est désormais NULLE sur 6/8 runs à 3 ans (5/8 au témoin) — la soif était
le seul régulateur ; le critère « mortalité ni nulle ni totale » échoue. Mortalité de fond = suivant.
