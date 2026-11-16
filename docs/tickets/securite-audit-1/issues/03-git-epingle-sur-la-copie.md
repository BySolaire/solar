Status: ready-for-agent
Blocked by: None (can start immediately)

# 03 : Git épinglé sur la copie, jamais découvert depuis son `.git`

Source : `docs/tickets/securite-audit-1/README.md`.

Piste `solar-core/github/commit_mission:copy-gitfile-config-exec` : git trouve son dépôt et sa configuration par le `.git` de la copie, que l'agent peut réécrire avec de simples fichiers texte ; `core.fsmonitor` ou `core.hooksPath` y lancent alors une commande dès `files::list` ou au commit (reproduit côté Solar).

**What to build :** une fonction du cœur qui, pour une copie, lit son `.git`, exige un fichier qui désigne un dossier d'administration hors de la copie dont le pointeur `gitdir` revient sur cette copie, et rend `GIT_DIR` et `GIT_WORK_TREE` ; sinon refuse (« copie altérée »). Toutes les commandes git et `gh` lancées dans une copie (commit, branche, push, PR) passent par elle. `files::list` ajoute `core.fsmonitor=false`.

- [ ] Test : un `.git` réécrit (gitdir dans la copie, `core.fsmonitor`) ne lance rien et le commit est refusé.
- [ ] Test : le commit, le push (faux `gh`) et la PR marchent toujours sur une copie intacte.
