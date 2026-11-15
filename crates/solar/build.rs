// Les textes de `locales/` sont compilés dans l'app par `rust_i18n::i18n!`, qui
// ne déclare pas ces fichiers à cargo : sans cette ligne, modifier un texte ne
// recompile rien.
fn main() {
    println!("cargo:rerun-if-changed=locales");
}
