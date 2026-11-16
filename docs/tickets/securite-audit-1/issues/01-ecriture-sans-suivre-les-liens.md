Status: resolved
Blocked by: None (can start immediately)

# 01 : Écrire dans le projet sans suivre les liens

Source : `docs/tickets/securite-audit-1/README.md`.

Constat confirmé `solar-core/checkpoint/Store-write-follows-project-symlinks` : `Store::write` (`checkpoint.rs`) écrit avec `fs::write` et `set_permissions`, qui suivent les liens. Un lien suivi du projet qui sort du projet, remplacé par un fichier dans la copie, fait écrire le contenu de l'agent hors du projet à l'application.

**What to build :** `write` refuse les chemins absolus et `..`, refuse un parent qui est un lien ou n'est pas un dossier, retire l'entrée existante sans la suivre, crée le fichier avec `create_new` et ses droits, recrée une entrée 120000 comme un lien. Couvre `put` (appliquer, restaurer) et `isolate`.

- [x] Test : le scénario de l'audit laisse le fichier extérieur intact.
- [x] Test : un lien de la copie revient comme un lien dans le projet.

## Answer

`Store::write` passe par `inside` (chemin normal seulement, parents vérifiés avec `lstat`, aucun lien traversé) puis `create` (`create_new`, mode fixé par le descripteur, lien recréé pour `120000`). Trois tests dans `crates/solar-core/tests/checkpoint.rs`, dont le scénario de l'audit et un dossier du projet devenu lien entre la revue et l'application (écriture refusée).
