Status: resolved
Blocked by: None (can start immediately)

# 06 : Publication possible ?

Spec : `docs/tickets/github/spec.md`.

**What to build :** Dans la vue Modifications d'une mission qui travaille dans sa copie de travail, un bouton « Publier sur GitHub » apparaît à côté des décisions. Il est grisé, avec la raison en infobulle, quand la publication est impossible : pas de copie de travail, pas de remote `origin` sur GitHub, fork, droit en lecture seule, `gh` absent ou déconnecté (la commande à lancer est alors indiquée). La fiche projet gagne une ligne GitHub : compte actif de `gh` et dépôt `origin`. Le clic n'ouvre encore qu'un dialogue vide (étape 1 à venir).

- [x] Le cœur lit l'état GitHub d'un projet (remote `origin`, dépôt, branche par défaut, droit, fork, compte actif) en arrière-plan, sans jamais lire de jeton.
- [x] Chaque raison d'impossibilité a son texte FR/EN.
- [x] Tests du cœur avec un dépôt jetable et un faux `gh` : dépôt poussable, fork, lecture seule, sans remote, `gh` déconnecté.
- [x] Bouton visible et grisé à l'écran selon le cas, fiche projet avec la ligne GitHub (captures 1024 et 720, FR/EN).

## Comments

16 novembre 2026, branche d'intégration `github-v1` : module `solar_core::github` (`Gh::availability`, `remote_repo`) testé avec des dépôts jetables et un faux `gh` (compte actif, fork, lecture seule, sans remote, hôte inconnu, déconnecté, dépôt introuvable, adresse qui ressemblerait à une option). Bouton « Publier sur GitHub » dans la vue Modifications d'une mission en copie, grisé avec la raison en infobulle ; dialogue (étapes, dépôt et compte, Annuler, Échap) encore sans contenu ; ligne GitHub de la fiche projet. Vu à l'écran en 1024 FR et 720 EN avec un faux Claude qui planifie puis écrit (`/tmp/solar-qa/gh/bin/claude`) et `origin` sur un dépôt GitHub réel, en lecture seule. Corrigé au passage : la carte « Plan prêt » passe ses boutons sous le texte dans une vue étroite.
