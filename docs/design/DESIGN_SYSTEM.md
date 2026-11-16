# SOLAR — Design system Aerospace Minimalism

Version 1.0 · Spécification cible pour une application native Rust/GPUI. Les valeurs proposées ne sont pas encore implémentées et n'autorisent pas un remplacement massif du thème.

## Intention et identité

SOLAR doit donner une impression de maîtrise : surfaces calmes, texte net, contrôles précis, activité intelligible. La sobriété, la précision et la simplicité évoquées par SpaceX, Starlink et Tesla guident la qualité, sans reprise de logos, compositions ou signes distinctifs de ces marques.

L'originalité SOLAR repose sur un horizon solaire ouvert, sa déclinaison éclipse pour l'icône, un ambre discret et la transition naturelle de la conversation vers une mission. Le mode d'usage est **Operate** : accomplir une tâche. La marque s'exprime dans les détails, sans décor de cockpit, jargon spatial gratuit, quadrillage, particules ou halos derrière les conversations.

Les scènes spatiales monochromes conviennent à une identité marketing éventuelle ; elles ne sont pas nécessaires au workspace. Ne pas ajouter de dépendance ou de nouvelle iconographie pour illustrer artificiellement le thème.

## Compatibilité avec l'existant

Le thème actuel est dans `crates/solar/src/theme.rs` : noir, blanc spectral `#F0F0FA`, surfaces fantômes à 10 %, teinte du verre 0,92 et voile du contenu 0,55 (voir « Langage visuel retenu »). Les composants utilisent les couleurs du thème GPUI. Le système cible doit étendre ce point de centralisation.

Conserver le flou de fenêtre comme capacité native, avec des surfaces de lecture suffisamment opaques. Ne pas imposer un fond opaque partout uniquement pour faire correspondre une image de référence. Vérifier les contrastes sur le résultat composite avec un arrière-plan clair et sombre ; les hex seuls ne suffisent pas sur du verre.

## Couleurs sémantiques proposées

Noms conceptuels à mapper au thème existant, pas de nouvelle API GPUI imposée.

| Token | Valeur cible | Usage |
|---|---|---|
| `background` / Night | `#050505` | Fond de référence sombre |
| `surface` | `#0E0F12` | Zone de conversation et panneau de lecture |
| `surface.elevated` | `#1A1C20` | Menus, dialogues et panneau temporaire |
| `border.subtle` | `#2A2D33` | Séparation de groupes et panneaux, jamais seule indication d'un champ |
| `border.control` | `#666C75` | Contour nécessaire à l'identification d'un contrôle, à vérifier sur son fond |
| `text.primary` / Ice | `#F5F5F2` | Texte principal |
| `text.secondary` / Titanium | `#9BA1A8` | Métadonnées encore lisibles et descriptions |
| `accent.solar` | `#E9B86B` | Action principale, repère de sélection, signature |
| `text.onAccent` | `#050505` | Texte sombre sur l'ambre |
| `signal.success` | `#76E6A1` | Résultat confirmé, test réussi |
| `signal.error` | `#FF7373` | Échec ou opération destructrice |
| `signal.warning` | `#E9B86B` | Avertissement avec icône et libellé explicites |
| `signal.info` | `#B7C5D3` | Information non bloquante |

L'accent n'est pas la couleur de tout le texte ou de chaque badge. Un bouton ambre principal au plus par région de décision. Le statut d'un agent n'emprunte pas l'accent sans texte qui précise sa signification. Les arrière-plans de diff utilisent des teintes discrètes ; les signes `+`/`−`, numéros de lignes et légendes rendent ajouts/suppressions compréhensibles sans couleur.

Hover : un niveau de surface plus clair et un contour lisible. Pressé : retour immédiat sobre. Sélection : fond teinté plus repère et libellé ; focus clavier : anneau de 2 unités logiques avec séparation de 2 unités, indépendant du survol. Désactivé : libellé lisible, motif d'indisponibilité accessible, aucune action déclenchée.

