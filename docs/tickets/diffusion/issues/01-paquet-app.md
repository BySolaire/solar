Status: resolved
Blocked by: None (can start immediately)

# 01 : Paquet `SOLAR.app` et installation

Spec : `docs/tickets/diffusion/spec.md`.

**What to build :** `scripts/bundle-macos.sh` (release, Info.plist, icône, signature ad hoc, `--install`) ; notifications des bots vérifiées avec l'app installée ; section « Compiler depuis les sources » du README.

- [x] `codesign --verify` passe ; Info.plist porte identifiant et version.
- [x] App lancée depuis Applications, vue à l'écran ; une notification de bot affichée (ou la raison si macOS la refuse).

## Avancement (16 novembre 2026)

- `scripts/bundle-macos.sh` : paquet construit et signé ad hoc, `codesign --verify --deep --strict` passe, Info.plist `com.bysolaire.solar` 0.0.1. `--install` copie dans `/Applications` et refuse si SOLAR y est ouvert. `spctl` rejette le paquet (attendu sans notarisation) ; compilé sur place, il n'a pas de quarantaine et s'ouvre.
- Lancée depuis le Finder, l'app n'a qu'un PATH minimal : `solar_core::env` le complète par celui du shell de connexion (`$SHELL -l -i`, 3 s au plus, ~40 ms mesurées). Vérifié à l'écran avec `PATH=/usr/bin:/bin` et un faux shell qui fournit le dossier du faux `claude` : l'agent est trouvé.
- `/Applications/SOLAR.app` lancée (état jetable, FR et EN), vue à l'écran.
- Notification : la fin d'une exécution de bot poste bien la notification (`Added notification request`, journal `usernoted`) ; macOS a affiché la demande d'autorisation, refusée à 17:50:42 (`allow: NO`), donc rien n'est affiché. Autorisation accordée ensuite par Solaire : à 17:57:54, la notification d'une nouvelle exécution est livrée (`Delivering … to [.alert .lockScreen .notificationCenter]`, `Presenting … as banner`), mais macOS coupe la bannière tant que l'écran est partagé (`resolutionReason: display shared`) : elle va au centre de notifications, où Solaire l'a vue (« Veille des issuesx · Attend ta réponse. »). L'exécution était ensuite ouverte dans SOLAR, comme le prévoit le clic sur la notification.
- README : section « Compiler depuis les sources ».
