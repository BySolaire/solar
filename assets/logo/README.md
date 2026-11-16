# Solar — logo

## Source de vérité : le kit d'identité officiel

`identity-kit/` est le **Solar Official Identity Kit** fourni par l'utilisateur le 15 novembre 2026, copié tel quel. Sa notice (`identity-kit/README.md`, en anglais) décrit chaque fichier et son usage ; elle fait foi pour la marque.

- **Provenance** : archive `SOLAR_Official_Identity_Kit.zip` (SHA-256 `f481e7c0427b7ddae549d421548984a13ce0f0565ebf163f4c9ffadd7929ea88`), partagée depuis la bibliothèque ChatGPT de l'utilisateur.
- **Contenu** : logo maître et variantes en SVG, mot-symbole seul, arc seul, icône macOS (`.icns` et `.iconset`), PNG haute définition, favicons, référence approuvée et comparaison avec le vecteur.
- **Lettrage** : silhouettes dessinées en tracés (S et L à coupes à 45°, A sans traverse, R semi-ouvert), aucune police nécessaire. Ne pas le redessiner séparément.

## Usage dans l'app

`crates/solar/src/assets.rs` embarque :

- `identity-kit/assets/svg/solar-wordmark.svg`, le mot-symbole affiché seul dans la barre de titre, comme le prévoit le kit pour l'interface ;
- `solar-planet.png`, voir ci-dessous.

`macOS/Solar.icns` servira d'icône du Dock quand l'app sera empaquetée (étape de diffusion) ; une app lancée avec `cargo run` garde l'icône générique.

## Planète de l'accueil

`solar-planet.png` (1800 × 900, blanc sur fond transparent) est le décor de l'accueil depuis le 15 novembre 2026 : le limbe d'une planète, en travers, comme sur les planches 1 et 3 (`docs/design/references/`). Ce n'est pas un logo.

- **Provenance** : `solar-planet-source.png` (1774 × 887), image générée le 15 novembre 2026 par le modèle d'image d'OpenAI, via le CLI Codex de l'utilisateur, avec cette demande : « Photorealistic view of a planet's horizon seen from low orbit, strictly black and white grayscale, no color at all. Wide landscape format. Pure black empty space fills the top half, no stars, no sun, no lens flare. A thin, brilliant white atmospheric limb glow traces a gentle wide arc […]. Just below the bright rim, the dark planet surface shows subtle soft grey cloud swirls […], fading into deep black toward the bottom of the image. […] no text, no spacecraft, no borders. »
- **Traitement** : luminance convertie en opacité (blanc pur, le noir devient transparent), fondus sur les bords gauche, droit et bas, puis rotation de −14°, agrandissement de 1,25, décalage de (250, 150) dans un cadre de 1800 × 900, fondu vers le bas à partir de 45 % de la hauteur et vers la droite à partir de 78 % de la largeur.
- SHA-256 : `solar-planet.png` `28f71f841ef1a174a965001854e14a0ea7a0d3e8b0973b6ac1e143ff44a5df86`, `solar-planet-source.png` `f950142f483bab4ac34cfb24e4e5eb58c3f878c50d7f83d87727919cddc5b6c5`.

## Ancien grand horizon d'ambiance

Plus affiché depuis l'arrivée de la planète, conservé. `solar-horizon.svg` et `solar-horizon.png` ne font pas partie du logo : c'est le décor de l'accueil, derrière le composer, d'après le bas de la planche « direction 03 » (`docs/design/references/direction 03.jpg`). Le limbe d'une planète vue de l'espace, en lumière seule sur fond transparent ; l'accueil l'étire à la largeur de sa zone. L'app embarque le PNG (export à 2× du SVG), car le moteur SVG de GPUI ne rend pas ses masques et ses grands flous.

## Versions précédentes

- `archive/pre-identity-kit-2026-11-15/` : version « direction 03 » redessinée à la main, avant le kit.
- `archive/pre-direction-03-2026-11-14/` : essai dans le vocabulaire de Starlink.
- `archive/pre-starlink-2026-11-14/` : horizon à dégradé et point ambre, icône éclipse.
- `archive/pre-horizon-2026-11-14/` : première éclipse ambre.
