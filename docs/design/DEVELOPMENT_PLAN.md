# SOLAR — Plan de développement incrémental

Version 1.0 · 14 novembre 2026. Ce plan adapte la vision au dépôt Rust/GPUI existant. Il ne constitue pas une demande de développer toutes les phases maintenant.

## Stratégie

Conserver `solar-core` pour les règles métier et adaptateurs, `solar` pour les vues GPUI. Étendre le shell et `SessionView` ; ne pas lancer de migration de framework ou de réécriture. Chaque incrément doit laisser le chat existant utilisable et annoncer les fonctions seulement lorsqu'elles sont réelles.

Commencer par les trois écrans Home, Mission Planning et Agent Workspace, avec états vides et données de démonstration explicitement signalées si une maquette en exige. Séparer la preuve visuelle de la preuve d'exécution : une belle maquette ne prouve ni orchestration, ni revue sûre, ni preview fonctionnelle.

## Phase 0 — Confirmer la base et les contrats

**Appuis** : audit, manifestes, tests et fichiers actuels.

- Relire l'état Git et les instructions, car l'inventaire initial est entièrement non suivi et sans commit.
- Vérifier l'environnement de développement sans démarrer d'agent sur un dépôt réel ; inventorier les versions réellement installées si elles deviennent nécessaires.
- Définir le plus petit modèle de navigation interne et la conservation des entités de session.
- Décider le stockage compatible des missions/brouillons, le mode de planification sans mutation et la signification d'« accepter/refuser » selon que les changements sont sur disque ou isolés.
- Établir un état jetable, des fixtures et un faux CLI pour les parcours de développement.

**Livrable** : petit plan de fichiers à toucher, contrats de données et risques concrets. Les choix de stockage, sécurité et isolation sont à résoudre avant les fonctions qui en dépendent ; ils ne nécessitent pas de redessiner tout le moteur.

**Sortie de phase** : architecture compatible expliquée, aucune migration destructive, point d'entrée de validation identifié.

## Phase 1 — Socle visuel ciblé et Home

**Zones probables** : `theme.rs`, `shell.rs`, `session_view.rs`, `locales/app.yml` ; composants natifs réutilisés.

- Introduire des tokens sémantiques en conservant la palette existante comme base de transition.
- Créer l'accueil conversationnel, la sélection du dossier et les récents à partir des projets actuels.
- Réutiliser le composer ; éviter deux implémentations divergentes de saisie/modèles/permissions/images.
- Rendre la navigation rétractable pour la petite fenêtre, conserver onglets/import et restauration.
- Conserver le brouillon lorsqu'on choisit un projet ou qu'un lancement échoue.
- Afficher les options réellement disponibles ; ne pas livrer de menu Agents/Auto vide comme fonctionnalité.

**Sortie de phase** : Home utilisable dans la vraie app, premier message envoyé une seule fois dans le bon projet, chat/import inchangés fonctionnellement, contrôle clavier/FR/EN et tailles natives validés.

## Phase 2 — Mission Planning persistante

**Dépend de** : contrat de données et de permission de phase 0 ; Home de phase 1.

- Ajouter mission, tâches et version du plan dans `solar-core` ou un module adjacent seulement si nécessaire.
- Mapper la conversation source et le projet sans changer les identifiants Claude.
- Créer objectif, périmètre, critères de réussite, dépendances et affectation initiale à Claude.
- Sauvegarder le plan et le brouillon avec compatibilité des anciens JSON.
- Séparer « planifier » et « exécuter » dans le moteur ; traiter explicitement le défaut actuel `bypassPermissions` et les préférences utilisateur.
- Distinguer demande de correction, enregistrement, approbation et lancement.

