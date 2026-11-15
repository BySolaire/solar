//! Fenêtre principale : barre de titre, barre latérale des projets, accueil et
//! onglets de conversations.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenu, PopupMenuItem};
use gpui_kit::component::sidebar::{Sidebar, SidebarMenu, SidebarMenuItem};
use gpui_kit::component::{ActiveTheme, Icon, Sizable, TitleBar, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;
use solar_core::history::{self, Found};
use solar_core::state::{MissionStatus, Project, Session, State};

use crate::eclipse::brand;
use crate::session_view::{Relaunch, SessionView, Status, age};
use crate::theme::{BLACK, DEPTH, caps};

actions!(solar, [NewConversation, ToggleSidebar]);

pub struct Shell {
    state: State,
    state_path: PathBuf,
    /// Brouillon de l'accueil : une conversation pas encore commencée. Elle
    /// devient un onglet à son premier message.
    home: Entity<SessionView>,
    tabs: Vec<Entity<SessionView>>,
    /// Onglet affiché ; `None` montre l'accueil.
    active: Option<usize>,
    /// Import en cours : le projet et ses sessions Claude Code absentes de SOLAR.
    importing: Option<(PathBuf, Vec<Found>)>,
    /// Le menu d'un projet est ouvert : aucune page de preview ne s'affiche.
    menu_open: bool,
    /// Conversations fermées pendant que Claude travaillait : la vue reste en vie,
    /// et son processus avec elle, jusqu'à ce qu'on la rouvre.
    // shortcut: une vue en fond reste en mémoire jusqu'à la fin de l'app, même
    // après son tour ; la libérer à la fin du tour si elles s'accumulent.
    background: Vec<Entity<SessionView>>,
    sidebar_open: bool,
    /// Dernière activité Claude Code de chaque projet, relue quand l'accueil s'affiche.
    /// Date de dernière modification de chaque conversation, par identifiant. Relue
    /// à chaque retour sur l'accueil et quand une conversation apparaît.
    // shortcut: la date d'une conversation ouverte n'avance pas avant le prochain
    // retour sur l'accueil ; la rafraîchir en fin de tour si l'écart gêne.
    activity: HashMap<String, Option<SystemTime>>,
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_state(State::default_path(), window, cx)
    }

    fn with_state(state_path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = State::load(&state_path);
        let first = state.projects.first().map(|project| project.path.clone());
        let mut this = Self {
            home: Self::draft(first.unwrap_or_default(), window, cx),
            state: state.clone(),
            state_path,
            tabs: Vec::new(),
            active: None,
            importing: None,
            background: Vec::new(),
            menu_open: false,
            sidebar_open: true,
            activity: HashMap::new(),
        };
        // Rouvre les onglets de la dernière fois.
        for id in &state.open_tabs {
            let found = state.projects.iter().find_map(|project| {
                let session = project.sessions.iter().find(|session| session.id == *id)?;
                Some((project.path.clone(), session.clone()))
            });
            if let Some((path, session)) = found {
                this.open(path, session, window, cx);
            }
        }
        if this.tabs.is_empty() {
            this.go_home(None, window, cx);
        }
        this
    }

    /// Crée le brouillon de l'accueil pour un projet (ou sans projet).
    fn draft(project: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Entity<SessionView> {
        let title = t!("sidebar.new_session").into();
        let draft = cx.new(|cx| SessionView::new(project, None, title, window, cx));
        cx.observe_in(&draft, window, |this, changed, window, cx| {
            this.promote(&changed, window, cx);
            this.sync(cx);
        })
        .detach();
        cx.subscribe_in(&draft, window, Self::relaunch).detach();
        draft
    }

    /// Une mission demande une nouvelle tentative : une conversation neuve, au
    /// contexte vierge, qui reçoit le plan et part dans le mode de la mission.
    fn relaunch(
        &mut self,
        _: &Entity<SessionView>,
        event: &Relaunch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab = cx.new(|cx| {
            let mut tab =
                SessionView::new(event.project.clone(), None, event.title.clone(), window, cx);
            tab.mission = Some(event.mission.clone());
            tab.brief = Some(event.brief.clone());
            tab
        });
        cx.observe(&tab, |this, _, cx| this.sync(cx)).detach();
        cx.subscribe_in(&tab, window, Self::relaunch).detach();
        self.tabs.push(tab.clone());
        self.show(self.tabs.len() - 1, window, cx);
        tab.update(cx, |tab, cx| tab.start_attempt(cx));
    }

    /// Le brouillon de l'accueil a reçu son premier message : il devient un
    /// onglet, et l'accueil repart d'un brouillon neuf sur le même projet.
    fn promote(
        &mut self,
        changed: &Entity<SessionView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if *changed != self.home || !self.home.read(cx).started() {
            return;
        }
        let project = self.home.read(cx).project.clone();
        let started = std::mem::replace(&mut self.home, Self::draft(project, window, cx));
        self.tabs.push(started);
        self.active = Some(self.tabs.len() - 1);
    }

    /// Montre l'accueil, éventuellement sur un autre projet.
    fn go_home(&mut self, project: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        self.importing = None;
        self.active = None;
        self.activity.clear();
        self.home.update(cx, |home, cx| {
            if let Some(project) = project {
                home.set_project(project, cx);
            }
            home.activate(window, cx);
        });
        self.sync(cx);
    }

    /// Ouvre une session en onglet, ou montre son onglet s'il existe déjà.
    fn open(
        &mut self,
        project: PathBuf,
        session: Session,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_it =
            |tab: &Entity<SessionView>| tab.read(cx).session_id.as_deref() == Some(&session.id);
        // Une conversation qui tournait en fond revient telle quelle.
        if let Some(ix) = self.background.iter().position(is_it) {
            let tab = self.background.remove(ix);
            self.tabs.push(tab);
        }
        let existing = self.tabs.iter().position(is_it);
        let ix = existing.unwrap_or_else(|| {
            let (id, title) = (Some(session.id), session.title.into());
            let tab = cx.new(|cx| {
                let mut tab = SessionView::new(project, id, title, window, cx);
                tab.restore_mission(session.mission, cx);
                tab
            });
            cx.observe(&tab, |this, _, cx| this.sync(cx)).detach();
            cx.subscribe_in(&tab, window, Self::relaunch).detach();
            self.tabs.push(tab);
            self.tabs.len() - 1
        });
        self.show(ix, window, cx);
    }

    /// Les pages de preview sont des vues natives qui recouvrent tout : seule
    /// celle de l'onglet affiché reste visible.
    fn sync_previews(&self, cx: &mut Context<Self>) {
        let shown = self
            .active
            .filter(|_| self.importing.is_none() && !self.menu_open);
        for (ix, tab) in self.tabs.iter().chain(&self.background).enumerate() {
            tab.update(cx, |tab, cx| tab.set_preview_visible(Some(ix) == shown, cx));
        }
    }

    fn show(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(ix) else { return };
        self.importing = None;
        self.active = Some(ix);
        tab.update(cx, |tab, cx| tab.activate(window, cx));
        self.sync(cx);
    }

    /// Liste les sessions Claude Code du projet créées ailleurs (terminal, autre app).
    // shortcut: tous les fichiers de session du projet sont relus d'un bloc ;
    // passer en arrière-plan si un projet en accumule des centaines.
    fn start_import(&mut self, project: PathBuf, cx: &mut Context<Self>) {
        let known: Vec<&String> = self
            .state
            .projects
            .iter()
            .filter(|known| known.path == project)
            .flat_map(|known| known.sessions.iter().map(|session| &session.id))
            .collect();
        let found = history::list(&project)
            .into_iter()
            .filter(|found| !known.contains(&&found.id))
            .collect();
        self.importing = Some((project, found));
        self.sync_previews(cx);
        cx.notify();
    }

    fn import(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((path, mut found)) = self.importing.take() else {
            return;
        };
        if ix >= found.len() {
            return;
        }
        let found = found.swap_remove(ix);
        let session = Session {
            id: found.id,
            title: found.title,
            mission: None,
        };
        if let Some(project) = self
            .state
            .projects
            .iter_mut()
            .find(|known| known.path == path)
        {
            project.sessions.insert(0, session.clone());
        }
        self.open(path, session, window, cx);
    }

    /// Ferme un onglet. Sa conversation reste dans la barre latérale. Si Claude
    /// y travaille encore, il continue en fond ; sinon son processus s'arrête.
    fn close(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if ix >= self.tabs.len() {
            return;
        }
        let closed = self.tabs.remove(ix);
        if closed.read(cx).status != Status::Idle {
            self.background.push(closed);
        }
        match self.active {
            Some(active) if active > ix => self.active = Some(active - 1),
            Some(active) if active == ix && self.tabs.is_empty() => {
                return self.go_home(None, window, cx);
            }
            Some(active) if active == ix => {
                return self.show(ix.min(self.tabs.len() - 1), window, cx);
            }
            _ => {}
        }
        self.sync(cx);
    }

    fn add_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                // L'accueil prend ce dossier ; `sync` l'ajoute aux projets.
                if let Some(path) = paths.into_iter().next() {
                    this.go_home(Some(path), window, cx);
                }
            });
        })
        .detach();
    }

    /// Reporte dans l'état ce que les vues ont appris (dossier choisi sur
    /// l'accueil, identifiant de session), l'écrit sur disque s'il a changé, et
    /// tient l'accueil au courant des projets.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let before = self.state.clone();
        let chosen = self.home.read(cx).project.clone();
        if !chosen.as_os_str().is_empty()
            && !self
                .state
                .projects
                .iter()
                .any(|project| project.path == chosen)
        {
            self.state.projects.push(Project {
                path: chosen,
                sessions: Vec::new(),
            });
        }
        self.state.open_tabs.clear();
        for (ix, tab) in self.tabs.iter().chain(&self.background).enumerate() {
            let tab = tab.read(cx);
            let Some(id) = &tab.session_id else { continue };
            // Une conversation en fond n'est pas un onglet ouvert, mais sa mission avance.
            if ix < self.tabs.len() {
                self.state.open_tabs.push(id.clone());
            }
            let project = self
                .state
                .projects
                .iter_mut()
                .find(|project| project.path == tab.project);
            let Some(project) = project else { continue };
            // La mission suit la conversation : sa version la plus récente est gardée.
            match project
                .sessions
                .iter_mut()
                .find(|session| session.id == *id)
            {
                Some(session) => session.mission = tab.mission.clone(),
                None => project.sessions.insert(
                    0,
                    Session {
                        id: id.clone(),
                        title: tab.title.to_string(),
                        mission: tab.mission.clone(),
                    },
                ),
            }
        }
        if self.state != before
            && let Err(error) = self.state.save(&self.state_path)
        {
            eprintln!("solar: état non sauvegardé : {error}");
        }
        for project in &self.state.projects {
            for session in &project.sessions {
                if !self.activity.contains_key(&session.id) {
                    let modified = history::modified(&project.path, &session.id);
                    self.activity.insert(session.id.clone(), modified);
                }
            }
        }
        let projects = self
            .state
            .projects
            .iter()
            .map(|project| project.path.clone())
            .collect();
        self.home
            .update(cx, |home, cx| home.set_projects(projects, cx));
        self.sync_previews(cx);
        cx.notify();
    }

    fn status_of(&self, session_id: &str, cx: &App) -> Option<Status> {
        self.tabs.iter().chain(&self.background).find_map(|tab| {
            let tab = tab.read(cx);
            (tab.session_id.as_deref() == Some(session_id)).then_some(tab.status)
        })
    }

    fn on_home(&self) -> bool {
        self.active.is_none() && self.importing.is_none()
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active_tab = self.active.and_then(|ix| self.tabs.get(ix));
        let active_id = active_tab.and_then(|tab| tab.read(cx).session_id.clone());
        // Le projet en cours : celui de l'onglet affiché, ou celui de l'accueil.
        let current = active_tab.unwrap_or(&self.home).read(cx).project.clone();
        let muted = cx.theme().muted_foreground;

        let shell = cx.entity();
        let projects = self
            .state
            .projects
            .iter()
            .enumerate()
            .map(|(ix, project)| {
                let name = project
                    .path
                    .file_name()
                    .unwrap_or(project.path.as_os_str())
                    .to_string_lossy()
                    .to_string();
                let sessions = project.sessions.iter().map(|session| {
                    let (path, opened) = (project.path.clone(), session.clone());
                    // En cours : point plein. En attente d'une réponse : anneau.
                    // Au repos : l'ancienneté de la conversation.
                    let status = self.status_of(&session.id, cx);
                    let white = cx.theme().foreground;
                    let since = self.activity.get(&session.id).copied().flatten();
                    let mission = session
                        .mission
                        .as_ref()
                        .is_some_and(|mission| mission.status != MissionStatus::Cancelled);
                    SidebarMenuItem::new(session.title.clone())
                        .label_style(StyleRefinement::default().block().min_w_0().truncate())
                        .active(active_id.as_deref() == Some(&session.id))
                        // La conversation affichée : une petite flèche blanche. Les autres
                        // gardent sa place, invisible, pour que les titres restent alignés.
                        .icon(Icon::new(IconName::ChevronRight).xsmall().text_color(
                            if active_id.as_deref() == Some(&session.id) {
                                white
                            } else {
                                transparent_black()
                            },
                        ))
                        .suffix(move |_, _| {
                            let dot = div().flex_shrink_0().size_2().rounded_full();
                            let state = match status {
                                Some(Status::Working) => dot.bg(white).into_any_element(),
                                Some(Status::NeedsYou) => {
                                    dot.border_1().border_color(white).into_any_element()
                                }
                                _ => div()
                                    .flex_shrink_0()
                                    .text_xs()
                                    .text_color(muted)
                                    .invisible()
                                    .group_hover(SIDEBAR_GROUP, |age| age.visible())
                                    .children(since.map(short_age))
                                    .into_any_element(),
                            };
                            // Une mission se distingue d'une conversation par son icône.
                            h_flex()
                                .flex_shrink_0()
                                .gap_1p5()
                                .when(mission, |row| {
                                    row.child(
                                        Icon::new(IconName::ListChecks).xsmall().text_color(muted),
                                    )
                                })
                                .child(state)
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open(path.clone(), opened.clone(), window, cx)
                        }))
                });
                // Les actions du projet : au clic droit sur chaque projet, et derrière
                // « ··· » pour le seul projet en cours, pour ne pas charger la liste.
                let menu = {
                    let (shell, path) = (shell.clone(), project.path.clone());
                    move |menu: PopupMenu, _: &mut Window, _: &mut App| {
                        let (new_shell, new_path) = (shell.clone(), path.clone());
                        let (import_shell, import_path) = (shell.clone(), path.clone());
                        menu.item(PopupMenuItem::new(t!("sidebar.new_session")).on_click(
                            move |_, window, cx| {
                                new_shell.update(cx, |this, cx| {
                                    this.go_home(Some(new_path.clone()), window, cx)
                                });
                            },
                        ))
                        .item(
                            PopupMenuItem::new(t!("sidebar.import")).on_click(move |_, _, cx| {
                                import_shell.update(cx, |this, cx| {
                                    this.start_import(import_path.clone(), cx)
                                });
                            }),
                        )
                    }
                };
                let menu = std::rc::Rc::new(menu);
                let dropdown = menu.clone();
                let owner = shell.clone();
                let actions = move |_: &mut Window, _: &mut App| {
                    let menu = dropdown.clone();
                    let shell = owner.clone();
                    div()
                        .invisible()
                        .group_hover(SIDEBAR_GROUP, |actions| actions.visible())
                        .child(
                            Button::new(("project-menu", ix))
                                .ghost()
                                .xsmall()
                                .icon(IconName::Ellipsis)
                                .tooltip(t!("sidebar.project_menu"))
                                .dropdown_menu(move |popup, window, cx| menu(popup, window, cx))
                                .on_open_change({
                                    move |open, _, cx| {
                                        // La page de preview ne doit pas couvrir ce menu.
                                        shell.update(cx, |shell, cx| {
                                            shell.menu_open = *open;
                                            shell.sync_previews(cx);
                                        })
                                    }
                                }),
                        )
                };
                let path = project.path.clone();
                SidebarMenuItem::new(name)
                    .icon(IconName::Folder)
                    .default_open(true)
                    .label_style(StyleRefinement::default().block().min_w_0().truncate())
                    .when(project.path == current, |item| item.suffix(actions))
                    .context_menu(move |popup, window, cx| menu(popup, window, cx))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.go_home(Some(path.clone()), window, cx)
                    }))
                    .children(sessions)
            })
            .collect::<Vec<_>>();
        // `SidebarMenu::children` remplace les éléments : tout passe en un seul appel.
        let projects = (!projects.is_empty()).then(|| SidebarMenu::new().children(projects));
        // L'intitulé des projets porte leur seule action d'ajout.
        let heading = h_flex()
            .h_8()
            .mt_2()
            .pl_2()
            .justify_between()
            .text_xs()
            .text_color(cx.theme().sidebar_foreground.opacity(0.7))
            .child(caps(&t!("sidebar.projects")))
            .child(
                Button::new("add-project")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Plus)
                    .tooltip(t!("sidebar.add_project"))
                    .on_click(cx.listener(|this, _, window, cx| this.add_project(window, cx))),
            );

        // Les détails (ancienneté, actions du projet) n'apparaissent qu'au survol.
        div().group(SIDEBAR_GROUP).h_full().flex_shrink_0().child(
            Sidebar::new("sidebar")
                .collapsible(false)
                .w(SIDEBAR_WIDTH)
                .header(
                    v_flex()
                        .w_full()
                        .child(self.render_row(
                            "home",
                            IconName::House,
                            t!("sidebar.home").to_string().into(),
                            self.on_home(),
                            |this, window, cx| this.go_home(None, window, cx),
                            cx,
                        ))
                        .child(heading),
                )
                .children(projects),
        )
    }

    /// Une ligne de la barre latérale hors des groupes de projets, au même
    /// dessin que leurs éléments.
    fn render_row(
        &self,
        id: &'static str,
        icon: IconName,
        label: SharedString,
        selected: bool,
        on_click: fn(&mut Self, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        // Un bouton : la ligne entre dans la navigation au clavier.
        Button::new(id)
            .ghost()
            .small()
            .w_full()
            .rounded_none()
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .child(Icon::new(icon).small())
                    .child(label),
            )
            .text_color(theme.sidebar_foreground)
            .when(selected, |row| {
                row.text_color(theme.sidebar_accent_foreground)
                    .border_l_2()
                    .border_color(theme.sidebar_accent_foreground)
            })
            .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
    }

    fn render_import(&self, found: &[Found], cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let rows = found.iter().enumerate().map(|(ix, session)| {
            Button::new(("import", ix))
                .ghost()
                .w_full()
                .justify_start()
                .child(
                    h_flex()
                        .w_full()
                        .gap_3()
                        .child(div().flex_1().truncate().child(session.title.clone()))
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(muted)
                                .child(age(session.modified)),
                        ),
                )
                .on_click(cx.listener(move |this, _, window, cx| this.import(ix, window, cx)))
        });
        v_flex()
            .id("import-list")
            .size_full()
            .items_center()
            .overflow_y_scroll()
            .text_sm()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(760.))
                    .p_6()
                    .gap_1()
                    .child(
                        h_flex()
                            .justify_between()
                            .pb_3()
                            .child(div().child(t!("import.title").to_string()))
                            .child(
                                Button::new("import-cancel")
                                    .ghost()
                                    .small()
                                    .label(t!("import.cancel").to_string())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.importing = None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .when(found.is_empty(), |column| {
                        column.child(
                            div()
                                .text_color(muted)
                                .child(t!("import.empty").to_string()),
                        )
                    })
                    .children(rows),
            )
    }

    /// Les onglets : le titre de l'onglet affiché est souligné d'un trait fin qui
    /// s'arrête avant sa croix ; les autres restent en gris, sans croix.
    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (white, muted, border) = (theme.foreground, theme.muted_foreground, theme.border);
        let tabs = self.tabs.iter().enumerate().map(|(ix, tab)| {
            let active = self.active == Some(ix);
            h_flex()
                .h_full()
                .gap_0p5()
                // Un bouton : il entre dans la navigation au clavier (Tab, Entrée).
                .child(
                    Button::new(("tab", ix))
                        .ghost()
                        .small()
                        .h_full()
                        .rounded_none()
                        .border_b_2()
                        .border_color(if active { white } else { transparent_black() })
                        .text_color(if active { white } else { muted })
                        .label(tab.read(cx).title.clone())
                        .on_click(
                            cx.listener(move |this, _, window, cx| this.show(ix, window, cx)),
                        ),
                )
                .when(active, |tab| {
                    tab.child(
                        Button::new(("close", ix))
                            .ghost()
                            .xsmall()
                            .icon(IconName::X)
                            .tooltip(t!("tab.close"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close(ix, window, cx);
                            })),
                    )
                })
        });
        h_flex()
            .id("tabs")
            .flex_shrink_0()
            .h(px(40.))
            .px_4()
            .gap_4()
            .text_sm()
            .border_b_1()
            .border_color(border)
            .overflow_x_scroll()
            .children(tabs.collect::<Vec<_>>())
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match (
            &self.importing,
            self.active.and_then(|ix| self.tabs.get(ix)),
        ) {
            (Some((_, found)), _) => self.render_import(found, cx).into_any_element(),
            (None, Some(tab)) => tab.clone().into_any_element(),
            (None, None) => self.home.clone().into_any_element(),
        };

        v_flex()
            .size_full()
            .text_color(cx.theme().foreground)
            .on_action(
                cx.listener(|this, _: &NewConversation, window, cx| this.go_home(None, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.sidebar_open = !this.sidebar_open;
                cx.notify();
            }))
            .child(
                TitleBar::new().child(
                    h_flex()
                        .gap_3()
                        .child(
                            Button::new("toggle-sidebar")
                                .ghost()
                                .xsmall()
                                .icon(IconName::PanelLeft)
                                .tooltip(t!("sidebar.toggle"))
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(ToggleSidebar), cx)
                                }),
                        )
                        .child(brand(cx)),
                ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .when(self.sidebar_open, |row| row.child(self.render_sidebar(cx)))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .bg(rgb(BLACK).opacity(DEPTH))
                            .when(!self.tabs.is_empty(), |column| {
                                column.child(self.render_tabs(cx))
                            })
                            .child(div().flex_1().min_h_0().child(content)),
                    ),
            )
    }
}

