# « SpaceX » d'open-design — référence de couleurs et de boutons (15 novembre 2026)

Source : [`design-systems/spacex`](https://github.com/nexu-io/open-design/tree/main/design-systems/spacex) du dépôt `nexu-io/open-design` (licence Apache-2.0), dernier commit du dossier `f1a0b60c6cd1c9f5c735ae5645e47244b468e71c` (18 août 2026), lu le 15 novembre 2026 (`DESIGN.md`, `USAGE.md`, `tokens.css`).

Le paquet se présente comme « inspiré de SpaceX » et précise lui-même qu'il vient d'un jeu de données préparé, pas d'un relevé du site d'origine. Ce n'est donc pas une source officielle : on s'en sert comme d'une proposition de design, sans logo, nom ni image de la marque.

## Valeurs reprises dans SOLAR

| Domaine | Valeur | Dans SOLAR |
|---|---|---|
| Fond | noir pur `#000000` | teinte de la fenêtre, sur le flou natif conservé |
| Texte | blanc « spectral » `#F0F0FA`, secondaire à 70 % | `SPECTRAL`, `SECONDARY` |
| Bouton fantôme | fond `#F0F0FA` à 10 %, contour à 35 %, rayon 32 | amorces de l'accueil (`STARTER_RADIUS`) |
| Séparations | `#F0F0FA` à 10 % | filets, sélection, survol (`GHOST`) |
| Survol / appui d'un bouton principal | `#FFFFFF` / `#D8D8E6` | `HOVER`, `PRESSED` |
| États | vert `#16A34A`, rouge `#DC2626`, jaune `#EAB308` | `success`, `danger`, `warning` |
| Ombres et cartes | aucune | le composer et les conversations récentes reposent sur le fond |
| Titre principal | 48 px, gras, interligne 1 | titre de l'accueil |

## Valeurs écartées

- **Capitales partout et interlettrage positif** : essayés le 15 novembre 2026, ils tronquaient les libellés et gênaient la lecture des titres de conversation. SOLAR garde la casse normale et les capitales pour les seuls petits intitulés. GPUI ne sait de toute façon pas espacer les lettres.
- **Police D-DIN** : SOLAR garde Space Grotesk.
- **Photographie plein écran** : remplacée par l'horizon du kit d'identité.
