use nsh_core::element::{px, Root};
use nsh_core::Size;
use nsh_engine::EngineConfig;
use nsh_testing::Headless;
use snout::config::Manager;
use snout::i18n::Lang;
use snout::types::{Config, Schema};
use snout_desktop::controller::Paths;
use snout_desktop::{App, AppProps};
use std::time::Duration;
use tempfile::TempDir;

fn workbench() -> (TempDir, Paths, Headless) {
    workbench_with(|_| {})
}

fn workbench_with(configure: impl FnOnce(&mut Config)) -> (TempDir, Paths, Headless) {
    let root = TempDir::new().expect("temporary root");
    let paths = Paths {
        config: root.path().join("config.json"),
        rime: root.path().join("rime"),
        cache: root.path().join("cache"),
    };
    let mut manager = Manager::with_paths(
        paths.config.clone(),
        paths.rime.clone(),
        paths.cache.clone(),
    )
    .expect("temporary manager");
    manager.config.language = "en".into();
    configure(&mut manager.config);
    manager.save().expect("initial configuration");
    let app_paths = paths.clone();
    let app = Root::new::<App>(move || AppProps {
        config_path: app_paths.config.to_string_lossy().to_string().into(),
        rime_path: app_paths.rime.to_string_lossy().to_string().into(),
        cache_path: app_paths.cache.to_string_lossy().to_string().into(),
    });
    let config = EngineConfig {
        viewport: Size::new(px(1040.0), px(760.0)),
        ui_sets: vec![nsh_base::registry()],
        ..EngineConfig::new(app)
    };
    let mut app = Headless::start(config).expect("real NSH Rust root");
    app.first_render().expect("first native render");
    app.wait_for("Ready")
        .expect("worker loaded temporary configuration");
    (root, paths, app)
}

fn persisted(paths: &Paths) -> Config {
    serde_json::from_slice(&std::fs::read(&paths.config).expect("read configuration"))
        .expect("configuration JSON")
}

fn wait_for_navigation(app: &mut Headless, navigation: &str, heading: &str) {
    app.wait_until(
        &format!("{navigation} selected and {heading} mounted"),
        Duration::from_secs(5),
        |screen| screen.selected(navigation) == Ok(true) && screen.shows(heading),
    )
    .expect("navigation state and page content committed");
}

fn navigate(app: &mut Headless, navigation: &str, heading: &str) {
    app.press(navigation).expect("sidebar navigation");
    wait_for_navigation(app, navigation, heading);
}

#[test]
fn actual_root_navigates_all_five_pages_and_lists_every_schema() {
    let (_root, paths, mut app) = workbench();
    assert!(app.shows("Selected scheme"));
    navigate(&mut app, "nav-schemes", "Choose a scheme");
    for schema in Schema::all() {
        assert!(
            app.shows(&schema.display_name_lang(Lang::En)),
            "missing schema {schema:?}"
        );
    }
    for (navigation, expected) in [
        ("nav-appearance", "Candidate window"),
        ("nav-settings", "General"),
        ("nav-activity", "Update activity"),
        ("nav-overview", "Selected scheme"),
    ] {
        navigate(&mut app, navigation, expected);
    }
    assert!(
        !paths.rime.exists(),
        "navigation must not install or patch anything"
    );
}

#[test]
fn draft_actions_follow_navigation_and_revert_without_writing() {
    let (_root, paths, mut app) = workbench();
    app.press("browse-schemes").expect("scheme shortcut");
    wait_for_navigation(&mut app, "nav-schemes", "Choose a scheme");
    app.press("schema-Ice").expect("select another scheme");
    app.wait_for("unsaved-changes").expect("draft visible");
    navigate(&mut app, "nav-overview", "Selected scheme");
    assert_eq!(persisted(&paths).schema, Schema::WanxiangBase);
    app.press("discard-draft").expect("revert draft");
    app.wait_for_gone("unsaved-changes", Duration::from_secs(2))
        .expect("draft cleared");
    assert!(app.shows(&Schema::WanxiangBase.display_name_lang(Lang::En)));
    assert_eq!(persisted(&paths).schema, Schema::WanxiangBase);
    assert!(!paths.rime.exists());
}

