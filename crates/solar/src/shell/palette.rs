//! La palette de recherche (⌘K) : sessions, commandes, fichiers et réglages,
//! rangés par catégorie et parcourus au clavier. Les sessions sont trouvées par
//! leur projet, titre, plan, statut et le texte de leur conversation.

use std::path::PathBuf;

use gpui_kit::assets::IconName;
use gpui_kit::base::input::IndentInline;
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IndexPath, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;
use solar_core::agent::Provider;
use solar_core::state::Session;

use super::{NewConversation, Shell, ToggleSidebar};
use crate::session_view::MODES;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Scope {
    All,
    Sessions,
    Commands,
    Files,
    Settings,
    Library,
}

/// L'écart entre le haut de la fenêtre et la palette.
const PALETTE_TOP: f32 = 72.;

const SCOPES: [Scope; 6] = [
    Scope::All,
    Scope::Sessions,
    Scope::Commands,
    Scope::Files,
    Scope::Settings,
    Scope::Library,
];

impl Scope {
    fn key(self) -> &'static str {
        match self {
            Scope::All => "palette.all",
            Scope::Sessions => "palette.sessions",
            Scope::Commands => "palette.commands",
            Scope::Files => "palette.files",
            Scope::Settings => "palette.settings",
            Scope::Library => "palette.library",
        }
    }
}