**État (15 novembre 2026)** : premier incrément livré. Bascule « Planifier » dans le composer de l'accueil ; le CLI passe en mode `plan` avant le premier message (lecture seule imposée par le moteur) ; le plan proposé par l'outil `ExitPlanMode` s'affiche en entier ; un message écrit pendant qu'il attend est une correction (refus avec ce message, nouvelle version) ; « Lancer la mission » propose Accès complet, Accepter les modifications ou Demander, et envoie ce mode au CLI avant l'accord. La mission (statut, versions du plan, mode de lancement) est rangée dans la session de `state.json`, avec copie `.bak` de l'ancien fichier. Ajoutés ensuite : « Annuler la mission » (refus envoyé au CLI, statut Annulée, la conversation reste) et une icône de mission dans la barre latérale. Le plan reste du Markdown rédigé par Claude, choix de l'utilisateur ; les tâches et critères structurés sont repoussés à l'orchestration multi-agents (phase 6). Éprouvé le 15 novembre 2026 avec le vrai CLI (2.1.325, Haiku, dossiers jetables) : le plan arrive par `ExitPlanMode` (champ `plan`, plus `planFilePath`), aucun fichier du projet n'existe avant l'accord, l'exécution suit l'accord. Lancé en « Demander » depuis un CLI démarré en accès complet, Claude demande bien avant `echo hi > info.txt` : le mode choisi s'applique. À savoir : en mode plan, Claude peut lancer des commandes en lecture seule (`uname`) et écrit son plan dans `~/.claude/plans/`, hors du projet ; « Accepter les modifications » autorise aussi les commandes qui ne font qu'écrire un fichier (`printf … > hello.txt`).

**Sortie de phase** : plan révisable et restaurable, aucun effet de développement pendant la préparation, lancement sur version approuvée, annulation sans perte du contexte. Tests de lecture d'ancien état et transitions métier nécessaires.

## Phase 3 — Agent Workspace adaptatif

**Dépend de** : mission identifiée et exécution autorisée de phase 2.

- Construire la barre de contexte et les cinq destinations, en gardant la conversation opérationnelle.
- Ajouter une activité factuelle et un Plan lié aux tâches réelles ; au départ une seule exécution Claude.
- Maintenir entités et abonnements en arrière-plan ; la navigation ne doit pas déclencher `Drop` sur un processus vivant.
- Ajouter fichiers en lecture seule ; traiter absence de dépôt Git, fichiers non suivis et gros contenus sans bloquer l'UI.
- Implémenter états d'attente utilisateur, interruption, fin de tentative et réconciliation à la réouverture.
- Pour Modifications et Preview avant leur implémentation : un état d'indisponibilité explicite dans la maquette, puis une capacité réelle avant de présenter leurs actions comme actives.

