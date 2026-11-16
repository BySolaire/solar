Status: resolved
Blocked by: 01 — Paquet ; 04 — CI

# 05 : Première release

Spec : `docs/tickets/diffusion/spec.md`.

**What to build :** `release.yml` sur tag `v*` : paquet, zip, release en brouillon ; README : téléchargement et premier lancement (« Ouvrir quand même », `xattr`). Numéro de version à choisir avec Solaire. Le zip porte `LICENSE-MIT`, `LICENSE-APACHE`, `THIRD_PARTY.md` et la liste des licences des crates compilées (générée depuis `cargo metadata`, sans outil nouveau), comme l'annonce `THIRD_PARTY.md`.

- [x] Une release brouillon porte le zip ; le paquet téléchargé s'ouvre en suivant les instructions.

## Answer

Version 0.1.0 (choix de Solaire).

`.github/workflows/release.yml` se lance sur un tag `v*` :
- il refuse un tag qui ne correspond pas à la version de `Cargo.toml` ;
- il construit le paquet sans cache ;
- il zippe `Solar.app` avec les licences, avec `ditto` sans attributs étendus ;
- il calcule l'empreinte SHA-256 ;
- il crée une release en brouillon avec les notes `.github/release-notes.md` (FR/EN).

`scripts/third-party-licenses.py` liste les 517 crates compilées : licence, auteurs, dép��t, fichiers de licence, plus les textes de référence MIT et Apache 2.0. Le script de paquet compile en `--locked`. Le README a une section « Télécharger ».

Essai réel avec le tag `v0.1.0` (run `43953896429`) : la release en brouillon porte `Solar-0.1.0-macos-arm64.zip` (11,6 Mo) et son `.sha256`. Une fois téléchargé :
- l'empreinte est juste et la signature ad hoc vérifiée ;
- avec l'attribut de quarantaine, Gatekeeper refuse (« rejected »), ce qu'annoncent les instructions ;
- après `xattr -dr com.apple.quarantine`, l'app s'ouvre (vue à l'écran, état jetable).

Le chemin « Ouvrir quand même » des Réglages n'a pas été essayé : il passe par un dialogue du système.

La CI ne tourne plus sur les tags : la release construit déjà le paquet.
