Status: ready-for-agent
Blocked by: 06 — Publication possible ?

# 07 : Commit et push

Spec : `docs/tickets/github/spec.md`.

**What to build :** L'étape 1 du dialogue fonctionne de bout en bout : branche de mission proposée (`solar/<titre>`, `-<n>` pour une tentative n), fichiers de la mission avec `+n −m`, fichiers que l'utilisateur avait modifiés avant le lancement décochés et expliqués, message prérempli, rappel du compte et des hooks. « Commit et push » construit le commit dans la copie avec un index temporaire (ADR 0001), lance les hooks, pousse avec le compte actif de `gh`, sans force. Un hook en échec ou un push refusé s'affiche en clair, rien n'est poussé. La publication (branche, base, commit de départ, dernier commit poussé) est gardée dans la mission ; la mission note son commit de départ à son lancement.

- [ ] Commit limité aux fichiers cochés, posé sur le commit de départ ; deuxième commit sur la même branche.
- [ ] Branche, index et fichiers de l'utilisateur intacts octet pour octet (test sur dépôt jetable).
- [ ] Hook `pre-commit` lancé ; son échec remonté sans commit ni push.
- [ ] Push vers un dépôt nu `origin` avec l'aide d'identification de `gh`, jamais `--force` ; refus non fast-forward expliqué.
- [ ] Ancien `state.json` sans publication relu sans perte.
- [ ] Dialogue au clavier, Échap ferme sans rien faire, boutons fixés en bas en 720 × 480, FR/EN.
