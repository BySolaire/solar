Status: ready-for-agent
Blocked by: None (can start immediately)

# 06 : Durcissements de l'audit

Source : `docs/tickets/securite-audit-1/README.md`.

Non-constats relevés par les chercheurs, retenus parce que peu coûteux :

- `find_url` (`solar-core/src/preview.rs`) : `http://localhost:1@hôte/` mène à un hôte distant ; le suffixe ne commence qu'à `/`, `?` ou `#`.
- Images Codex : créées sans suivre de lien (`create_new`), droits 0600, retirées après le tour.

- [ ] Tests du cœur pour `find_url`.
