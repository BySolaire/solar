Status: ready-for-agent
Blocked by: None (can start immediately)

# 01 : Modèles personnels par `/` et ⌘K

Spec : `docs/tickets/library/spec.md`.

**What to build :** Le cœur lit les modèles de `~/.solar/library/` et les exemples intégrés (en-tête, trous, `$ARGUMENTS`), les fusionne et les remplit. Dans le composer, `/` montre un groupe « Bibliothèque » ; choisir un modèle à trous ouvre un formulaire au-dessus du composer (un champ par trou, Entrée insère), sans trou il s'insère directement ; un modèle mission active Planifier sur une conversation neuve ; un agent conseillé bascule l'agent d'une conversation neuve, sinon une note le dit. ⌘K gagne la catégorie « Bibliothèque ».

- [ ] Tests du cœur : en-tête complet, absent ou cassé ; trous, `{{`, `$ARGUMENTS`, trou répété ; remplissage ; fusion personnel > exemple.
- [ ] Test sans fenêtre : `/`, formulaire, texte inséré non envoyé, Planifier activé, agent basculé.
- [ ] Exemples FR/EN intégrés ; à l'écran en 1024 et 720.
