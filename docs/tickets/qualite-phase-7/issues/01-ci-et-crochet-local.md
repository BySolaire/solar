Status: ready-for-human

# 01 : Garde-fou automatique : CI et crochet local

Le dépôt n'a ni CI ni crochet git : `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets` et `cargo test --workspace` ne tournent que si l'agent y pense. Proposition : un workflow GitHub Actions sur macOS (les tests GPUI sans fenêtre, le faux CLI et git y tournent) déclenché sur push et PR, et un crochet `pre-commit` local facultatif (format et clippy, rapides). Demande ton accord : publier un workflow agit sur GitHub, et les minutes macOS sont facturées sur un dépôt privé.
