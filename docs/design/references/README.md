# Solar — Références visuelles retenues

Sélection réajustée par l'utilisateur le 14 novembre 2026. Les **trois planches ci-dessous sont les références actives**. Les explorations précédentes sont conservées dans `archive/`.

## Les trois planches actives

1. [Planche 1 — UX adaptative](<planche 1.png>) : Home, Agent Workspace et Modifications ; horizon ouvert au-dessus du mot Solar.
2. [Planche 2 — Aerospace](<planche 2.png>) : sobriété, titane, variantes de marque et icône éclipse. Les mentions de terminal dans cette recherche ne remplacent pas la règle produit sans terminal visible.
3. [Planche 3 — Accueil conversationnel](<planche 3.png>) : identité à horizon solaire, typographie espacée, accueil et projets récents.

Les planches restent des concepts, pas des captures de l'application actuelle. Leurs chiffres, modèles, tarifs/crédits, boutons et intégrations sont illustratifs. Une maquette autonome Mission Planning reste à détailler à partir de [l'architecture UX](../UX_ARCHITECTURE.md).

## Logo refait à partir de cette sélection

Les nouveaux fichiers éditables sont dans [`assets/logo/`](../../../assets/logo/README.md) :

- [Signature principale](../../../assets/logo/solar-lockup.svg) : horizon ouvert, lumière chaude localisée et mot-symbole Solar.
- [Mot-symbole](../../../assets/logo/solar-wordmark.svg) et [horizon seul](../../../assets/logo/solar-mark.svg), sur fond transparent.
- [Symbole compact](../../../assets/logo/solar-glyph.svg), monochrome et renforcé pour les petites tailles.
- [Icône application](../../../assets/logo/solar-icon.svg), dérivée de l'éclipse de la planche 2.
- [Signature monochrome](../../../assets/logo/solar-lockup-mono.svg) pour fond clair.

Les anciens SVG de production sont archivés dans `assets/logo/archive/pre-horizon-2026-11-14/`. Les copies `solar-icon-existing.svg` et `solar-glyph-existing.svg` ci-dessous restent des références historiques et ne sont pas mises à jour pour imiter les nouvelles sources.

## Direction 03 — identité retenue

[Planche « Brand identity — direction 03 »](<direction 03.jpg>), fournie par l'utilisateur le 14 novembre 2026 : un seul arc d'horizon et le mot Solar. Elle a abouti au kit d'identité officiel, désormais source de vérité du logo (`assets/logo/identity-kit/`). Depuis le 15 novembre 2026, le décor de l'accueil suit plutôt les planches 1 et 3 : une planète dont le limbe monte en travers (`assets/logo/solar-planet.png`) ; l'interface, elle, suit le relevé Starlink ci-dessous.

## Référence Starlink

[Relevé du design system de Starlink](starlink-design-system.md), fait le 14 novembre 2026 sur leur site et les captures de leur app : tokens, casse, listes, états. C'est l'inspiration retenue pour le langage visuel : police, casse et géométrie.

## Référence « SpaceX » d'open-design

[Couleurs et boutons fantômes](spacex-open-design.md), repris le 15 novembre 2026 du design system communautaire `nexu-io/open-design` (Apache-2.0) : noir, blanc spectral, surfaces à 10 %, contours à 35 %. Ses capitales partout et sa police D-DIN ont été écartées.

## Archives de la conversation

Quatre des images originales, générées pour le projet dans une conversation ChatGPT, sont conservées sans modification (deux autres, qui portaient des informations personnelles, ont quitté le dépôt le 16 novembre 2026) :

- [Concept UX original](archive/solar-ux-concept.png).
- [Identité Agent Orchestrator](archive/solar-brand.png), avec des propositions de fonctions antérieures au brief.
- [Aerospace V2](archive/solar-aerospace-v2.png), ancienne recherche avec terminal.
- [Workspace initial](archive/solar-agent-workspace-early.png), ancienne recherche avec terminal.

Le [manifeste](manifest.json) distingue ces archives originales des trois planches sélectionnées/réajustées par l'utilisateur. Il indique chemins actuels, dimensions, tailles et empreintes.

## SVG historiques du dépôt

- [Icône existante](solar-icon-existing.svg), copie initiale de `assets/logo/solar-icon.svg` avant la refonte du logo.
- [Symbole existant](solar-glyph-existing.svg), copie initiale de `assets/logo/solar-glyph.svg` avant la refonte du logo.

## Interprétation pour l'implémentation

L'identité Solar reste originale. Suivre Home → Mission Planning → Agent Workspace, la stack native Rust/GPUI et les tokens du design system. Ne pas copier les fonctions illustratives des images, réintroduire un terminal ou ajouter une migration de framework. Les trois fichiers fournis par l'utilisateur et les archives n'ont pas été retouchés lors de la création du logo.
