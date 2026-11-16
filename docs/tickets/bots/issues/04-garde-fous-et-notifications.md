Status: ready-for-agent
Blocked by: 02 — Horaire et rattrapage ; 03 — Issues GitHub

# 04 : Garde-fous et notifications

Spec : `docs/tickets/bots/spec.md`.

**What to build :** Une exécution à la fois par bot (un déclenchement pendant qu'une exécution attend la revue est sauté et noté), plafond quotidien réglable (3 par défaut), coût par exécution ; notification macOS à la fin d'une exécution ou quand elle attend une réponse (sans effet hors de l'app empaquetée), point d'état sur l'entrée « Bots » et sur la mission. Docs à jour.

- [ ] Tests du cœur : décisions de lancement et de saut, plafond.
- [ ] Test sans fenêtre : notification envoyée, point d'état.
- [ ] Plan de développement, design system et README à jour.
