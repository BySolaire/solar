## Installer (macOS, Apple Silicon)

1. Télécharge `Solar-<version>-macos-arm64.zip` ci-dessous et décompresse-le.
2. Glisse `Solar.app` dans **Applications**.
3. Premier lancement : Solar n'est pas notarisé par Apple (projet libre, sans compte Apple payant), macOS le bloque donc la première fois. Ouvre-le une fois, puis **Réglages Système → Confidentialité et sécurité → Ouvrir quand même**, et confirme. Ou, dans un terminal : `xattr -dr com.apple.quarantine /Applications/Solar.app`.

Prérequis : le CLI `claude` installé et connecté ; `codex`, `grok` et `gh` sont facultatifs. Solar les lance avec ton compte et leur configuration.

Intégrité : `shasum -a 256 -c Solar-<version>-macos-arm64.zip.sha256`. Le zip contient aussi les licences (`LICENSE-MIT`, `LICENSE-APACHE`, `THIRD_PARTY.md`, `THIRD_PARTY_LICENSES.txt`).

## Install (macOS, Apple Silicon)

Unzip, move `Solar.app` to Applications. Solar is not notarized: on first launch, open it once, then **System Settings → Privacy & Security → Open Anyway**, or run `xattr -dr com.apple.quarantine /Applications/Solar.app`. Requires the `claude` CLI, installed and signed in.
