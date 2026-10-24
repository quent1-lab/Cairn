# Domaines — où vit le savoir du projet

Chaque domaine (et sous-domaine) a **la même structure** :

| Fichier | Contenu | Quand l'écrire |
|---|---|---|
| `CONTEXTE.md` | ce que le code fait (modules, constantes clés), la référence (BRIEF, réalité), les décisions de l'utilisateur, datées | à chaque jalon qui touche le domaine |
| `SOURCES.md` | chiffres réels : valeur, unité, population, lien, date, qui l'a trouvé | dès qu'un chiffre est trouvé |
| `CHANTIERS.md` | registre local des chantiers et jalons (✔ ▶ ○ ?) et leur fichier | à chaque changement d'état |
| `RESULTATS.md` | acquis mesurés (régime affiché) et acquis négatifs (réfutations, corrections annulées) | à la validation d'un jalon |
| `HISTORIQUE.md` | journal daté du domaine | à chaque jalon |
| `chantiers/<CODE>.md` | le fichier vivant d'un chantier : lecture, hypothèses, prédictions **écrites avant**, mesures, débat, ce que j'ai pu manquer | pendant le chantier |

Un chantier vit dans le domaine de son **défaut** (là où le code s'écarte de la référence) ;
s'il touche d'autres domaines, une ligne de renvoi dans leur `CHANTIERS.md`.

Arbre :
- `humain/` — `cognition/` (cerveau, mémoire, rendements), `deplacement/` (marche, camp),
  `alimentation/` (cueillette, chasse, partage, stock), `corps/` (énergie, soif, maladie,
  enfance, mort), `societe/` (liens, clans, conflits)
- `faune/`, `flore/`, `monde/`, `technique/`, `divin/`, `performance/`, `serveur/` (le serveur, le fil, l'observation)

Ailleurs :
- `docs/CODE.md` — la carte globale du code et **l'ordre des chantiers** (§5).
- `docs/BRIEF.md`, `docs/COMMANDES.md`, `docs/CARNET.md` — inchangés.
- `docs/JOURNAL.md`, `docs/ETAT.md` — **archives** (avant le 2026-10-16), lues par `grep`.
- `out/` — sorties brutes des bancs (non suivies par git) ; jamais de conclusion là.
