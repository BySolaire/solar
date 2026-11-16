# Solar

Le vocabulaire de Solar, l'app native qui pilote des agents de code (Claude, Codex, Grok) sans terminal visible.

## Travail

**Projet** :
Un dossier de l'utilisateur, ajouté à Solar, où les agents travaillent.
_Éviter_ : workspace, repo (quand on parle du projet et non de son dépôt git)

**Conversation** :
Un échange suivi avec un agent dans un projet, ouvert dans un onglet.
_Éviter_ : chat

**Tour** :
Ce que fait un agent entre un message et sa réponse finale.
_Éviter_ : run, requête

**Mission** :
Une conversation qui suit un plan approuvé, de sa préparation à la décision de l'utilisateur sur le résultat.
_Éviter_ : tâche, job

**Tentative** :
Une nouvelle exécution d'une même mission, dans une conversation neuve, après une restauration ou une annulation.
_Éviter_ : retry, run

**Copie de travail** :
Le dossier à part où travaille une mission d'un dépôt git, pour ne pas toucher au projet pendant l'exécution.
_Éviter_ : sandbox, clone

**Instantané** :
L'état des fichiers du projet, enregistré par Solar au lancement d'une mission, auquel on compare le résultat.
_Éviter_ : checkpoint (dans les textes de l'interface), sauvegarde

**Bot** :
Une consigne confiée à un agent sur un projet, que Solar lance à un déclencheur ; chaque fois, une mission à examiner.
_Éviter_ : automatisation, workflow, agent (l'agent est Claude, Codex ou Grok)

**Déclencheur** :
Ce qui lance un bot : un horaire, une nouvelle issue GitHub portant une étiquette, ou « Lancer maintenant ».
_Éviter_ : trigger, hook

**Exécution** :
Une fois où un bot est parti, ou aurait dû partir (sautée par un garde-fou, ou impossible) ; une exécution partie est une mission.
_Éviter_ : run, job

## Décisions sur le résultat

**Accepter** :
Décider que le résultat examiné convient, sans effet sur les fichiers ni sur git.
_Éviter_ : valider, merger

**Appliquer** :
Écrire dans le projet le résultat examiné d'une mission qui travaillait dans sa copie de travail, sans commit.
_Éviter_ : merger, intégrer

**Publication** :
L'envoi du résultat d'une mission sur GitHub : un commit sur la branche de la mission, poussé, puis une pull request.
_Éviter_ : déploiement, livraison, export

**Branche de mission** :
La branche git créée par Solar pour publier une mission ; elle ne contient que les changements de la mission, posés sur le commit de départ.
_Éviter_ : branche de travail, branche Solar
