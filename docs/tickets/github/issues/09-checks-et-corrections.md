Status: resolved
Blocked by: 08 — Ouvrir la PR

# 09 : Checks et corrections

Spec : `docs/tickets/github/spec.md`.

**What to build :** Sous la ligne d'état de la PR, la liste des checks avec leur état (réussi, en échec, en cours, ignoré, annulé) et leur durée. Rafraîchissement toutes les 30 s tant qu'un check est en cours, pour une mission ouverte dans un onglet, et bouton « Rafraîchir » ; rien n'est interrogé sinon. Un check en échec montre les 20 dernières lignes de son journal et un lien vers le journal complet. « Envoyer l'échec à l'agent » transmet le check et son journal comme correction ; la correction se publie par un nouveau « Commit et push » sur la même branche. La barre de mission résume l'état des checks.

- [x] Lecture des checks avec `bucket` ; le code 8 (en attente) n'est pas une erreur.
- [x] Journal d'un échec GitHub Actions lu par l'identifiant du job tiré de son lien.
- [x] Le rafraîchissement s'arrête quand tous les checks sont finis ou que l'onglet se ferme.
- [x] Envoi à l'agent puis nouveau commit sur la même branche (test sans fenêtre avec faux `gh` et dépôt nu).

## Comments

16 novembre 2026, branche `github-v1`. Cœur : `Gh::checks` (`Check`, `Bucket`, durée ; code 8 et « no checks reported » ne sont pas des erreurs), `Gh::failed_log` (job tiré du lien, préfixes job/étape/horodatage retirés, n dernières lignes). Interface : liste des checks sous la ligne de la PR (icône, nom, durée ou « en cours »), journal des échecs déplié (20 lignes), « Envoyer l'échec à l'agent », « Journal complet », « Rafraîchir » ; relecture toutes les 30 s tant qu'un check est en cours (tâche gardée par la vue, arrêtée quand tout est fini ou avec la vue) ; relecture après l'ouverture de la PR, après chaque push sur une PR et à la réouverture d'une mission publiée ; résumé des checks dans la barre de mission. Test sans fenêtre du parcours entier (dépôt nu local, faux `gh`, faux `claude`) : publication du seul fichier de la mission au nom de l'utilisateur, branche de l'utilisateur intacte, PR #12 vers sa branche de lancement, checks et journal lus, échec envoyé à l'agent ; il couvre aussi les critères d'interface laissés ouverts aux tickets 07 et 08. Vu à l'écran (1024 FR) avec le faux `gh`.
