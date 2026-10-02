# Carnet de bord

Ce carnet ne double pas le journal technique. [JOURNAL.md](JOURNAL.md) dit ce qui a été construit et mesuré ; [CODE.md](CODE.md) dit ce que fait le programme. Ici, on raconte comment notre façon de penser le projet a changé : les découvertes, les erreurs, les impasses, les moments où l'un de nous deux a vu ce que l'autre ne voyait pas. On n'y écrit pas chaque échange. On y écrit quand quelque chose a vraiment bougé, ou quand Quentin le demande.

Le carnet est tenu par Claude, à la première personne quand il parle de ses propres raisonnements, et relu par Quentin.

---

## Avant le carnet : du 14 juillet au 23 septembre

Le premier commit tient en une ligne : un brief, un fichier d'instructions, un `.gitignore`. Le même jour, il y a déjà un générateur aléatoire déterministe et un premier champ d'altitude exporté en image. Deux jours plus tard, la phase 1 est close : un monde infini qu'on parcourt dans un navigateur, des continents de trois mille kilomètres, des déserts placés sous le vent des montagnes parce que c'est là que la physique les met.

Juillet va vite. La vie arrive le 17, la faune le lendemain, la démographie et la mémoire le 20, les clans entre le 21 et le 23, l'invention le 26, la Chronique le 30. Chaque phase a son moment de vérité, et c'est souvent un échec qui l'apporte. Le premier critère de formation des clans, une densité de liens, cachait une règle de taille déguisée en arithmétique : les groupes de plus de cinquante-quatre personnes ne pouvaient mathématiquement plus le satisfaire, et les clans vivaient trois jours et demi en médiane. Il a fallu le remplacer par une question différente (ce groupe a-t-il une ligne de faille ?) pour que la taille cesse d'être lue quelque part.

La Chronique, elle, a fait exactement ce que le brief promettait d'un journal narratif : servir aussi d'outil de débogage. Elle annonçait quarante-deux morts au combat quand la démographie n'en comptait qu'une, de soif. Aucune blessure mortelle ne tuait : la santé se régénérait dans le tick même où le coup l'avait mise à zéro. Le monde était plausible et faux. C'est de là que vient la cinquième règle, confronter deux compteurs indépendants, et c'est peut-être la leçon la plus durable de l'été.

Le 14 septembre, le développement déménage sur la machine de déploiement elle-même : deux processeurs, trois gigaoctets de mémoire. Le lendemain, une run de huit heures y montre ce qu'aucun test ne voyait. Le débit passe de soixante-trois ticks par seconde à un dixième, un facteur six cent trente-six, en quinze ans de jeu. Le banc de référence mesurait quatre cents ticks, la fin de la première année : il aurait déclaré victoire pendant que la simulation s'effondrait cinq cents jours plus tard.

Commence alors ce qu'on appellera le chantier de dérive. Pendant trois jours, sept hypothèses sur la cause de l'effondrement, quatre réfutées à grands frais. Puis un profileur, qui mesure où passe le temps au lieu de le déduire des sorties, et qui désigne un coupable que personne n'avait suspecté : l'écologie. On la rend trente fois plus rapide, la run entière cinq fois, la trajectoire reste identique au bit près, et la dérive ne bouge pas d'un jour. C'est la découverte qui oriente la suite : un coût fixe réduit ne rattrape jamais une croissance. Le lendemain, quatre graines au lieu d'une retournent le diagnostic qui tenait depuis des semaines : sur trois d'entre elles, ce n'est pas la faune qui explose, c'est le gibier qui s'éteint. Une seule graine lisait un tirage.

Les règles de méthode du projet sont nées ainsi, une par erreur. Mesurer au lieu de déduire. Dire dans quel régime on mesure. Échantillonner à la cadence du phénomène. Un changement à la fois. Deux compteurs. Un test qui casse avant le correctif. Quatre graines. Une statistique qui suit la forme de la grandeur. Elles ont toutes un prix, et ce prix est écrit à côté.

---

## 25 septembre : fixer ce qu'on cherche

