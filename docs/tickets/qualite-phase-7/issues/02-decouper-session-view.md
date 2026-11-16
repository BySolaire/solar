Status: resolved

# 02 : Découper `session_view.rs`

`session_view.rs` : environ 2 560 lignes de code et 1 400 de tests dans le même fichier ; la vue mêle composer, fil, événements des agents et cycle de vie des missions (planification, lancement, annulation, relance, lancement automatique des bots). Proposition : sortir le cycle de vie des missions dans `session_view/mission.rs` et les tests dans `session_view/tests.rs`, sans changer le comportement (déplacement seul, tests inchangés). Gain : lecture et revue ; risque : conflits avec un chantier en cours, à faire entre deux chantiers.

## Comments

16 novembre 2026 : fait, déplacement seul. `session_view/mission.rs` (reprise, lancement, annulation, tentative, exécution de bot ; méthodes en `pub(super)`) et `session_view/tests.rs` ; `session_view.rs` passe de 3 956 à 2 293 lignes. Tests inchangés, tous verts.