#[test]
fn keyboard_navigation_save_and_escape_keep_confirmation_safe() {
    let (_root, paths, mut app) = workbench();
    app.press_key("mod+2").expect("scheme shortcut");
    wait_for_navigation(&mut app, "nav-schemes", "Choose a scheme");
    app.press("schema-Ice").expect("change selection");
    app.press_key("mod+s").expect("save shortcut");
    app.wait_for("confirmation").expect("review draft");
    app.press_key("mod+4")
        .expect("navigation during confirmation");
    assert!(app.screen().selected("nav-schemes").expect("selection"));
    app.press_key("escape").expect("dismiss modal");
    app.wait_for_gone("confirmation", Duration::from_secs(2))
        .expect("modal dismissed");
    assert_eq!(persisted(&paths).schema, Schema::WanxiangBase);
    app.press("save-draft").expect("save draft strip");
    app.wait_for("confirmation").expect("review again");
    app.press("confirm-continue").expect("confirm save");
    app.wait_for("Completed").expect("saved");
    app.wait_for_gone("unsaved-changes", Duration::from_secs(2))
        .expect("clean state");
    assert_eq!(persisted(&paths).schema, Schema::Ice);
    assert!(!paths.rime.exists());
}

#[test]
fn diagnostics_disclosure_and_preview_work_in_dark_reduced_motion_at_small_size() {
    let (_root, paths, mut app) = workbench();
    app.set_viewport(Size::new(px(800.0), px(640.0)))
        .expect("narrow window");
    app.set_environment("dark with reduced motion and larger text", |environment| {
        environment.color_scheme = nsh_engine::ColorScheme::Dark;
        environment.reduce_motion = true;
        environment.font_scale = 1.2;
    })
    .expect("accessibility preferences");
    assert!(!app.shows("Locations"));
    app.press("toggle-diagnostics").expect("open details");
    app.wait_for("Locations").expect("paths disclosed");
    app.press("toggle-diagnostics").expect("close details");
    app.wait_for_gone("Locations", Duration::from_secs(2))
        .expect("details hidden");
    app.press("open-appearance").expect("appearance shortcut");
    wait_for_navigation(&mut app, "nav-appearance", "Candidate window");
    app.wait_for("candidate-preview").expect("layout preview");
    assert!(app.shows("你好"));
    assert!(!app.shows("呢好"));
    app.press("label:Candidates per page")
        .expect("open count picker");
    app.press("text:9").expect("nine candidates");
    app.wait_for("拟").expect("preview follows selection");
    assert!(!paths.rime.exists(), "preview must not apply settings");
}

#[test]
fn actual_root_confirms_updates_and_cancel_does_not_run_anything() {
    let (_root, paths, mut app) = workbench();
    app.press("update-all").expect("request update");
    app.wait_for("confirmation").expect("explicit confirmation");
    assert!(app.shows("Continue · preserve user data"));
    assert!(app.shows("Discard user data and continue"));
    app.press("confirm-cancel").expect("cancel confirmation");
    app.wait_for_gone("confirmation", Duration::from_secs(2))
        .expect("confirmation dismissed");
    assert!(!paths.rime.exists(), "cancel must not download or install");
}

#[test]
fn actual_root_edits_settings_then_persists_only_after_confirmation() {
    let (_root, paths, mut app) = workbench();
    navigate(&mut app, "nav-settings", "General");
    app.press("mirror").expect("change mirror preference");
    assert!(
        !persisted(&paths).use_mirror,
        "draft must not write immediately"
    );
    app.press("save-settings").expect("save request");
    app.wait_for("confirmation").expect("save confirmation");
    app.press("confirm-continue").expect("confirm persistence");
    app.wait_for("Completed")
        .expect("shared core save finished");
    assert!(persisted(&paths).use_mirror);
    assert!(
        !paths.rime.exists(),
        "saving a non-patch setting must not install anything"
    );
}

