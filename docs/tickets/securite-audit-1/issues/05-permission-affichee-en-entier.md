Status: ready-for-agent
Blocked by: None (can start immediately)

# 05 : Une demande de permission montre ce qui sera approuvé

Source : `docs/tickets/securite-audit-1/README.md`.

Signalé par le critique de l'audit : `render_permission` n'affiche que la première ligne tronquée de `tool_summary` (commande, chemin…), alors qu'« Autoriser » approuve l'entrée complète (`answer_permission`).

**What to build :** la demande affiche la commande entière (toutes ses lignes, sans troncature silencieuse ; défilement si longue) et la description séparément.

- [ ] Vu à l'écran, FR et EN, avec une commande sur plusieurs lignes.
