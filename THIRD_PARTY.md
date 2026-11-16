# Fichiers et marques de tiers

Le code de SOLAR est publié sous licence MIT ou Apache 2.0, au choix (`LICENSE-MIT`, `LICENSE-APACHE`). Les éléments ci-dessous viennent de tiers et gardent leur propre licence.

## Embarqués dans l'app

| Élément | Où | Licence | Provenance |
|---|---|---|---|
| Police Space Grotesk (Regular, Medium, Bold) | `assets/fonts/space-grotesk/` | SIL Open Font License 1.1 (`OFL.txt`) | The Space Grotesk Project Authors, voir le `README.md` du dossier |
| Icônes Lucide | crate `gpui-kit-assets` 0.9.1 | ISC ; certaines icônes, issues de Feather, sous MIT | Lucide Icons and Contributors, Cole Bemis (Feather) ; texte dans le fichier `LICENSE-LUCIDE` de la crate |
| Logo de Claude | `assets/providers/claude.svg` | CC0 1.0 (tracé) | simple-icons 16.37.0, voir `assets/providers/README.md` |

## Marques

Claude est une marque d'Anthropic, Codex d'OpenAI, Grok de xAI, GitHub de GitHub, Inc. SOLAR pilote leurs outils en ligne de commande installés par l'utilisateur. Leurs noms et le logo de Claude ne servent qu'à indiquer quel agent répond. SOLAR n'est ni affilié à ces entreprises ni approuvé par elles. Codex et Grok n'ont pas de logo dans SOLAR : leurs conversations portent une icône neutre de Lucide.

## Dépendances Rust

Les crates sont téléchargées à la compilation et gardent leur licence, déclarée dans `Cargo.toml` ou dans le fichier de licence de chaque crate. Relevé du 16 novembre 2026 (`cargo metadata`) : MIT, Apache 2.0 et leurs combinaisons pour l'essentiel ; aussi BSD, ISC, Zlib, Unicode-3.0, CC0 ; sept crates en MPL-2.0 (`cbindgen`, `cssparser`, `cssparser-macros`, `dtoa-short`, `dwrote`, `option-ext`, `selectors`), dont la copyleft s'applique fichier par fichier et reste compatible avec la licence de SOLAR ; sur macOS, seules `option-ext` et `cbindgen` (outil de compilation) entrent dans la construction. Aucune n'est seulement sous GPL. Une release binaire porte la liste complète de ces licences.

## Documents de conception

- `docs/design/references/` : planches de concept générées pour le projet (voir son `README.md`) ; elles ne reprennent ni logo ni image d'une autre marque.
- `docs/design/references/spacex-open-design.md` : relevé de valeurs du design system communautaire `nexu-io/open-design` (Apache 2.0), source citée dans le fichier.
