//! La session pilotée de bout en bout dans une fenêtre invisible, face à un faux
//! `claude` qui rejoue une vraie session enregistrée (création d'un fichier, en
//! mode « demander »).

// `use super::*` amène le `test` de gpui_kit, que sa propre macro rappelle
// sans fin : on rend explicite celui de Rust.
use core::prelude::v1::test;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use gpui_kit::test::{TestAppContextExt, TestWindowExt};

use super::*;

/// Les tests qui posent un faux `claude` en tête de PATH passent un par un.
pub(crate) static PATH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn tool_paths_are_short() {
    let root = std::path::Path::new("/work/app");
    let short = |file: &str| short_path(std::path::Path::new(file), root);
    assert_eq!(short("/work/app/src/main.rs"), "src/main.rs");
    assert_eq!(short("/tmp/a/b/hello.md"), "…/b/hello.md");
    assert_eq!(short("/etc/hosts"), "/etc/hosts");
}

// Le verrou est tenu pendant tout le test : il change PATH.
#[allow(clippy::await_holding_lock)]
#[gpui_kit::test]
async fn draft_survives_a_failed_launch_and_a_project_change(cx: &mut TestAppContext) {
    let _path = PATH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = std::env::temp_dir().join(format!("solar-draft-test-{}", std::process::id()));
    let (first, second) = (dir.join("a"), dir.join("b"));
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    // Aucun `claude` sur ce PATH : chaque lancement échoue.
    let path = std::env::var("PATH").unwrap_or_default();
    unsafe { std::env::set_var("PATH", dir.display().to_string()) };

    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SessionView::new(first.clone(), None, "Test".into(), window, cx))
        })
        .unwrap()
    });
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| view.activate(window, cx));
        window.render_frame(cx);
        window.input("Mon brouillon", cx);
        view.update(cx, |view, cx| view.set_project(second.clone(), cx));
        window.render_frame(cx);
    })
    .unwrap();
    unsafe { std::env::set_var("PATH", path) };

    cx.update(|cx| {
        let view = view.read(cx);
        assert!(
            view.launch_error.is_some(),
            "le lancement aurait dû échouer"
        );
        assert_eq!(view.project, second);
        assert!(!view.started());
        assert_eq!(view.input.read(cx).value().as_ref(), "Mon brouillon");
    });
}

/// Pose en tête de PATH un faux CLI qui note tout ce qu'il reçoit et, à
/// chaque message, propose un plan ; ouvre une vue qui planifie une mission.
/// Rend la fenêtre, la vue, le journal et le PATH d'origine.
/// `repo` : le projet est un dépôt git, dans un sous-dossier, et la mission
/// travaille dans sa copie.
fn planned_mission(
    name: &str,
    repo: bool,
    cx: &mut TestAppContext,
) -> (
    AnyWindowHandle,
    Entity<SessionView>,
    std::path::PathBuf,
    String,
) {
    let dir = std::env::temp_dir().join(format!("solar-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("stdin.log");
    let _ = std::fs::remove_file(&log);
    let plan = r#"{"plan":"1. Lire le code\n2. Ajouter la page"}"#;
    let script = format!(
        "#!/bin/sh\nprintf 'cwd %s\\n' \"$PWD\" >> '{log}'\nwhile read -r line; do\n  printf '%s\\n' \"$line\" >> '{log}'\n  case \"$line\" in\n    \
         *'\"type\":\"user\"'*)\n      \
         printf '%s\\n' '{{\"type\":\"assistant\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"tool_use\",\"id\":\"p1\",\"name\":\"ExitPlanMode\",\"input\":{plan}}}]}}}}'\n      \
         printf '%s\\n' '{{\"type\":\"control_request\",\"request_id\":\"perm-plan\",\"request\":{{\"subtype\":\"can_use_tool\",\"tool_name\":\"ExitPlanMode\",\"input\":{plan}}}}}'\n      ;;\n  \
         esac\ndone\n",
        log = log.display()
    );
    std::fs::write(dir.join("claude"), script).unwrap();
    std::fs::set_permissions(dir.join("claude"), PermissionsExt::from_mode(0o755)).unwrap();
    let path = std::env::var("PATH").unwrap_or_default();
    unsafe { std::env::set_var("PATH", format!("{}:{path}", dir.display())) };
    let project = if repo {
        let repo = dir.join("repo");
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(repo.join("app")).unwrap();
        std::fs::write(repo.join("app/a.txt"), "v1\n").unwrap();
        for args in [
            &["init", "-q"][..],
            &["add", "."],
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-qm",
                "un",
            ],
        ] {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap();
        }
        repo.join("app")
    } else {
        dir.clone()
    };

    cx.executor().allow_parking();
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SessionView::new(project, None, "Test".into(), window, cx))
        })
        .unwrap()
    });
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.checkpoints = dir.join("checkpoints");
            view.activate(window, cx);
            view.planning = true;
        });
        window.render_frame(cx);
        window.input("Ajoute une page de connexion", cx);
        window.press("enter", cx);
    })
    .unwrap();
    (window, view, log, path)
}

// Le verrou est tenu pendant tout le test : il change PATH.
#[allow(clippy::await_holding_lock)]
#[gpui_kit::test]
async fn mission_is_planned_read_only_then_launched_in_the_chosen_mode(cx: &mut TestAppContext) {
    let _path = PATH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (window, view, log, path) = planned_mission("plan-test", false, cx);

    let real_time = || std::thread::sleep(Duration::from_millis(5));
    cx.wait_for(window, Duration::from_secs(5), |_, cx| {
        real_time();
        view.read(cx).plan_pending()
    })
    .await;
    cx.update(|cx| {
        let view = view.read(cx);
        assert_eq!(view.mode, PLAN_MODE);
        let mission = view.mission.as_ref().expect("une mission est ouverte");
        assert_eq!(mission.status, MissionStatus::Ready);
        assert_eq!(mission.plans, ["1. Lire le code\n2. Ajouter la page"]);
    });

    cx.update(|cx| view.update(cx, |view, cx| view.launch_mission("acceptEdits", cx)));
    let read_log = || std::fs::read_to_string(&log).unwrap_or_default();
    cx.wait_for(window, Duration::from_secs(5), |_, _| {
        real_time();
        read_log().contains("\"behavior\":\"allow\"")
    })
    .await;
    unsafe { std::env::set_var("PATH", path) };

    let lines: Vec<String> = read_log().lines().map(String::from).collect();
    let at = |needle: &str| {
        lines
            .iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("{needle} absent de : {lines:#?}"))
    };
    // Le mode plan précède le premier message : rien n'est écrit pendant la préparation.
    assert!(at("\"mode\":\"plan\"") < at("\"type\":\"user\""));
    // Le mode choisi précède l'accord : le CLI ne repasse pas en accès complet.
    assert!(at("\"mode\":\"acceptEdits\"") < at("\"behavior\":\"allow\""));
    cx.update(|cx| {
        let mission = view.read(cx).mission.clone().unwrap();
        assert_eq!(mission.status, MissionStatus::Running);
        assert_eq!(mission.launched_mode.as_deref(), Some("acceptEdits"));
    });
}

