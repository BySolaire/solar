# Le commit publié se construit dans la copie, avec un index à part

Publier une mission demande un vrai commit, mais la copie de travail reprend aussi les changements non commités de l'utilisateur. Solar crée donc la branche de mission dans la copie, puis construit le commit avec un index temporaire (`GIT_INDEX_FILE`) qui part du commit de départ et ne reçoit que les fichiers de la mission ; `git commit` lance les hooks de l'utilisateur sur cet index. Le dépôt de l'utilisateur ne voit qu'une nouvelle branche : sa branche courante, son index et ses fichiers ne changent pas.

## Options écartées

- Committer dans le projet après « Appliquer » : touche la branche et l'index de l'utilisateur, et mêle ses changements à ceux de la mission.
- Committer tout l'état de la copie : la PR emporterait les changements non commités de l'utilisateur.
- Construire l'arbre à la main (`write-tree`, `commit-tree`) : aucun hook ne serait lancé, contre la décision de faire passer un commit publié par les mêmes contrôles qu'un commit manuel.

## Conséquences

- Un fichier que l'utilisateur avait modifié sans commit avant le lancement, puis que la mission a touché, part avec ses deux modifications : l'aperçu le signale et demande une confirmation.
- La branche de mission apparaît dans `git branch` du projet, comme toute branche d'un worktree.