## Langage visuel retenu (décision du 14 novembre 2026)

Après comparaison de trois variantes dans l'app (SpaceX, Starlink, xAI), l'utilisateur a retenu **l'inspiration Starlink**. Les valeurs viennent d'un relevé de leur design system réel : [références Starlink](references/starlink-design-system.md). Ni logo, ni nom, ni image de la marque. Ces décisions remplacent les valeurs proposées plus bas quand elles divergent ; leur source de vérité dans le code est `crates/solar/src/theme.rs`.

- **Logo** : le kit d'identité officiel (`assets/logo/identity-kit/`), issu de la « direction 03 » : un arc d'horizon lumineux et un mot-symbole sur mesure, en noir et blanc. Voir `assets/logo/README.md`. Slogan : « Engineered to Build », toujours en anglais, absent de l'interface (voir `PRODUCT_BRIEF.md`).
- **Police** : Space Grotesk (libre, OFL 1.1, embarquée depuis `assets/fonts/space-grotesk`), recommandée par le kit d'identité et retenue le 15 novembre 2026. Elle remplace D-DIN, la DIN libre choisie d'abord pour rester proche de la NeoDIN de Starlink. Les messages et la saisie restent dans la police du système.
- **Casse normale et gras** pour les titres, les noms et les états. Les capitales (`theme::caps`) sont réservées à de tout petits intitulés de section, sans interlettrage.
- **Couleur** (révisée le 15 novembre 2026 d'après le design system « SpaceX » d'open-design, voir [la référence](references/spacex-open-design.md)) : fond noir `#000` sur le flou de la fenêtre, texte blanc spectral `#F0F0FA` à 100 et 70 %, surfaces et filets « fantômes » en blanc spectral à 10 %, contour des contrôles à 35 %. Aucune carte pleine : le composer et les listes reposent sur le fond. Aucune couleur d'accent, et plus d'ambre : ce qui est actif ou principal est blanc. Vert et rouge pour les seuls états ; les logos des fournisseurs d'agents gardent leur couleur de marque (`assets/providers/README.md`), et le logo d'un projet (barre latérale, fiche) s'affiche tel quel, dans ses couleurs d'origine ; « en cours » et « en attente » se distinguent par l'icône, la forme du point et le texte.
- **Géométrie** (révisée le 16 novembre 2026, d'après les menus de Monocode Native : `popover.rs`, `conversation.rs`, `modal.rs`) : rayon de 12 pour les contrôles, le composer et le cadre des menus et popovers ; 8 pour les lignes surlignées d'un menu ; 16 pour les dialogues. gpui-kit n'ayant qu'un rayon global, boutons et champs prennent aussi 12 : un petit bouton peut approcher d'une pilule, c'est accepté. Pas de pilule voulue pour autant (les amorces de l'accueil, retirées le 15 novembre 2026, ne reviennent pas).
- **Accueil** (15 novembre 2026, d'après les planches 1 et 3, puis épuré) : centré verticalement, un message d'accueil à gauche (« Bonjour {prénom}, » en 40, gras, puis « Prêt à construire ? » et l'état de Claude), puis le composer, rien d'autre ; les conversations restent dans la barre latérale. Derrière, une planète photoréaliste en noir et blanc, calée sur le message d'accueil : son limbe monte en travers à sa droite. Le composer porte un voile noir à 80 % (`theme::VEIL`) pour rester lisible sur l'image. En fenêtre étroite, l'image est simplement recadrée par le bord droit.
- **Fil de conversation** : ton message à droite, sans avatar ; seul le logo du fournisseur signe les réponses. Pas de ligne projet/état au-dessus du fil (la barre latérale situe la conversation, l'indicateur de travail et la carte de permission disent l'état). Les outils montrent un fichier relatif au projet, ou ses deux derniers éléments s'il est ailleurs. Les blocs de code sont cernés d'un filet fantôme, sans fond.
- **Outils** : une icône Lucide par sorte d'outil (crayon pour écrire, œil pour lire, loupe pour chercher…), une roue pendant qu'il tourne, une alerte rouge s'il échoue.
- **Envoi** : un simple contour tant qu'il n'y a rien à envoyer, blanc plein dès qu'il y a du texte.
- **Onglets** : le titre de l'onglet actif est souligné d'un trait blanc fin qui s'arrête avant sa croix, sans fond ; la croix n'apparaît que sur lui.
- **Un seul noir et sélection** : la barre latérale porte le même voile que la zone centrale. Les éléments au repos sont en blanc à 70 % ; la sélection passe en blanc plein, sans bloc gris ; la conversation affichée porte une petite flèche blanche (chevron), « Accueil » un trait fin à gauche.
- **Sans projet** : l'accueil remplace le composer par un bouton « Choisir un dossier ».
- **Espace de travail d'une mission** : une barre de 40 sous les onglets, sur fond noir et filet fantôme : icône de mission, titre en gras, état en blanc à 70 %, bascule Conversation / Plan (boutons fantômes, la vue affichée en blanc plein), Arrêter quand Claude travaille. La vue Fichiers : aperçu du fichier à gauche avec son chemin en blanc à 70 %, arborescence de 280 à droite derrière un filet fantôme. La vue Plan reprend le plan en Markdown puis l'activité, en groupes (Fichiers modifiés, Commandes, Fichiers lus) avec les icônes des outils ; un échec est en rouge.
- **Revue d'une mission** (vue Modifications) : en tête, la base en blanc à 70 %, la validation (vert réussis, rouge échoués, gris périmés ou non exécutés), puis Accepter (bouton blanc) et Restaurer (fantôme, confirmation en ligne). Diff unifié de gpui-kit dessous ; liste des fichiers à droite comme l'arborescence de la vue Fichiers, ajout en vert, suppression en rouge.
- **Preview** : une ligne d'état sous la barre de mission (blanc à 70 %, rouge en cas d'échec) avec Recharger et Arrêter le serveur en boutons fantômes, puis la page web sur toute la largeur. Sans page : texte d'explication, bouton blanc « Lancer la preview », champ d'adresse.
- **Mouvement** : la planète de l'accueil apparaît en fondu (1,2 s) ; GPUI saute l'effet quand « Réduire les animations » est activé dans macOS.
- **Moins d'informations à l'écran** : le composer ne montre que le projet et le modèle ; permissions et effort sont dans un menu Réglages (icône curseurs). L'accès complet, mode par défaut où Claude écrit sans demander, reste signalé par un bouclier. Dans la barre latérale, l'ancienneté des conversations et le menu « ··· » du projet n'apparaissent qu'au survol de la barre ; les points d'état restent visibles, et l'ajout de projet est le + de l'intitulé PROJETS.
- **Listes** : lignes pleine largeur sans fond, icône, titre gras, précision secondaire, chevron ; le survol pose le voile fantôme.
- **État** : dit en toutes lettres à côté d'un point de couleur, à partir de l'état réel du CLI.

Variantes écartées : SpaceX (capitales partout, angles de 2 unités), xAI (pilules, police du système), Apple (gris de macOS, bleu système, police du système ; trop proche des apps de conversation existantes) et les capitales partout du design system « SpaceX » d'open-design, dont seules les couleurs et les boutons fantômes ont été retenus.

## Typographie

| Usage | Intention | Repli et contraintes |
|---|---|---|
| Titres, navigation, boutons | Space Grotesk, regular, medium et bold, en casse normale | Embarquée ; police du système si le chargement échoue |
| Conversation, saisie | Police du système | Lisibilité des textes longs ; préserver le rendu GPUI |
| Code, chemins, identifiants | JetBrains Mono, graisse 400–500 | Monospace du thème existant |

Ne pas embarquer d'autre police sans vérifier sa licence, ses variantes et son chargement dans l'app. JetBrains Mono n'est pas embarquée : le code utilise la police à chasse fixe du thème. Chiffres tabulaires pour compteurs et durée si supportés.

Échelle cible en unités logiques de l'interface native : 12 pour métadonnées non essentielles, 14 pour contrôles, 15–16 pour lecture de conversation, 20 pour titres de section, 40 pour le message d'accueil. Interligne 1,4–1,6 pour le contenu, 1,1–1,3 pour les titres. Le contenu utile ne descend pas sous 12. Les noms de fichiers et chaînes longues doivent rester copiables et consultables en entier.

Le wordmark SOLAR utilise un espacement mesuré. Les capitales sont réservées aux petits intitulés de section ; jamais les titres, les boutons, les états, les messages ni les données de l'utilisateur. Les textes FR/EN restent dans `crates/solar/locales/app.yml`, sans chaîne visible ajoutée directement dans un rendu Rust.

## Géométrie et densité

| Élément | Proposition de départ |
|---|---|
| Unité | 4 ; espacements usuels 8, 12, 16, 24, 32, 48 |
| Sidebar | 240, rétractable ; rail compact autour de 56 si utile |
| Barre de contexte | Environ 56, compatible avec la barre de titre native |
| Conversation | Largeur de lecture maximale autour de 760, déjà cohérente avec le composer existant |
| Panneau contextuel | 280–360 si l'espace disponible le permet |
| Champs/boutons usuels | Hauteur 36–40 ; cible interactive élargie si icône seule |
| Rayons | 12 pour les contrôles, le composer et les menus ; 8 pour les lignes de menu ; 16 pour les dialogues |
| Bordures | 1 unité logique, alignement net aux facteurs d'échelle usuels |
| Ombres | Aucune dans le contenu ; hiérarchie par contours fantômes et espacement |

Ces dimensions sont des repères à éprouver dans GPUI, pas des valeurs CSS à coller. Conserver le minimum natif actuel de 720 × 480. Quand le contenu manque d'espace, fermer un panneau ou changer la disposition avant de réduire le texte. La conversation garde une place prioritaire ; il n'y a pas trois colonnes permanentes.

## Logo et icônes

- La source du logo est le kit d'identité officiel, `assets/logo/identity-kit/` (logo maître, mot-symbole, arc, icône macOS, PNG). Voir `assets/logo/README.md` pour la provenance et les usages ; les versions précédentes sont archivées. Dans l'app, `eclipse.rs` affiche le mot-symbole du kit dans la barre de titre.
- Copies de référence disponibles dans [references](references/README.md). Ne pas utiliser ces copies comme nouvelle source de production.
- Horizon monochrome compact pour les petits usages, signature à horizon pour la marque et icône éclipse pour l'app. Pas de halo répété sur les panneaux.
- L'accueil affiche la planète `assets/logo/solar-planet.png` (provenance dans `assets/logo/README.md`). Ce n'est pas un logo. Elle remplace le grand horizon `solar-horizon.png`, conservé mais plus affiché.
- Privilégier les icônes déjà disponibles via les composants natifs. Le langage linéaire de Lucide cité dans le blueprint est une intention ; ne pas ajouter une bibliothèque web à GPUI.
- Toute icône actionnable possède un nom accessible et une infobulle. Ne pas inventer un logo définitif, un monogramme ou une variante approuvée.

## Contrats des composants

| Composant | Contenu et comportement | États nécessaires |
|---|---|---|
| Bouton | Verbe précis, action réelle, hiérarchie primaire/secondaire/discrète/danger | Normal, hover, focus, pressé, chargé, désactivé |
| Champ | Label, aide courte, erreur près du champ ; ne pas dépendre du placeholder | Vide, rempli, focus, invalide, lecture seule |
| Composer | Texte multiligne, projet, modèle/mode réels, pièces jointes, envoyer/arrêter | Brouillon, envoi, streaming, erreur récupérable, permission en attente |
| Navigation | Destination active nommée, repli sans perte de contexte | Vide, actif, compact, débordement |
| Ligne projet/mission | Nom, contexte, dernière activité ou état exact | Récent, actif, indisponible, nom long |
| Agent et affectation | Rôle, provider/modèle disponibles, tâche, statut | Inactif, réflexion, en cours, attente, erreur |
| Plan | Objectif, tâches ordonnées, dépendances et résultat attendu | Proposition, modifié, approuvé, bloqué, obsolète |
| Badge d'état | Icône + texte ; pas seulement un point coloré | États métier explicites |
| Timeline | Faits horodatés, agent et tâche identifiables, détails repliés | Nouveau, historique, échec, attente utilisateur |
| Fichiers et code | Chemin complet accessible, lecture, état modifié ; pas d'édition implicite | Vide, chargement, binaire, trop volumineux, erreur |
| Diff | Fichiers, base, ajouts/suppressions, décision sur version exacte | Aucun changement, prêt, obsolète, conflit, non textuel |
| Résultats de validation | Intitulé, état, heure, détails techniques facultatifs | Non exécuté, en cours, réussi, échoué, interrompu |
| Preview | Origine et version visées, chargement, recharger/ouvrir si possible | Non supporté, indisponible, chargement, prêt, échec |
| Dialogue de permission | Action, portée, conséquence et choix explicites | Attente, réponse en cours, refus, erreur d'envoi |
| Palette de commandes | Destinations et actions produit nommées, sans prompt shell | Recherche, vide, action indisponible |

Commencer avec les composants GPUI existants. Introduire un composant SOLAR seulement lorsqu'un contrat partagé le justifie ; ne pas créer une seconde bibliothèque UI ou dupliquer le composer.

## Animation et feedback

- Feedback direct : 100–150 ms ; ouverture de panneau : 180–240 ms ; transformation Home/workspace : 220–300 ms, sans retarder l'action.
- « Solar Pulse » : variation discrète d'opacité d'un indicateur actif ; le texte de statut suffit lorsque l'animation est coupée.
- « Solar Flash » : accent bref sur une livraison réussie, sans flash lumineux ni plein écran.
- Pas d'animation de pourcentage inventé, de token décoratif, de scroll forcé pendant une lecture ou de panneau qui vole le focus.
- Respecter la réduction des animations et de la transparence via les capacités natives à vérifier. Prévoir une alternative statique et opaque ; ne pas supposer que le support existe déjà.
- Réserver les animations continues à la seule zone active, les arrêter hors vue et éviter de reconstruire toute la fenêtre à chaque événement.

## Accessibilité et critères de qualité

Cibles de conception : contraste texte courant au moins 4,5:1, gros texte 3:1, limites/indicateurs indispensables 3:1. Mesurer les couples effectivement rendus, y compris le flou, les états de focus et les teintes de diff ; ces seuils ne sont pas une certification déjà obtenue.

Tester le parcours clavier complet, l'ordre de focus, les noms/rôles exposés à l'accessibilité native, le retour du focus après fermeture d'un dialogue et les annonces d'état avec VoiceOver. Ne pas prescrire d'attributs DOM/ARIA à une interface GPUI. Conserver les conventions macOS ; des raccourcis futurs doivent éviter les collisions natives et être affichés.

Les erreurs expliquent ce qui a échoué et la prochaine action possible : « Claude n'a pas pu démarrer. Réessayer » avec détails dépliables. Les confirmations parlent du résultat réel : « Changements prêts à examiner », « Tests non exécutés ». Aucun « Système opérationnel » sans vérification correspondante.

## Vérification visuelle lors de l'implémentation

Capturer la vraie app pour Home, Planning, Conversation, Modifications et les états erreur/permission. Examiner 1440 × 900, 1024 × 768 et 720 × 480 en unités logiques, avec noms longs, FR/EN et arrière-plans clair/sombre derrière le verre. Les croquis du document UX ne remplacent pas cette vérification. Le client mobile restera un chantier distinct.