// Le verrou est tenu pendant tout le test : il change PATH.
#[allow(clippy::await_holding_lock)]
#[gpui_kit::test]
async fn cancelled_mission_is_never_approved(cx: &mut TestAppContext) {
    let _path = PATH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (window, view, log, path) = planned_mission("cancel-test", false, cx);
    let real_time = || std::thread::sleep(Duration::from_millis(5));
    cx.wait_for(window, Duration::from_secs(5), |_, cx| {
        real_time();
        view.read(cx).plan_pending()
    })
    .await;

    cx.update(|cx| view.update(cx, |view, cx| view.cancel_mission(cx)));
    let read_log = || std::fs::read_to_string(&log).unwrap_or_default();
    cx.wait_for(window, Duration::from_secs(5), |_, _| {
        real_time();
        read_log().contains("cancelled this mission")
    })
    .await;
    unsafe { std::env::set_var("PATH", path) };

    let sent = read_log();
    assert!(sent.contains("\"behavior\":\"deny\""), "{sent}");
    assert!(!sent.contains("\"behavior\":\"allow\""), "{sent}");
    cx.update(|cx| {
        let view = view.read(cx);
        assert_eq!(view.mode, PLAN_MODE, "la mission annulée n'a rien autorisé");
        assert_eq!(
            view.mission.as_ref().unwrap().status,
            MissionStatus::Cancelled
        );
    });
}

// Le verrou est tenu pendant tout le test : il change PATH.
#[allow(clippy::await_holding_lock)]
#[gpui_kit::test]
async fn a_relaunch_starts_a_fresh_attempt_with_the_plan_and_the_issues(cx: &mut TestAppContext) {
    let _path = PATH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (window, view, log, path) = planned_mission("relaunch-test", false, cx);
    let real_time = || std::thread::sleep(Duration::from_millis(5));
    cx.wait_for(window, Duration::from_secs(5), |_, cx| {
        real_time();
        view.read(cx).plan_pending()
    })
    .await;

    let relaunched = std::rc::Rc::new(std::cell::RefCell::new(None));
    cx.update(|cx| {
        let relaunched = relaunched.clone();
        cx.subscribe(&view, move |_, event: &Relaunch, _| {
            *relaunched.borrow_mut() = Some((
                event.project.clone(),
                event.mission.clone(),
                event.brief.clone(),
            ));
        })
        .detach();
        view.update(cx, |view, cx| {
            view.cancel_mission(cx);
            let mission = view.mission.as_mut().unwrap();
            mission.launched_mode = Some("acceptEdits".into());
            mission.reviews.push(solar_core::state::AgentReview {
                agent: "Claude".into(),
                tree: "t".into(),
                ok: Some(false),
                text: "VERDICT: ISSUES\n- test manquant".into(),
                at: String::new(),
            });
            view.relaunch(cx);
        });
    });
    let (project, mission, brief) = relaunched.borrow_mut().take().expect("relance émise");
    cx.update(|cx| {
        assert_eq!(
            view.read(cx).mission.as_ref().unwrap().next_attempt,
            Some(2)
        );
    });
    assert_eq!(mission.attempt, 2);
    assert!(mission.reviews.is_empty() && mission.checkpoint.is_none());
    assert!(brief.contains("Lire le code") && brief.contains("test manquant"));

    // La fenêtre ouvre la tentative dans une conversation neuve.
    std::fs::remove_file(&log).unwrap();
    let (_, next) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut next = SessionView::new(project, None, "T2".into(), window, cx);
                next.checkpoints = view.read(cx).checkpoints.clone();
                next.mission = Some(mission);
                next.brief = Some(brief);
                next
            })
        })
        .unwrap()
    });
    cx.update(|cx| next.update(cx, |next, cx| next.start_attempt(cx)));
    let read_log = || std::fs::read_to_string(&log).unwrap_or_default();
    cx.wait_for(window, Duration::from_secs(5), |_, _| {
        real_time();
        read_log().contains("test manquant")
    })
    .await;
    unsafe { std::env::set_var("PATH", path) };

    let sent = read_log();
    assert!(sent.contains("\"mode\":\"acceptEdits\""), "{sent}");
    cx.update(|cx| {
        let next = next.read(cx);
        let mission = next.mission.as_ref().unwrap();
        // Le faux CLI répond à tout par un plan : le statut qui suit ne dit rien ici.
        assert_eq!(mission.launched_mode.as_deref(), Some("acceptEdits"));
        assert!(mission.checkpoint.is_some(), "un instantané pour la revue");
        assert!(
            next.session_id.is_none(),
            "session neuve, lancée sans --resume"
        );
    });
}

