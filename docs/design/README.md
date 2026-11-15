# SOLAR — Référence produit et design

Version 1.0 · 14 novembre 2026 · Documentation de conception, sans changement de l'application.

Ce dossier transmet à Claude la direction retenue pour SOLAR : une application native de développement agentique, conversation d'abord, qui s'adapte de **Home → Mission Planning → Agent Workspace**, puis présente une livraison à examiner. L'interface ne comporte **aucun terminal visible**.

## Ordre de lecture

1. [Audit du dépôt](EXISTING_PROJECT_AUDIT.md) : stack réelle, capacités présentes, limites et points à préserver.
2. [Brief produit](PRODUCT_BRIEF.md) : objectifs, entités, périmètre et critères de réussite.
3. [Design system](DESIGN_SYSTEM.md) : Aerospace Minimalism, tokens proposés et composants.
4. [Architecture UX](UX_ARCHITECTURE.md) : parcours, maquettes fonctionnelles et états.
5. [Plan de développement](DEVELOPMENT_PLAN.md) : incréments, dépendances et vérification.
6. [Références visuelles](references/README.md) : planches originales récupérées, provenance et archives.

Les règles permanentes pour Claude se trouvent dans [CLAUDE.md](../../CLAUDE.md).

## Statut et autorité

- **Décision produit** : choix explicite de l'utilisateur dans la demande du 14 novembre 2026, enrichi par la conversation « Créer logo Solar ».
- **Existant vérifié** : lecture des fichiers du dépôt à cette date. Il ne s'agit pas d'une validation de l'application exécutée.
- **Cible** : comportement ou composant à développer. Les documents ne prouvent pas sa présence dans le code.
- **Proposition technique** : point de départ à confirmer par un petit incrément dans la stack existante.
- **À compléter** : asset ou capacité indisponible, indiqué explicitement.

Les instructions actuelles de l'utilisateur priment. Pour le produit et le design, ce dossier formalise la nouvelle direction ; il ne donne pas mandat pour réécrire l'application. Le code décrit l'état réellement implémenté. Le README racine conserve une ancienne roadmap qui mentionne un terminal : **cette intention est remplacée par la décision sans terminal visible**, sans modification de ce README dans cette livraison.

## Source et limites

Conversation source : `(conversation privée)`, « Créer logo Solar ». Le premier export textuel retournait cinq tours sans pièce jointe. Un complément dans le navigateur connecté a permis de récupérer six planches originales, dont les messages plus anciens : elles sont désormais dans `references/`, avec provenance et empreintes. La sélection active réajustée par l’utilisateur est maintenant `planche 1.png`, `planche 2.png`, `planche 3.png` ; les six originaux sont conservés dans `references/archive/`. Le nouveau logo vectoriel est documenté dans `assets/logo/README.md`.

Les mentions de T3 Code et Grok Bot désignent les usages souhaités par l'utilisateur : travail sur des projets et conversation simple. Elles ne constituent ni un audit actuel de ces produits, ni un engagement d'intégration. Même principe pour SpaceX, Starlink et Tesla : inspiration de sobriété et de précision, identité SOLAR originale.

## Périmètre livré

Des fichiers de documentation, deux copies des SVG existants et, en complément, six planches PNG originales et leur manifeste dans `references/`. Le premier lot était documentaire. Le logo a ensuite été refait à la demande de l'utilisateur depuis ses trois planches : les deux anciennes sources SVG sont archivées, et la nouvelle famille se trouve dans `assets/logo/`. Aucun code applicatif, manifeste Cargo, lockfile ou donnée utilisateur n'est modifié par ce complément. Aucun commit ni changement d'index Git n'est demandé.

Les schémas et maquettes textuelles sont des spécifications fonctionnelles. Les six planches conceptuelles de la conversation sont maintenant disponibles en référence ; elles ne sont pas des captures d'une interface exécutée. Une maquette détaillée autonome de Mission Planning reste à produire. Les planches historiques qui montrent un terminal ne priment pas sur la décision sans terminal visible.