**État (15 novembre 2026) : phase terminée.** Vues Conversation, Plan et Fichiers ; Modifications et Preview viendront avec les phases 4 et 5. Une mission a une barre sous les onglets : titre, état en clair (Planification, Plan prêt, En cours, Attend ta réponse, Tour terminé à examiner, Annulée, Arrêt demandé), bascule Conversation / Plan, Arrêter. La vue Plan montre la dernière version du plan et l'activité réelle tirée des outils (fichiers du projet modifiés et lus, commandes avec leur échec). Composer, permissions et carte du plan restent joignables depuis les deux vues. Fermer un onglet où Claude travaille le laisse continuer en fond ; la barre latérale garde son état et le rouvrir rend la même vue. Réconciliation : une mission rouverte dont l'historique finit sur un tour inachevé (message ou outil sans résultat) s'affiche « Interrompue à la fermeture de SOLAR » ; rien n'est relancé, un nouveau message reprend. Vue Fichiers en lecture seule : arborescence lue en arrière-plan (`solar_core::files` : fichiers suivis et nouveaux non ignorés d'un dépôt git, sinon parcours sans dossiers cachés, `target` ni `node_modules`, borné à 10 000), aperçu coloré du fichier choisi, coupé à 200 Ko, fichiers binaires et illisibles signalés. Les mentions `@` profitent du même listage, y compris hors git. Limites connues : VoiceOver ne voit pas le contenu (gpui-kit), une vue passée en fond reste en mémoire jusqu'à la fermeture de l'app.

**Sortie de phase** : passage entre vues sans perte du streaming/brouillon, contrôle d'interruption honnête, permissions joignables partout, aucune interface terminal. Captures des trois écrans dans la même identité SOLAR.

## Phase 4 — Revue, validation et récupération

**Dépend de** : identification de l'exécution et choix de la base de comparaison.

- Inventorier l'état de travail avant exécution, y compris modifications utilisateur et dépôt sans premier commit.
- Introduire un ChangeSet versionné, lecture de fichiers et diff unifié ; le côte à côte vient si la largeur le permet.
- Afficher fichiers nouveaux/supprimés/renommés, binaires et cas volumineux.
- Structurer les résultats de tests, leur date et la version concernée ; « Non exécuté » reste un état distinct.
- Ajouter acceptation et demande de correction ; n'exposer la restauration qu'après preuve de checkpoint fiable.
- Établir l'isolation, par exemple worktrees si adaptée et validée, avant parallélisme d'écriture ou promesse de rollback.
- Rendre une revue obsolète si le contenu a changé depuis l'examen. Séparer acceptation, commit, push, PR et déploiement.

**État (15 novembre 2026) : phase terminée.** Au lancement d'une mission, un instantané du projet est pris dans un dépôt git privé (`~/.solar/checkpoints/<projet>.git`, `solar_core::checkpoint`) : le dépôt, la branche et l'index de l'utilisateur ne sont jamais touchés, un dossier sans git ni commit marche aussi, `.gitignore` est respecté et `node_modules`, `target` et `.DS_Store` sont toujours ignorés. Vue « Modifications » : diff unifié de l'instantané à l'état actuel (composant de gpui-kit), liste des fichiers à droite (ajout, modification, suppression, binaire, `+n −m`), validation tirée du dernier test lancé par Claude (réussis, échoués, en cours, périmés si un fichier a changé après, ou « Non exécutés »), avec son heure : celle que Claude note dans l'historique, sinon celle de réception. « Accepter le résultat » n'est qu'une décision liée à l'arbre exact examiné, sans commit ; refusée et relue si le projet a changé depuis l'affichage. « Restaurer l'état d'avant » remet les seuls fichiers de la mission après confirmation, et refuse tout si l'un a changé depuis l'examen. La correction se demande en écrivant à Claude ; un message après une décision rouvre la mission. Tests : instantané, restauration ciblée, refus sur fichier modifié, dépôt de l'utilisateur intact octet pour octet, parcours complet de la revue. Non fait : diff côte à côte, isolation par worktree (reportée à l'orchestration multi-agents, phase 6), textes de gpui-kit en français (la bibliothèque n'a pas cette langue).

**Sortie de phase** : décision liée aux changements exacts, perte de travail préexistante exclue par vérification dédiée, refus et restauration sans ambiguïté. Aucun rollback global ou destructif.

## Phase 5 — Preview intégrée

**Dépend de** : contrat de preview et vérification du support natif disponible.

- Vérifier les capacités et versions compatibles du mécanisme natif envisagé ; le README mentionne `gpui-webview`, absent des manifestes actuels.
- Commencer par un projet web de test connu ; identifier l'origine autorisée et l'exécution observée.
- Distinguer les processus créés par SOLAR des services de l'utilisateur ; gérer démarrage, échec, rechargement et fin de vie sans arrêter un service existant.
- Prévoir un état honnête pour backend ou app native sans preview intégrable.
- Montrer la version périmée et les erreurs de chargement ; ne pas afficher une preview de démonstration comme résultat de l'agent.

**État (15 novembre 2026) : phase terminée.** `gpui-webview` 0.9.1 (WKWebView par `lb-wry` 0.53, même GPUI que gpui-kit 0.9.1) ; `Cargo.lock` gagne 67 paquets, surtout Linux, sans changer de version existante. Vue « Preview » d'une mission : si une commande de Claude a fait apparaître une adresse locale, la page s'y affiche (« un serveur que SOLAR n'a pas lancé et n'arrêtera pas ») ; sinon SOLAR propose « Lancer la preview » avec la commande détectée (`solar_core::preview::dev_command` : script dev/start/preview de `package.json`, gestionnaire selon le fichier de verrouillage). Le serveur lancé tourne dans son groupe de processus, ses dernières lignes expliquent un échec (bloc copiable), son adresse est lue dans sa sortie ; il s'arrête par « Arrêter le serveur », à la fermeture de la conversation, quand SOLAR quitte, et même si SOLAR plante ou est tué (`kill -9`) grâce à un processus gardien. Projet sans web : « Aperçu indisponible », renvoi vers Modifications, champ d'adresse manuel. La page web est une vue native qui recouvre l'interface : SOLAR la cache à chaque changement de vue, d'onglet ou d'écran. Éprouvé avec un vrai serveur (`npm run dev`) : page affichée, arrêt à la fermeture et après `kill -9` vérifiés. Menus : la page se cache tant qu'un menu du composer, de la carte du plan ou d'un projet est ouvert (`on_open_change`), et revient à sa fermeture ; vérifié à l'écran, comme le masquage au passage à une autre vue. Clavier : quand la page apparaît, le focus reste au composer ; une fois dans la page, Échap (s'il n'est pas déjà traité par la page) rend la main à SOLAR, par un script injecté et un message IPC de Wry — chemin non vérifié à l'écran, faute de pouvoir placer le focus dans la page en simulation. Limites : le menu contextuel (clic droit) de la barre latérale n'a pas ce masquage, les infobulles passent sous la page, pas de détection de version périmée (rechargement manuel ; les serveurs de dev rechargent souvent seuls).

**Sortie de phase** : preview réelle du projet attendu et gestion fiable de l'indisponibilité ; aucun terminal ou barre shell ajouté pour piloter le serveur.

## Phase 6 — Orchestration multi-agents réelle

**Dépend de** : états fiables, revue et isolation des phases précédentes.

1. Rôles séquentiels Architecte → Implémenteur → Reviewer/Testeur, avec contrats de sortie et contexte partagé borné.
2. Planification des dépendances, blocages et retries ; nouveaux identifiants de tentative, historique conservé.
3. Parallélisme seulement sur tâches indépendantes avec maîtrise des écritures et conflits.
4. Adaptateurs supplémentaires derrière un contrat commun si la demande le justifie ; préserver Claude et ses fonctions existantes.

**État (15 novembre 2026)** : premier incrément livré, choix de l'utilisateur : rôles séquentiels, Claude seul, relecture à la demande, verdict affiché. Architecte = la planification (phase 2), Implémenteur = l'exécution lancée, **Relecteur** = nouveau : « Faire relire » dans la vue Modifications lance une seconde session Claude au contexte neuf, en mode plan (lecture seule imposée par le moteur), avec le plan approuvé et le diff exact examiné (coupé à 60 Ko, le relecteur peut lire les fichiers). Son verdict (`VERDICT: OK` / `PROBLÈMES`, ou « pas de verdict net ») est rangé dans la mission (`AgentReview` : agent, arbre exact relu, texte, heure), affiché dans la revue, et périmé dès que le projet change. « Envoyer à l'implémenteur » transmet les problèmes comme correction ; accepter reste la décision de l'utilisateur. Un seul agent écrit à la fois. Éprouvé avec le vrai CLI (Haiku) : réponse en texte avec la ligne de verdict exacte, problèmes réels relevés, aucun fichier modifié. **Tentatives et blocages** (livré) : une mission restaurée ou annulée propose « Relancer (tentative n) ». La relance ouvre une conversation neuve (session Claude vierge, sans `--resume`) qui reçoit le plan approuvé et, s'il y en a, les problèmes relevés par le relecteur, prend un nouvel instantané et part dans le mode choisi pour la mission. La mission porte son numéro (`attempt`) et la précédente sait qu'elle est remplacée (`next_attempt`) : son historique reste intact. Les états nomment l'agent concerné : implémenteur au travail ou en attente de permission, relecteur au travail ou arrêté sans verdict, dernier tour en erreur. Vérifié par un test sans fenêtre (brief, mode, instantané, session neuve) et à l'écran en FR/EN ; les états d'échec n'ont pas été provoqués à l'écran. **Missions parallèles** (livré) : dans un dépôt git, chaque mission travaille dans sa propre copie, un worktree détaché rangé hors du projet (`~/.solar/checkpoints/worktrees/<id>`), qui reprend aussi les changements non commités ; plusieurs missions d'un même projet tournent donc en même temps sans se gêner, chacune avec son index d'instantanés. Claude y planifie et y travaille dès le premier message ; la revue, les fichiers, la preview et le relecteur lisent la copie. « Appliquer au projet » écrit le résultat examiné dans le projet, sans commit, et refuse tout si l'un des fichiers a changé dans le projet depuis le départ de la mission ; « Abandonner la copie » la retire sans toucher au projet ; une mission annulée perd aussi sa copie. Les hooks git de l'utilisateur ne sont pas lancés, aucune branche n'est créée. Un dossier sans git (ou sans commit) garde l'ancien fonctionnement, dans le projet. Vérifié par des tests (copie, application, conflit, mode exécutable, parcours complet dans la vue) et à l'écran : revue lue dans la copie, application au projet, copie retirée. Limites connues : les dépendances ignorées par git (`node_modules`…) ne sont pas dans la copie, l'agent doit les installer pour lancer la preview ou les tests ; une conversation acceptée ou abandonnée ne peut plus continuer (relancer une mission) ; la copie d'une conversation fermée sans décision reste sur le disque ; les dépendances entre missions sont reportées. **Codex** (livré, relecteur et implémenteur) : le pilote `solar-core/src/codex.rs` lance `codex exec --json`, un processus par tour, repris par l'identifiant de session, et traduit ses événements en ceux de Claude ; l'énumération `Agent` les présente derrière les mêmes gestes. L'agent se choisit dans le composer de l'accueil et reste noté sur la session (`provider`). Codex ne demande jamais de permission : sa sandbox, choisie à chaque tour, borne ce qu'il fait (plan → lecture seule, « Accepter les modifications » → écriture dans le projet, « Accès complet » → sans sandbox ; « Demander » n'est pas proposé). En mission, la fin d'un tour en lecture seule devient le plan à approuver ; l'approuver relance la même session en écriture. « Faire relire » propose Claude ou Codex (grisé s'il n'est pas installé). L'historique d'une conversation Codex est relu depuis son fichier de session. Vérifié par des tests sur de vrais tours enregistrés, un test de mission Codex complète avec un faux CLI, puis avec le vrai Codex : relecture en lecture seule avec verdict exact et aucun fichier touché, puis approbation qui reprend la session et corrige le fichier ; à l'écran, le choix de Codex sur l'accueil. Limites : pas de choix de modèle ni d'effort pour Codex (sa configuration s'applique), pas de commandes `/` ; une icône neutre remplace son logo, la page de marque d'OpenAI refusant les accès automatisés ; une conversation Codex n'a pas été vue à l'écran. **Grok** (livré, relecteur et implémenteur) : `grok -p --output-format streaming-json`, un processus par tour comme Codex ; le moteur commun `solar-core/src/turns.rs` tient la file des messages, l'interruption et le plan à approuver, chaque CLI ne fournissant que sa commande et sa traduction (`codex.rs`, `grok.rs`). Sans terminal, Grok refuse tout outil non autorisé d'avance : plan → mode plan de Grok (seul son fichier de plan s'écrit), « Accepter les modifications » → éditions de fichiers autorisées, commandes refusées, « Accès complet » → tout ; « Demander » n'est pas proposé. L'historique est relu depuis son journal de session (`updates.jsonl`). Vérifié par des tests sur de vrais tours enregistrés (plan, approbation, journal), puis avec le vrai Grok : plan sans toucher au projet, approbation qui reprend la session et corrige le fichier ; à l'écran, le choix de Grok et son menu de permissions. Limites : images non prises en charge pour Grok ; pas de choix de modèle ; icône neutre en attendant le logo de xAI.

