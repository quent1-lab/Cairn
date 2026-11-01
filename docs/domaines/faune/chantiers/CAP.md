# CAP — ce qui borne vraiment les ongulés

Ouvert le 2026-11-01 depuis CHA-5 (décision de l'utilisateur : la capacité avant le champ par
maille). Lire d'abord `CHA-5.md`, sections « Diagnostics — mesurés » et « Avant le champ ».

## Le défaut (lu et mesuré dans CHA-5)

Sans le périmètre qui efface le gibier, le modèle monte à 30-90 têtes /km² en 2 ans, satiété
encore > 0,5 (croissance continue) ; capacité calculée de fourrage : 23-740 têtes /km² selon le
biome. Réel (`../SOURCES.md`, section CHA-5) : cerf 10-35 sans prédateurs, 12,7 avec loups ;
wapiti 2-9 ; renne 0,8-3 ; bison 0,3-0,5 ; saïga 0,5-0,8. Écart : ×2-5 (cerf en forêt) à
×50-1 000 (milieux ouverts). Les meutes sont déjà au plafond réel (0,05 /km²).

Référence : la réalité (règle 10 : on corrige des mécanismes, on ne règle pas une constante
vers ces chiffres ; ils jugent).

## Mécanismes candidats (à lire dans le code avant tout choix)

1. Le fourrage accessible (`fauna::accessible_forage_g_m2_yr`) : des parts broutables écrites
   comme « point de départ, pas mesure » (5 % en forêt, 20-30 % en milieu herbacé) — à sourcer.
2. L'hiver : la maille juge sur une production **annuelle moyenne** (`Rangeland::production`) ;
   seul la pâture de la tuile voit la saison. Le réel : la capacité est fixée par le goulot
   d'hiver (neige, fourrage de qualité basse), pas par la moyenne.
3. Une mortalité qui ne dépend pas de la densité (mortalité de fond fixe, 0,010 /j) : chez les
   ongulés, la mortalité juvénile et hivernale monte avec la densité.
4. La réponse fonctionnelle des prédateurs (0,05 prise /prédateur /j, ~18 par an) : dans la
   fourchette d'un loup, à vérifier contre les sources (Białowieża : 0,57-1,05 cerf /km² /an).

## Premier pas (à faire)

Un banc de référence **sans humains**, par biome : une boîte fixe de faune (troupeaux et meutes,
sans périmètre mobile ni immigration), 4 seeds, plusieurs années, relevant densité (minimum et
médiane : c'est un réservoir), rapport proies / prédateurs et satiété par saison. Prédictions à
écrire avant (capacité calculée ci-dessus, et la part que l'hiver retire). Un appel direct à
`fauna::update_herds` / `update_packs` (comme les tests de `sim.rs`) évite les humains.
Sources à demander au `sourceur` : part du fourrage réellement broutée par biome ; mortalité
hivernale densité-dépendante des cervidés ; capacité de charge d'hiver.
