//! La vue Bibliothèque : les modèles du projet en cours, les personnels et les
//! exemples ; créer, modifier, dupliquer, supprimer. Les fichiers restent la
//! source : un modèle changé ailleurs n'est jamais écrasé.

use std::path::PathBuf;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;
use solar_core::library::{self, DeleteError, Origin, Skill, Template};
use solar_core::project::{self, SaveError};

use super::Shell;
use crate::theme::caps;

pub(super) struct LibraryView {
    /// Le projet de la vue en cours, dont on montre les modèles.
    project: Option<PathBuf>,
    templates: Vec<Template>,
    editing: Option<Editing>,
    /// Le modèle dont la suppression attend confirmation (son fichier).
    confirm_delete: Option<PathBuf>,
    note: Option<String>,
    /// Les skills installés, lus en arrière-plan ; `None` pendant la lecture.
    skills: Option<Vec<Skill>>,
}

impl LibraryView {
    #[cfg(test)]
    pub(super) fn templates(&self) -> &[Template] {
        &self.templates
    }
}

struct Editing {
    /// Le fichier modifié ; `None` pour un nouveau modèle, rangé dans `dir`.
    file: Option<PathBuf>,
    dir: PathBuf,
    name: Option<Entity<InputState>>,
    editor: Entity<TextareaState>,
    opened: Option<String>,
}

impl Shell {
    pub(super) fn open_library(&mut self, cx: &mut Context<Self>) {
        let project = Some(self.current_view().read(cx).project.clone())
            .filter(|project| !project.as_os_str().is_empty());
        self.importing = None;
        self.settings_open = false;
        self.card = None;
        self.bots_view = None;
        self.take_focus = true;
        self.library = Some(LibraryView {
            project,
            templates: Vec::new(),
            editing: None,
            confirm_delete: None,
            note: None,
            skills: None,
        });
        self.reload_library(cx);
        self.load_skills(cx);
        self.sync_previews(cx);
        cx.notify();
    }

    /// Relit les modèles de la vue, et la présence d'une bibliothèque à soi ;
    /// les conversations relisent les leurs.
    pub(super) fn reload_library(&mut self, cx: &mut Context<Self>) {
        let examples = crate::session_view::library_examples();
        if let Some(view) = self.library.as_mut() {
            view.templates =
                library::load(&library::personal_dir(), view.project.as_deref(), &examples);
        }
        self.refresh_has_library(cx);
        for view in std::iter::once(&self.home).chain(&self.tabs) {
            view.update(cx, |view, cx| view.load_templates(cx));
        }
        cx.notify();
    }

