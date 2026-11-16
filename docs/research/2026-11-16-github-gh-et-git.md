# GitHub par `gh` et `git` : ce que Solar peut utiliser (16 novembre 2026)

Relevé sur la machine de Solaire : `gh` 2.103.3, `git` 2.56.0, macOS. Commandes lues dans l'aide de `gh` et essayées en lecture seule, plus un essai de commit dans un dépôt jetable (`/tmp/solar-qa/gitspike`).

## Compte

- `gh auth status --json hosts` : les comptes connectés par hôte et le compte actif, sans jeton (ne jamais passer `--show-token`). Deux comptes sont connectés (BySolaire actif, ItsSolaire).
- Le `git push` de Solaire passe par le trousseau macOS (`credential.helper=osxkeychain`, dans `/opt/homebrew/etc/gitconfig`), qui peut tenir un autre compte que celui de `gh`. Pour pousser avec le compte actif de `gh` sans rien écrire dans sa configuration : `git -c credential.helper= -c 'credential.helper=!gh auth git-credential' push origin <branche>`.

## Dépôt

- `gh repo view --json nameWithOwner,defaultBranchRef,viewerPermission,isFork,parent,url` : la branche par défaut, le droit du compte (`ADMIN`, `WRITE`… ; `READ` ne permet pas de pousser) et s'il s'agit d'un fork. Essayé sur `BySolaire/solar` : `main`, `ADMIN`, pas un fork.

## Pull request

- `gh pr create --head <branche> --base <base> --title … --body-file - [--draft]` : `--head` évite toute question et tout fork automatique ; la branche doit déjà être poussée. Le corps passe par l'entrée standard. L'adresse de la PR s'affiche en sortie.
- `gh pr view <n> --json number,url,state,isDraft,headRefOid,mergedAt,closedAt,statusCheckRollup` : l'état (`OPEN`, `MERGED`, `CLOSED`) et les checks.

## Checks

- `gh pr checks <n> --json name,state,bucket,link,workflow,startedAt,completedAt` : `bucket` classe chaque check en `pass`, `fail`, `pending`, `skipping` ou `cancel`. Code de sortie 8 tant que des checks sont en attente : ce n'est pas une erreur.
- Le `link` d'un check GitHub Actions contient `/actions/runs/<run>/job/<job>` ; `gh run view --job <job> --log-failed` donne le journal des étapes en échec.

## Commit de la mission

Essayé dans un dépôt jetable : un fichier modifié par l'utilisateur sans commit, une copie détachée qui le reprend, puis la mission modifie ce fichier, en modifie un autre et en crée un.

```sh
git switch -c solar/essai                 # dans la copie
GIT_INDEX_FILE=<tmp> git read-tree HEAD
GIT_INDEX_FILE=<tmp> git add -- <fichiers de la mission>
GIT_INDEX_FILE=<tmp> git commit -m "…"
```

Résultat : le commit ne contient que les trois fichiers de la mission ; le hook `pre-commit` de l'utilisateur tourne et voit ces seuls fichiers indexés ; dans le projet, branche courante, index et fichiers sont inchangés, et la branche `solar/essai` apparaît dans `git branch`. Le fichier déjà modifié par l'utilisateur part avec ses deux modifications (voir l'ADR 0001).