#[test]
fn actual_root_exclusion_crud_and_errors_use_temporary_configuration() {
    let (_root, paths, mut app) = workbench();
    navigate(&mut app, "nav-settings", "General");
    app.type_text("exclusion-input", "personal.txt")
        .expect("type exclusion");
    app.press("save-exclusion").expect("add rule");
    app.wait_until("exclusion saved", Duration::from_secs(5), |screen| {
        screen.shows("personal.txt")
    })
    .expect("saved rule shown");
    let config = persisted(&paths);
    let index = config
        .exclude_files
        .iter()
        .position(|pattern| pattern == "personal.txt")
        .expect("persisted rule");
    app.press(&format!("edit-exclusion-{index}"))
        .expect("edit rule");
    app.clear("exclusion-input").expect("clear rule");
    app.type_text("exclusion-input", "personal-*.txt")
        .expect("replacement rule");
    app.press("save-exclusion").expect("save edited rule");
    app.wait_until("edited rule saved", Duration::from_secs(5), |screen| {
        screen.shows("personal-*.txt")
    })
    .expect("edited rule");
    app.press(&format!("remove-exclusion-{index}"))
        .expect("remove rule");
    app.wait_for_gone("personal-*.txt", Duration::from_secs(5))
        .expect("removed rule");
    assert!(!persisted(&paths)
        .exclude_files
        .iter()
        .any(|pattern| pattern.starts_with("personal")));
}

#[test]
fn actual_root_exports_real_completion_through_native_save_dialog() {
    let (root, _paths, mut app) = workbench();
    let target = root.path().join("snout.fish");
    app.on_pick_save_path(Some(target.clone()))
        .expect("native save dialog response");
    navigate(&mut app, "nav-activity", "Update activity");
    app.press("export-fish").expect("export completion");
    app.wait_for("Completed").expect("export worker completed");
    assert_eq!(
        std::fs::read_to_string(target).expect("completion file"),
        snout::cli::completion_script(snout::cli::CompletionShell::Fish)
    );
}

#[test]
fn invalid_exclusion_remains_editable_after_worker_rejects_it() {
    let (_root, paths, mut app) = workbench();
    navigate(&mut app, "nav-settings", "General");
    let original = persisted(&paths).exclude_files;
    app.type_text("exclusion-input", "[")
        .expect("invalid regex");
    app.press("save-exclusion").expect("submit rule");
    app.wait_for("invalid exclude pattern: [")
        .expect("shared validation error");
    assert_eq!(
        app.screen().input_text("exclusion-input").expect("input"),
        Some("[")
    );
    assert_eq!(persisted(&paths).exclude_files, original);
    app.clear("exclusion-input").expect("correct rule");
    app.type_text("exclusion-input", "personal.txt")
        .expect("valid replacement");
    app.press("save-exclusion").expect("save correction");
    app.wait_for("Completed").expect("correction saved");
    assert!(persisted(&paths)
        .exclude_files
        .iter()
        .any(|pattern| pattern == "personal.txt"));
    assert_eq!(
        app.screen().input_text("exclusion-input").expect("input"),
        Some("")
    );
}

#[test]
fn invalid_settings_keep_draft_and_do_not_open_save_confirmation() {
    let (_root, paths, mut app) = workbench();
    navigate(&mut app, "nav-settings", "General");
    app.clear("countdown").expect("clear countdown");
    app.type_text("countdown", "99").expect("invalid countdown");
    app.press("save-settings").expect("request save");
    assert!(!app.shows("confirmation"));
    assert_eq!(
        app.screen().input_text("countdown").expect("input"),
        Some("99")
    );
    assert_ne!(persisted(&paths).auto_update_countdown, 99);
    app.press("revert-settings").expect("revert draft");
    assert_ne!(
        app.screen().input_text("countdown").expect("input"),
        Some("99")
    );
}

