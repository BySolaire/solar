# Carte : intégration GitHub

## Destination

Une spec de la première intégration GitHub, `docs/tickets/github/spec.md`, découpée en tickets prêts à coder : depuis une mission, branche, commit et push, pull request, puis suivi de ses checks.

## Notes

- Domaine : `docs/design/GLOSSARY.md` (publication, branche de mission, copie de travail…) et `docs/design/adr/`.
- Faits établis : `docs/research/2026-11-16-github-gh-et-git.md`.
- Skills : `grilling` et `domain-modeling` pour les tickets de conversation, `prototype` pour l'aperçu, `impeccable` (mode Operate) pour l'écran de publication, `security-audit` en revue ciblée (lancement de `gh` et `git`, réseau, écriture dans le dépôt).
- Préférences de Solaire : questions de cadrage avec une recommandation en premier ; chaque geste GitHub confirmé ; aucun commit, push ou publication sans demande.
- Essais réels : il faudra un dépôt GitHub d'essai, privé, créé avec l'accord de Solaire ; ne jamais essayer sur ses dépôts de travail.

## Decisions so far

- [Cadrage de la v1](issues/01-cadrage-de-la-v1.md) : v1 = mission → branche → commit → push → PR → checks, corrections renvoyées à l'agent ; compte actif de `gh`, rien d'automatique, hooks lancés, jamais de force-push.
- [Commandes gh et git disponibles](issues/02-commandes-gh-et-git.md) : `gh` donne dépôt, droits, PR et checks en JSON ; pousser avec l'aide d'identification de `gh` pour rester sur son compte actif.
- [Construire le commit de la mission](issues/03-construire-le-commit.md) : branche dans la copie, index temporaire depuis le commit de départ, fichiers de la mission seuls ; hooks lancés, dépôt de l'utilisateur intact (ADR 0001).
- [Tentatives et PR](issues/05-tentatives-et-pr.md) : une branche et une PR par tentative (`solar/<titre>-<n>`, « Remplace #n ») ; abandonner propose de fermer la PR, décoché par défaut.

## Not yet specified

- Issues GitHub (v2) : les lister dans la fiche projet, lancer une mission depuis une issue, lier la PR à l'issue.
- Commentaires de revue d'une PR renvoyés à l'agent (v2).
- Texte de la PR rédigé par l'agent, en option.
- Suivi des checks pour les missions qui ne sont pas ouvertes dans un onglet.

## Out of scope

- Cloner un dépôt GitHub pour en faire un projet.
- Forks, GitLab et dépôts sans remote GitHub (boutons grisés avec la raison).
- Force-push, amend, rebase, merge automatique et déploiement.
