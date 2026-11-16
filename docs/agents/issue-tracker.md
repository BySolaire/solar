# Suivi des tickets : fichiers Markdown dans le dépôt

Les specs et les tickets de Solar sont des fichiers Markdown versionnés dans `docs/tickets/`. Rien n'est publié ailleurs (ni GitHub Issues ni autre service). `docs/design/DEVELOPMENT_PLAN.md` reste la feuille de route ; un chantier découpé en tickets y renvoie.

## Conventions

- Un chantier par dossier : `docs/tickets/<chantier>/`.
- La spec : `docs/tickets/<chantier>/spec.md`.
- Un fichier par ticket : `docs/tickets/<chantier>/issues/<NN>-<slug>.md`, numérotés à partir de `01`, jamais un fichier de tickets combiné.
- L'état de tri est une ligne `Status:` en haut de chaque ticket (rôles dans `triage-labels.md`).
- Les échanges s'ajoutent en bas du fichier sous un titre `## Comments`.

## Quand un skill dit « publier dans le suivi »

Créer un fichier sous `docs/tickets/<chantier>/` (en créant le dossier si besoin).

## Quand un skill dit « récupérer le ticket »

Lire le fichier au chemin indiqué ; l'utilisateur donne d'ordinaire le chemin ou le numéro.

## Opérations de `/wayfinder`

La **carte** est un fichier, avec un fichier **enfant** par ticket.

- **Carte** : `docs/tickets/<chantier>/map.md` (Notes, Decisions-so-far, Fog).
- **Ticket enfant** : `docs/tickets/<chantier>/issues/NN-<slug>.md`, numéroté à partir de `01`, la question dans le corps. Une ligne `Type:` donne le type (`research`, `prototype`, `grilling`, `task`) ; une ligne `Status:` vaut `claimed` ou `resolved`.
- **Blocage** : une ligne `Blocked by: NN, NN` en haut. Un ticket est débloqué quand tous ceux qu'il cite sont `resolved`.
- **Front** : parcourir `docs/tickets/<chantier>/issues/` pour les tickets ouverts, débloqués et non pris ; le plus petit numéro passe en premier.
- **Prendre** : mettre `Status: claimed` et enregistrer avant tout travail.
- **Résoudre** : ajouter la réponse sous `## Answer`, mettre `Status: resolved`, puis ajouter un pointeur (l'essentiel et le lien) aux Decisions-so-far de `map.md`.