La session commence par une reprise de contexte, et presque aussitôt par une question de Quentin qui aurait dû être posée plus tôt : ce chantier vise-t-il toujours la performance, et va-t-il borner la simulation ? Je proposais de libérer l'exploration humaine, gelée dès la première année. C'était un chantier d'innovation, qui aurait même aggravé le débit.

On fixe alors un objectif chiffré, et c'est lui qui donne sa forme à tout ce qui suit : au moins vingt ticks par seconde dans chaque fenêtre annuelle, sur cinquante ans, quatre graines, deux scènes. Un plancher et non une moyenne, parce qu'une moyenne sur vingt et un ans notait à quarante ticks une run qui était morte.

## 26 septembre : quarante-quatre mille cerfs au kilomètre carré

Deux mesures confirment ce qu'on soupçonnait : rien ne freine un troupeau, à aucune densité, et le coût de la faune passe par le store de chunks, pas par les requêtes de voisinage. La grille spatiale, prévue depuis le brief, est abandonnée avant d'être écrite.

Puis Quentin demande un pas en arrière avant la correction : interpoler, recroiser avec les données d'avant, vérifier qu'on ne passe pas à côté de quelque chose. Le calcul le plus simple de la session tombe alors. Une tuile de forêt repousse quatre unités par jour ; une tête en mange vingt-quatre. La capacité implicite du modèle est de quarante-quatre mille cerfs au kilomètre carré, quand une forêt réelle en porte une cinquantaine. Les quatre tentatives de régulation par le fourrage n'avaient pas échoué sur la représentation : la consommation d'une bête était mille fois trop faible. Le même pas en arrière fait sortir une seconde dérive, indépendante de la faune, et une anomalie : la capacité du store change l'histoire du monde.

Sans ce pas en arrière, on aurait corrigé la natalité avec une capacité mille fois trop haute, et elle n'aurait rien borné.

## 27 septembre : chaque correction en révèle une autre

La journée ressemble à une descente d'escalier. La natalité suit désormais ce que produit le pays, et le gibier s'éteint, parce que des prédateurs calibrés soixante fois trop vite le dévorent. On les ramène au rythme d'un loup, et c'est l'aire occupée par la faune qui grandit sans fin. J'essaie un bord qui retire le gibier trop loin des humains : il vide la zone qu'il devait borner, parce que les troupeaux fuient les hommes et que la fuite les pousse au bord. Annulé. Un bord qui renvoie au lieu d'absorber tient, mais la prédation masque encore un défaut de ma propre conception, que le territoire des prédateurs finit par révéler.

Quentin pose la question qu'il fallait poser devant une correction de rayon : solution viable ou pansement ? La réponse honnête était « les deux » : le rayon était faux, mais aucune valeur de rayon ne pouvait être la bonne tant que retirer une entité fait disparaître des bêtes. La mesure l'a confirmé : un rayon serré draine le gibier, un rayon large laisse l'aire grandir.

Ce jour-là, je me suis aussi trompé sur des choses plus terre à terre. Un binaire témoin copié depuis une vieille compilation, parce que l'outil de compilation manquait au chemin de recherche. Une commande d'arrêt qui a tué mon propre terminal parce qu'elle filtrait sur un motif trop large. Un témoin dont le débit était faussé par mes compilations lancées en parallèle. Chaque fois, c'est une vérification qui l'a attrapé, pas une intuition. La procédure a tenu mieux que moi.

## 28 septembre : un million de têtes

Le dépôt devient public, avec un historique réécrit pour en retirer les traces de l'ancienne machine et les mentions de co-auteur. Puis une mesure de cinq ans montre, sur une seule graine, un million de têtes de gibier sur quatre-vingt-une mailles. Plutôt que de corriger ce que je croyais comprendre, je mesure d'abord : les amas les plus denses étaient faits de troupeaux en fuite perpétuelle, qui gardaient leur satiété de naissance et se reproduisaient au plafond. Le frein existait, mais seulement pour qui broutait. Une ligne déplacée, et le million redevient douze mille.

C'est aussi le jour où le coût d'un troupeau se laisse enfin décomposer. Un troupeau coûte entre douze et trente-deux microsecondes par tick ; chaque chunk qu'il force à régénérer en coûte mille huit cents, dont les deux tiers pour recalculer une humidité qui ne change jamais. Un cache, et le débit gagne moitié, la trajectoire restant identique au bit près.

