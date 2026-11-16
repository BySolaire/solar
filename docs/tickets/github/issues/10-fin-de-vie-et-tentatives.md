Status: ready-for-agent
Blocked by: 09 — Checks et corrections

# 10 : Fin de vie et tentatives

Spec : `docs/tickets/github/spec.md`.

**What to build :** Quand la lecture voit la PR mergée ou fermée, la mission s'affiche « Mergée » ou « PR fermée » et SOLAR propose de retirer la copie de travail, sans le faire seul. Abandonner la copie d'une mission dont la PR est ouverte rappelle que la PR reste ouverte et propose « Fermer aussi la PR #n », décoché par défaut (`gh pr close`, sans supprimer la branche). Une tentative n publie sur sa propre branche et sa PR porte « Remplace #n » quand une tentative précédente a ouvert une PR. « Appliquer au projet » reste possible indépendamment.

- [ ] États « Mergée » et « PR fermée » déduits de la publication, sans nouvel état de mission dans le stockage.
- [ ] Retrait de la copie proposé, jamais automatique ; branche distante jamais supprimée.
- [ ] Case de fermeture de la PR décochée par défaut ; `gh pr close` sans `--delete-branch`.
- [ ] Branche `-<n>` et « Remplace #n » vérifiés par test.