    /// Un modèle personnel ou d'un projet connu existe-t-il ? En arrière-plan.
    pub(super) fn refresh_has_library(&mut self, cx: &mut Context<Self>) {
        let projects: Vec<PathBuf> = self.state.projects.iter().map(|p| p.path.clone()).collect();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async move {
                    library::has_templates(&library::personal_dir())
                        || projects
                            .iter()
                            .any(|project| library::has_templates(&library::project_dir(project)))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.has_library != found {
                    this.has_library = found;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Relit les skills des agents (projet en cours et dossier de l'utilisateur).
    pub(super) fn load_skills(&mut self, cx: &mut Context<Self>) {
        let project = self
            .library
            .as_ref()
            .and_then(|view| view.project.clone())
            .or_else(|| {
                Some(self.current_view().read(cx).project.clone())
                    .filter(|project| !project.as_os_str().is_empty())
            });
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async move {
                    library::skills(
                        &std::env::home_dir().unwrap_or_default(),
                        project.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(view) = this.library.as_mut() {
                    view.skills = Some(found.clone());
                }
                this.skills = found;
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn edit_template(
        &mut self,
        file: Option<PathBuf>,
        dir: PathBuf,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.library.as_mut() else {
            return;
        };
        let opened = file
            .as_ref()
            .and_then(|file| std::fs::read_to_string(file).ok());
        let editor = cx.new(|cx| TextareaState::new(window, cx).auto_grow(10, 28));
        editor.update(cx, |editor, cx| editor.set_value(text, window, cx));
        let name = file.is_none().then(|| {
            let free = library::free_name(&dir, &t!("library.new_name"));
            let input = cx.new(|cx| InputState::new(window, cx));
            input.update(cx, |input, cx| {
                input.set_value(free, window, cx);
                input.focus(window, cx);
            });
            input
        });
        if name.is_none() {
            editor.update(cx, |editor, cx| editor.focus(window, cx));
        }
        view.editing = Some(Editing {
            file,
            dir,
            name,
            editor,
            opened,
        });
        view.note = None;
        cx.notify();
    }

    pub(super) fn save_template(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.library.as_mut() else {
            return;
        };
        let Some(editing) = view.editing.as_ref() else {
            return;
        };
        let text = editing.editor.read(cx).value().to_string();
        let file = match (&editing.file, &editing.name) {
            (Some(file), _) => file.clone(),
            (None, Some(name)) => {
                let name = name.read(cx).value().trim().to_string();
                if !library::valid_name(&name) {
                    view.note = Some(t!("library.bad_name").to_string());
                    cx.notify();
                    return;
                }
                let file = editing.dir.join(format!("{name}.md"));
                if file.exists() {
                    view.note = Some(t!("library.name_taken", name = name).to_string());
                    cx.notify();
                    return;
                }
                file
            }
            (None, None) => return,
        };
        match project::save(&file, editing.opened.as_deref(), &text) {
            Ok(()) => {
                view.editing = None;
                view.note = Some(t!("library.saved", name = stem(&file)).to_string());
                self.reload_library(cx);
            }
            Err(SaveError::ChangedElsewhere) => {
                view.note = Some(t!("library.changed_elsewhere").to_string());
                cx.notify();
            }
            Err(SaveError::Io(error)) => {
                view.note = Some(t!("library.failed", error = error).to_string());
                cx.notify();
            }
        }
    }

    /// Une copie personnelle d'un modèle (un exemple, ou celui d'un projet).
    pub(super) fn duplicate_template(&mut self, template: &Template, cx: &mut Context<Self>) {
        let Some(view) = self.library.as_mut() else {
            return;
        };
        let dir = library::personal_dir();
        let name = library::free_name(&dir, &template.name);
        let text = match &template.path {
            Some(path) => std::fs::read_to_string(path).unwrap_or_default(),
            None => crate::session_view::library_examples()
                .into_iter()
                .find(|example| example.name == template.name)
                .map(|example| example.text)
                .unwrap_or_default(),
        };
        let file = dir.join(format!("{name}.md"));
        view.note = Some(match project::save(&file, None, &text) {
            Ok(()) => t!("library.duplicated", name = name).to_string(),
            Err(SaveError::ChangedElsewhere) => t!("library.name_taken", name = name).to_string(),
            Err(SaveError::Io(error)) => t!("library.failed", error = error).to_string(),
        });
        self.reload_library(cx);
    }

    fn delete_template(&mut self, file: PathBuf, cx: &mut Context<Self>) {
        let Some(view) = self.library.as_mut() else {
            return;
        };
        view.confirm_delete = None;
        let opened = view
            .templates
            .iter()
            .find(|t| t.path.as_ref() == Some(&file))
            .and_then(|_| std::fs::read_to_string(&file).ok())
            .unwrap_or_default();
        view.note = Some(match library::delete(&file, &opened) {
            Ok(()) => t!("library.deleted", name = stem(&file)).to_string(),
            Err(DeleteError::ChangedElsewhere) => t!("library.changed_elsewhere").to_string(),
            Err(DeleteError::Io(error)) => t!("library.failed", error = error).to_string(),
        });
        self.reload_library(cx);
    }

    pub(super) fn render_library(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(view) = self.library.as_ref() else {
            return div().into_any_element();
        };
        let theme = cx.theme();
        let (muted, border) = (theme.muted_foreground, theme.border);
        let section = |text: String| {
            div()
                .pt_6()
                .pb_2()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(muted)
                .child(caps(&text))
        };
        let project_name = view
            .project
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string());

        let row = |ix: usize, template: &Template, cx: &mut Context<Self>| {
            let deletable = template.origin != Origin::Example;
            let confirming = template.path.is_some() && view.confirm_delete == template.path;
            let (edit_file, edit_dir) = (
                template.path.clone(),
                template
                    .path
                    .as_ref()
                    .and_then(|p| p.parent())
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(library::personal_dir),
            );
            let duplicate = template.clone();
            let delete_file = template.path.clone();
            let detail = match (&template.problem, template.description.is_empty()) {
                (Some(problem), _) => t!("library.problem", problem = problem).to_string(),
                (None, true) => format!("/{}", template.name),
                (None, false) => format!("/{} · {}", template.name, template.description),
            };
            v_flex()
                .py_3()
                .gap_2()
                .border_b_1()
                .border_color(border)
                .child(
                    h_flex()
                        .gap_3()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .font_weight(FontWeight::BOLD)
                                        .truncate()
                                        .child(template.title.clone()),
                                )
                                .child(div().text_sm().text_color(muted).truncate().child(detail)),
                        )
                        .when(deletable, |row| {
                            row.child(
                                Button::new(("template-edit", ix))
                                    .ghost()
                                    .small()
                                    .label(t!("library.edit").to_string())
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        let text = edit_file
                                            .as_ref()
                                            .and_then(|f| std::fs::read_to_string(f).ok())
                                            .unwrap_or_default();
                                        this.edit_template(
                                            edit_file.clone(),
                                            edit_dir.clone(),
                                            text,
                                            window,
                                            cx,
                                        )
                                    })),
                            )
                        })
                        .child(
                            Button::new(("template-duplicate", ix))
                                .ghost()
                                .small()
                                .label(t!("library.duplicate").to_string())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.duplicate_template(&duplicate, cx)
                                })),
                        )
                        .when(deletable && !confirming, |row| {
                            let file = delete_file.clone();
                            row.child(
                                Button::new(("template-delete", ix))
                                    .ghost()
                                    .small()
                                    .label(t!("library.delete").to_string())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(view) = this.library.as_mut() {
                                            view.confirm_delete = file.clone();
                                        }
                                        cx.notify();
                                    })),
                            )
                        }),
                )
                .when(confirming, |column| {
                    let file = delete_file.clone().unwrap_or_default();
                    column.child(
                        h_flex()
                            .flex_wrap()
                            .gap_2()
                            .child(div().w_full().child(
                                t!("library.confirm_delete", name = template.name).to_string(),
                            ))
                            .child(
                                Button::new(("template-delete-cancel", ix))
                                    .ghost()
                                    .small()
                                    .label(t!("library.cancel").to_string())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(view) = this.library.as_mut() {
                                            view.confirm_delete = None;
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new(("template-delete-go", ix))
                                    .primary()
                                    .small()
                                    .label(t!("library.delete").to_string())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.delete_template(file.clone(), cx)
                                    })),
                            ),
                    )
                })
        };

        let templates = view.templates.clone();
        let group = |origin: Origin| -> Vec<(usize, Template)> {
            templates
                .iter()
                .enumerate()
                .filter(|(_, t)| t.origin == origin)
                .map(|(ix, t)| (ix, t.clone()))
                .collect()
        };
        let (of_project, personal, examples) = (
            group(Origin::Project),
            group(Origin::Personal),
            group(Origin::Example),
        );

        let editing = view.editing.as_ref().map(|editing| {
            v_flex()
                .pt_4()
                .gap_3()
                .when_some(editing.name.as_ref(), |column, name| {
                    column.child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(muted)
                                    .child(caps(&t!("library.name"))),
                            )
                            .child(Input::new(name).small()),
                    )
                })
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(t!("library.saved_in", dir = editing.dir.display()).to_string()),
                )
                .child(
                    div()
                        .p_2()
                        .rounded(theme.radius)
                        .border_1()
                        .border_color(border)
                        .font_family(crate::theme::READING_FONT)
                        .child(Textarea::new(&editing.editor).appearance(false)),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("template-save")
                                .primary()
                                .small()
                                .label(t!("library.save").to_string())
                                .on_click(cx.listener(|this, _, _, cx| this.save_template(cx))),
                        )
                        .child(
                            Button::new("template-close")
                                .ghost()
                                .small()
                                .label(t!("library.cancel").to_string())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(view) = this.library.as_mut() {
                                        view.editing = None;
                                        view.note = None;
                                    }
                                    cx.notify();
                                })),
                        ),
                )
        });
        let note = view.note.clone();
        let project_dir = view.project.as_ref().map(|p| library::project_dir(p));
        let new_buttons = h_flex()
            .flex_wrap()
            .gap_2()
            .child(
                Button::new("template-new")
                    .outline()
                    .small()
                    .icon(IconName::Plus)
                    .label(t!("library.new_personal").to_string())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.edit_template(
                            None,
                            library::personal_dir(),
                            t!("library.skeleton").to_string(),
                            window,
                            cx,
                        )
                    })),
            )
            .when_some(project_dir.zip(project_name.clone()), |row, (dir, name)| {
                row.child(
                    Button::new("template-new-project")
                        .outline()
                        .small()
                        .icon(IconName::Plus)
                        .label(t!("library.new_project", project = name).to_string())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.edit_template(
                                None,
                                dir.clone(),
                                t!("library.skeleton").to_string(),
                                window,
                                cx,
                            )
                        })),
                )
            });

        let mut body = v_flex()
            .w_full()
            .max_w(px(760.))
            .mx_auto()
            .px_8()
            .py_8()
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::BOLD)
                    .child(t!("library.title").to_string()),
            )
            .child(
                div()
                    .pt_1()
                    .text_color(muted)
                    .child(t!("library.intro").to_string()),
            )
            .children(note.map(|text| div().pt_3().child(text)))
            .child(div().pt_4().child(new_buttons))
            .children(editing);
        if let Some(name) = project_name.filter(|_| !of_project.is_empty()) {
            body = body
                .child(section(
                    t!("library.of_project", project = name).to_string(),
                ))
                .children(of_project.iter().map(|(ix, t)| row(*ix, t, cx)));
        }
        body = body
            .child(section(t!("library.personal").to_string()))
            .when(personal.is_empty(), |column| {
                column.child(
                    div()
                        .text_color(muted)
                        .child(t!("library.none").to_string()),
                )
            })
            .children(personal.iter().map(|(ix, t)| row(*ix, t, cx)))
            .child(section(t!("library.examples").to_string()))
            .children(examples.iter().map(|(ix, t)| row(*ix, t, cx)))
            .child(section(t!("library.skills").to_string()))
            .child(
                div()
                    .pb_2()
                    .text_sm()
                    .text_color(muted)
                    .child(t!("library.skills_intro").to_string()),
            )
            .children(self.render_skills(cx));
        div()
            .id("library")
            .size_full()
            .overflow_y_scroll()
            .child(body)
            .into_any_element()
    }
}

