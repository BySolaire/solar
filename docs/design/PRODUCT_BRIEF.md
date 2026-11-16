# SOLAR — Brief produit

Version 1.0 · Direction retenue le 14 novembre 2026. Ce document décrit la cible ; consulter l'[audit](EXISTING_PROJECT_AUDIT.md) pour l'existant.

## Vision

SOLAR permet de transformer une intention en logiciel en dialoguant avec un assistant, puis en supervisant des agents dans le contexte d'un projet. La complexité de l'exécution reste dans le moteur ; l'utilisateur dispose d'objectifs, d'un plan, de résultats vérifiables et de moyens d'intervenir.

**Slogan : « Engineered to Build »** (décidé le 15 novembre 2026, remplace la proposition « Build Beyond »). Il reste en anglais quelle que soit la langue de l'interface, et vit dans la communication de la marque : l'app ne l'affiche pas. Principe produit : « Complex engineering. Simple experience. » L'éclipse exprime la convergence des agents autour d'un projet. Elle reste une signature sobre, déjà représentée dans les assets du dépôt.

Les usages évoqués par T3 Code et Grok Bot se traduisent ici par la continuité entre conversation et travail de développement. SOLAR conserve son architecture native Rust/GPUI et développe une expérience originale.

## Public et situations d'usage

| Utilisateur | Besoin | Réponse attendue |
|---|---|---|
| Développeur individuel | Faire évoluer un dépôt local sans perdre le contexte | Conversation liée au dossier, plan incrémental, changements inspectables |
| Créateur technique ou peu technique | Décrire un résultat sans piloter des commandes | Composer clair, reformulation de l'objectif, preview et résumé accessibles |
| Utilisateur expérimenté d'agents | Répartir le travail et vérifier les sorties | Affectations explicites, dépendances, historique et contrôles |

Le premier périmètre est une application locale macOS pour un utilisateur. Collaboration d'équipe, partage distant, synchronisation et contrôle à distance ne sont pas acquis.

## Principes de décision

1. **Conversation d'abord** : expliquer une idée doit suffire pour démarrer la préparation. Ne pas imposer un organigramme d'agents.
2. **Adaptation progressive** : l'espace de travail apparaît à mesure que la mission le demande ; la conversation reste la même.
3. **Contexte stable** : le projet, la mission active et la tentative d'exécution restent identifiables.
4. **Contrôle humain concret** : l'utilisateur comprend ce qu'il autorise, peut interrompre et examiner les conséquences.
5. **Preuve avant déclaration** : distinguer travail en cours, tests exécutés, livraison examinable et résultat accepté.
6. **Aucune interface terminal** : navigation, actions, permissions, résultats et diagnostics sont visuels et structurés.
7. **Conserver l'existant** : faire évoluer les sessions et composants opérationnels plutôt que reconstruire un autre produit.

## Entités et relations de la cible

Ces concepts ne sont pas tous des structures existantes. Le premier incrément doit mapper les identifiants et sessions actuels sans migration destructive.

| Entité | Responsabilité | Relations proposées |
|---|---|---|
| Workspace | Environnement local global | Contient projets et configurations d'agents ; pas un compte cloud obligatoire |
| Project | Dossier, contexte, conventions et ressources | Contient conversations et missions ; conserve le chemin canonique |
| Conversation | Dialogue durable et pièces de contexte | Appartient à un projet après rattachement ; peut initier plusieurs missions |
| Mission | Objectif, périmètre et critères de réussite | Rattachée à un projet et une conversation source ; possède tâches et exécutions |
| Task | Travail borné, dépendances, résultat attendu | Appartient à une mission ; affectée à un rôle/agent |
| Agent | Configuration d'un exécutant | Provider, modèle, rôle, capacités et permissions ; plusieurs affectations possibles |
| Assignment | Participation d'un agent | Relie agent, tâche et exécution ; ne pas partager son contexte mutable entre projets |
| Execution | Tentative concrète, datée | Appartient à une mission ; conserve événements, contrôles, changements et résultats de tests |
| ChangeSet | Ensemble exact de changements examinés | Lié à une exécution et une base ; acceptation référencée à cette version |

Un rôle Architecte, Implémenteur, Reviewer ou Testeur ne désigne pas un provider. Claude, Codex et Grok sont les adaptateurs actuels (depuis la phase 6) ; d'autres providers relèvent d'extensions futures. Un choix de modèle ne crée pas, à lui seul, un nouvel agent.

## Périmètre de l'expérience

### Home

Accueil conversationnel : « Qu'est-ce qu'on construit ? », composer, projet sélectionné ou choix de dossier, modèle et mode connus, projets récents. Les fonctions non disponibles ne deviennent pas des boutons décoratifs. Le mode « Auto » signifie une proposition de configuration explicable et limitée aux agents réellement disponibles ; il ne contourne aucune permission.

### Mission Planning

À partir de la demande : objectif reformulé, fichiers/contexte concernés, hypothèses, critères de réussite, tâches et dépendances, agents proposés, opérations nécessitant une décision. L'utilisateur peut corriger, approuver le plan ou poursuivre la conversation. Une planification seule ne doit pas lancer de modifications ; le mode effectif du moteur doit faire respecter cette limite.

