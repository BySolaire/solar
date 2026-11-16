Status: ready-for-agent

# Spec : publier une mission sur GitHub (v1)

Carte et décisions : `docs/tickets/github/map.md`. Vocabulaire : `docs/design/GLOSSARY.md` (publication, branche de mission, copie de travail, instantané, tentative). Décision d'architecture : `docs/design/adr/0001-commit-publie-hors-index-utilisateur.md`. Faits sur `gh` et `git` : `docs/research/2026-11-16-github-gh-et-git.md`.

## Problem Statement

Quand une mission a produit un résultat que j'ai examiné dans SOLAR, je dois quitter l'app pour en faire quelque chose sur GitHub : créer une branche, choisir à la main les fichiers de la mission sans embarquer mes propres changements non commités, écrire le message, pousser, ouvrir la pull request, puis surveiller ses checks dans le navigateur et recopier à l'agent ce qui a échoué. Je perds le lien entre la mission, sa revue et la PR, et je risque de publier des fichiers qui ne viennent pas de la mission.

## Solution

Depuis la vue Modifications d'une mission qui travaille dans sa copie de travail, « Publier sur GitHub » ouvre un dialogue en deux étapes, chacune confirmée : « Commit et push » (branche de mission, fichiers de la mission, message), puis « Ouvrir la PR » (titre, description, branche visée, brouillon ou non). SOLAR construit le commit avec les seuls changements de la mission, sur le commit de départ, sans toucher à la branche, à l'index ni aux fichiers du projet, lance les hooks git de l'utilisateur et pousse avec le compte actif de `gh`. La PR ouverte, son état et ses checks s'affichent en tête de la revue et dans la barre de mission ; un check en échec montre la fin de son journal et peut être envoyé à l'agent comme correction, que l'on publie ensuite par un nouveau « Commit et push ». Rien n'est jamais automatique : accepter une mission ne publie rien, SOLAR ne force jamais un push, ne merge pas et ne supprime aucune branche distante.

## User Stories

