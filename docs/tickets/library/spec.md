Status: ready-for-agent

# Spec : la bibliothèque (modèles de messages et skills)

Cadrage du 16 novembre 2026 (deux tours de questions, recommandations acceptées). Faits : `docs/research/2026-11-16-skills-des-agents.md`.

## Problem Statement

Je retape souvent les mêmes demandes à mes agents (corriger un bug avec un test qui le prouve, ajouter une page en passant par une mission, relire un fichier), en les formulant chaque fois un peu différemment. Je ne sais pas non plus, depuis Solar, quels skills mes agents connaissent déjà : ils sont éparpillés entre `~/.claude/skills`, `~/.agents/skills`, les extensions et les dossiers des projets.

## Solution

Une bibliothèque de **modèles** : des fichiers Markdown, personnels (`~/.solar/library/`) ou propres à un projet (`.solar/library/` dans le dépôt, versionnable), avec des trous `{nom}`. On les choisit par `/` dans le composer ou par ⌘K ; un petit formulaire remplit les trous, puis le texte complété est inséré dans le composer, modifiable, jamais envoyé seul. Un modèle peut se lancer comme mission (Planifier activé) et conseiller un agent. Quelques exemples intégrés, en lecture seule, se dupliquent. Une vue « Bibliothèque » crée, modifie, duplique et supprime les modèles, et consulte en lecture seule les skills installés pour Claude, Codex et Grok.

## User Stories

1. En tant qu'utilisateur, je veux enregistrer un message que je réutilise, pour ne pas le retaper.
2. En tant qu'utilisateur, je veux des modèles personnels, disponibles dans tous mes projets.
3. En tant qu'utilisateur, je veux des modèles propres à un projet, rangés dans son dépôt, pour les partager avec mon équipe.
4. En tant qu'utilisateur, je veux qu'un modèle de projet l'emporte sur un modèle personnel de même nom.
5. En tant qu'utilisateur, je veux que chaque modèle soit un fichier Markdown lisible et modifiable sans Solar.
6. En tant qu'utilisateur, je veux des trous `{nom}` dans un modèle, que je remplis au moment de l'utiliser.
7. En tant qu'utilisateur, je veux taper `/` dans le composer et voir mes modèles à côté des commandes de l'agent.
8. En tant qu'utilisateur, je veux trouver mes modèles dans ⌘K, dans une catégorie « Bibliothèque ».
9. En tant qu'utilisateur, quand un modèle a des trous, je veux un petit formulaire au-dessus du composer, un champ par trou, Entrée pour insérer.
10. En tant qu'utilisateur, je veux que le texte complété arrive dans le composer, modifiable, sans être envoyé.
11. En tant qu'utilisateur, je veux qu'un modèle sans trou s'insère directement.
12. En tant qu'utilisateur, je veux qu'un modèle « mission » active Planifier quand la conversation n'a pas commencé.
13. En tant qu'utilisateur, je veux qu'un modèle qui conseille un agent bascule sur lui quand la conversation n'a pas commencé, et sinon me le dise sans rien changer.
14. En tant qu'utilisateur, je veux quelques exemples intégrés, en lecture seule, pour démarrer.
15. En tant qu'utilisateur, je veux dupliquer un exemple ou un modèle pour en faire le mien.
16. En tant qu'utilisateur, je veux une vue « Bibliothèque » pour créer, modifier, dupliquer et supprimer mes modèles.
17. En tant qu'utilisateur, je veux choisir, à la création, entre modèle personnel et modèle du projet courant.
18. En tant qu'utilisateur, je veux que la vue refuse d'écraser un modèle modifié ailleurs depuis son ouverture, comme les conventions de la fiche projet.
19. En tant qu'utilisateur, je veux une confirmation avant de supprimer un modèle.
20. En tant qu'utilisateur, je veux que la vue apparaisse dans la barre latérale dès qu'un modèle existe, et rester joignable par ⌘K avant.
21. En tant qu'utilisateur, je veux consulter les skills installés : nom, description, origine, agents qui lisent ce dossier.
22. En tant qu'utilisateur, je veux ouvrir le dossier d'un skill dans le Finder.
23. En tant qu'utilisateur, je veux chercher dans les modèles et les skills.
24. En tant qu'utilisateur, je veux qu'un modèle ou un skill illisible (en-tête cassé) soit signalé sans casser la liste.
25. En tant qu'utilisateur anglophone, je veux la vue et les exemples en anglais.
26. En tant qu'utilisateur au clavier, je veux choisir un modèle, remplir ses trous et l'insérer sans souris.

