Status: resolved
Blocked by: None (can start immediately)

# 06 : Durcissements de l'audit

Source : `docs/tickets/securite-audit-1/README.md`.

Non-constats relevés par les chercheurs, retenus parce que peu coûteux :

- `find_url` (`solar-core/src/preview.rs`) : `http://localhost:1@hôte/` mène à un hôte distant ; le suffixe ne commence qu'à `/`, `?` ou `#`.
- Images Codex : créées sans suivre de lien (`create_new`), droits 0600.

- [x] Tests du cœur pour `find_url`.

## Answer

`find_url` ne garde après le port qu'un chemin, une requête ou une ancre (test avec `http://localhost:1@example.com/`). Les images Codex sont écrites par `write_private` (`create_new`, 0600). Toujours non fait : leur retrait après le tour (commentaire `shortcut` conservé).