1. En tant qu'utilisateur, je veux un bouton « Publier sur GitHub » dans la vue Modifications d'une mission, pour envoyer le résultat examiné sans quitter SOLAR.
2. En tant qu'utilisateur, je veux que ce bouton soit grisé avec la raison quand la publication est impossible (pas de copie de travail, pas de remote `origin` sur GitHub, fork, droit en lecture seule, `gh` absent ou déconnecté), pour savoir quoi corriger.
3. En tant qu'utilisateur sans `gh` connecté, je veux voir la commande à lancer pour me connecter, comme pour les agents dans les réglages.
4. En tant qu'utilisateur, je veux un dialogue en deux étapes affichées en tête (1 Commit et push, 2 Pull request), pour savoir où j'en suis.
5. En tant qu'utilisateur, je veux que la branche proposée soit `solar/<titre-de-la-mission>`, modifiable, pour la reconnaître sur GitHub.
6. En tant qu'utilisateur, je veux qu'une tentative n publie sur `solar/<titre>-<n>`, pour ne jamais devoir forcer un push sur la branche d'une tentative précédente.
7. En tant qu'utilisateur, je veux voir la liste des fichiers de la mission avec leurs `+n −m`, pour savoir ce qui partira.
8. En tant qu'utilisateur, je veux qu'un fichier que j'avais modifié sans commit avant le lancement, puis que la mission a touché, soit décoché par défaut et expliqué, pour ne pas publier mes propres changements sans le décider.
9. En tant qu'utilisateur, je veux pouvoir cocher ce fichier pour l'inclure en connaissance de cause.
10. En tant qu'utilisateur, je veux pouvoir décocher n'importe quel fichier de la mission, pour publier une partie du résultat.
11. En tant qu'utilisateur, je veux un message de commit prérempli avec le titre de la mission, modifiable.
12. En tant qu'utilisateur, je veux lire, avant de confirmer, la branche de départ, le compte GitHub utilisé, que mes hooks seront lancés et que rien ne sera poussé de force.
13. En tant qu'utilisateur, je veux que mes hooks git (`pre-commit`, `commit-msg`, `pre-push`) tournent sur ce commit, pour qu'il passe les mêmes contrôles qu'un commit manuel.
14. En tant qu'utilisateur, je veux voir la sortie d'un hook en échec en tête du dialogue, avec « Réessayer », et savoir que rien n'a été commité ni poussé.
15. En tant qu'utilisateur, je veux que ma branche courante, mon index et mes fichiers ne changent pas quand SOLAR publie, pour continuer mon propre travail en parallèle.
16. En tant qu'utilisateur, je veux que le push parte avec le compte actif de `gh`, même si mon trousseau tient un autre compte, pour que la PR soit à mon nom.
17. En tant qu'utilisateur, je veux qu'un push refusé (branche distante déplacée, droits insuffisants, réseau) soit expliqué en clair, sans nouvel essai forcé.
18. En tant qu'utilisateur, après le push, je veux voir « Poussé sur <branche> · n commits » puis l'étape Pull request.
19. En tant qu'utilisateur, je veux un titre de PR prérempli avec le titre de la mission, modifiable.
20. En tant qu'utilisateur, je veux une description préremplie avec le plan approuvé, l'état des tests et le verdict du relecteur s'il y en a un, l'agent nommé, sans signature publicitaire.
21. En tant qu'utilisateur, je veux que la PR d'une tentative porte « Remplace #n » quand une tentative précédente a ouvert une PR.
22. En tant qu'utilisateur, je veux que la PR vise par défaut la branche où j'étais au lancement si elle existe sur GitHub, sinon la branche par défaut du dépôt, et pouvoir la changer.
23. En tant qu'utilisateur, je veux une case « Brouillon », décochée par défaut.
24. En tant qu'utilisateur, je veux pouvoir fermer le dialogue entre les deux étapes et revenir plus tard ouvrir la PR d'une branche déjà poussée.
25. En tant qu'utilisateur, une fois la PR ouverte, je veux une ligne d'état en tête de la revue : numéro, branche → base, « Ouvrir sur GitHub ».
26. En tant qu'utilisateur, je veux que la barre de mission dise « PR #n ouverte » et l'état des checks, et que la barre latérale garde un point d'état.
27. En tant qu'utilisateur, je veux la liste des checks avec leur état (réussi, en échec, en cours, ignoré, annulé) et leur durée.
28. En tant qu'utilisateur, je veux qu'un check en échec montre les 20 dernières lignes de son journal et un lien vers le journal complet sur GitHub.
29. En tant qu'utilisateur, je veux « Envoyer l'échec à l'agent », qui transmet le nom du check et son journal comme correction dans la conversation de la mission.
30. En tant qu'utilisateur, je veux publier la correction de l'agent par un nouveau « Commit et push » sur la même branche, qui ajoute un commit à la PR.
31. En tant qu'utilisateur, je veux que la conversation reste active dans sa copie pendant que la PR est ouverte.
32. En tant qu'utilisateur, je veux que les checks se rafraîchissent seuls toutes les 30 s tant que certains sont en cours, pour une mission ouverte dans un onglet, et un bouton « Rafraîchir ».
33. En tant qu'utilisateur, je veux que rien ne soit interrogé quand aucune mission n'a de checks en cours, ou quand l'app est fermée.
34. En tant qu'utilisateur, je veux voir la mission passer à « Mergée » ou « PR fermée » quand c'est le cas sur GitHub.
35. En tant qu'utilisateur, je veux que SOLAR me propose alors de retirer la copie de travail, sans le faire seul.
36. En tant qu'utilisateur, je veux que « Appliquer au projet » reste possible indépendamment de la publication.
37. En tant qu'utilisateur, quand j'abandonne la copie d'une mission dont la PR est ouverte, je veux un rappel que la PR reste ouverte et une case « Fermer aussi la PR #n », décochée par défaut.
38. En tant qu'utilisateur, je veux que SOLAR ne supprime jamais une branche distante, ne merge jamais et ne force jamais un push.
39. En tant qu'utilisateur, je veux que la fiche projet montre le compte GitHub utilisé et le dépôt `origin`.
40. En tant qu'utilisateur, je veux que la publication et l'état de la PR soient gardés dans la mission et retrouvés à la réouverture de SOLAR.
41. En tant qu'utilisateur anglophone, je veux tous ces textes en anglais.
42. En tant qu'utilisateur au clavier, je veux parcourir le dialogue, cocher les fichiers et confirmer au clavier, et que Échap le ferme sans rien faire.
43. En tant qu'utilisateur en fenêtre de 720 × 480, je veux un dialogue qui défile avec ses boutons fixés en bas.

