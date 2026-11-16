Status: resolved
Blocked by: 06 — Publication possible ?

# 07 : Commit et push

Spec : `docs/tickets/github/spec.md`.

**What to build :** L'étape 1 du dialogue fonctionne de bout en bout : branche de mission proposée (`solar/<titre>`, `-<n>` pour une tentative n), fichiers de la mission avec `+n −m`, fichiers que l'utilisateur avait modifiés avant le lancement décochés et expliqués, message prérempli, rappel du compte et des hooks. « Commit et push » construit le commit dans la copie avec un index temporaire (ADR 0001), lance les hooks, pousse avec le compte actif de `gh`, sans force. Un hook en échec ou un push refusé s'affiche en clair, rien n'est poussé. La publication (branche, base, commit de départ, dernier commit poussé) est gardée dans la mission ; la mission note son commit de départ à son lancement.

- [x] Commit limité aux fichiers cochés, posé sur le commit de départ ; deuxième commit sur la même branche.
- [x] Branche, index et fichiers de l'utilisateur intacts octet pour octet (test sur dépôt jetable).
- [x] Hook `pre-commit` lancé ; son échec remonté sans commit ni push.
- [x] Push vers un dépôt nu `origin` avec l'aide d'identification de `gh`, jamais `--force` ; refus non fast-forward expliqué.
- [x] Ancien `state.json` sans publication relu sans perte.
- [x] Dialogue au clavier, Échap ferme sans rien faire, boutons fixés en bas en 720 × 480, FR/EN.

## Comments

16 novembre 2026, branche `github-v1`. Cœur : `launch_state` (commit de départ, branche, fichiers modifiés par l'utilisateur, noté au lancement de chaque copie), `commit_mission` (index temporaire, hooks, `BadBranch`, `Nothing`, `Failed`), `Gh::push` (aide d'identification de `gh`, `--porcelain`, jamais de force, `Moved`), `branch_head`, `branch_name` ; mission : `launch` et `publication` (branche, commits, dernier poussé), absents d'un ancien fichier. Tests sur dépôts jetables avec un dépôt nu comme `origin` : commit limité aux fichiers choisis (ajout, modification, suppression) sur le commit de départ, auteur = identité de l'utilisateur, projet intact, second commit, rien de neuf, hook en échec, branche refusée, push puis refus quand la branche a bougé ailleurs (rien d'écrasé). Interface : étape 1 du dialogue (branche figée après le premier push, fichiers, message, rappel du compte et des hooks), erreurs en chasse fixe, succès « Poussé sur … · n commit(s) » ; un push qui avait échoué après un commit est repoussé au nouvel essai. Vu à l'écran (1024 FR, 720 EN) avec un `origin` dont l'adresse de push est un dépôt nu local : README modifié avant le lancement décoché, seul `bonjour.txt` poussé, hook en échec affiché puis second commit poussé, projet de l'utilisateur intact. Le test sans fenêtre du parcours complet viendra avec le ticket 09, quand le parcours sera entier.
