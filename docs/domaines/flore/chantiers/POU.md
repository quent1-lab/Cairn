# POU — la saison de pousse

Ouvert au registre le 2026-11-02 depuis CAP (**pas encore travaillé** ; prépare le terrain).

## Ce qu'on sait déjà

- La végétation pousse les jours où la température moyenne du jour dépasse 5 °C
  (`climate::grows`, `GROWTH_THRESHOLD_C`), et seulement alors : ni la lumière (durée du jour) ni
  l'eau n'y entrent. Même règle pour le fourrage des troupeaux (`fauna::Rangeland`, CAP 3.1b) et
  la cueillette (`gathering`, D10).
- Conséquences mesurées dans CAP : des forêts tempérées proches du point de référence (seeds 42,
  1337) ne descendent jamais sous 5 °C et **poussent toute l'année** ; leur gibier se pose à
  1,7 fois la capacité du fourrage (niveau fixé par la part d'entretien, non sourcée). Dans le
  réel, une forêt océanique douce ralentit l'hiver (lumière, durée du jour).
- Scènes de référence (`cap_scene`, CAP, 2026-11-03) : la scène tempérée de seed 42 pousse 360 jours sur 49/49 mailles à 8 km du foyer ; seed 1337, 29/49 ; les autres scènes ont un hiver (154-284 jours de pousse).
- La savane pousse toute l'année faute de saison sèche (voir **PLU**).

## Ce qu'il faudra faire

Lecture : la saison de pousse réelle par biome (température, photopériode, eau), contre la règle
des 5 °C. Dépend de **PLU** pour l'eau.