## Implementation Decisions

- **Module du cœur `github`** dans `solar-core`, sans dépendance UI. Il expose : l'état du dépôt pour la publication (remote `origin` GitHub, `nameWithOwner`, branche par défaut, droit du compte, fork, compte actif de `gh`) ; la construction du commit de mission ; le push ; la création de la PR ; la lecture de l'état de la PR et de ses checks ; le journal d'un check en échec ; la fermeture d'une PR. Chaque opération lance `git` ou `gh` sans shell, en arguments séparés, et rend une erreur lisible (sortie d'erreur du programme).
- **Commit de mission** (ADR 0001) : dans la copie de travail, créer ou reprendre la branche de mission (`git switch -c` la première fois), charger un index temporaire depuis le commit de départ pour le premier commit, puis depuis la tête de la branche pour les suivants, n'y ajouter que les fichiers choisis (suppressions comprises), `git commit` avec ce même index (hooks lancés). Jamais `--no-verify`, jamais d'amend.
- **Push** : `git push origin <branche>` depuis la copie, avec `-c credential.helper= -c credential.helper=!gh auth git-credential` pour pousser avec le compte actif de `gh`, sans écrire de configuration. Jamais `--force`. Un refus non fast-forward devient l'erreur « la branche a bougé sur GitHub ».
- **Fichiers modifiés avant le lancement** : ceux où le commit de départ et l'instantané de lancement diffèrent. Ils sont proposés décochés, avec une explication.
- **Commit de départ** : la mission note, au lancement dans sa copie, le commit sur lequel la copie a été créée. Pour une mission plus ancienne, sans cette information, il est lu dans la copie tant qu'aucune branche de mission n'y a été créée.
- **PR** : `gh pr create --head <branche> --base <base> --title … --body-file -` (`--draft` si la case est cochée), lancé dans la copie ; l'adresse rendue est lue pour le numéro. Base par défaut : la branche de l'utilisateur au lancement si elle existe sur `origin`, sinon la branche par défaut.
- **Checks** : `gh pr checks <n> --json name,state,bucket,link,workflow,startedAt,completedAt` (le code 8, checks en attente, n'est pas une erreur) et `gh pr view <n> --json state,url,isDraft,headRefOid,mergedAt,closedAt` pour l'état. Journal d'un check GitHub Actions : identifiant du job tiré de son lien, `gh run view --job <id> --log-failed`, 20 dernières lignes.
- **Données de la mission** : un champ optionnel `publication` (absent d'un ancien fichier, aucune migration) : branche, base, commit de départ, dernier commit poussé, et la PR (numéro, adresse, état ouvert, mergé ou fermé, brouillon). Les checks ne sont pas sauvegardés : relus à l'ouverture. Aucun nouvel état de mission dans le stockage : « PR ouverte », « Mergée » et « PR fermée » se déduisent de la publication et s'affichent à la place de l'état courant.
- **Interface** : le dialogue suit le prototype retenu (ticket « Aperçu de publication ») : deux étapes en tête, champs branche, fichiers, message, puis titre, description, base, brouillon ; boutons fixés en bas ; avertissements en blanc avec icône, rouge pour les erreurs, vert pour le succès, aucun jaune. Une fois la PR ouverte : ligne d'état et checks en tête de la revue, état dans la barre de mission, point d'état dans la barre latérale. Composants gpui-kit existants (Input, Checkbox, Button, TextView) ; textes FR/EN dans les locales.
- **Rafraîchissement** : une tâche par mission ouverte dans un onglet, active seulement quand au moins un check est en cours ou après un push, toutes les 30 s ; arrêtée à la fermeture de l'onglet ou quand tous les checks sont finis ; « Rafraîchir » relance une lecture.
- **Envoyer l'échec à l'agent** : comme « Envoyer à l'implémenteur » du relecteur, un message de correction qui cite le check et son journal.
- **Abandon d'une copie publiée** : la confirmation existante gagne le rappel et la case « Fermer aussi la PR #n » (`gh pr close`, sans `--delete-branch`).
- **Fiche projet** : une ligne GitHub (compte actif, `origin`), lue en arrière-plan comme l'état git.
- **Sécurité** : aucun jeton lu, stocké ni affiché (`gh auth status` sans `--show-token`) ; aucun hook désactivé ; textes saisis passés en arguments ou sur l'entrée standard, jamais interprétés par un shell ; aucun effet sur la configuration git de l'utilisateur.

## Testing Decisions

- Un bon test vérifie ce que voit l'utilisateur ou ce qui arrive au dépôt (contenu du commit, branche et index de l'utilisateur intacts, commande reçue par `gh`), pas la forme du code.
- **Seam principal, dans le cœur** : le module `github` face à de vrais dépôts git jetables, avec un dépôt nu comme `origin` et un faux `gh` en tête de PATH qui note ses arguments et rend des réponses enregistrées. Cas : commit limité aux fichiers choisis sur le commit de départ ; fichier modifié avant le lancement repéré ; hook `pre-commit` lancé et son échec remonté sans commit ; dépôt de l'utilisateur intact octet pour octet (branche, index, fichiers) ; deuxième commit sur la même branche ; push refusé quand la branche distante a bougé, sans force ; lecture des checks (réussi, échec, en attente avec code 8) et du journal d'un échec ; création de la PR avec titre, corps sur l'entrée standard, base et brouillon. Antériorité : les tests de `checkpoint` (dépôts réels jetables) et les faux CLI des tests de Codex et Grok.
- **Seam de l'interface** : un test sans fenêtre du parcours dans la vue de mission (bouton, dialogue, commit et push, PR, checks en échec, envoi à l'agent), avec les mêmes faux ; antériorité : les tests de revue et de mission Codex dans la vue, sous le verrou de PATH.
- Lecture d'un ancien `state.json` sans `publication` : test du stockage.
- Essai réel sur un dépôt GitHub privé d'essai, créé avec l'accord de Solaire, jamais sur ses dépôts de travail.

## Out of Scope

- Issues GitHub (liste dans la fiche projet, mission depuis une issue), commentaires de revue renvoyés à l'agent, texte de la PR rédigé par l'agent, suivi des checks des missions qui ne sont pas ouvertes dans un onglet : plus tard, voir la carte.
- Cloner un dépôt, forks, GitLab, dépôts sans remote GitHub.
- Force-push, amend, rebase, merge, suppression de branche distante, déploiement.
- Missions sans copie de travail (dossier sans git ou sans commit).

## Further Notes

- `gh` 2.103.3 et `git` 2.56.0 ont servi aux essais ; vérifier les versions minimales au moment de l'implémentation.
- Revue `security-audit` ciblée à faire sur le lancement de `gh` et `git`, le réseau et l'écriture dans le dépôt.
- La branche locale `prototype/apercu-publication` garde le prototype ; elle n'est jamais fusionnée.
