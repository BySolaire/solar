# Starlink — relevé du design system (14 novembre 2026)

Relevé factuel, fait à la demande de l'utilisateur pour servir de référence à Solar. On s'en inspire ; on ne reprend ni le logo, ni le nom, ni les images, ni la police propriétaire.

## Sources

- **Site** `https://starlink.com/` : sa page d'accueil charge une bibliothèque de composants interne, « starlink-ui » (composants préfixés `SX` : `SXCard`, `SXInput`, `SXDialog`, `SXBadge`, `SXSegmentedControl`…). Les valeurs ci-dessous sont lues dans ses 40 feuilles de style (`/assets-new/*.css`).
- **Rendu du site** : capture de la page d'accueil en 1440 × 900.
- **App mobile** : les cinq captures officielles de la fiche App Store (`apps.apple.com/us/app/starlink/id1537177988`).

Non consulté : l'app elle-même en fonctionnement, la version Android, d'éventuelles règles de marque internes.

## Tokens relevés (thème sombre)

| Domaine | Valeurs |
|---|---|
| Police | `NeoDIN` (regular 400, bold 700, et leurs italiques) ; chasse fixe `Roboto Mono` |
| Fond | `#000` |
| Texte | blanc à 100 %, 73 % (secondaire) et 54 % (tertiaire) ; désactivé 45 % |
| Surfaces | `#2E2E2E` à 70 %, 50 % et 30 % |
| Trait de séparation | blanc à 25 %, 1 px |
| Couleurs d'état | vert `oklch(77% .154 146)` ≈ `#6CCE76`, rouge ≈ `#ED524A`, jaune ≈ `#FFD555`, bleu ≈ `#3E80E0` |
| Rayons | 0, 2, 4, 8, 16, 9999. Cartes, champs, badges et listes : **4**. Dialogues et contrôles segmentés : **8** |
| Espacements | 2, 5, 10, 15, 20, 30, 70, 110 (échelle compacte : 2, 4, 6, 8, 12, 20, 32, 56) |
| Corps de texte | 17 / 15 / 13 px (compact : 14 / 12 / 10) |
| Titres | 50, 35, 24, 19, 18 px |
| Boutons | hauteur 42 (moyen) et 32 (petit) ; principal blanc à texte noir |
| Focus | anneau de 4 px : 2 px de fond, puis blanc à 54 % |
| Flou | 8 px (widgets) et 16 px (calques) derrière des surfaces translucides |
| Transitions | 90 ms (clic), 180 ms, 250 ms ; courbes `cubic-bezier(.5, 0, 0, 1)` et `(0, .5, 0, 1)` |

## Ce que montrent les captures

- **Casse** : titres et boutons en casse normale, **en gras**. Les capitales sont rares : une seule règle dans tout le CSS (`letter-spacing: .04em`), pour de tout petits intitulés.
- **État** : l'app ouvre sur un point vert suivi de « Online », en gras. L'état se lit en toutes lettres.
- **Listes** : des lignes pleine largeur, icône à gauche, titre gras, précision en texte secondaire, chevron à droite.
- **Cartes** : remplies de gris, peu arrondies. Sur le site, elles sont en verre dépoli avec un trait clair.
- **Mesures** : grands chiffres gras, unité en petit, courbe en trait fin.
- **Couleur** : aucune couleur d'accent. Ce qui est actif est blanc ; le vert, le rouge et le jaune ne servent qu'aux états.
- **Illustrations** : dessins au trait blanc lumineux et motifs de courbes de niveau.

## Ce que Solar en tire

Voir « Langage visuel retenu » dans [le design system](../DESIGN_SYSTEM.md). Écart assumé : Space Grotesk, recommandée par le kit d'identité Solar, à la place de leur NeoDIN (propriétaire). L'ambre a été retiré de l'app et du logo.
