Status: resolved
Blocked by: 02 — Préparer la publication open source

# 03 : Rendre le dépôt public

Spec : `docs/tickets/diffusion/spec.md`.

**What to build :** Geste de Solaire (ou de l'agent sur feu vert explicite) : `gh repo edit BySolaire/solar --visibility public --accept-visibility-change-consequences`, description et sujets du dépôt.

- [x] Dépôt public, page d'accueil lisible.

## Answer

16 novembre 2026, sur feu vert de Solaire : `BySolaire/solar` est public, avec une description et les sujets `rust`, `gpui`, `macos`, `claude-code`, `codex`, `ai-agents` et `desktop-app`.

Vérifié sans être connecté :
- la page répond (200) ;
- le README s'affiche ;
- GitHub détecte la licence (Apache-2.0, une des deux proposées).

Reste ouvert : l'ancien commit d'avant la réécriture (`1b7c06c`) répond encore par son identifiant. Il ne s'efface que si le support GitHub le purge (demande « Remove sensitive data »).