### Agent Workspace

Une barre de contexte projet/mission/exécution, cinq vues : **Conversation, Plan, Fichiers, Modifications, Preview**. Le nombre de panneaux visibles s'adapte à la tâche et à la largeur. L'activité est consultable sans devenir une colonne permanente. La navigation ne détruit pas la session.

### Livraison et revue

Le résultat indique : objectif traité, changements précis, tests effectués et leur issue, limites, preview disponible ou raison d'indisponibilité. Les actions continuer, demander une correction, accepter ou refuser sont distinctes. Accepter dans SOLAR ne signifie pas automatiquement committer, pousser, merger ou déployer.

En première revue, si les agents écrivent dans le dossier existant, afficher explicitement que les modifications sont déjà sur disque. Un refus ne peut pas promettre une annulation sans checkpoint fiable. Le staging isolé/worktree et le rollback ciblé sont des capacités ultérieures à prouver.

## Orchestration et supervision

- Commencer par une seule exécution Claude, puis des rôles séquentiels. Le parallélisme vient après les dépendances et l'isolation.
- Donner à chaque tâche un résultat attendu, des entrées, un propriétaire, des prérequis et un état indépendant.
- Rendre visibles les blocages, demandes de permission, erreurs et dépendances ; ne pas inventer de pourcentage d'avancement.
- Ne pas laisser deux agents écrire les mêmes fichiers sans mécanisme de coordination. Une revue ne valide pas un contenu différent de celui examiné.
- Conserver une timeline corrélée à la mission, à l'exécution et à l'agent. Ne pas afficher de chaîne de raisonnement interne ; une activité factuelle suffit.
- Une interruption utilisateur est un arrêt demandé, pas un échec de développement. Une reprise crée une tentative identifiable lorsque nécessaire.

## Priorités et exclusions

| Priorité | Contenu |
|---|---|
| P0 — Continuité | Sessions, streaming, import, permissions, modèles/effort, images, FR/EN et données existantes |
| P1 — Parcours principal | Home, planification, workspace adaptatif, état persistant et erreurs compréhensibles |
| P2 — Résultat vérifiable | Fichiers, diff, tests structurés, preview liée au projet, livraison et décisions |
| P3 — Orchestration | Rôles séquentiels, dépendances, puis parallélisme avec isolation |
| Plus tard | Autres providers, bots, GitHub, library avancée, mobile, distribution multi-plateforme |

Hors périmètre de cette transmission : implémenter ces écrans, changer la stack, réécrire le cœur, remplacer les logos, supprimer des fonctionnalités, ajouter un terminal, publier ou initialiser un historique Git. Une palette de commandes future ne doit pas servir de shell déguisé.

## Critères d'acceptation produit

1. Depuis un dossier existant, une demande devient une préparation de mission sans perte du texte ou des pièces jointes ; annuler conserve le brouillon.
2. La phase de planification expose le périmètre et n'effectue aucune écriture tant que l'exécution n'est pas autorisée dans le mode choisi.
3. Passer entre les cinq vues et revenir conserve la session, les événements reçus, le brouillon et la position de revue.
4. Chaque action visible a un effet réel ou une indisponibilité expliquée. Aucun faux succès, agent fictif ou contrôle inopérant.
5. Une fin de tour Claude ne clôture pas automatiquement la mission. Un test non exécuté reste « Non exécuté ».
6. Un résultat peut être examiné avant son acceptation ; toute restauration précise ce qu'elle annule et protège le travail antérieur de l'utilisateur.
7. Une fermeture/réouverture restaure le contexte sauvegardé sans prétendre que des processus tués continuent de tourner.
8. À la taille native minimale, le clavier permet d'atteindre les actions principales ; le flou n'altère pas la lisibilité et les changements d'état ne reposent pas sur la couleur seule.

## Indicateurs à mesurer ultérieurement

Temps jusqu'à la première demande valide, compréhension du plan, fréquence de corrections avant lancement, perte de brouillon/contexte, erreurs de reprise et décisions de revue sur une version obsolète. Les objectifs chiffrés seront fixés après observation d'un parcours réel ; aucune télémétrie distante n'est ajoutée par ce brief.

## Décisions encore ouvertes

- Contrat et stockage versionné des missions, brouillons, événements et checkpoints.
- Politique du mode de permission par défaut, avec traitement explicite du mode actuel « Accès complet ».
- Niveau de reprise réellement possible après arrêt du processus Claude.
- Première stratégie de diff/rollback et isolation des écritures.
- Intégration native de la preview, origine autorisée et gestion de son processus.
- Chargement/licences des polices et choix du futur client mobile. Les six planches originales sont désormais disponibles dans [les références](references/README.md) ; une maquette autonome Mission Planning reste à réaliser.

Ces choix sont ordonnés dans le [plan de développement](DEVELOPMENT_PLAN.md) ; ils ne justifient pas une refonte globale.