/// Ce que fait une ligne choisie.
#[derive(Clone)]
enum Target {
    Session(PathBuf, Box<Session>),
    /// Une action liée à un raccourci : la palette l'a déjà envoyée.
    Dispatched,
    NewIn(PathBuf),
    NewProject,
    Import(PathBuf),
    File(String),
    AllFiles,
    Provider(Provider),
    Mode(&'static str),
    OpenSettings,
    Card(PathBuf),
    /// Un modèle de la bibliothèque, par son nom.
    Template(String),
    OpenLibrary,
    NewBot,
    OpenBots,
}

/// Une ligne avant tri : sa pertinence, son élément et ce qu'elle fait.
type Ranked = (u32, CommandItem, Target);

/// Les lignes triées par pertinence (l'ordre d'origine départage), et la
/// meilleure pertinence du groupe.
fn ranked(mut rows: Vec<Ranked>) -> (Vec<(CommandItem, Target)>, u32) {
    rows.sort_by_key(|(rank, ..)| *rank);
    let best = rows.first().map_or(u32::MAX, |(rank, ..)| *rank);
    (
        rows.into_iter()
            .map(|(_, item, target)| (item, target))
            .collect(),
        best,
    )
}

/// Un groupe affiché : sa catégorie, son titre, ses lignes et ce qu'elles font.
type Group = (Scope, String, Vec<(CommandItem, Target)>);

pub(super) struct Palette {
    command: Entity<CommandState>,
    input: Entity<InputState>,
    pub(super) scope: Scope,
    /// Les fichiers du dossier de la vue en cours, les plus récemment touchés
    /// par l'agent en tête ; vide tant qu'ils ne sont pas listés.
    files: Vec<String>,
    /// Ce que fait chaque ligne affichée, dans l'ordre des groupes.
    targets: Vec<Vec<Target>>,
    _subscription: Subscription,
}

impl Shell {
    pub(super) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(t!("palette.placeholder")));
        let subscription = cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                let query = input.read(cx).value().to_string();
                this.set_query(&query, cx);
            }
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        // La liste des fichiers vient du dossier de la vue en cours.
        let view = self.current_view().read(cx);
        let (root, touched) = (view.workdir(), view.touched_files());
        cx.spawn(async move |this, cx| {
            let listed = cx
                .background_spawn(async move { solar_core::files::list(&root) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(palette) = this.palette.as_mut() {
                    let rest: Vec<String> = listed
                        .into_iter()
                        .filter(|path| !touched.contains(path))
                        .collect();
                    palette.files = touched.into_iter().chain(rest).collect();
                    cx.notify();
                }
            });
        })
        .detach();
        self.palette = Some(Palette {
            command: cx.new(|cx| CommandState::new(window, cx)),
            input,
            scope: Scope::All,
            files: Vec::new(),
            targets: Vec::new(),
            _subscription: subscription,
        });
        // La page de preview, vue native, passerait devant la palette.
        self.menu_open = true;
        self.sync_previews(cx);
        cx.notify();
    }

    pub(super) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.take().is_some() {
            self.set_query("", cx);
            self.menu_open = false;
            self.sync_previews(cx);
            // Le composer reprend la main.
            let view = self.current_view().clone();
            view.update(cx, |view, cx| view.activate(window, cx));
            cx.notify();
        }
    }

    fn next_scope(&mut self, cx: &mut Context<Self>) {
        if let Some(palette) = self.palette.as_mut() {
            let at = SCOPES
                .iter()
                .position(|scope| *scope == palette.scope)
                .unwrap_or(0);
            palette.scope = SCOPES[(at + 1) % SCOPES.len()];
            cx.notify();
        }
    }

    pub(super) fn confirm(&mut self, path: IndexPath, window: &mut Window, cx: &mut Context<Self>) {
        let target = self
            .palette
            .as_ref()
            .and_then(|palette| palette.targets.get(path.section)?.get(path.row).cloned());
        let Some(target) = target else { return };
        if let Target::AllFiles = target {
            if let Some(palette) = self.palette.as_mut() {
                palette.scope = Scope::Files;
            }
            cx.notify();
            return;
        }
        self.close_palette(window, cx);
        let view = self.current_view().clone();
        match target {
            Target::Session(project, session) => self.open(project, *session, window, cx),
            Target::NewIn(project) => self.go_home(Some(project), window, cx),
            Target::NewProject => self.add_project(window, cx),
            Target::Import(project) => self.start_import(project, cx),
            Target::File(file) => {
                self.importing = None;
                view.update(cx, |view, cx| view.open_file(file, cx));
            }
            Target::Provider(provider) => {
                view.update(cx, |view, cx| view.set_provider(provider, window, cx))
            }
            Target::Mode(mode) => view.update(cx, |view, cx| view.set_mode(mode, cx)),
            Target::OpenSettings => self.open_settings(cx),
            Target::Card(project) => self.open_card(project, cx),
            Target::OpenLibrary => self.open_library(cx),
            Target::NewBot => self.edit_bot(None, window, cx),
            Target::OpenBots => self.open_bots(cx),
            Target::Template(name) => {
                view.update(cx, |view, cx| view.use_template(&name, window, cx))
            }
            Target::Dispatched | Target::AllFiles => {}
        }
    }

    /// Les groupes de la palette pour la recherche en cours, avec ce que fait
    /// chaque ligne, et le nombre de résultats par catégorie.
    fn palette_groups(&self, cx: &App) -> (Vec<Group>, [usize; 6]) {
        let Some(palette) = self.palette.as_ref() else {
            return (Vec::new(), [0; 6]);
        };
        let query = self.query.as_str();
        let search = &solar_core::search::Query::new(query);
        let muted = cx.theme().muted_foreground;

        // Chaque ligne porte sa pertinence (0 : la meilleure) ; les groupes sont
        // triés dessus, et dans « Tout » ils passent du meilleur au moins bon.
        let sessions: Vec<Ranked> = self
            .visible()
            .into_iter()
            .flat_map(|(project, sessions)| {
                let name: SharedString = project
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
                    .into();
                sessions.into_iter().map(move |session| {
                    let (title, place) = (SharedString::from(session.title.clone()), name.clone());
                    let icon = if session.mission.is_some() {
                        IconName::ListChecks
                    } else {
                        IconName::MessageSquare
                    };
                    let item = CommandItem::new().label(title.clone()).child(move |_, _| {
                        h_flex()
                            .w_full()
                            .gap_2()
                            .child(Icon::new(icon).small())
                            .child(div().flex_1().min_w_0().truncate().child(title.clone()))
                            .child(div().flex_shrink_0().text_color(muted).child(place.clone()))
                    });
                    // Trouvée par son plan ou sa conversation : après les titres.
                    let rank = search.rank(&session.title).unwrap_or(10);
                    (
                        rank,
                        item,
                        Target::Session(project.path.clone(), Box::new(session.clone())),
                    )
                })
            })
            .collect();

        let current = self.current_view().read(cx);
        // Une ligne de commande ou de réglage, gardée si son texte contient la recherche.
        let row = |label: String, icon: IconName| {
            search
                .rank(&label)
                .map(|rank| (rank, CommandItem::new().label(label).icon(icon)))
        };
        let mut commands: Vec<Ranked> = [
            row(t!("sidebar.new_session").into(), IconName::SquarePen).map(|(rank, item)| {
                (
                    rank,
                    item.action(Box::new(NewConversation)),
                    Target::Dispatched,
                )
            }),
            row(t!("sidebar.add_project").into(), IconName::FolderPlus)
                .map(|(rank, item)| (rank, item, Target::NewProject)),
            row(t!("bots.new").into(), IconName::Bot)
                .map(|(rank, item)| (rank, item, Target::NewBot)),
            (!self.state.bots.is_empty())
                .then(|| row(t!("bots.open").into(), IconName::Bot))
                .flatten()
                .map(|(rank, item)| (rank, item, Target::OpenBots)),
            row(t!("palette.toggle_sidebar").into(), IconName::PanelLeft).map(|(rank, item)| {
                (
                    rank,
                    item.action(Box::new(ToggleSidebar)),
                    Target::Dispatched,
                )
            }),
        ]
        .into_iter()
        .flatten()
        .collect();
        for known in &self.state.projects {
            let name = known.path.file_name().unwrap_or_default().to_string_lossy();
            commands.extend(
                row(
                    t!("palette.new_in", project = name).into(),
                    IconName::Folder,
                )
                .map(|(rank, item)| (rank, item, Target::NewIn(known.path.clone()))),
            );
            commands.extend(
                row(
                    t!("palette.import_in", project = name).into(),
                    IconName::Import,
                )
                .map(|(rank, item)| (rank, item, Target::Import(known.path.clone()))),
            );
            commands.extend(
                row(
                    t!("palette.card", project = name).into(),
                    IconName::BookOpen,
                )
                .map(|(rank, item)| (rank, item, Target::Card(known.path.clone()))),
            );
        }

        let files: Vec<Ranked> = palette
            .files
            .iter()
            // Le nom du fichier compte plus que son dossier.
            .filter_map(|path| {
                let name = path.rsplit('/').next().unwrap_or(path);
                let rank = search.rank(name).or_else(|| Some(search.rank(path)? + 4))?;
                Some((rank, path))
            })
            .take(500)
            .map(|(rank, path)| {
                let (name, folder) = match path.rsplit_once('/') {
                    Some((folder, name)) => (name.to_string(), folder.to_string()),
                    None => (path.clone(), String::new()),
                };
                let item = CommandItem::new().label(path.clone()).child(move |_, _| {
                    h_flex()
                        .w_full()
                        .gap_2()
                        .child(Icon::new(IconName::FileText).small())
                        .child(div().flex_1().min_w_0().truncate().child(name.clone()))
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(muted)
                                .child(folder.clone()),
                        )
                });
                (rank, item, Target::File(path.clone()))
            })
            .collect();

        // Les réglages qui existent : l'agent (avant le premier message) et le
        // mode de permission de la conversation en cours.
        let started = current.started();
        let mut settings: Vec<Ranked> = Provider::ALL
            .into_iter()
            .filter_map(|provider| {
                row(
                    t!("palette.agent", agent = provider.name()).into(),
                    IconName::Bot,
                )
                .map(|(rank, item)| {
                    (
                        rank,
                        item.checked(current.provider == provider)
                            .disabled(started || !provider.installed()),
                        Target::Provider(provider),
                    )
                })
            })
            .collect();
        settings.extend(
            row(t!("palette.open_settings").into(), IconName::Settings)
                .map(|(rank, item)| (rank, item, Target::OpenSettings)),
        );
        for mode in MODES
            .into_iter()
            .filter(|mode| current.provider.supports(mode))
        {
            let label = t!("palette.permissions", mode = t!(format!("mode.{mode}"))).into();
            settings.extend(row(label, IconName::ShieldCheck).map(|(rank, item)| {
                (
                    rank,
                    item.checked(current.mode() == mode),
                    Target::Mode(mode),
                )
            }));
        }

        // Les modèles de la bibliothèque, ceux du projet de la vue en cours.
        let library: Vec<Ranked> = current
            .templates()
            .iter()
            .filter_map(|template| {
                let label = format!("{} · /{}", template.title, template.name);
                row(label, IconName::BookOpen)
                    .map(|(rank, item)| (rank, item, Target::Template(template.name.clone())))
            })
            .collect();
        let mut library = library;
        library.extend(self.skills.iter().filter_map(|skill| {
            let label = t!("library.skill_row", name = skill.name).to_string();
            row(label, IconName::Sparkles).map(|(rank, item)| (rank, item, Target::OpenLibrary))
        }));
        library.extend(
            row(t!("library.open").into(), IconName::BookOpen)
                .map(|(rank, item)| (rank, item, Target::OpenLibrary)),
        );
        let (library, best_library) = ranked(library);
        let (sessions, best_session) = ranked(sessions);
        let (commands, best_command) = ranked(commands);
        let (files, best_file) = ranked(files);
        let (settings, best_setting) = ranked(settings);
        let counts = [
            sessions.len() + commands.len() + files.len() + settings.len() + library.len(),
            sessions.len(),
            commands.len(),
            files.len(),
            settings.len(),
            library.len(),
        ];
        // « Tout » montre un aperçu de chaque catégorie ; les fichiers récents
        // renvoient vers leur catégorie.
        let all = palette.scope == Scope::All;
        let cap = |rows: Vec<(CommandItem, Target)>, n: usize| -> Vec<(CommandItem, Target)> {
            if all {
                rows.into_iter().take(n).collect()
            } else {
                rows
            }
        };
        let file_count = files.len();
        let mut files = cap(files, 5);
        if all && file_count > files.len() {
            files.push((
                CommandItem::new()
                    .label(t!("palette.all_files", count = file_count))
                    .icon(IconName::List),
                Target::AllFiles,
            ));
        }
        let groups = vec![
            (
                best_session,
                Scope::Sessions,
                t!("palette.sessions").to_string(),
                cap(sessions, 6),
            ),
            (
                best_file,
                Scope::Files,
                t!(if query.is_empty() {
                    "palette.recent_files"
                } else {
                    "palette.files"
                })
                .to_string(),
                files,
            ),
            (
                best_command,
                Scope::Commands,
                t!("palette.actions").to_string(),
                cap(commands, 6),
            ),
            (
                best_setting,
                Scope::Settings,
                t!("palette.settings").to_string(),
                cap(settings, 6),
            ),
            (
                best_library,
                Scope::Library,
                t!("palette.library").to_string(),
                cap(library, 6),
            ),
        ];
        let mut groups: Vec<_> = groups
            .into_iter()
            .filter(|(_, scope, _, rows)| (all || *scope == palette.scope) && !rows.is_empty())
            .collect();
        // Entrée ouvre la première ligne : le groupe du meilleur résultat d'abord.
        if !query.is_empty() {
            groups.sort_by_key(|(best, ..)| *best);
        }
        let groups = groups
            .into_iter()
            .map(|(_, scope, label, rows)| (scope, label, rows))
            .collect();
        (groups, counts)
    }

    pub(super) fn render_palette(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // Dans une petite fenêtre, la palette garde son pied visible : la liste
        // prend ce qui reste sous le champ, les catégories et le pied (`CHROME`).
        const CHROME: f32 = 120.;
        let height = (window.viewport_size().height - px(PALETTE_TOP + CHROME + 16.))
            .clamp(px(120.), px(420.));
        let (groups, counts) = self.palette_groups(cx);
        let palette = self.palette.as_mut()?;
        let theme = cx.theme();
        let (muted, border) = (theme.muted_foreground, theme.border);
        let shell = cx.entity();

        palette.targets = groups
            .iter()
            .map(|(_, _, rows)| rows.iter().map(|(_, target)| target.clone()).collect())
            .collect();
        let command = groups.into_iter().fold(
            Command::new(&palette.command)
                .searchable(false)
                .filterable(false)
                .bordered(false)
                // Le cadre de la palette : ses lignes en prennent 8 (12 moins la marge).
                .rounded(crate::theme::RADIUS)
                .max_h(height),
            |command, (_, label, rows)| {
                command.group(
                    CommandGroup::new()
                        .label(label)
                        .items(rows.into_iter().map(|(item, _)| item)),
                )
            },
        );

        let scope = palette.scope;
        let (ghost, white, radius) = (theme.muted, theme.foreground, theme.radius);
        let tab_shell = shell.clone();
        let tabs = move || {
            SCOPES.into_iter().zip(counts).map({
                let shell = tab_shell.clone();
                move |(each, count)| {
                    let selected = each == scope;
                    let shell = shell.clone();
                    div()
                        .id(each.key())
                        .px_2()
                        .py_0p5()
                        .rounded(radius)
                        .cursor_pointer()
                        .when(selected, |tab| tab.bg(ghost).text_color(white))
                        .when(!selected, |tab| tab.text_color(muted))
                        .child(
                            h_flex()
                                .gap_1p5()
                                .child(t!(each.key()).to_string())
                                .child(div().text_xs().text_color(muted).child(count.to_string())),
                        )
                        .on_click(move |_, _, cx| {
                            shell.update(cx, |shell, cx| {
                                if let Some(palette) = shell.palette.as_mut() {
                                    palette.scope = each;
                                }
                                cx.notify();
                            })
                        })
                }
            })
        };
        let key = move |label: &str| {
            div()
                .px_1()
                .rounded(crate::theme::INNER_RADIUS)
                .border_1()
                .border_color(border)
                .text_xs()
                .child(label.to_string())
        };
        let hint = move |keys: Vec<&str>, label: String| {
            h_flex()
                .gap_1()
                .children(keys.into_iter().map(key))
                .child(div().pl_1().child(label))
        };

        let input = palette.input.clone();
        let (confirm_shell, cancel_shell) = (shell.clone(), shell.clone());
        let command = command
            .header(move |_, _, _| {
                v_flex()
                    .child(
                        div()
                            .flex_none()
                            .px_3()
                            .border_b_1()
                            .border_color(border)
                            .child(
                                Input::new(&input)
                                    .prefix(Icon::new(IconName::Search).text_color(muted))
                                    .appearance(false)
                                    .p_0(),
                            ),
                    )
                    // Les catégories, sous le champ ; Tab passe à la suivante.
                    .child(
                        h_flex()
                            .px_2()
                            .py_1p5()
                            .gap_1()
                            .text_sm()
                            .border_b_1()
                            .border_color(border)
                            .children(tabs()),
                    )
                    .into_any_element()
            })
            .footer(move |_, _, _| {
                h_flex()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(border)
                    .justify_between()
                    .text_xs()
                    .text_color(muted)
                    .child(hint(vec!["↵"], t!("palette.run").to_string()))
                    .child(
                        h_flex()
                            .gap_4()
                            .child(hint(vec!["⇥"], t!("palette.next_scope").to_string()))
                            .child(hint(vec!["↑", "↓"], t!("palette.navigate").to_string()))
                            .child(hint(vec!["esc"], t!("palette.close").to_string())),
                    )
            })
            .on_confirm(move |path, window, cx| {
                confirm_shell.update(cx, |shell, cx| shell.confirm(path, window, cx))
            })
            .on_cancel(move |window, cx| {
                cancel_shell.update(cx, |shell, cx| shell.close_palette(window, cx))
            });

        let backdrop_shell = shell.clone();
        Some(
            div()
                .id("palette-backdrop")
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(PALETTE_TOP))
                .bg(gpui_kit::rgb(crate::theme::BLACK).opacity(0.4))
                .occlude()
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    backdrop_shell.update(cx, |shell, cx| shell.close_palette(window, cx))
                })
                .child(
                    v_flex()
                        .id("palette")
                        .w(px(640.))
                        .max_w_full()
                        .h_auto()
                        .rounded(crate::theme::RADIUS)
                        .border_1()
                        .border_color(border)
                        .bg(theme.popover)
                        .overflow_hidden()
                        // Tab passe à la catégorie suivante.
                        .capture_action(cx.listener(|this, _: &IndentInline, _, cx| {
                            this.next_scope(cx);
                            cx.stop_propagation();
                        }))
                        // Un clic dans la palette ne la ferme pas.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(command),
                )
                .into_any_element(),
        )
    }
}
