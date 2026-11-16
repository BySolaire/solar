//! Le PATH d'une app lancée depuis le Finder, complété par celui du shell.

use solar_core::env::{merge_paths, path_from_shell_output};

#[test]
fn the_shell_path_completes_the_app_path_without_duplicates() {
    // L'app garde son ordre ; les dossiers du shell manquants suivent, dans le leur.
    assert_eq!(
        merge_paths(
            "/usr/bin:/bin",
            "/Users/a/.local/bin:/opt/homebrew/bin:/usr/bin:/bin:/Users/a/.local/bin"
        ),
        "/usr/bin:/bin:/Users/a/.local/bin:/opt/homebrew/bin"
    );
    assert_eq!(merge_paths("/usr/bin", ""), "/usr/bin");
    assert_eq!(merge_paths("", "/opt/homebrew/bin"), "/opt/homebrew/bin");
}

#[test]
fn the_path_is_read_between_markers_whatever_the_shell_prints() {
    // Un .zshrc bavard écrit avant et après : seul le PATH balisé compte.
    let out = "Bienvenue !\n__SOLAR_PATH__/a/bin:/usr/bin__SOLAR_PATH__\nnvm chargé\n";
    assert_eq!(
        path_from_shell_output(out).as_deref(),
        Some("/a/bin:/usr/bin")
    );
    assert_eq!(path_from_shell_output("rien d'utile"), None);
}
