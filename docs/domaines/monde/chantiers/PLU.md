# PLU — la pluie a une saison

Ouvert au registre le 2026-11-02 depuis CAP (décision de l'utilisateur : **pas encore
travaillé** ; on finit CAP, ce fichier prépare le terrain). Rien n'est lu en détail ici.

## Ce qu'on sait déjà (vu en passant, pendant CAP)

- L'humidité d'une tuile est une **moyenne annuelle figée** (`worldgen/src/humidity.rs` :
  advection le long du vent, recharge sur l'océan, décharge sur le relief). Aucune saison.
- Le BRIEF (§2, « Cycle saisonnier annuel (température et précipitations modulées) ») veut des
  précipitations modulées dans l'année ; seule la température l'est (`climate::seasonal_offset`).
- La météo (`weather.rs`) : cellules d'averse ou de sécheresse de 1,5 km, 5 jours, 6 % par jour,
  **nées à 0,5-4 km d'un humain seulement** — un pays sans humains n'a jamais de sécheresse,
  contre la règle « la nature reste la nature » (CLAUDE.md, règle 10).
- Conséquence côté faune (CAP) : pas de saison sèche, donc la savane, la steppe et les déserts
  n'ont pas de saison maigre (réel : la mortalité des veaux de gnou en saison sèche borne la
  population, Serengeti) ; CAP n'y juge rien.

## Ce qu'il faudra faire (à la reprise, méthode `chantier`)

Étape 1, lecture seule : le modèle de l'eau (humidité, vent, sources, météo) contre le réel —
régimes de pluie par latitude (zone de convergence intertropicale, été sec méditerranéen,
mousson), sans calendrier écrit : la saison des pluies doit sortir du déplacement des ceintures
de vent avec la saison. Voisins : la flore (**POU**, la saison de pousse) et la faune (CAP,
étape 4 à refaire hors des biomes tempérés et froids).
