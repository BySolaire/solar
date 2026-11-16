Status: resolved
Blocked by: None (can start immediately)

# 01 : Bot, vue Bots et « Lancer maintenant »

Spec : `docs/tickets/bots/spec.md`.

**What to build :** Le cœur décrit un bot (rangé dans `state.json`) et compose sa consigne. Une vue « Bots » (entrée de la barre latérale dès le premier bot, ⌘K « Nouveau bot ») crée, modifie, met en pause et supprime un bot ; « Lancer maintenant » crée une mission marquée du bot dans son projet, avec sa consigne, son agent, son mode et sa portée (plan seulement, ou mission complète dans sa copie, plan approuvé automatiquement). L'exécution apparaît dans la barre latérale avec une icône de bot ; la vue montre la dernière exécution.

- [x] Tests du cœur : bot sérialisé, ancien `state.json` relu, consigne composée (modèle rempli, texte libre).
- [x] Test sans fenêtre : « Lancer maintenant » crée la mission avec la consigne, le mode et la marque du bot.
- [x] À l'écran en 1024 et 720, FR et EN.

## Comments

16 novembre 2026, branche `bots-v1`. Cœur `bots` : `Bot` (déclencheur `Manual` pour l'instant, consigne texte ou modèle aux trous préremplis, agent, modèle, effort, mode, portée plan ou mission complète, actif, vingt dernières exécutions), `prompt()` (consigne vide, modèle disparu, trous vides refusés), `State.bots` et `Session.bot`, absents d'un ancien fichier. Interface : vue « Bots » (`shell/bots_view.rs`), entrée de la barre latérale dès le premier bot, ⌘K « Nouveau bot » et « Ouvrir les bots » ; éditeur en ligne (nom, consigne, projet, agent, modèle, permissions sans le plan, portée, avertissement pour l'accès complet) ; ligne : déclencheur, projet, agent, portée, dernière exécution et « Ouvrir », « Lancer maintenant », le reste derrière « ··· » (pause, modifier, supprimer après confirmation). Une exécution est une conversation en fond, titrée du nom du bot, qui planifie en lecture seule ; en mission complète, le plan part dès son arrivée dans le mode du bot, seulement si la mission a sa copie de travail (sinon le plan attend, avec un message). Icône de bot dans la barre latérale. Une exécution impossible (consigne, mode, dossier disparu) est notée sans rien ouvrir.

Vérifié : trois tests du cœur, deux sans fenêtre (exécution complète dans la copie avec faux CLI : plan avant consigne, mode du bot avant l'accord, projet intact ; consigne impossible notée) ; à l'écran en 1024 FR et 720 EN avec un faux Claude (création, modèle à trous, trou vide refusé, exécution, ouverture, projet sans git, suppression). Captures : `/tmp/solar-qa/captures/bots-01/`.

Limites : le menu des modèles de Claude ne liste ses modèles qu'une fois un Claude lancé ; l'effort n'est pas réglable dans l'éditeur ; la notification et le point d'état viennent au ticket 04.
