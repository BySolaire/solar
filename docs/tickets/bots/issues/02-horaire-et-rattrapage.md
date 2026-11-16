Status: resolved
Blocked by: 01 — Bot, vue Bots et « Lancer maintenant »

# 02 : Horaire et rattrapage

Spec : `docs/tickets/bots/spec.md`.

**What to build :** Un déclencheur horaire (chaque jour à une heure, jours ouvrés, toutes les N heures) ; le planificateur du shell évalue les bots chaque minute, app ouverte ; la vue montre la prochaine exécution. Au lancement de Solar, les exécutions manquées sont dites (« 2 exécutions manquées depuis hier ») avec « Lancer une fois » ; rien ne part seul.

- [x] Tests du cœur : prochaine échéance (jour, jours ouvrés, intervalle, passage de minuit), exécutions manquées.
- [x] Test sans fenêtre : une échéance passée lance une exécution, une seule.

## Comments

16 novembre 2026, branche `bots-v1`. Cœur : `Trigger::Daily { time, weekdays_only }` et `Trigger::Every { hours }` (heures fixes depuis minuit), `next_due` et `due_between` (plafonné à 100), `Bot.checked` (jusqu'où l'horaire a été regardé) et `Bot.missed`. Shell : au lancement, `catch_up_bots` compte les échéances passées Solar fermé sans rien lancer ; toutes les 30 secondes, `tick_bots` lance une fois un bot actif dont une échéance est passée (une seule même après une veille). Un horaire neuf ou changé, ou un bot réactivé, part de maintenant. Vue : déclencheur choisi dans l'éditeur (Manuel, Chaque jour, Jours ouvrés avec l'heure, À intervalle de 1 à 12 h), heure vérifiée ; ligne « prochaine : … » ; bandeau « Manquées pendant que Solar était fermé : N. Rien n'est parti seul. » avec « Lancer une fois » et « Ignorer ».

Vérifié : quatre tests du cœur (jour, jours ouvrés, intervalle, passage de minuit, heure invalide, comptage plafonné, relecture d'un bot du ticket 01) et un sans fenêtre (manquées comptées sans lancement, une seule exécution par échéance, rien en pause) ; à l'écran en 1024 FR (bandeau, « Lancer une fois », éditeur). Captures : `/tmp/solar-qa/captures/bots-02/`.

Limites : les manquées ne se voient que dans la vue Bots (le point d'état de la barre latérale vient au ticket 04) ; un changement d'heure (été/hiver) peut décaler une échéance d'une heure.
