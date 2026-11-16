Status: resolved
Blocked by: 07 — Commit et push

# 08 : Ouvrir la PR

Spec : `docs/tickets/github/spec.md`.

**What to build :** Après le push, l'étape 2 du dialogue propose le titre (titre de la mission), la description (plan approuvé, état des tests, verdict du relecteur, agent nommé, « Remplace #n » pour une tentative), la base (branche de l'utilisateur au lancement si elle existe sur `origin`, sinon la branche par défaut) et la case « Brouillon ». « Ouvrir la PR » la crée avec `gh` ; son numéro et son adresse sont gardés. La revue montre alors une ligne d'état (PR #n ouverte, branche → base, « Ouvrir sur GitHub »), la barre de mission dit « PR #n ouverte », la barre latérale garde un point d'état. On peut fermer le dialogue après le push et ouvrir la PR plus tard.

- [x] `gh pr create` reçoit `--head`, `--base`, le titre, le corps sur l'entrée standard et `--draft` si coché (faux `gh`).
- [x] Brouillon de description construit à partir de la mission, sans signature publicitaire.
- [x] Numéro et adresse relus depuis la sortie de `gh` et sauvegardés ; état retrouvé après redémarrage.
- [ ] Test sans fenêtre du parcours bouton → push → PR (reporté au ticket 09, avec le parcours entier) ; captures FR faites.

## Comments

16 novembre 2026, branche `github-v1`. Cœur : `Gh::create_pr` (`NewPr` : head, base, titre, corps sur l'entrée standard, brouillon ; numéro et adresse relus), `Gh::branch_exists` (`gh api … --hostname`, segment encodé), `free_branch` et `CommitError::BranchExists` (une branche déjà présente dans le dépôt n'est ni reprise ni proposée : `-2`, `-3`…). Mission : `publication.pr`, `publication.base`, `replaces` (posé à la relance depuis la PR de la tentative précédente). Interface : étape 2 du dialogue (titre, description préremplie : plan, validation, relecture, « Remplace #n », agent ; base = branche de lancement si GitHub la connaît, sinon branche par défaut ; brouillon), ligne d'état en tête de la revue (PR #n ouverte, branche → base, Ouvrir sur GitHub ; ou branche poussée sans PR avec « Ouvrir la PR »), état de la barre de mission, icône de PR dans la barre latérale. Vu à l'écran (1024 FR) avec un faux `gh` (rien sur GitHub) et un push vers un dépôt local : nom de branche pris → `-2` proposé, étape 2, PR #12 affichée partout ; arguments et corps reçus par `gh` vérifiés dans son journal.