## 29 septembre : lire le code avant de le mesurer

La validation contre l'objectif tient trois runs sur huit, deux fois de suite. Les échecs ont gagné un facteur trois sans franchir la ligne. Quentin demande alors un vrai pas en arrière : revenir à la logique, puis aux fonctions, et dessiner le tout. J'en fais une carte soignée, et il pose la question qui la défait : suis-je vraiment remonté dans le code, ou est-ce le récit de la session ?

La réponse était non. La carte résumait ce que nous avions touché, vu à travers le chantier. Relu dans le code, le modèle dit autre chose. La pression d'innovation que j'avais déclarée « saturée », et donc coupable, n'est lue par aucun système. Le feu, racine de tout l'arbre technologique, exige une pression de froid : un peuple qui ne gèle jamais ne peut pas le découvrir, par construction, et vingt et un ans sans technologie ne demandaient aucune autre explication. Un humain sans clan ne peut rien inventer et n'est rappelé que vers son voisin le plus proche. Les rivières du générateur n'existent pas pour les humains. La délibération revient tous les quatre ticks, pas tous les dix. Nous avions passé des jours à régler la performance d'un monde dont le modèle de base portait ces défauts en amont.

Puis vient le débat sur le feu, et c'est Quentin qui a raison. J'avais vu le feu absent en pays tempéré et proposé de le rendre possible. Il refuse de le faciliter : la nature reste la nature, le hasard de trouver un matériau reste du hasard. En regardant l'autre bout, le feu apparaît en un ou deux ans en pays froid, beaucoup trop vite pour un brief qui parle en siècles. Le même mécanisme produit les deux extrêmes : une porte de froid tout ou rien, et des incendies qui naissent près d'un humain tiré au hasard plutôt que selon la surface et le climat. Le défaut n'était pas qu'un résultat manquait, c'était que les mécanismes n'étaient pas naturels.

Deux règles en sortent. La neuvième fait de la lecture du code le point de départ de toute correction : lire, classer, prédire, mesurer, témoin, corriger, valider en regardant les voisins, puis revenir à la carte, réfutations comprises. La dixième dit comment juger un défaut : corriger des mécanismes, jamais des résultats ; regarder les deux bouts d'une distribution ; laisser la nature naturelle ; ne garder une porte tout ou rien que devant une impossibilité physique. Quentin y ajoute la clause qui compte peut-être le plus : au moindre doute sur un mécanisme, lui demander sa vision. Il s'attend à trouver beaucoup d'incohérences de ce genre, et c'est lui qui porte l'intention du monde.

Le chantier a changé de nature ce jour-là. Pendant quatre jours, on mesurait pour savoir quoi corriger. On va maintenant lire pour savoir quoi mesurer.

## 30 septembre et 1er octobre : passer un hiver

Quentin m'a laissé la nuit sur la nourriture des humains, avec une seule consigne : borner, sans faciliter. Le premier soir a commencé par un échec utile. J'avais écrit un test censé prouver que la terre ne nourrit pas deux cents bouches serrées sur quelques hectares, et il est passé. Les deux cents s'étaient dispersés jusqu'à douze kilomètres, sans avoir faim. Ce n'était pas la nourriture qui fixait leur densité, c'était l'errance. Quentin a tiré de cet épisode une question qu'il voulait voir dans la méthode : est-ce que je suis passé à côté de quelque chose ? Elle est devenue une étape de la boucle, avec une condition que j'ai ajoutée, qu'on y réponde toujours en nommant une chose.

La suite a été une descente plus longue encore que celle de la faune. Un stock comestible saisonnier, réaliste, a éteint cinq populations sur huit au premier hiver. Ce n'était pas le stock qui avait tort : il mettait à nu tout ce qui manquait pour passer un hiver. Le partage, que j'ai d'abord mal écrit, et qui tuait davantage parce que la viande offerte et non mangée finissait jetée. La réserve, plafonnée à six jours. Et surtout un geste si évident que personne ne l'avait codé : garder sa prise. Un chasseur sans clan mangeait sa ration et abandonnait le reste du cerf. Corriger cette seule ligne a presque doublé les survivants. Aucune des idées plus savantes de la série n'a fait la moitié de cet effet.