// Le verrou est tenu pendant tout le test : il change PATH.
#[allow(clippy::await_holding_lock)]
#[gpui_kit::test]
async fn a_mission_works_in_its_copy_and_is_applied_on_acceptance(cx: &mut TestAppContext) {
    let _path = PATH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (window, view, log, path) = planned_mission("copy-test", true, cx);
    let real_time = || std::thread::sleep(Duration::from_millis(5));
    cx.wait_for(window, Duration::from_secs(10), |_, cx| {
        real_time();
        view.read(cx).plan_pending()
    })
    .await;
    let (project, copy) = cx.update(|cx| {
        let view = view.read(cx);
        (view.project.clone(), view.workdir())
    });
    assert_ne!(copy, project, "la mission a sa copie");
    let read_log = || std::fs::read_to_string(&log).unwrap_or_default();
    assert!(
        read_log().contains(&format!("cwd {}", copy.display())),
        "Claude planifie dans la copie : {}",
        read_log()
    );

    cx.update(|cx| view.update(cx, |view, cx| view.launch_mission("acceptEdits", cx)));
    cx.wait_for(window, Duration::from_secs(5), |_, _| {
        real_time();
        read_log().contains("\"behavior\":\"allow\"")
    })
    .await;
    // L'agent écrit dans sa copie ; l'utilisateur, dans son projet, ailleurs.
    std::fs::write(copy.join("a.txt"), "v2\n").unwrap();
    std::fs::write(project.join("mine.txt"), "à moi\n").unwrap();
    assert_eq!(
        std::fs::read_to_string(project.join("a.txt")).unwrap(),
        "v1\n"
    );

    cx.update(|cx| view.update(cx, |view, cx| view.refresh_review(cx)));
    cx.wait_for(window, Duration::from_secs(5), |_, cx| {
        real_time();
        view.read(cx)
            .review
            .as_ref()
            .is_some_and(|r| r.changes.len() == 1)
    })
    .await;
    cx.update(|cx| view.update(cx, |view, cx| view.accept_result(cx)));
    cx.wait_for(window, Duration::from_secs(5), |_, cx| {
        real_time();
        view.read(cx).mission.as_ref().unwrap().status == MissionStatus::Accepted
    })
    .await;
    unsafe { std::env::set_var("PATH", path) };

    assert_eq!(
        std::fs::read_to_string(project.join("a.txt")).unwrap(),
        "v2\n"
    );
    assert_eq!(
        std::fs::read_to_string(project.join("mine.txt")).unwrap(),
        "à moi\n"
    );
    assert!(!copy.exists(), "la copie est retirée");
    cx.update(|cx| assert!(view.read(cx).claude.is_none()));
}

/// Le parcours de publication, de bout en bout, sans rien envoyer sur
/// GitHub : un faux `gh` et un dépôt nu local qui reçoit le push.
// Le verrou est tenu pendant tout le test : il change PATH.
#[allow(clippy::await_holding_lock)]
#[gpui_kit::test]
async fn a_mission_is_published_then_its_failed_check_goes_to_the_agent(cx: &mut TestAppContext) {
    let _path = PATH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (window, view, _, path) = planned_mission("publish-test", true, cx);
    let dir = std::env::temp_dir().join(format!("solar-publish-test-{}", std::process::id()));
    let git = |cwd: &std::path::Path, args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    let repo = dir.join("repo");
    let origin = dir.join("origin.git");
    let _ = std::fs::remove_dir_all(&origin);
    git(&dir, &["init", "-q", "--bare", origin.to_str().unwrap()]);
    let user_branch = git(&repo, &["branch", "--show-current"]);
    git(&repo, &["config", "user.name", "Utilisateur"]);
    git(&repo, &["config", "user.email", "u@example.com"]);
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/BySolaire/essai.git",
        ],
    );
    git(
        &repo,
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            origin.to_str().unwrap(),
        ],
    );
    // Le faux `gh`, à côté du faux `claude`, sur le PATH du test.
    let gh_log = dir.join("gh.log");
    let _ = std::fs::remove_file(&gh_log);
    let checks = r#"[{"name":"tests","state":"FAILURE","bucket":"fail","link":"https://github.com/BySolaire/essai/actions/runs/7/job/42","workflow":"CI","startedAt":"2026-11-16T10:00:00Z","completedAt":"2026-11-16T10:01:42Z"}]"#;
    let script = format!(
        "#!/bin/sh\necho \"ARGS $*\" >> '{log}'\ncase \"$1 $2\" in\n  \
         'auth status') printf '%s' '{{\"hosts\":{{\"github.com\":[{{\"state\":\"success\",\"active\":true,\"login\":\"BySolaire\"}}]}}}}' ;;\n  \
         'repo view') printf '%s' '{{\"nameWithOwner\":\"BySolaire/essai\",\"defaultBranchRef\":{{\"name\":\"main\"}},\"viewerPermission\":\"WRITE\",\"isFork\":false,\"url\":\"https://github.com/BySolaire/essai\"}}' ;;\n  \
         'api '*) echo '{{}}' ;;\n  \
         'pr create') cat > /dev/null; echo 'https://github.com/BySolaire/essai/pull/12' ;;\n  \
         'pr checks') printf '%s' '{checks}' ;;\n  \
         'pr view') printf '%s' '{{\"state\":\"MERGED\"}}' ;;\n  \
         'run view') printf 'test auth::login_ok ... FAILED\\n' ;;\n  \
         *) exit 2 ;;\nesac\n",
        log = gh_log.display()
    );
    std::fs::write(dir.join("gh"), script).unwrap();
    std::fs::set_permissions(dir.join("gh"), PermissionsExt::from_mode(0o755)).unwrap();

    let real_time = || std::thread::sleep(Duration::from_millis(5));
    cx.wait_for(window, Duration::from_secs(10), |_, cx| {
        real_time();
        view.read(cx).plan_pending()
    })
    .await;
    cx.update(|cx| view.update(cx, |view, cx| view.launch_mission("acceptEdits", cx)));
    let copy = cx.update(|cx| view.read(cx).workdir());
    // Le travail de la mission, dans sa copie ; le tour est fini.
    std::fs::write(copy.join("bonjour.txt"), "Bonjour !\n").unwrap();
    cx.update(|cx| {
        view.update(cx, |view, cx| {
            view.status = Status::Idle;
            view.open_changes(cx);
        })
    });
    cx.wait_for(window, Duration::from_secs(10), |_, cx| {
        real_time();
        let view = view.read(cx);
        view.review.as_ref().is_some_and(|r| r.changes.len() == 1)
            && view.github.as_ref().is_some_and(|found| found.is_ok())
    })
    .await;

    // Étape 1 : commit et push.
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.open_publish(window, cx);
            view.commit_and_push(window, cx);
        })
    })
    .unwrap();
    cx.wait_for(window, Duration::from_secs(10), |_, cx| {
        real_time();
        view.read(cx)
            .publish
            .as_ref()
            .is_some_and(|form| form.has_pr_step())
    })
    .await;
    // La branche porte le titre de la conversation, tiré du premier message.
    let branch = "solar/ajoute-une-page-de-connexion";
    assert_eq!(git(&origin, &["branch", "--list"]), branch);
    assert_eq!(
        git(&origin, &["show", "--name-only", "--format=%an", branch]),
        "Utilisateur\n\napp/bonjour.txt",
        "seul le fichier de la mission est poussé, au nom de l'utilisateur"
    );
    assert_eq!(
        git(&repo, &["branch", "--show-current"]),
        user_branch,
        "la branche de l'utilisateur ne bouge pas"
    );

    // Étape 2 : la PR, puis ses checks et le journal de l'échec.
    cx.update(|cx| view.update(cx, |view, cx| view.open_pr(cx)));
    cx.wait_for(window, Duration::from_secs(10), |_, cx| {
        real_time();
        let view = view.read(cx);
        view.publish.is_none() && !view.check_logs.is_empty()
    })
    .await;
    cx.update(|cx| {
        let view = view.read(cx);
        let publication = view.mission.as_ref().unwrap().publication.clone().unwrap();
        assert_eq!(publication.pr.unwrap().number, 12);
        // La branche de lancement existe sur GitHub (selon le faux `gh`) : elle est visée.
        assert_eq!(publication.base.as_deref(), Some(user_branch.as_str()));
        assert_eq!(
            view.checks_summary(),
            Some(t!("publish.checks.fail", count = 1).to_string())
        );
        // Mergée sur GitHub (selon le faux `gh`) : la mission le sait.
        let pr = view
            .mission
            .as_ref()
            .unwrap()
            .publication
            .as_ref()
            .unwrap()
            .pr
            .clone();
        assert_eq!(pr.unwrap().state, solar_core::github::PrState::Merged);
    });

    // L'échec part chez l'agent, comme une correction.
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.send_check_failure(
                "https://github.com/BySolaire/essai/actions/runs/7/job/42".into(),
                window,
                cx,
            )
        })
    })
    .unwrap();
    unsafe { std::env::set_var("PATH", path) };
    cx.update(|cx| {
        let sent =
            view.read(cx).items.iter().any(
                |item| matches!(item, Item::User(text) if text.contains("login_ok ... FAILED")),
            );
        assert!(sent, "le journal de l'échec est envoyé à l'agent");
    });
    // La copie d'une PR mergée se retire sur demande ; le projet reste.
    cx.update(|cx| view.update(cx, |view, cx| view.remove_published_copy(cx)));
    cx.wait_for(window, Duration::from_secs(10), |_, _| {
        real_time();
        !copy.exists()
    })
    .await;
    assert!(repo.join("app/a.txt").exists());
    let calls = std::fs::read_to_string(&gh_log).unwrap();
    assert!(
        calls.contains(&format!(
            "pr create --repo github.com/BySolaire/essai --head {branch} --base {user_branch}"
        )),
        "{calls}"
    );
}

