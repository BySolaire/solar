Type: research
Status: resolved

## Question

Quelles commandes de `gh` et de `git` donnent, sans interaction et en JSON quand c'est possible, le compte, le dépôt et les droits, la création de la PR, son état, ses checks et le journal d'un check en échec ? Avec quel compte le push part-il ?

## Answer

Voir `docs/research/2026-11-16-github-gh-et-git.md`. L'essentiel : `gh repo view --json …viewerPermission,isFork,defaultBranchRef`, `gh pr create --head … --base … --body-file -`, `gh pr view --json state,statusCheckRollup,…`, `gh pr checks --json bucket,…` (code 8 = en attente), `gh run view --job <id> --log-failed`. Le `git push` de Solaire passe par le trousseau macOS, qui peut tenir un autre compte : SOLAR pousse avec `-c 'credential.helper=!gh auth git-credential'`, sans écrire de configuration, pour rester sur le compte actif de `gh`.
