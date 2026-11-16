Status: ready-for-agent
Blocked by: None (can start immediately)

# 01 : Bot, vue Bots et « Lancer maintenant »

Spec : `docs/tickets/bots/spec.md`.

**What to build :** Le cœur décrit un bot (rangé dans `state.json`) et compose sa consigne. Une vue « Bots » (entrée de la barre latérale dès le premier bot, ⌘K « Nouveau bot ») crée, modifie, met en pause et supprime un bot ; « Lancer maintenant » crée une mission marquée du bot dans son projet, avec sa consigne, son agent, son mode et sa portée (plan seulement, ou mission complète dans sa copie, plan approuvé automatiquement). L'exécution apparaît dans la barre latérale avec une icône de bot ; la vue montre la dernière exécution.

- [ ] Tests du cœur : bot sérialisé, ancien `state.json` relu, consigne composée (modèle rempli, texte libre).
- [ ] Test sans fenêtre : « Lancer maintenant » crée la mission avec la consigne, le mode et la marque du bot.
- [ ] À l'écran en 1024 et 720, FR et EN.
