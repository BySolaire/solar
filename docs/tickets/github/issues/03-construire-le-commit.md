Type: research
Status: resolved

## Question

Comment construire un commit qui ne contient que les changements de la mission, sur le commit de départ, en lançant les hooks de l'utilisateur, sans toucher à sa branche courante, à son index ni à ses fichiers, alors que la copie de travail reprend aussi ses changements non commités ?

## Answer

Dans la copie : `git switch -c solar/<titre>`, puis un index temporaire (`GIT_INDEX_FILE`) chargé depuis le commit de départ (`read-tree HEAD`), qui ne reçoit que les fichiers de la mission (`git add --`, suppressions comprises), et `git commit`. Essayé dans un dépôt jetable : commit limité aux fichiers de la mission, hook `pre-commit` lancé, dépôt de l'utilisateur inchangé hormis la nouvelle branche visible dans `git branch`. Décision et options écartées : `docs/design/adr/0001-commit-publie-hors-index-utilisateur.md`. Pour la spec : noter dans la mission le commit de départ de la copie, pour retrouver les fichiers que l'utilisateur avait modifiés avant le lancement.
