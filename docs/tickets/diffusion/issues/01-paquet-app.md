Status: ready-for-agent
Blocked by: None (can start immediately)

# 01 : Paquet `SOLAR.app` et installation

Spec : `docs/tickets/diffusion/spec.md`.

**What to build :** `scripts/bundle-macos.sh` (release, Info.plist, icône, signature ad hoc, `--install`) ; notifications des bots vérifiées avec l'app installée ; section « Compiler depuis les sources » du README.

- [ ] `codesign --verify` passe ; Info.plist porte identifiant et version.
- [ ] App lancée depuis Applications, vue à l'écran ; une notification de bot affichée (ou la raison si macOS la refuse).