## Implementation Decisions

- **Module du cœur `library`**, sans UI : lecture des modèles (dossier personnel, dossier du projet, exemples intégrés), fusion par nom (projet > personnel > exemple), analyse de l'en-tête et des trous, remplissage, écriture et suppression avec refus d'écraser un fichier changé ailleurs (même règle que `project::save`) ; lecture des skills par dossier, avec les agents qui lisent chaque dossier.
- **Format d'un modèle** : `<nom>.md`, en-tête YAML facultatif entre `---` : `title`, `description`, `agent` (`claude`, `codex`, `grok`), `mission` (booléen) ; le corps est le texte, avec des trous `{nom}` (lettres, chiffres, `_` et `-`) ; `{{` et `}}` écrivent une accolade. Le nom du fichier sert d'identifiant et de commande `/nom`. Sans en-tête, le titre est le nom. Proche des commandes de Claude Code ; un `$ARGUMENTS` est lu comme un trou `arguments`.
- **Emplacements** : `~/.solar/library/` (ou `SOLAR_LIBRARY` pour les essais), `<projet>/.solar/library/`. Solar ne crée ces dossiers qu'à l'enregistrement du premier modèle.
- **Exemples intégrés** : trois ou quatre, embarqués dans le binaire, FR ou EN selon la langue, en lecture seule ; « Dupliquer » les copie dans le dossier personnel.
- **En-tête YAML** : lu sans nouvelle dépendance si un analyseur simple (clé: valeur sur une ligne) suffit ; un en-tête plus riche est ignoré proprement avec un signalement.
- **Composer** : `/` fusionne commandes de l'agent et modèles (groupe « Bibliothèque »), comme les suggestions actuelles ; choisir un modèle ouvre le formulaire des trous (ou insère directement). ⌘K gagne une catégorie « Bibliothèque » (modèles et skills).
- **Vue Bibliothèque** : vue pleine comme Réglages et fiche projet (680–760 de large, sections en petites capitales) : modèles du projet courant, personnels, exemples ; éditeur en zone de texte simple ; puis « Skills » en lecture seule, groupés par origine. Entrée dans la barre latérale (à côté de Réglages) dès qu'un modèle personnel ou de projet existe.
- **Skills** : dossiers lus (voir la note de recherche) : Claude (`~/.claude/skills`, `.claude/skills` du projet, extensions activées du cache), Codex (`~/.codex/skills`, `~/.agents/skills`, `.agents/skills`), Grok (`~/.grok/skills`, `.grok/skills`, plus les dossiers Claude et `.agents`). Chaque dossier porte la liste des agents qui le lisent ; la vue dit « lu par » et ne promet pas qu'un skill est actif.
- Textes FR/EN dans les locales ; aucune couleur d'accent.

## Testing Decisions

- Cœur, sur dossiers jetables : analyse de l'en-tête (complet, absent, cassé), trous (dont `{{`, `$ARGUMENTS`, trou répété), remplissage, fusion projet > personnel > exemple, écriture refusée si le fichier a changé, suppression, lecture des skills (dont `SKILL.md` sans description, dossier sans `SKILL.md`, extension désactivée). Antériorité : tests de `project` (conventions et conflit d'enregistrement).
- Interface, sans fenêtre : choisir un modèle à trous par `/`, remplir, vérifier le texte dans le composer et Planifier activé pour un modèle mission ; un modèle conseillant Codex bascule l'agent d'une conversation neuve. Antériorité : tests de la palette et du composer.
- À l'écran : vue Bibliothèque, formulaire des trous, `/` et ⌘K, en 1024 et 720, FR et EN.

## Out of Scope

- Installer, activer ou désactiver des skills ; les modifier depuis Solar.
- Synchroniser la bibliothèque entre machines ; partage autrement que par le dépôt.
- Exporter un modèle vers `.claude/commands` (possible plus tard grâce au format proche).
- Modèles avec logique (conditions, boucles) ; images jointes à un modèle.

## Further Notes

- Les emplacements des skills de Codex et Grok changent avec leurs versions : la note de recherche est datée, les règles vivent à un seul endroit du code.
