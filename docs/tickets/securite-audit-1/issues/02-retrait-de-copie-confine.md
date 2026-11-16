Status: ready-for-agent
Blocked by: None (can start immediately)

# 02 : Retirer une copie sans demander à git ce qu'il faut supprimer

Source : `docs/tickets/securite-audit-1/README.md`.

Piste `solar-core/checkpoint/remove_worktree-deletes-git-reported-toplevel` : `remove_worktree` supprime le dossier que `git rev-parse --show-toplevel` désigne dans la copie ; un `.git` réécrit par l'agent y désigne un dossier extérieur, supprimé en entier (reproduit côté Solar).

**What to build :** la racine à retirer se déduit du chemin seul : le dossier `worktrees/<id>` qui contient la copie, sous le dossier des copies de Solar ; rien hors de ce dossier n'est retiré.

- [ ] Test : un `.git` de copie qui désigne un dossier extérieur ne fait rien supprimer hors des copies.
- [ ] Test : le retrait normal marche toujours (copie à la racine et dans un sous-dossier).
