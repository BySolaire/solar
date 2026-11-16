Status: resolved
Blocked by: 05 — Première release

# 06 : Annoncer une nouvelle version

Spec : `docs/tickets/diffusion/spec.md`.

**What to build :** Vérification au lancement (une fois par jour au plus) de la dernière release ; ligne discrète avec le lien ; réglage pour la couper (`state.json`).

- [x] Tests du cœur : versions comparées, réponse invalide ignorée.
- [x] À l'écran, FR et EN : annonce, réglage.

## Answer

- **Cœur** (`solar_core::update`) :
  - `newer` compare les versions numéro par numéro, et une préversion passe avant sa version.
  - `latest_from_json` écarte les brouillons, les préversions, les réponses illisibles et les liens hors de `github.com/BySolaire/solar/releases/`.
  - `due` décide s'il faut redemander : une fois par jour, ou si la date est illisible ou dans le futur.
  - `fetch_latest` lance `curl` en https seulement, avec 10 s au plus.
  - Trois tests.
- **État** : `update` (dernière vérification réussie) et `settings.no_update_check`, absents d'un ancien fichier et couverts par le test d'aller-retour.
- **Shell** : vérification au lancement de l'app empaquetée seulement (ni `cargo run` ni les tests). Un échec n'est pas noté. Ligne « Solar X disponible » en bas de la barre latérale, qui ouvre la release. Réglage « Vérifier les nouvelles versions » avec la version installée.
- **Vu à l'écran**, FR et EN, depuis le paquet :
  - requête réelle notée (0.1.0, rien d'annoncé) ;
  - 0.2.0 simulée : la ligne apparaît ;
  - réglage coupé : la ligne disparaît et le choix est enregistré.
- **Non essayé** : le clic qui ouvre le navigateur.
