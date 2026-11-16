//! L'annonce d'une nouvelle version : au lancement de l'app installée, au plus
//! une fois par jour, Solar demande à GitHub sa dernière release ; plus récente,
//! elle s'annonce en bas de la barre latérale, avec son lien. Rien n'est
//! installé.

use gpui_kit::*;
use solar_core::update::{self, Checked, Release};

use super::Shell;
use super::bots::now;

/// La version de cette app.
const VERSION: &str = env!("CARGO_PKG_VERSION");

impl Shell {
    /// Demande la dernière release si c'est le moment ; un échec (hors ligne)
    /// n'est pas noté, la question revient au lancement suivant.
    pub(super) fn check_updates(&mut self, cx: &mut Context<Self>) {
        let at = now();
        let checked = self
            .state
            .update
            .as_ref()
            .map(|checked| checked.at.as_str());
        if !crate::bundled() || self.state.settings.no_update_check || !update::due(checked, &at) {
            return;
        }
        cx.spawn(async move |this, cx| {
            let latest = cx.background_spawn(async { update::fetch_latest() }).await;
            let _ = this.update(cx, |shell, cx| {
                if let Ok(latest) = latest {
                    shell.state.update = Some(Checked { at, latest });
                    shell.save_state();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// La release à annoncer : connue, plus récente que cette app, et la
    /// vérification n'a pas été coupée.
    pub(super) fn available_update(&self) -> Option<&Release> {
        if self.state.settings.no_update_check {
            return None;
        }
        self.state
            .update
            .as_ref()?
            .latest
            .as_ref()
            .filter(|release| update::newer(VERSION, &release.version))
    }

    /// Ouvre la page de la release dans le navigateur.
    pub(super) fn open_update(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(release) = self.available_update() {
            cx.open_url(&release.url);
        }
    }
}