impl Shell {
    /// Les skills, groupés par dossier d'origine, en lecture seule.
    fn render_skills(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = cx.theme();
        let (muted, border) = (theme.muted_foreground, theme.border);
        let Some(skills) = self.library.as_ref().and_then(|view| view.skills.as_ref()) else {
            return vec![
                div()
                    .text_color(muted)
                    .child(t!("library.skills_reading").to_string())
                    .into_any_element(),
            ];
        };
        if skills.is_empty() {
            return vec![
                div()
                    .text_color(muted)
                    .child(t!("library.skills_none").to_string())
                    .into_any_element(),
            ];
        }
        let mut out = Vec::new();
        let mut current: Option<&str> = None;
        for (ix, skill) in skills.iter().enumerate() {
            if current != Some(skill.source.as_str()) {
                current = Some(skill.source.as_str());
                out.push(
                    div()
                        .pt_4()
                        .pb_1()
                        .text_sm()
                        .font_weight(FontWeight::BOLD)
                        .text_color(muted)
                        .child(skill.source.clone())
                        .into_any_element(),
                );
            }
            let readers = skill
                .readers
                .iter()
                .map(|p| p.name())
                .collect::<Vec<_>>()
                .join(", ");
            let path = skill.path.clone();
            let detail = match &skill.problem {
                Some(problem) => t!("library.problem", problem = problem).to_string(),
                None => skill.description.clone(),
            };
            out.push(
                h_flex()
                    .py_2()
                    .gap_3()
                    .items_start()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .font_weight(FontWeight::BOLD)
                                            .child(skill.name.clone()),
                                    )
                                    .child(div().text_xs().text_color(muted).child(
                                        t!("library.read_by", agents = readers).to_string(),
                                    )),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .line_clamp(2)
                                    .child(detail),
                            ),
                    )
                    .child(
                        Button::new(("skill-reveal", ix))
                            .ghost()
                            .small()
                            .icon(IconName::FolderOpen)
                            .tooltip(t!("library.reveal"))
                            .on_click(move |_, _, cx| cx.reveal_path(&path)),
                    )
                    .into_any_element(),
            );
        }
        out
    }
}

fn stem(file: &std::path::Path) -> String {
    file.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}
