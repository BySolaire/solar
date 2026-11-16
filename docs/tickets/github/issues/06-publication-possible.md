Status: ready-for-agent
Blocked by: None (can start immediately)

# 06 : Publication possible ?

Spec : `docs/tickets/github/spec.md`.

**What to build :** Dans la vue Modifications d'une mission qui travaille dans sa copie de travail, un bouton « Publier sur GitHub » apparaît à côté des décisions. Il est grisé, avec la raison en infobulle, quand la publication est impossible : pas de copie de travail, pas de remote `origin` sur GitHub, fork, droit en lecture seule, `gh` absent ou déconnecté (la commande à lancer est alors indiquée). La fiche projet gagne une ligne GitHub : compte actif de `gh` et dépôt `origin`. Le clic n'ouvre encore qu'un dialogue vide (étape 1 à venir).

- [ ] Le cœur lit l'état GitHub d'un projet (remote `origin`, dépôt, branche par défaut, droit, fork, compte actif) en arrière-plan, sans jamais lire de jeton.
- [ ] Chaque raison d'impossibilité a son texte FR/EN.
- [ ] Tests du cœur avec un dépôt jetable et un faux `gh` : dépôt poussable, fork, lecture seule, sans remote, `gh` déconnecté.
- [ ] Bouton visible et grisé à l'écran selon le cas, fiche projet avec la ligne GitHub (captures 1024 et 720, FR/EN).
