Status: resolved
Blocked by: 09 — Checks et corrections

# 10 : Fin de vie et tentatives

Spec : `docs/tickets/github/spec.md`.

**What to build :** Quand la lecture voit la PR mergée ou fermée, la mission s'affiche « Mergée » ou « PR fermée » et Solar propose de retirer la copie de travail, sans le faire seul. Abandonner la copie d'une mission dont la PR est ouverte rappelle que la PR reste ouverte et propose « Fermer aussi la PR #n », décoché par défaut (`gh pr close`, sans supprimer la branche). Une tentative n publie sur sa propre branche et sa PR porte « Remplace #n » quand une tentative précédente a ouvert une PR. « Appliquer au projet » reste possible indépendamment.

- [x] États « Mergée » et « PR fermée » déduits de la publication, sans nouvel état de mission dans le stockage.
- [x] Retrait de la copie proposé, jamais automatique ; branche distante jamais supprimée.
- [x] Case de fermeture de la PR décochée par défaut ; `gh pr close` sans `--delete-branch`.
- [x] Branche `-<n>` et « Remplace #n » vérifiés par test.

## Comments

16 novembre 2026, branche `github-v1`. Cœur : `PrState` (ouverte, mergée, fermée ; gardé dans `PullRequest`, ouverte par défaut pour un ancien fichier), `Gh::pr_state`, `Gh::close_pr` (sans `--delete-branch`). Interface : l'état de la PR est relu avec les checks ; « PR #n mergée » ou « fermée » dans la revue et la barre de mission ; une PR mergée ou fermée dont la copie est encore sur le disque propose « Retirer la copie » (la mission et sa publication restent) ; l'abandon d'une copie publiée rappelle que la PR reste ouverte et propose « Fermer aussi la PR #n », décoché par défaut, remis à zéro si l'on annule. Tentatives : branche `-<n>` et « Remplace #n » vérifiés par un test. Le test du parcours entier vérifie aussi la PR vue mergée et le retrait de la copie. Corrigé au passage : la confirmation d'abandon débordait sur la liste des fichiers ; une revue dont la copie a été retirée sans résultat appliqué restait sur « Comparaison… », elle le dit maintenant. Vu à l'écran (1024 FR) avec le faux `gh` : abandon avec fermeture de la PR (appel reçu par `gh`), PR mergée puis copie retirée.
