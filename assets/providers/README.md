# Logos des fournisseurs d'agents

Chaque réponse d'un agent est signée du logo de son fournisseur, à côté de son nom, dans la couleur de marque du fournisseur. Un fichier par fournisseur, en un seul tracé : l'app le colorie avec la couleur notée ici, déclarée dans `crates/solar/src/theme.rs`.

| Fichier | Fournisseur | Couleur | Provenance |
|---|---|---|---|
| `claude.svg` | Claude (Anthropic) | `#D97757` | simple-icons 16.37.0, `icons/claude.svg` et sa couleur de marque |

- **Licence des tracés** : simple-icons publie ses SVG sous CC0 1.0.
- **Marques** : Claude est une marque d'Anthropic. Le logo sert uniquement à indiquer quel agent répond ; SOLAR n'est ni affilié à Anthropic ni approuvé par elle.
- **Ajouter un fournisseur** (Codex, Grok…) : déposer son logo ici dans le même format (un seul tracé, fond transparent), noter sa provenance dans ce tableau, et l'embarquer dans `crates/solar/src/assets.rs`. OpenAI et xAI ne sont plus dans simple-icons : prendre leurs fichiers dans leurs kits de marque officiels.
- **Codex et Grok** : en attente des logos officiels d'OpenAI et de xAI (la page de marque d'OpenAI refuse les téléchargements automatisés). En attendant, leurs conversations portent l'icône neutre `Bot` de Lucide, sans couleur de marque.
