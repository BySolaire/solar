Status: ready-for-agent
Blocked by: 01 — Bot, vue Bots et « Lancer maintenant »

# 02 : Horaire et rattrapage

Spec : `docs/tickets/bots/spec.md`.

**What to build :** Un déclencheur horaire (chaque jour à une heure, jours ouvrés, toutes les N heures) ; le planificateur du shell évalue les bots chaque minute, app ouverte ; la vue montre la prochaine exécution. Au lancement de SOLAR, les exécutions manquées sont dites (« 2 exécutions manquées depuis hier ») avec « Lancer une fois » ; rien ne part seul.

- [ ] Tests du cœur : prochaine échéance (jour, jours ouvrés, intervalle, passage de minuit), exécutions manquées.
- [ ] Test sans fenêtre : une échéance passée lance une exécution, une seule.
