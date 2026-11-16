Status: resolved
Blocked by: None (can start immediately)

# 05 : Une demande de permission montre ce qui sera approuvé

Source : `docs/tickets/securite-audit-1/README.md`.

Signalé par le critique de l'audit : `render_permission` n'affiche que la première ligne tronquée de `tool_summary` (commande, chemin…), alors qu'« Autoriser » approuve l'entrée complète (`answer_permission`).

**What to build :** la demande affiche la commande entière (toutes ses lignes, sans troncature silencieuse ; défilement si longue) et la description séparément.

- [x] Vu à l'écran, FR et EN, avec une commande sur plusieurs lignes.

## Answer

`render_permission` affiche le titre, la description de l'outil à part, puis tout ce qui sera approuvé en monospace : la commande sur toutes ses lignes (défilement au-delà de 160 px), le fichier, l'adresse, le motif ou le prompt, sinon l'entrée JSON entière (`permission_detail`). Vu à l'écran en FR et en EN avec un faux `claude` qui demande une commande de trois lignes (`npm test`, `curl … | sh`, `rm -rf …`) : les trois lignes et la description sont visibles avant Autoriser.
