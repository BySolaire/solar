//! Ressources embarquées : le logo de SOLAR, tiré du kit d'identité officiel
//! (`assets/logo/identity-kit`), et les icônes Lucide fournies par gpui-kit.

use std::borrow::Cow;

use gpui_kit::assets::AllAssets;
use gpui_kit::{AssetSource, Result, SharedString};

/// Mot SOLAR du kit, formes pleines : prend la couleur du texte.
pub const WORDMARK: &str = "logo/solar-wordmark.svg";
/// Logo de Claude, qui signe ses réponses. Un logo par fournisseur, monochrome :
/// voir `assets/providers/README.md`.
pub const CLAUDE_LOGO: &str = "providers/claude.svg";

/// Planète de l'accueil : son limbe en travers, en blanc sur fond transparent.
/// Provenance dans `assets/logo/README.md`.
pub const PLANET: &str = "logo/solar-planet.png";

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let logo: &[u8] = match path {
            WORDMARK => {
                include_bytes!("../../../assets/logo/identity-kit/assets/svg/solar-wordmark.svg")
            }
            CLAUDE_LOGO => include_bytes!("../../../assets/providers/claude.svg"),
            PLANET => include_bytes!("../../../assets/logo/solar-planet.png"),
            _ => return AllAssets.load(path),
        };
        Ok(Some(Cow::Borrowed(logo)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        AllAssets.list(path)
    }
}