const SIDEBAR_WIDTH: Pixels = px(240.);
const SIDEBAR_GROUP: &str = "sidebar";
/// Ancienneté courte, pour la barre latérale : « 12 min », « 3 h », « 2 j ».
fn short_age(since: SystemTime) -> String {
    let minutes = since.elapsed().map_or(0, |elapsed| elapsed.as_secs() / 60);
    match minutes {
        0..60 => t!("age.minutes", count = minutes).to_string(),
        60..1440 => t!("age.hours", count = minutes / 60).to_string(),
        _ => t!("age.days", count = minutes / 1440).to_string(),
    }
}

/// Le parcours de l'accueil dans une fenêtre invisible, face à un faux `claude`
/// qui note où il est lancé et ce qu'il reçoit.
#[cfg(all(test, unix))]
mod tests {
    // `use super::*` amène le `test` de gpui_kit : on rend explicite celui de Rust.
    use core::prelude::v1::test;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    use gpui_kit::test::{TestAppContextExt, TestWindowExt};

    use super::*;
    use crate::session_view::tests::PATH_LOCK;

    // Le verrou est tenu pendant tout le test, attentes comprises : c'est son rôle.
    #[allow(clippy::await_holding_lock)]
    #[gpui_kit::test]
    async fn first_message_from_home_opens_one_tab_in_the_chosen_project(cx: &mut TestAppContext) {
        let _path = PATH_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!("solar-shell-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (first, second) = (dir.join("first"), dir.join("second"));
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let log = dir.join("received.log");
        // Chaque lancement note son dossier ; chaque ligne reçue est recopiée. Au
        // premier message, le faux CLI annonce une session puis termine le tour.
        let script = format!(
            "#!/bin/sh\necho \"cwd $PWD\" >> '{log}'\nwhile read -r line; do\n\
             echo \"$line\" >> '{log}'\ncase \"$line\" in *'\"type\":\"user\"'*)\n\
             echo '{{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"s-1\",\"model\":\"m\",\"permissionMode\":\"bypassPermissions\"}}'\n\
             echo '{{\"type\":\"result\",\"is_error\":false,\"usage\":{{}}}}';; esac\ndone\n",
            log = log.display()
        );
        std::fs::write(dir.join("claude"), script).unwrap();
        std::fs::set_permissions(dir.join("claude"), PermissionsExt::from_mode(0o755)).unwrap();
        let path = std::env::var("PATH").unwrap_or_default();
        unsafe { std::env::set_var("PATH", format!("{}:{path}", dir.display())) };

        let state_path = dir.join("state.json");
        let state = State {
            projects: [&first, &second]
                .map(|path| Project {
                    path: path.clone(),
                    sessions: Vec::new(),
                })
                .to_vec(),
            open_tabs: Vec::new(),
        };
        state.save(&state_path).unwrap();

        cx.executor().allow_parking();
        cx.update(gpui_kit::init);
        let (window, shell) = cx.update(|cx| {
            let path = state_path.clone();
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Shell::with_state(path, window, cx))
            })
            .unwrap()
        });

        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            // Le brouillon est tapé d'abord ; le projet est changé ensuite.
            window.input("Bonjour", cx);
            shell.update(cx, |shell, cx| {
                assert!(shell.active.is_none() && shell.tabs.is_empty());
                shell.go_home(Some(second.clone()), window, cx);
            });
            window.render_frame(cx);
            window.press("enter", cx);
        })
        .unwrap();

        let real_time = || std::thread::sleep(Duration::from_millis(5));
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            let shell = shell.read(cx);
            shell.tabs.len() == 1 && shell.tabs[0].read(cx).session_id.is_some()
        })
        .await;

        cx.update(|cx| {
            let shell = shell.read(cx);
            assert_eq!(shell.active, Some(0));
            let tab = shell.tabs[0].read(cx);
            assert_eq!(tab.project, second);
            assert_eq!(tab.title.as_ref(), "Bonjour");
            // L'accueil repart d'un brouillon neuf, sur le même projet.
            let home = shell.home.read(cx);
            assert!(!home.started());
            assert_eq!(home.project, second);
        });

        // Claude travaille encore : fermer l'onglet le garde en fond, rouvrir le rend.
        cx.update_window(window, |_, window, cx| {
            shell.update(cx, |shell, cx| {
                let tab = shell.tabs[0].clone();
                tab.update(cx, |tab, _| tab.status = Status::Working);
                let session = Session {
                    id: tab.read(cx).session_id.clone().unwrap(),
                    title: "Bonjour".into(),
                    mission: None,
                };
                shell.close(0, window, cx);
                assert!(shell.tabs.is_empty());
                assert!(shell.background.contains(&tab));
                assert!(shell.status_of(&session.id, cx) == Some(Status::Working));
                assert!(shell.state.open_tabs.is_empty(), "plus un onglet ouvert");
                shell.open(second.clone(), session, window, cx);
                assert!(shell.tabs == [tab]);
                assert!(shell.background.is_empty());
            });
        })
        .unwrap();

        let received = std::fs::read_to_string(&log).unwrap();
        let sent = received.matches(r#""type":"user""#).count();
        assert_eq!(sent, 1, "message envoyé {sent} fois :\n{received}");
        let message = received
            .lines()
            .find(|line| line.contains(r#""type":"user""#))
            .unwrap();
        assert!(message.contains("Bonjour"));
        // Le message part du processus lancé dans le second projet, le dernier démarré avant lui.
        let before = &received[..received.find(message).unwrap()];
        let cwd = before
            .lines()
            .rev()
            .find(|line| line.starts_with("cwd "))
            .unwrap();
        assert!(cwd.ends_with("/second"), "lancé dans : {cwd}");

        let saved = State::load(&state_path);
        assert_eq!(saved.open_tabs, ["s-1"]);
        assert_eq!(saved.projects[1].sessions[0].title, "Bonjour");
        assert!(saved.projects[0].sessions.is_empty());
    }
}
