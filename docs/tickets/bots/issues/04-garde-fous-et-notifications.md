Status: resolved
Blocked by: 02 — Horaire et rattrapage ; 03 — Issues GitHub

# 04 : Garde-fous et notifications

Spec : `docs/tickets/bots/spec.md`.

**What to build :** Une exécution à la fois par bot (un déclenchement pendant qu'une exécution attend la revue est sauté et noté), plafond quotidien réglable (3 par défaut), coût par exécution ; notification macOS à la fin d'une exécution ou quand elle attend une réponse (sans effet hors de l'app empaquetée), point d'état sur l'entrée « Bots » et sur la mission. Docs à jour.

- [x] Tests du cœur : décisions de lancement et de saut, plafond.
- [x] Test sans fenêtre : notification envoyée, point d'état.
- [x] Plan de développement, design system et README à jour.

## Comments

16 novembre 2026, branche `bots-v1`. Cœur : `decide` (Go, Busy, Capped ; seules les exécutions parties aujourd'hui comptent), `Bot.daily_cap` (3 par défaut, absent du fichier à 3), `Bot::skip` (un saut répété n'est noté qu'une fois), `Run.skipped`, `Run.cost`. Shell : les déclencheurs (horaire, issues) passent par les garde-fous, « Lancer maintenant » non ; occupé = une exécution du bot qui démarre, planifie, attend son plan, travaille ou attend la revue (mission ni acceptée, ni annulée, ni restaurée). `watch_bot_runs` à chaque synchronisation : une exécution qui passe de « au travail » à « fini » ou « t'attend » envoie `show_system_notification` (étiquette : la session ; un clic l'ouvre) et met un point sur l'entrée « Bots » jusqu'à son ouverture ; le coût cumulé annoncé par l'agent est noté sur l'exécution. `set_app_identity("com.bysolaire.solar", "Solar")` au démarrage (sans effet sur macOS, où l'identité vient du paquet). Éditeur : « Par jour, au plus » pour un bot à déclencheur.

Vérifié : deux tests du cœur et un sans fenêtre (plan qui attend : notification et point ; deux échéances pendant l'attente : une seule exécution sautée notée, aucune autre lancée ; ouverture : le point s'efface) ; à l'écran en 1024 FR (point d'état, plafond). Docs : plan de développement, design system, glossaire, README.

Limites : notification invisible hors de l'app empaquetée ; le coût vient du dernier total annoncé par l'agent (Codex n'en donne pas).
