Type: grilling
Status: resolved

## Question

Que couvre la première intégration GitHub, et sous quelles règles ?

## Answer

Réponses de Solaire le 16 novembre 2026 (recommandations acceptées) :

1. Destination : une spec dans `docs/tickets/github/spec.md`, découpée en tickets.
2. Périmètre v1 : depuis une mission, branche + commit, push, PR, suivi des checks. Issues et commentaires de revue plus tard ; cloner un dépôt hors périmètre.
3. Compte : uniquement le compte actif de `gh`, aucun jeton stocké par Solar ; `gh` absent ou déconnecté → la commande à lancer, comme pour les agents. La fiche projet montre le compte.
4. Rien d'automatique : « Commit et push » puis « Ouvrir la PR », chacun avec un aperçu modifiable et confirmé. Accepter ne déclenche rien.
5. Contenu : seulement les changements de la mission, sur le commit de départ. Un fichier que l'utilisateur avait déjà modifié sans commit est signalé et demande une confirmation pour lui seul.
6. Branche `solar/<titre-de-la-mission>`, modifiable ; base = la branche de l'utilisateur au lancement si elle existe sur GitHub, sinon la branche par défaut. Branche courante et index de l'utilisateur jamais touchés.
7. Textes : brouillon par Solar (titre de la mission ; plan approuvé, état des tests, verdict du relecteur), modifiable, aucune signature publicitaire ; l'agent est nommé dans le corps.
8. Interface : gestes dans la vue Modifications à côté d'Accepter ; numéro de PR et checks dans la barre de mission et la vue Modifications, point d'état dans la barre latérale, lien vers la PR.
9. Publier ne ferme pas la mission : nouvel état « PR ouverte », conversation active dans la copie ; « Appliquer au projet » reste possible à part ; copie gardée jusqu'au merge, à la fermeture ou à l'abandon.
10. Checks en échec dès la v1 : nom et fin du journal, « Envoyer à l'agent » comme correction, puis nouveau « Commit et push ».
11. Rafraîchissement toutes les 30 s, seulement si des checks sont en cours et pour les missions ouvertes dans un onglet ; bouton manuel ; rien quand l'app est fermée.
12. Les hooks de l'utilisateur tournent ; en cas d'échec, leur sortie s'affiche et rien n'est poussé.
13. PR ouverte prête pour la revue ; case « Brouillon » dans l'aperçu.
14. PR mergée ou fermée → état « Mergée » ou « PR fermée », retrait de la copie proposé, jamais automatique ; branche distante jamais supprimée.
15. v1 : remote `origin` sur GitHub où le compte peut pousser ; forks, GitLab, sans remote → boutons grisés avec la raison ; GitHub Enterprise seulement si `gh` y est connecté.
16. Chaque push ajoute un commit ; jamais de force-push, amend ni rebase ; branche distante déplacée → push refusé avec explication.
