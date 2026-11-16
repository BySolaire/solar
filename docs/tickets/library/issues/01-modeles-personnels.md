Status: resolved
Blocked by: None (can start immediately)

# 01 : Modèles personnels par `/` et ⌘K

Spec : `docs/tickets/library/spec.md`.

**What to build :** Le cœur lit les modèles de `~/.solar/library/` et les exemples intégrés (en-tête, trous, `$ARGUMENTS`), les fusionne et les remplit. Dans le composer, `/` montre un groupe « Bibliothèque » ; choisir un modèle à trous ouvre un formulaire au-dessus du composer (un champ par trou, Entrée insère), sans trou il s'insère directement ; un modèle mission active Planifier sur une conversation neuve ; un agent conseillé bascule l'agent d'une conversation neuve, sinon une note le dit. ⌘K gagne la catégorie « Bibliothèque ».

- [x] Tests du cœur : en-tête complet, absent ou cassé ; trous, `{{`, `$ARGUMENTS`, trou répété ; remplissage ; fusion personnel > exemple.
- [x] Test sans fenêtre : `/`, formulaire, texte inséré non envoyé, Planifier activé, agent basculé.
- [x] Exemples FR/EN intégrés ; à l'écran en 1024 et 720.

## Comments

16 novembre 2026, branche `library-v1`. Cœur : `solar_core::library` (`parse` : en-tête `title`, `description`, `agent`, `mission`, problèmes signalés ; `holes` et `fill` : `{nom}`, `{{`/`}}`, `$ARGUMENTS` ; `load` : projet > personnel > exemple ; `personal_dir` avec `SOLAR_LIBRARY` pour les essais). Interface : `/` propose les modèles (groupe « Bibliothèque · titre ») avant les commandes de l'agent, qui gardent deux places ; Tab, clic ou Entrée sur « /nom » utilise le modèle ; formulaire des trous au-dessus du composer (accueil et conversation), Entrée insère, Échap ferme ; Planifier et agent conseillé sur une conversation neuve, sinon une note ; quatre exemples FR/EN. ⌘K : catégorie « Bibliothèque » ; « ⇥ Catégorie suivante » passe dans le pied (six catégories ne tenaient plus sur une ligne). Tests : trois du cœur, un sans fenêtre (formulaire, texte inséré sans envoi, Planifier, agent). Vu à l'écran en 1024 FR et 720 EN.