// Le verrou est tenu pendant tout le test : il change PATH.
#[allow(clippy::await_holding_lock)]
#[gpui_kit::test]
async fn a_codex_mission_plans_read_only_then_resumes_with_write_access(cx: &mut TestAppContext) {
    let _path = PATH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = std::env::temp_dir().join(format!("solar-codex-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("codex.log");
    // Un faux `codex exec --json` : il note ses arguments et le message, puis
    // répond par un plan en un tour.
    let reply = [
        r#"{"type":"thread.started","thread_id":"th-1"}"#,
        r#"{"type":"turn.started"}"#,
        r#"{"type":"item.completed","item":{"id":"i1","type":"agent_message","text":"1. Lire\n2. Écrire"}}"#,
        r#"{"type":"turn.completed","usage":{"input_tokens":10}}"#,
    ]
    .join("\n");
    let script = format!(
        "#!/bin/sh\n[ \"$1\" = --version ] && exit 0\nprintf 'ARGS %s\\n' \"$*\" >> '{log}'\ncat >> '{log}'\necho >> '{log}'\ncat <<'EOF'\n{reply}\nEOF\n",
        log = log.display()
    );
    std::fs::write(dir.join("codex"), script).unwrap();
    std::fs::set_permissions(dir.join("codex"), PermissionsExt::from_mode(0o755)).unwrap();
    let path = std::env::var("PATH").unwrap_or_default();
    unsafe { std::env::set_var("PATH", format!("{}:{path}", dir.display())) };

    cx.executor().allow_parking();
    cx.update(gpui_kit::init);
    let project = dir.join("app");
    std::fs::create_dir_all(&project).unwrap();
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SessionView::new(project, None, "Test".into(), window, cx))
        })
        .unwrap()
    });
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.checkpoints = dir.join("checkpoints");
            view.set_provider(Provider::Codex, window, cx);
            view.activate(window, cx);
            view.planning = true;
        });
        window.render_frame(cx);
        window.input("Ajoute une page", cx);
        window.press("enter", cx);
    })
    .unwrap();
    let real_time = || std::thread::sleep(Duration::from_millis(5));
    cx.wait_for(window, Duration::from_secs(5), |_, cx| {
        real_time();
        view.read(cx).plan_pending()
    })
    .await;
    let read_log = || std::fs::read_to_string(&log).unwrap_or_default();
    let first = read_log();
    assert!(first.contains("sandbox_mode=\"read-only\""), "{first}");
    assert!(!first.contains("resume"), "{first}");
    assert!(first.contains("Ajoute une page"), "{first}");
    cx.update(|cx| {
        let view = view.read(cx);
        assert_eq!(view.mission.as_ref().unwrap().plans, ["1. Lire\n2. Écrire"]);
        assert_eq!(view.session_id.as_deref(), Some("th-1"));
    });

    cx.update(|cx| view.update(cx, |view, cx| view.launch_mission("acceptEdits", cx)));
    cx.wait_for(window, Duration::from_secs(5), |_, _| {
        real_time();
        read_log().contains("resume th-1")
    })
    .await;
    unsafe { std::env::set_var("PATH", path) };
    let second = read_log()[first.len()..].to_string();
    assert!(
        second.contains("sandbox_mode=\"workspace-write\""),
        "{second}"
    );
    assert!(second.contains("The plan is approved"), "{second}");
    cx.update(|cx| {
        let mission = view.read(cx).mission.clone().unwrap();
        assert_eq!(mission.launched_mode.as_deref(), Some("acceptEdits"));
    });
}

