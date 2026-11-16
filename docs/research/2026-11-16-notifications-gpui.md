# Notifications système dans GPUI (16 novembre 2026)

Lu dans `gpui-pre` 0.3.12 (la version de GPUI de gpui-kit 0.9.1) :

- `App::show_system_notification(SystemNotification { tag, title, body, actions })` poste une notification par `UNUserNotificationCenter` ; une notification de même `tag` remplace la précédente ; les clics reviennent par un rappel (`SystemNotificationResponse { tag, action_id }`).
- `App::set_app_identity(identifiant, nom)` fixe l'identité de l'app.
- Garde-fou de `gpui-pre-macos` (`system_notifications.rs`) : hors d'un paquet `.app` (binaire lancé par `cargo run`), le centre de notifications n'est pas créé et l'appel ne fait rien (pas d'abandon du processus). L'autorisation n'est demandée qu'au premier envoi.

Conséquence pour Solar : les notifications des bots ne s'afficheront que dans l'app empaquetée (chantier de diffusion) ; en développement, seul le point d'état les remplace. Le `TestAppContext` expose `shown_system_notifications()` pour les tester sans fenêtre.