Tous les points de la phase 6 sont livrés.

**Sortie de phase** : timeline corrélée, tâches non dupliquées, agent bloqué identifiable, interruption cohérente et conflits résolus avant acceptation. Aucun faux agent pour remplir un écran.

## Phase 7 — Gestion et extensions

Fiche projet, contexte/version des conventions, missions et sessions recherchables, réglages effectifs et configuration des agents. Puis, selon priorité produit, bibliothèque, bots/déclencheurs et GitHub. Ces ajouts ne doivent pas retarder la qualité du parcours principal.

**État (16 novembre 2026)** : premier incrément livré, la **recherche**. Une ligne « Rechercher ⌘K » de la barre latérale (ou ⌘K) ouvre une palette, sur le modèle de la commande de gpui-kit : champ, catégories (Tout, Sessions, Commandes, Fichiers, Réglages, avec leurs comptes ; Tab passe à la suivante), groupes, raccourcis affichés, pied « ↵ Ouvrir · ↑↓ Naviguer · esc Fermer ». Les sessions sont trouvées par projet, titre, plan, statut et le texte de leur conversation (Claude, Codex ou Grok), lu une fois en arrière-plan au début de chaque recherche. Les commandes reprennent les actions existantes (nouvelle session, nouveau projet, barre latérale, nouvelle session ou import par projet). Les fichiers sont ceux du dossier de la vue en cours, les derniers touchés par l'agent en tête ; un fichier choisi s'ouvre dans la vue Fichiers, désormais accessible hors mission (barre « Conversation · Fichiers » le temps de la vue). Les réglages sont ceux qui existent : agent du brouillon et mode de permission de la conversation. La page de preview est masquée tant que la palette est ouverte. Vérifié par un test (recherche et choix d'une ligne) et à l'écran, au clavier. Limites : pas de recherche approchée ni insensible aux accents ; les fichiers listés sont ceux d'un seul dossier ; la palette n'a été vue qu'en français, à 1280 × 800.

La distribution signée, mises à jour, Windows/Linux et client mobile sont des chantiers distincts. Leur infrastructure n'est pas présumée présente. Le mobile vise conversation, supervision et décisions consultables ; ce n'est pas un redimensionnement automatique de GPUI desktop.

## Qualité à chaque phase

Accessibilité, traduction, états erreur/vide/chargement, récupération et performance ne sont pas reportés à une phase finale. Polir le parcours réellement implémenté, puis élargir. Les motifs continus, lectures d'historique et scans de fichiers doivent rester compatibles avec une UI réactive.

### Validation par type de changement

Commandes ci-dessous : points d'entrée identifiés dans le dépôt, à exécuter au moment des changements applicatifs. Elles n'ont pas été exécutées pour l'ajout de ces documents.

| Changement | Vérification proportionnée |
|---|---|
| Documentation seule | Fichiers attendus, liens locaux, provenance des références, cohérence, empreintes des préexistants |
| Cœur/parsing/stockage | `cargo test -p solar-core` ; fixtures existantes et cas métier/migration réellement nouveaux |
| Code Rust modifié | `cargo fmt --all -- --check` et vérification de compilation adaptée ; puis tests concernés |
| Changement transversal UI/cœur | `cargo test --workspace` ; vérifier prérequis natifs avant de conclure d'un échec d'environnement |
| Interface | Vraie app avec état jetable, captures des états concernés, clavier, FR/EN, VoiceOver et tailles 1440 × 900, 1024 × 768, 720 × 480 |
| Orchestration/reprise | Faux adaptateur et fixtures déterministes pour dépendances, erreurs, permissions et interruption ; essai réel explicitement autorisé si nécessaire |
| Diff/rollback | Cas avec changements utilisateur, non suivis, aucun commit, conflit, revue périmée et interruption ; vérifier la préservation exacte des fichiers |
| Preview | Bon projet/origine, démarrage/échec, version périmée, fermeture et services préexistants préservés |

Pour ouvrir l'application lors d'un travail UI autorisé : `SOLAR_STATE=/tmp/solar-design-review.json cargo run -p solar`, en choisissant un nom de fichier temporaire libre. **Cette variable isole seulement l'état SOLAR** ; elle n'isole ni l'historique/configuration Claude, ni le dépôt choisi, ni les accès réseau ou la facturation. Utiliser un dossier de test et un faux CLI pour les scénarios qui ne nécessitent pas d'agent réel. Activer une session peut déjà démarrer `claude`.

Ne pas écrire de tests qui ne font que recopier un token visuel. Vérifier le comportement qui risque de régresser. Un build réussi ne prouve pas la qualité visuelle ; un screenshot ne prouve pas une permission correctement appliquée.

## Première unité de travail recommandée à Claude

Lire les documents, confirmer l'audit dans l'état courant, puis proposer un incrément **Home + conservation du composer et des onglets** : fichiers touchés, vues réutilisées, preuves de non-régression et états prévus. Résoudre le contrat de planification avant de brancher le lancement de missions. La présente livraison s'arrête à la documentation ; toute implémentation appartient à une demande suivante.

## Définition d'un incrément terminé

- Périmètre limité et cohérent avec la stack ; données et fonctions existantes préservées.
- Contrôles réels, textes FR/EN, états vides/erreurs, aucun terminal visible.
- Vérifications appropriées effectuées et résultats rapportés, sans confondre maquette, tests locaux et exécution réelle.
- Documents actualisés si un contrat change ; limites restantes nommées.
- Aucun commit, push ou publication comme effet de bord.