/// La description d'une tentative dit quelle PR elle remplace, et la
/// branche d'une tentative porte son numéro.
#[gpui_kit::test]
fn an_attempt_names_the_pull_request_it_replaces(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SessionView::new("/nowhere".into(), None, "Accueil".into(), window, cx))
        })
        .unwrap()
    });
    cx.update(|cx| {
        view.update(cx, |view, _| {
            view.mission = Some(Mission {
                plans: vec!["## Plan\n1. Écrire".into()],
                attempt: 2,
                replaces: Some(12),
                ..Mission::default()
            });
            assert_eq!(view.mission_label(), "Accueil", "sans titre de plan");
            view.mission.as_mut().unwrap().plans =
                vec!["# Plan : ajouter bonjour.txt\n1. Écrire".into()];
            assert_eq!(view.mission_label(), "Ajouter bonjour.txt");
            view.mission.as_mut().unwrap().plans = vec!["## Plan\n1. Écrire".into()];
            let body = view.pr_body();
            assert!(body.starts_with("## Plan\n1. Écrire"), "{body}");
            assert!(
                body.contains(&t!("publish.body.replaces", n = 12).to_string()),
                "{body}"
            );
        })
    });
    assert_eq!(
        solar_core::github::branch_name("Accueil", 2),
        "solar/accueil-2"
    );
}

/// Un modèle par `/` : son formulaire, le texte inséré sans envoi,
/// Planifier et l'agent conseillé sur une conversation neuve.
// Le verrou est tenu pendant tout le test : il change PATH.
#[allow(clippy::await_holding_lock)]
#[gpui_kit::test]
async fn a_library_template_fills_the_composer_without_sending(cx: &mut TestAppContext) {
    let _path = PATH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // Un faux `codex`, installé le temps du test.
    let dir = std::env::temp_dir().join(format!("solar-library-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("codex"), "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(dir.join("codex"), PermissionsExt::from_mode(0o755)).unwrap();
    let path = std::env::var("PATH").unwrap_or_default();
    unsafe { std::env::set_var("PATH", format!("{}:{path}", dir.display())) };

    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SessionView::new("/nowhere".into(), None, "Test".into(), window, cx))
        })
        .unwrap()
    });
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.templates = vec![solar_core::library::parse(
                "bug",
                "---\ntitle: Corriger un bug\nagent: codex\nmission: true\n---\nCorrige {bug} dans {fichier}.",
                solar_core::library::Origin::Personal,
                None,
            )];
            view.input
                .update(cx, |input, cx| input.set_value("/bu", window, cx));
            assert!(view.accept(0, window, cx), "le modèle est proposé par `/`");
            let form = view.template_form.as_ref().expect("formulaire des trous");
            let fields: Vec<_> = form.fields().iter().map(|(hole, _)| hole.clone()).collect();
            assert_eq!(fields, ["bug", "fichier"]);
            for ((_, input), value) in form.fields().iter().zip(["le crash", "login.rs"]) {
                input.update(cx, |input, cx| input.set_value(value, window, cx));
            }
            view.insert_template(window, cx);
            assert!(view.template_form.is_none());
            assert_eq!(
                view.input.read(cx).value().as_ref(),
                "Corrige le crash dans login.rs."
            );
            assert!(view.items.is_empty(), "rien n'est envoyé");
            assert!(view.planning, "un modèle mission active Planifier");
            assert_eq!(view.provider, Provider::Codex, "l'agent conseillé");
        })
    })
    .unwrap();
    unsafe { std::env::set_var("PATH", path) };
}

/// Rouverte, une conversation reprend son modèle, son effort et son mode ;
/// un mode que son agent ne tient pas est ignoré.
#[gpui_kit::test]
fn a_reopened_conversation_keeps_its_model_and_mode(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SessionView::new("/nowhere".into(), None, "Test".into(), window, cx))
        })
        .unwrap()
    });
    cx.update(|cx| {
        view.update(cx, |view, _| {
            view.restore_choices(Some("haiku".into()), Some("low".into()), Some("plan"));
            assert_eq!(
                view.choices(),
                (
                    Some("haiku".into()),
                    Some("low".into()),
                    Some("plan".into())
                )
            );
        })
    });
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.set_provider(Provider::Codex, window, cx);
            view.restore_choices(None, None, Some("manual"));
            assert_ne!(view.mode, "manual", "Codex ne sait pas demander");
        })
    })
    .unwrap();
}

/// Rouverte, une mission en cours garde le mode de son lancement.
#[gpui_kit::test]
fn a_reopened_mission_keeps_its_launch_mode(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SessionView::new("/nowhere".into(), None, "Test".into(), window, cx))
        })
        .unwrap()
    });
    cx.update(|cx| {
        view.update(cx, |view, cx| {
            assert_eq!(view.mode, MODES[0], "accès complet par défaut");
            view.restore_mission(
                Some(Mission {
                    status: MissionStatus::Running,
                    launched_mode: Some("acceptEdits".into()),
                    ..Mission::default()
                }),
                cx,
            );
            assert_eq!(view.mode, "acceptEdits");
        })
    });
}

