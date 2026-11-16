Status: ready-for-agent

# Spec : bots et déclencheurs (v1)

Cadrage du 16 novembre 2026 (deux tours, recommandations acceptées ; Q3 : rien ne tourne app fermée). Faits : `docs/research/2026-11-16-notifications-gpui.md`.

## Problem Statement

Certaines demandes reviennent à heure fixe (relire les dépendances chaque lundi, résumer les changements de la veille) ou à l'arrivée d'une issue (« corriger le bug signalé »). Aujourd'hui je dois penser à ouvrir Solar, choisir le projet, retrouver la consigne et lancer la mission moi-même, chaque fois.

## Solution

Des **bots** : un nom, un projet, un déclencheur (horaire, manuel, nouvelle issue GitHub portant une étiquette), une consigne (un modèle de la bibliothèque aux trous préremplis, ou un texte), un agent avec son modèle, un mode de permission et une portée (planifier seulement, ou toute la mission dans sa copie de travail). Chaque déclenchement crée une **mission** marquée du bot, qui s'arrête au plan prêt ou au tour terminé à examiner : rien n'est publié, appliqué ni mergé sans moi. Rien ne tourne quand Solar est fermé ; à sa réouverture, il dit ce qui a été manqué et propose de lancer une fois. Garde-fous : une exécution à la fois par bot, plafond quotidien, une mission par issue.

## User Stories

1. En tant qu'utilisateur, je veux créer un bot avec un nom, un projet, un déclencheur et une consigne.
2. En tant qu'utilisateur, je veux prendre la consigne dans la bibliothèque et préremplir ses trous, ou écrire un texte.
3. En tant qu'utilisateur, je veux choisir l'agent, son modèle et le mode de permission du bot ; « Accepter les modifications » par défaut, l'accès complet avec un avertissement.
4. En tant qu'utilisateur, je veux choisir si le bot planifie seulement ou mène toute la mission dans sa copie de travail.
5. En tant qu'utilisateur, je veux que la mission complète exige un dépôt git avec un commit, pour qu'elle reste isolée.
6. En tant qu'utilisateur, je veux « Lancer maintenant » pour essayer un bot.
7. En tant qu'utilisateur, je veux un horaire : chaque jour à une heure, les jours ouvrés, ou toutes les N heures.
8. En tant qu'utilisateur, je veux un déclencheur « nouvelle issue GitHub avec l'étiquette X », dont le numéro, le titre et le texte remplissent la consigne.
9. En tant qu'utilisateur, je veux qu'un bot d'issues ne traite que les issues créées après lui, sauf option contraire.
10. En tant qu'utilisateur, je veux qu'une issue ne donne jamais deux missions.
11. En tant qu'utilisateur, je veux qu'un bot n'ait qu'une exécution à la fois, et qu'un déclenchement pendant qu'une exécution attend ma revue soit sauté et noté.
12. En tant qu'utilisateur, je veux un plafond quotidien d'exécutions par bot (3 par défaut).
13. En tant qu'utilisateur, je veux qu'à la réouverture de Solar les exécutions manquées soient dites, avec « Lancer une fois ».
14. En tant qu'utilisateur, je veux voir dans une vue « Bots » chaque bot, son déclencheur, sa prochaine et sa dernière exécution, son état.
15. En tant qu'utilisateur, je veux activer, mettre en pause, modifier et supprimer un bot.
16. En tant qu'utilisateur, je veux retrouver chaque exécution comme une mission du projet, avec une icône de bot, dans la barre latérale.
17. En tant qu'utilisateur, je veux être prévenu quand une exécution finit ou attend ma réponse : notification macOS dans l'app empaquetée, point d'état dans tous les cas.
18. En tant qu'utilisateur, je veux voir le coût de chaque exécution quand l'agent le donne.
19. En tant qu'utilisateur, je veux que l'entrée « Bots » de la barre latérale n'apparaisse qu'à partir du premier bot, et « Nouveau bot » dans ⌘K.
20. En tant qu'utilisateur anglophone et au clavier, je veux la vue en anglais et utilisable au clavier.

## Implementation Decisions

- **Cœur `bots`** : le type `Bot` (identifiant, nom, projet, déclencheur, consigne, agent, modèle, effort, mode, portée, actif, plafond quotidien, issues déjà traitées, historique court des exécutions : heure, mission, issue, issue de l'exécution ou raison d'un saut), rangé dans `state.json` (`bots`, absent d'un ancien fichier). Fonctions pures et testées : prochaine échéance d'un horaire (heure locale, jours ouvrés, toutes les N heures), exécutions manquées depuis une date, décision de lancer ou de sauter (en cours, plafond atteint, issue déjà traitée), consigne finale (modèle rempli + champs de l'issue).
- **Déclencheur issues** : `gh issue list --repo … --label … --state open --json number,title,body,createdAt,url` (module `github`) toutes les 5 minutes, app ouverte ; issues plus anciennes que le bot ignorées sauf option.
- **Planificateur** : une tâche du shell, toutes les minutes tant que l'app est ouverte, qui évalue chaque bot actif ; au lancement, calcul des exécutions manquées et proposition « Lancer une fois » dans la vue (rien n'est lancé seul après une absence).
- **Exécution** : réutilise la mission existante : conversation neuve dans le projet, Planifier, envoi de la consigne ; portée « mission complète » : le plan est approuvé automatiquement dans le mode du bot (copie de travail obligatoire) ; jamais de publication, d'application ni de merge. La session et la mission portent l'identifiant du bot.
- **Vue « Bots »** : vue pleine comme Réglages et Bibliothèque ; éditeur d'un bot en ligne ; entrée de la barre latérale dès qu'un bot existe ; ⌘K « Nouveau bot », « Ouvrir les bots ».
- **Prévenir** : `show_system_notification` (sans effet hors d'un paquet `.app`, voir la recherche), et point d'état sur l'entrée « Bots » et sur la mission.
- Aucun nouveau service système ; aucune dépendance nouvelle si `chrono` suffit pour les horaires.

## Testing Decisions

- Cœur : prochaine échéance (jour, jours ouvrés, intervalle, changement de jour), rattrapage, décisions de lancement et de saut, consigne avec issue, filtre des issues (date, étiquette, déjà traitées), lecture d'un ancien `state.json`. Faux `gh` pour la liste d'issues.
- Sans fenêtre : « Lancer maintenant » crée une mission marquée du bot avec la consigne ; un déclenchement pendant qu'une exécution attend la revue est sauté et noté ; notification envoyée (`shown_system_notifications`).
- À l'écran : vue Bots, éditeur, exécution vue dans la barre latérale ; un essai réel court (Haiku, dépôt jetable).

## Out of Scope

- Exécution app fermée (agent `launchd`) ; déclencheurs fichier modifié et check en échec ; publication automatique d'une PR ; bots partagés par le dépôt ; enchaînements de bots.

## Further Notes

- Les notifications attendront l'app empaquetée (chantier de diffusion).
