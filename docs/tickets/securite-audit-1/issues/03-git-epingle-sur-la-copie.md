Status: resolved
Blocked by: None (can start immediately)

# 03 : Git épinglé sur la copie, jamais découvert depuis son `.git`

Source : `docs/tickets/securite-audit-1/README.md`.

Piste `solar-core/github/commit_mission:copy-gitfile-config-exec` : git trouve son dépôt et sa configuration par le `.git` de la copie, que l'agent peut réécrire avec de simples fichiers texte ; `core.fsmonitor` ou `core.hooksPath` y lancent alors une commande dès `files::list` ou au commit (reproduit côté Solar).

**What to build :** une fonction du cœur qui, pour une copie, lit son `.git`, exige un fichier qui désigne un dossier d'administration hors de la copie dont le pointeur `gitdir` revient sur cette copie, et rend `GIT_DIR` et `GIT_WORK_TREE` ; sinon refuse (« copie altérée »). Toutes les commandes git et `gh` lancées dans une copie (commit, branche, push, PR) passent par elle. `files::list` ajoute `core.fsmonitor=false`.

- [x] Test : un `.git` réécrit (gitdir dans la copie, `core.fsmonitor`) ne lance rien et le commit est refusé.
- [x] Test : le commit, le push (faux `gh`) et la PR marchent toujours sur une copie intacte.

## Answer

- **`checkpoint::CopyGit::of`** : le `.git` d'une copie doit être un fichier (pas un lien ni un dossier), désigner un dossier d'administration absolu hors de la copie, dont le fichier `gitdir` revient sur cette copie. Sinon, erreur « copie altérée ». `CopyGit::pin` fixe `GIT_DIR` et `GIT_WORK_TREE`.
- **Publication** : `commit_mission`, `Gh::push` et `Gh::create_pr` (`gh` lance git) passent par le dépôt épinglé. Une copie altérée donne `CommitError::Altered` et `PushError::Altered`, avec un message FR/EN `publish.error.altered`.
- **Lectures** : `read_git` ajoute `core.fsmonitor=false`. `files::list` épingle une copie intacte et désactive toujours `core.fsmonitor`.
- **Agents** : `pin_for_agent` passe le dépôt épinglé aux processus Claude, Codex et Grok lancés dans une copie, dont les propres appels à git ne suivent plus un `.git` réécrit. Une copie altérée est refusée au lancement.
- **Tests** : `crates/solar-core/tests/github.rs` (copie altérée en texte seul, rien n'est lancé ni publié ; `.git` devenu dossier) et `crates/solar-core/tests/checkpoint.rs` (`pin_for_agent`). Les tests existants (commit, push avec faux `gh`, PR, hooks voulus de l'utilisateur) passent.
- **Non vérifié** : le comportement des vrais CLI d'agents avec `GIT_DIR` épinglé.