/// Codex et Grok en lecture seule : la fin d'un tour n'est un plan à
/// approuver que pendant la préparation d'une mission.
#[gpui_kit::test]
fn a_read_only_turn_is_a_plan_only_while_planning_a_mission(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SessionView::new("/nowhere".into(), None, "Test".into(), window, cx))
        })
        .unwrap()
    });
    let plan = || Event::PermissionRequest {
        request_id: solar_core::turns::PLAN_REQUEST.into(),
        tool: PLAN_TOOL.into(),
        input: serde_json::json!({"plan": "Rouge"}),
    };
    cx.update(|cx| {
        view.update(cx, |view, cx| {
            view.on_event(plan(), cx);
            assert!(view.mission.is_none(), "une réponse n'est pas une mission");
            assert!(view.permission.is_none());
            view.mission = Some(Mission {
                status: MissionStatus::Planning,
                ..Mission::default()
            });
            view.on_event(plan(), cx);
            assert!(view.plan_pending());
            assert_eq!(view.mission.as_ref().unwrap().status, MissionStatus::Ready);
        })
    });
}

#[gpui_kit::test]
fn an_unfinished_turn_reopens_as_interrupted_without_relaunch(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SessionView::new("/nowhere".into(), None, "Test".into(), window, cx))
        })
        .unwrap()
    });
    let mission = |status| {
        Some(Mission {
            status,
            ..Mission::default()
        })
    };
    cx.update(|cx| {
        view.update(cx, |view, cx| {
            view.items.push(Item::Tool {
                id: "t".into(),
                name: "Bash".into(),
                input: Value::Null,
                result: None,
                finished: None,
                open: false,
            });
            view.restore_mission(mission(MissionStatus::Running), cx);
            assert!(view.interrupted);
            assert!(view.claude.is_none(), "rien n'est relancé");
            // Un plan prêt attend une décision : il n'est pas interrompu.
            view.restore_mission(mission(MissionStatus::Ready), cx);
            assert!(!view.interrupted);
            // Un tour fini non plus.
            view.items.push(Item::Text("fait".into()));
            view.restore_mission(mission(MissionStatus::Running), cx);
            assert!(!view.interrupted);
        })
    });
}

#[gpui_kit::test]
fn a_message_queued_during_a_turn_is_never_lost(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_global(QueueWhileWorking(true)));
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SessionView::new("/nowhere".into(), None, "Test".into(), window, cx))
        })
        .unwrap()
    });
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.status = Status::Working;
            view.input.update(cx, |input, cx| {
                input.set_value("Ajoute un test", window, cx)
            });
            view.send(window, cx);
            assert_eq!(view.queued.len(), 1, "mis en file, pas envoyé");
            assert!(view.items.is_empty());
            assert!(view.input.read(cx).value().is_empty());
            // La fin du tour l'envoie ; sans agent pour le recevoir, il reste en file.
            view.on_event(
                Event::TurnDone {
                    error: None,
                    context_tokens: 0,
                    cost_usd: None,
                },
                cx,
            );
            assert_eq!(view.queued.len(), 1);
            // Steer depuis la file, sans agent : il y reste aussi.
            view.steer_queued(0, cx);
            assert_eq!(view.queued.len(), 1);
            // Arrêter un tour rend la file au champ.
            view.status = Status::Working;
            view.interrupt(window, cx);
            assert!(view.queued.is_empty());
            assert_eq!(view.input.read(cx).value().as_ref(), "Ajoute un test");
        })
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_turn_line_says_thinking_then_how_long_it_thought(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SessionView::new("/nowhere".into(), None, "Test".into(), window, cx))
        })
        .unwrap()
    });
    cx.update(|cx| {
        view.update(cx, |view, cx| {
            view.begin_turn(cx);
            assert!(!view.turn_progress(true).unwrap().contains("thinking"));
            view.on_event(Event::Thinking, cx);
            assert!(view.turn_progress(true).unwrap().ends_with(" · thinking"));
            view.thinking_since =
                Some(std::time::Instant::now() - std::time::Duration::from_secs(12));
            assert!(
                view.turn_progress(true)
                    .unwrap()
                    .ends_with("still thinking")
            );
            view.on_event(Event::Text("Voici".into()), cx);
            assert!(
                view.turn_progress(true)
                    .unwrap()
                    .ends_with(" · thought for 12s")
            );
            // Le fil garde la durée à la place de « Réflexion ».
            assert!(matches!(view.items[0], Item::Thinking(Some(12))));
        })
    });
}

// `git` doit rester sur le PATH : un autre test le vide un instant.
#[allow(clippy::await_holding_lock)]
#[gpui_kit::test]
async fn files_view_lists_the_project_and_shows_the_chosen_file(cx: &mut TestAppContext) {
    let _path = PATH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = std::env::temp_dir().join(format!("solar-files-view-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("README.md"), "# Bonjour").unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();

    cx.executor().allow_parking();
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SessionView::new(root.clone(), None, "Test".into(), window, cx))
        })
        .unwrap()
    });
    cx.update(|cx| {
        view.update(cx, |view, cx| {
            view.mission = Some(Mission::default());
            view.items.push(Item::User("x".into()));
            view.open_files(cx);
        })
    });
    let real_time = || std::thread::sleep(Duration::from_millis(5));
    cx.wait_for(window, Duration::from_secs(5), |_, cx| {
        real_time();
        view.read(cx).files_listed
    })
    .await;

    let tree = cx.update(|cx| view.read(cx).file_tree.clone().unwrap());
    cx.update(|cx| {
        tree.update(cx, |tree, cx| {
            let ix = tree.index_of(&"README.md".into()).expect("README.md listé");
            tree.set_selected_index(Some(ix), cx);
        })
    });
    cx.wait_for(window, Duration::from_secs(5), |_, cx| {
        real_time();
        matches!(
            &view.read(cx).opened,
            Some((_, files::Content::Text { text, .. })) if text == "# Bonjour"
        )
    })
    .await;
    // La vue se dessine avec le fichier ouvert.
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
}

