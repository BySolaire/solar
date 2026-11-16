Status: ready-for-agent
Blocked by: 01 — Paquet ; 04 — CI

# 05 : Première release

Spec : `docs/tickets/diffusion/spec.md`.

**What to build :** `release.yml` sur tag `v*` : paquet, zip, release en brouillon ; README : téléchargement et premier lancement (« Ouvrir quand même », `xattr`). Numéro de version à choisir avec Solaire. Le zip porte `LICENSE-MIT`, `LICENSE-APACHE`, `THIRD_PARTY.md` et la liste des licences des crates compilées (générée depuis `cargo metadata`, sans outil nouveau), comme l'annonce `THIRD_PARTY.md`.

- [ ] Une release brouillon porte le zip ; le paquet téléchargé s'ouvre en suivant les instructions.
