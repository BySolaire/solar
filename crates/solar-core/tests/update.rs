//! La vérification des nouvelles versions : comparer, lire la réponse de
//! GitHub, et ne demander qu'une fois par jour.

use solar_core::update::{Release, due, latest_from_json, newer};

#[test]
fn versions_compare_by_number_not_by_text() {
    assert!(newer("0.1.9", "0.1.10"));
    assert!(newer("0.1.0", "v0.2.0"));
    assert!(newer("0.9.9", "1.0.0"));
    assert!(!newer("0.1.0", "0.1.0"));
    assert!(!newer("0.2.0", "0.1.10"));
    // Une préversion précède la version qu'elle prépare.
    assert!(newer("0.2.0-beta.1", "0.2.0"));
    assert!(!newer("0.2.0", "0.2.0-beta.1"));
    // Un numéro illisible n'annonce rien.
    assert!(!newer("0.1.0", "nightly"));
    assert!(!newer("0.1.0", ""));
}

#[test]
fn the_latest_release_is_read_from_github_and_anything_else_is_ignored() {
    let json = r#"{"tag_name":"v0.2.0","html_url":"https://github.com/BySolaire/solar/releases/tag/v0.2.0","draft":false,"prerelease":false,"body":"…"}"#;
    assert_eq!(
        latest_from_json(json),
        Some(Release {
            version: "0.2.0".into(),
            url: "https://github.com/BySolaire/solar/releases/tag/v0.2.0".into(),
        })
    );
    // Réponse invalide, brouillon, préversion, ou lien hors du dépôt : rien.
    assert_eq!(latest_from_json("<html>rate limited</html>"), None);
    assert_eq!(latest_from_json(r#"{"message":"Not Found"}"#), None);
    assert_eq!(
        latest_from_json(
            r#"{"tag_name":"v0.3.0","html_url":"https://github.com/BySolaire/solar/releases/tag/v0.3.0","draft":true,"prerelease":false}"#
        ),
        None
    );
    assert_eq!(
        latest_from_json(
            r#"{"tag_name":"v0.3.0","html_url":"https://github.com/BySolaire/solar/releases/tag/v0.3.0","draft":false,"prerelease":true}"#
        ),
        None
    );
    assert_eq!(
        latest_from_json(
            r#"{"tag_name":"v0.3.0","html_url":"https://exemple.invalid/solar","draft":false,"prerelease":false}"#
        ),
        None
    );
}

#[test]
fn github_is_asked_at_most_once_a_day() {
    let now = "2026-11-17T09:00:00+01:00";
    assert!(due(None, now));
    assert!(due(Some("2026-11-16T08:59:00+01:00"), now));
    assert!(!due(Some("2026-11-16T09:30:00+01:00"), now));
    // Une date illisible, ou dans le futur (horloge changée) : on redemande.
    assert!(due(Some("hier"), now));
    assert!(due(Some("2026-11-20T09:00:00+01:00"), now));
}