// `git` doit rester sur le PATH : un autre test le vide un instant.
#[allow(clippy::await_holding_lock)]
#[gpui_kit::test]
async fn review_shows_exact_changes_refuses_stale_decisions_and_restores(cx: &mut TestAppContext) {
    let _path = PATH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = std::env::temp_dir().join(format!("solar-review-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("lib.rs"), "fn a() {}\n").unwrap();
    std::fs::write(root.join("mine.txt"), "à moi\n").unwrap();
    let store_dir = root
        .join("../solar-review-store")
        .join(std::process::id().to_string());
    let base = solar_core::checkpoint::Store::for_project(&store_dir, &root)
        .checkpoint()
        .unwrap();
    // La mission : une modification et un ajout.
    std::fs::write(root.join("lib.rs"), "fn a() {}\nfn b() {}\n").unwrap();
    std::fs::write(root.join("new.rs"), "// neuf\n").unwrap();

    cx.executor().allow_parking();
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SessionView::new(root.clone(), None, "Test".into(), window, cx))
        })
        .unwrap()
    });
    cx.update(|cx| {
        view.update(cx, |view, cx| {
            view.checkpoints = store_dir.clone();
            view.items.push(Item::User("x".into()));
            view.mission = Some(Mission {
                status: MissionStatus::Running,
                checkpoint: Some(base),
                ..Mission::default()
            });
            view.open_changes(cx);
        })
    });
    let real_time = || std::thread::sleep(Duration::from_millis(5));
    cx.wait_for(window, Duration::from_secs(5), |_, cx| {
        real_time();
        let view = view.read(cx);
        view.review.is_some() && !view.review_busy
    })
    .await;
    cx.update(|cx| {
        let paths: Vec<_> = view
            .read(cx)
            .review
            .as_ref()
            .unwrap()
            .changes
            .iter()
            .map(|c| c.path.clone())
            .collect();
        assert_eq!(paths, ["lib.rs", "new.rs"]);
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();

    // Un fichier change après l'affichage : l'acceptation est refusée.
    std::fs::write(root.join("lib.rs"), "fn a() {}\nfn b() {}\nfn c() {}\n").unwrap();
    cx.update(|cx| view.update(cx, |view, cx| view.accept_result(cx)));
    cx.wait_for(window, Duration::from_secs(5), |_, cx| {
        real_time();
        let view = view.read(cx);
        view.review_note.is_some() && !view.review_busy
    })
    .await;
    cx.update(|cx| {
        assert_eq!(
            view.read(cx).mission.as_ref().unwrap().status,
            MissionStatus::Running
        );
    });

    // Examiné de nouveau, puis accepté : une décision, rien d'autre.
    cx.update(|cx| view.update(cx, |view, cx| view.accept_result(cx)));
    cx.wait_for(window, Duration::from_secs(5), |_, cx| {
        real_time();
        let view = view.read(cx);
        view.mission.as_ref().unwrap().status == MissionStatus::Accepted
    })
    .await;
    cx.update(|cx| assert!(view.read(cx).mission.as_ref().unwrap().accepted.is_some()));

    // Restaurer remet les fichiers de la mission, et eux seuls.
    std::fs::write(root.join("mine.txt"), "toujours à moi\n").unwrap();
    cx.update(|cx| {
        view.update(cx, |view, cx| {
            view.mission.as_mut().unwrap().status = MissionStatus::Running;
            view.restore_result(cx);
        })
    });
    cx.wait_for(window, Duration::from_secs(5), |_, cx| {
        real_time();
        let view = view.read(cx);
        view.mission.as_ref().unwrap().status == MissionStatus::Restored
    })
    .await;
    assert_eq!(
        std::fs::read_to_string(root.join("lib.rs")).unwrap(),
        "fn a() {}\n"
    );
    assert!(!root.join("new.rs").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("mine.txt")).unwrap(),
        "toujours à moi\n"
    );
}

// Le verrou est tenu pendant tout le test : il change PATH.
#[allow(clippy::await_holding_lock)]
#[gpui_kit::test]
async fn a_reviewer_reads_the_exact_diff_read_only_and_its_verdict_is_kept(
    cx: &mut TestAppContext,
) {
    let _path = PATH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = std::env::temp_dir().join(format!("solar-reviewer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (project, bin) = (root.join("project"), root.join("bin"));
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(project.join("lib.rs"), "fn a() {}\n").unwrap();
    let store_dir = root.join("checkpoints");
    let base = solar_core::checkpoint::Store::for_project(&store_dir, &project)
        .checkpoint()
        .unwrap();
    std::fs::write(project.join("lib.rs"), "fn a() {}\nfn b() {}\n").unwrap();
    // Le faux relecteur note ses arguments et ce qu'il reçoit, puis rend un verdict.
    let log = root.join("reviewer.log");
    let script = format!(
        "#!/bin/sh\necho \"args $*\" >> '{log}'\nwhile read -r line; do\n  printf '%s\\n' \"$line\" >> '{log}'\n  case \"$line\" in\n    \
         *'\"type\":\"user\"'*)\n      \
         printf '%s\\n' '{{\"type\":\"stream_event\",\"event\":{{\"type\":\"content_block_delta\",\"delta\":{{\"type\":\"text_delta\",\"text\":\"VERDICT: PROBLÈMES\\\\n- lib.rs : b() sans test\"}}}}}}'\n      \
         printf '%s\\n' '{{\"type\":\"result\",\"is_error\":false,\"usage\":{{}}}}'\n      ;;\n  \
         esac\ndone\n",
        log = log.display()
    );
    std::fs::write(bin.join("claude"), script).unwrap();
    std::fs::set_permissions(bin.join("claude"), PermissionsExt::from_mode(0o755)).unwrap();
    let path = std::env::var("PATH").unwrap_or_default();
    unsafe { std::env::set_var("PATH", format!("{}:{path}", bin.display())) };

    cx.executor().allow_parking();
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SessionView::new(project.clone(), None, "Test".into(), window, cx))
        })
        .unwrap()
    });
    cx.update(|cx| {
        view.update(cx, |view, cx| {
            view.checkpoints = store_dir.clone();
            view.items.push(Item::User("x".into()));
            view.mission = Some(Mission {
                status: MissionStatus::Running,
                checkpoint: Some(base),
                plans: vec!["Ajouter b()".into()],
                ..Mission::default()
            });
            view.open_changes(cx);
        })
    });
    let real_time = || std::thread::sleep(Duration::from_millis(5));
    cx.wait_for(window, Duration::from_secs(5), |_, cx| {
        real_time();
        view.read(cx).review.is_some()
    })
    .await;
    cx.update(|cx| view.update(cx, |view, cx| view.start_review(Provider::Claude, cx)));
    cx.wait_for(window, Duration::from_secs(5), |_, cx| {
        real_time();
        view.read(cx)
            .mission
            .as_ref()
            .is_some_and(|m| !m.reviews.is_empty())
    })
    .await;
    unsafe { std::env::set_var("PATH", path) };

    cx.update(|cx| {
        let view = view.read(cx);
        let done = view.mission.as_ref().unwrap().reviews.last().unwrap();
        assert_eq!(done.ok, Some(false));
        assert_eq!(
            done.tree,
            view.review.as_ref().unwrap().tree,
            "lié à l'état relu"
        );
        assert!(done.text.contains("b() sans test"), "{}", done.text);
        assert!(!view.reviewing && view.reviewer.is_none());
    });
    let sent = std::fs::read_to_string(&log).unwrap();
    assert!(
        sent.contains("--permission-mode plan"),
        "lecture seule : {sent}"
    );
    assert!(
        sent.contains("Ajouter b()") && sent.contains("+fn b() {}"),
        "{sent}"
    );
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
}

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../solar-core/tests/fixtures/write_file.jsonl"
);

