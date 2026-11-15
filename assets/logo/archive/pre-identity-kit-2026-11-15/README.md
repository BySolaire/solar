# SOLAR — logo

Version du 14 novembre 2026, d'après la planche « Brand identity — direction 03 » choisie par l'utilisateur (`docs/design/references/direction 03.jpg`) : un seul arc d'horizon lumineux et un mot-symbole large au A ouvert, en noir et blanc. Aucune couleur.

- `solar-lockup.svg` : signature principale, l'arc au-dessus de SOLAR, blanche sur fond transparent.
- `solar-lockup-mono.svg` : la même en noir et sans halo, pour fond clair.
- `solar-mark.svg` : l'arc seul, avec son halo.
- `solar-wordmark.svg` : mot SOLAR seul. Prend la couleur du texte (`currentColor`).
- `solar-glyph.svg` : symbole compact pour 16 à 32 px, l'arc plein. Prend la couleur du texte.
- `solar-horizon.svg` et `solar-horizon.png` : grand horizon d'ambiance, le limbe d'une planète vue de l'espace (liseré éclatant, surface éclairée et nuageuse, brume légère), d'après le bas de la planche « direction 03 ». Il ne contient que de la lumière sur fond transparent, pour se fondre dans le fond translucide de l'app ; l'accueil l'étire à la largeur de sa zone. C'est le décor de l'accueil, derrière le composer ; ce n'est pas un logo. L'app embarque le PNG (export à 2× du SVG), car le moteur SVG de GPUI ne rend pas ses masques et ses grands flous.
- `solar-icon.svg` et `solar-icon.png` : icône de l'app, l'arc sur une tuile sombre. Le PNG est un export du SVG.

## Construction

- **Arc** : un croissant très fin, plein au centre, qui s'efface vers ses pointes par un dégradé de transparence. Son halo est une copie floutée.
- **Lettrage** : cinq lettres larges et très espacées, dessinées au trait d'épaisseur constante, A sans traverse. Ce sont des tracés originaux : aucun fichier de police n'est nécessaire, et aucune police n'a servi de modèle.
- **Couleurs** : blanc `#FFFFFF` et noir `#000000`.

Conserver des marges autour de la signature. Utiliser le symbole compact plutôt que réduire la signature entière.

## Usage dans l'app

`crates/solar/src/assets.rs` embarque le symbole compact, le mot-symbole et le grand horizon ; `eclipse.rs` les affiche dans la barre de titre et sur l'accueil.

## Versions précédentes

- `archive/pre-direction-03-2026-11-14/` : essai dans le vocabulaire de Starlink (arc double, point, lettrage D-DIN gras).
- `archive/pre-starlink-2026-11-14/` : horizon à dégradé et point de lumière ambre, icône éclipse.
- `archive/pre-horizon-2026-11-14/` : première éclipse ambre.