#[test]
fn candidate_override_and_confirmed_reset_use_shared_core() {
    let (_root, paths, mut app) = workbench();
    navigate(&mut app, "nav-appearance", "Candidate window");
    app.press("apply-candidate")
        .expect("apply default candidate size");
    app.wait_for("Completed")
        .expect("candidate override written");
    assert_eq!(
        snout::custom::candidate_page_size(&paths.rime, Schema::WanxiangBase)
            .expect("candidate override"),
        Some(5)
    );
    app.press("reset-candidate").expect("request reset");
    app.wait_for("confirmation").expect("reset confirmation");
    app.press("confirm-continue").expect("confirm reset");
    app.wait_for("Completed").expect("reset complete");
    assert_eq!(
        snout::custom::candidate_page_size(&paths.rime, Schema::WanxiangBase)
            .expect("scheme default"),
        None
    );
}

#[test]
fn json_export_matches_shared_report_for_real_failed_operation() {
    let (root, paths, mut app) = workbench_with(|config| {
        config.schema = Schema::Frost;
        config.github_token = "never-export-this-token".into();
    });
    app.press("update-dict").expect("dictionary update request");
    app.press("confirm-continue")
        .expect("confirm real core dispatch");
    app.wait_for("Needs attention")
        .expect("unsupported standalone dictionary fails visibly");
    let displayed = app
        .screen()
        .input_text("report-json")
        .expect("report field")
        .expect("JSON report")
        .to_string();
    let target = root.path().join("report.json");
    app.on_pick_save_path(Some(target.clone()))
        .expect("native save dialog response");
    app.press("export-json").expect("export shared JSON");
    app.wait_for("Completed").expect("export completed");
    let content = std::fs::read_to_string(target).expect("JSON export");
    assert_eq!(content, displayed);
    let report: serde_json::Value = serde_json::from_str(&content).expect("structured report");
    assert_eq!(report["command"], "dict");
    assert_eq!(report["ok"], false);
    assert_eq!(report["schema_version"], 1);
    assert!(!content.contains("never-export-this-token"));
    assert!(
        !paths.rime.exists(),
        "unsupported component must not install anything"
    );
}

#[test]
fn countdown_can_be_cancelled_and_hooks_are_explicit_before_any_update() {
    let (_root, paths, mut app) = workbench_with(|config| {
        config.auto_update = true;
        config.auto_update_countdown = 60;
        config.pre_update_hook = "printf hook-must-not-run".into();
    });
    app.wait_for("Not now").expect("visible startup countdown");
    app.press("cancel-countdown")
        .expect("cancel automatic update");
    app.wait_for_gone("Not now", Duration::from_secs(2))
        .expect("countdown stopped");
    app.press("update-all").expect("manual request");
    app.wait_for("confirmation").expect("hook confirmation");
    assert!(app.screen().texts().iter().any(|text| text
        .contains("Warning: configured hooks execute local commands.")
        && text.contains("printf hook-must-not-run")));
    assert!(app.shows("Discard user data and continue"));
    app.press("confirm-cancel")
        .expect("cancel instead of execute hooks");
    assert!(!paths.rime.exists());
}

#[test]
fn navigation_waits_for_a_queued_engine_event_before_querying_page_controls() {
    let (_root, paths, mut app) = workbench();
    let (entered, blocked) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    app.engine().control().flush(move |_| {
        let _ = entered.send(());
        let _ = released.recv_timeout(Duration::from_secs(5));
    });
    blocked
        .recv_timeout(Duration::from_secs(5))
        .expect("engine queue gated before navigation");
    app.press("nav-settings")
        .expect("press only submits the navigation event");
    assert!(
        app.shows("Selected scheme"),
        "the queued navigation has not run yet"
    );
    assert_eq!(
        app.screen().matches("countdown"),
        Ok(false),
        "press returning is not a render barrier"
    );
    release
        .send(())
        .expect("release the engine without a sleep or retry press");
    wait_for_navigation(&mut app, "nav-settings", "General");
    app.clear("countdown")
        .expect("settings input is now mounted");
    app.type_text("countdown", "99")
        .expect("real input interaction after navigation commit");
    app.wait_until("draft input rendered", Duration::from_secs(5), |screen| {
        screen.input_text("countdown") == Ok(Some("99"))
    })
    .expect("real component re-render after editing");
    assert_ne!(persisted(&paths).auto_update_countdown, 99);
    assert!(!paths.rime.exists());
}