// Le verrou est tenu pendant tout le test, attentes comprises : c'est son rôle.
#[allow(clippy::await_holding_lock)]
#[gpui_kit::test]
async fn message_is_sent_streamed_and_permission_answered(cx: &mut TestAppContext) {
    let _path = PATH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = std::env::temp_dir().join(format!("solar-ui-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let answer = dir.join("answer.json");
    // Le faux CLI s'arrête sur la demande de permission (6e ligne) et note la réponse.
    let script = format!(
        "#!/bin/sh\nread -r init\nread -r settings\nread -r first\nhead -n 6 '{FIXTURE}'\nread -r answer\n\
         printf '%s' \"$answer\" > '{}'\ntail -n +7 '{FIXTURE}'\nread -r rest\n",
        answer.display()
    );
    std::fs::write(dir.join("claude"), script).unwrap();
    std::fs::set_permissions(dir.join("claude"), PermissionsExt::from_mode(0o755)).unwrap();
    let path = std::env::var("PATH").unwrap_or_default();
    // Seul test de ce binaire à toucher à PATH.
    unsafe { std::env::set_var("PATH", format!("{}:{path}", dir.display())) };

    // Le faux CLI réveille l'app depuis un vrai thread : le test n'est pas
    // déterministe, on l'autorise.
    cx.executor().allow_parking();
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SessionView::new(dir.clone(), None, "Test".into(), window, cx))
        })
        .unwrap()
    });

    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| view.activate(window, cx));
        window.render_frame(cx);
        window.input("Crée note.txt", cx);
        window.press("enter", cx);
    })
    .unwrap();

    // Le faux CLI tourne en temps réel alors que l'horloge du test est simulée.
    let real_time = || std::thread::sleep(Duration::from_millis(5));
    cx.wait_for(window, Duration::from_secs(5), |_, cx| {
        real_time();
        view.read(cx).permission.is_some()
    })
    .await;
    cx.update(|cx| {
        let view = view.read(cx);
        assert!(view.status == Status::NeedsYou);
        assert_eq!(view.title.as_ref(), "Crée note.txt");
        assert!(matches!(view.items[0], Item::User(_)));
        assert!(matches!(&view.items[1], Item::Tool { name, .. } if name == "Write"));
    });

    cx.update_window(window, |_, window, cx| window.click("allow", cx))
        .unwrap();
    cx.wait_for(window, Duration::from_secs(5), |_, cx| {
        real_time();
        matches!(
            view.read(cx).items.last(),
            Some(Item::Note { error: false, .. })
        )
    })
    .await;

    cx.update(|cx| {
        let view = view.read(cx);
        assert!(view.status == Status::Idle);
        assert_eq!(
            view.session_id.as_deref(),
            Some("a4483f72-ef29-4b31-97b5-f025efb8edcf")
        );
        assert!(matches!(
            &view.items[1],
            Item::Tool {
                result: Some((false, _)),
                ..
            }
        ));
        assert!(
            view.items
                .iter()
                .any(|item| matches!(item, Item::Text(text) if text == "fait."))
        );
    });
    let sent = std::fs::read_to_string(&answer).unwrap();
    assert!(
        sent.contains(r#""behavior":"allow""#),
        "réponse envoyée : {sent}"
    );
    assert!(sent.contains("316a49cc-17ff-4c37-bde7-0322d3b388fb"));
}

#[gpui_kit::test]
fn slash_and_at_menus_complete_what_is_typed(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut view =
                    SessionView::new("/nulle-part".into(), None, "Test".into(), window, cx);
                view.commands = ["simplify", "code-review"]
                    .map(|name| SlashCommand {
                        name: name.into(),
                        description: String::new(),
                    })
                    .to_vec();
                view.files = Some(vec!["src/main.rs".into(), "README.md".into()]);
                view
            })
        })
        .unwrap()
    });
    let labels = |view: &Entity<SessionView>, cx: &App| -> Vec<String> {
        let suggestions = view.read(cx).suggestions(cx);
        suggestions.iter().map(|s| s.label.to_string()).collect()
    };

    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.input.update(cx, |input, cx| input.focus(window, cx))
        });
        window.render_frame(cx);

        window.input("/rev", cx);
        assert_eq!(labels(&view, cx), ["/code-review"]);
        window.press("tab", cx);
        assert_eq!(
            view.read(cx).input.read(cx).value().as_ref(),
            "/code-review "
        );
        // Une commande suivie d'un espace n'est plus complétée.
        assert!(labels(&view, cx).is_empty());

        window.input("lis @main", cx);
        assert_eq!(labels(&view, cx), ["src/main.rs"]);
        window.press("tab", cx);
        assert_eq!(
            view.read(cx).input.read(cx).value().as_ref(),
            "/code-review lis @src/main.rs "
        );
    })
    .unwrap();
}
