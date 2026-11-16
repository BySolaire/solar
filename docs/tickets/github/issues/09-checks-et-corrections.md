Status: ready-for-agent
Blocked by: 08 — Ouvrir la PR

# 09 : Checks et corrections

Spec : `docs/tickets/github/spec.md`.

**What to build :** Sous la ligne d'état de la PR, la liste des checks avec leur état (réussi, en échec, en cours, ignoré, annulé) et leur durée. Rafraîchissement toutes les 30 s tant qu'un check est en cours, pour une mission ouverte dans un onglet, et bouton « Rafraîchir » ; rien n'est interrogé sinon. Un check en échec montre les 20 dernières lignes de son journal et un lien vers le journal complet. « Envoyer l'échec à l'agent » transmet le check et son journal comme correction ; la correction se publie par un nouveau « Commit et push » sur la même branche. La barre de mission résume l'état des checks.

- [ ] Lecture des checks avec `bucket` ; le code 8 (en attente) n'est pas une erreur.
- [ ] Journal d'un échec GitHub Actions lu par l'identifiant du job tiré de son lien.
- [ ] Le rafraîchissement s'arrête quand tous les checks sont finis ou que l'onglet se ferme.
- [ ] Envoi à l'agent puis nouveau commit sur la même branche (test sans fenêtre avec faux `gh` et dépôt nu).