J'ai beaucoup appris sur mes propres instruments. Un relevé pris après le tick m'a montré des affamés qui ne faisaient « rien » un tiers du temps, et j'ai failli toucher au rythme des décisions avant de comprendre que je mesurais mal. Une scène de test sans source d'eau tuait de soif les chasseurs que je croyais incapables de chasser. Deux fois, une correction n'a rien donné parce que je l'avais mal écrite, et c'est la mesure, pas la relecture, qui l'a attrapé.

Puis le diagnostic au jour près a montré l'image qui m'est restée. Soixante fondateurs repus tuent cent huit bêtes le premier jour, en mangent à peine, et meurent de faim trois semaines plus tard. En donnant à la viande sa vraie valeur, j'avais rendu visible un défaut ancien : chaque approche tuait à coup sûr. Avec le taux de réussite des Hadza, le massacre disparaît. Et l'arrivée en fin d'hiver, sans réserve, devient mortelle.

C'est là que Quentin a tranché une question que je n'osais pas poser franchement : faut-il qu'un hiver soit survivable ? Non. Des inconnus lâchés sans rien dans la taïga en fin d'hiver meurent, et le modèle a raison de les tuer. Les dates et les lieux de départ servent à tester des cas définis, extrêmes compris, pas à garantir une issue. Il a aussi dit non à un campement décrété, et oui à un campement qui naîtrait du comportement : on rentre dormir près des siens.

La validation sur cinq ans, partie en été, a donné ce que nous cherchions et une surprise. Huit populations sur huit survivent, à des densités que Binford reconnaîtrait, avec des famines en pays froid. Mais aucune ne croît. Ce n'est plus la nourriture qui les borne, c'est la soif, qui tue plus que la faim, et une natalité trois fois trop basse. Le chantier a tenu sa promesse et révélé le suivant.

## 1er octobre : lire avant de corriger

La journée a eu la forme d'une leçon répétée trois fois. La validation de la nourriture laissait deux coupables évidents : la soif qui tuait plus que la faim, et une natalité trois fois trop basse. J'avais une hypothèse pour chacun, et Quentin m'a laissé aller les mesurer avant de toucher au code. Les deux étaient fausses.

La natalité n'était pas basse. Les femmes passaient les trois quarts de leurs années fécondes enceintes ou à allaiter, et les conceptions attendues égalaient les naissances à deux près. Ce que j'avais pris pour un défaut était le dessin d'une cohorte : des fondatrices arrivées ensemble, qui conçoivent ensemble et allaitent ensemble trois ans. La soif, elle, ne venait pas d'un manque d'eau. Les traces heure par heure ont montré un homme qui meurt à trois cents mètres d'une source, de l'autre côté d'un lac, après l'avoir visée en vain pendant deux jours, avec quinze autres sources en mémoire. Rien en lui ne retenait l'échec. Quentin a trouvé ce bug « pas évident à trouver ». Il ne l'était pas : deux hypothèses écartées avant lui, et c'est la trace d'un seul mourant qui l'a livré.

Le troisième coupable est sorti de l'instrument suivant, celui qui classe les morts par âge et par cause : la violence, près de la moitié des morts. Là aussi l'idée reçue a cédé. Je croyais à une guerre de fond ; la mesure a montré des épisodes rares mais meurtriers, où chacun retourne au combat quelle que soit sa blessure. Un prototype où l'on ne razzie plus blessé ramène la violence à l'ordre de grandeur réel, sur deux runs seulement. Il attend l'avis de Quentin.

Entre deux mesures, Quentin a ouvert une question plus large que tous ces défauts : nos humains chassent et se battent avec des outils qui n'existent pas dans le modèle. J'avais calé la chasse sur des archers sans l'avoir dit. Sa réponse a fixé un point de départ que le projet n'avait jamais nommé : les fondateurs arrivent avec l'outillage de leur espèce, la pierre taillée et l'épieu, comme ils savent marcher. Les inventions commencent après.
