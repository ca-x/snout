use crate::config::{self, Manager};
use crate::i18n::{L10n, Lang};
use crate::types::Schema;
use crate::ui::config_logic::{
    build_config_status_snapshot, config_actions, effective_user_data_policy_label,
    next_language_value, next_proxy_type_value, next_tui_theme_mode, next_user_data_policy,
    update_detail_text, update_notice_text, ConfigAction, ConfigStatusSnapshot,
};
use crate::ui::style::{
    accent_text, color_accent, color_border, color_selection_bg, color_warning, contrast_color,
    panel_block, primary_text, secondary_text, section_title_text, selection_style,
    selector_highlight_symbol, tertiary_text,
};
use crate::ui::update_view::UpdateOutcome;
use crate::updater;
use crate::updater::{UpdateComponent, UpdateEvent, UpdatePhase};
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

// ── 应用状态 ──
pub enum AppScreen {
    Menu,
    Updating,
    Result,
    UpdateConfirm,
    UserDataPolicyConfirm,
    SchemeSelector,
    SkinSelector,
    ThemePatchPresetSelector,
    ThemePatchDefaultSelector,
    SkinRoundPrompt,
    Fcitx5LightThemeSelector,
    Fcitx5DarkThemeSelector,
    ConfigView,
    ConfigInput,
    ExcludeRules,
    WanxiangDiagnosis,
}

pub struct App {
    pub should_quit: bool,
    pub screen: AppScreen,
    pub menu_selected: usize,
    pub scheme_selected: usize,
    pub skin_selected: usize,
    pub skin_round_choice: bool,
    pub fcitx5_light_selected: Option<String>,
    pub fcitx5_dark_selected: Option<String>,
    pub theme_patch_selections: std::collections::HashSet<String>,
    pub theme_patch_default: Option<String>,
    pub config_selected: usize,
    pub schema: Schema,
    pub rime_dir: String,
    pub config_path: String,
    pub t: L10n,
    // 更新状态
    pub update_msg: String,
    pub update_pct: f64,
    pub update_done: bool,
    pub update_results: Vec<String>,
    pub update_stage_lines: Vec<String>,
    update_outcome: Option<UpdateOutcome>,
    update_in_progress: bool,
    progress_rx: Option<mpsc::Receiver<UpdateEvent>>,
    result_rx: Option<mpsc::Receiver<UpdateTaskResult>>,
    update_task: Option<JoinHandle<()>>,
    cancel_signal: Option<crate::types::CancelSignal>,
    config_status_rx: Option<mpsc::Receiver<ConfigStatusSnapshot>>,
    config_status_loading: bool,
    config_status: ConfigStatusSnapshot,
    config_input_field: Option<ConfigInputField>,
    config_input_value: String,
    // 通知
    notification: Option<Notification>,
    pending_skin_selection: Option<PendingSkinSelection>,
    pending_update_mode: Option<UpdateMode>,
    pending_user_data_policy: Option<String>,
    update_user_data_policy_summary: Option<String>,
    exclude_selected: usize,
    exclude_edit_index: Option<usize>,
}

#[derive(Debug)]
enum UpdateTaskError {
    Cancelled,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdateMode {
    All,
    Scheme,
    Dict,
    Model,
}

#[derive(Debug)]
struct UpdateTaskResult {
    results: Result<Vec<updater::UpdateResult>, UpdateTaskError>,
}

#[derive(Debug, Clone)]
struct Notification {
    message: String,
    shown_at: Instant,
    ttl: Option<Duration>,
}

#[derive(Debug, Clone)]
struct PendingSkinSelection {
    light_key: String,
    dark_key: String,
    target: SkinMenuTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfigInputField {
    ProxyAddress,
    CandidatePageSize,
    DownloadThreads,
    ExcludePattern,
}

#[derive(Clone)]
struct ResolvedUpdateContext {
    schema: Schema,
    config: crate::types::Config,
    cache_dir: std::path::PathBuf,
    rime_dir: std::path::PathBuf,
}

#[derive(Debug, Clone)]
enum SkinMenuTarget {
    ThemePatch(std::path::PathBuf),
    Fcitx5Theme,
}

impl App {
    pub fn new(manager: &Manager) -> Self {
        let lang = Lang::from_str(&manager.config.language);
        Self {
            should_quit: false,
            screen: AppScreen::Menu,
            menu_selected: 0,
            scheme_selected: 0,
            skin_selected: 0,
            skin_round_choice: true,
            fcitx5_light_selected: None,
            fcitx5_dark_selected: None,
            theme_patch_selections: std::collections::HashSet::new(),
            theme_patch_default: None,
            config_selected: 0,
            schema: manager.config.schema,
            rime_dir: manager.rime_dir.display().to_string(),
            config_path: manager.config_path.display().to_string(),
            t: L10n::new(lang),
            update_msg: String::new(),
            update_pct: 0.0,
            update_done: false,
            update_results: Vec::new(),
            update_stage_lines: Vec::new(),
            update_outcome: None,
            update_in_progress: false,
            progress_rx: None,
            result_rx: None,
            update_task: None,
            cancel_signal: None,
            config_status_rx: None,
            config_status_loading: false,
            config_status: ConfigStatusSnapshot::default(),
            config_input_field: None,
            config_input_value: String::new(),
            notification: None,
            pending_skin_selection: None,
            pending_update_mode: None,
            pending_user_data_policy: None,
            update_user_data_policy_summary: None,
            exclude_selected: 0,
            exclude_edit_index: None,
        }
    }

    /// 动态菜单项 (i18n)
    pub fn menu_items(&self) -> Vec<(&str, &str)> {
        let skin_label = match skin_menu_target(self) {
            Some(SkinMenuTarget::Fcitx5Theme) => self.t.t("menu.fcitx5_theme"),
            _ => self.t.t("menu.skin_patch"),
        };
        vec![
            ("1", self.t.t("menu.update_all")),
            ("2", self.t.t("menu.update_scheme")),
            ("3", self.t.t("menu.update_dict")),
            ("4", self.t.t("menu.update_model")),
            ("5", self.t.t("menu.model_patch")),
            ("6", skin_label),
            ("7", self.t.t("menu.switch_scheme")),
            ("8", self.t.t("menu.config")),
            ("Q", self.t.t("menu.quit")),
        ]
    }

    pub fn notify(&mut self, msg: impl Into<String>) {
        self.notify_for(msg, Some(Duration::from_secs(3)));
    }

    fn notify_for(&mut self, msg: impl Into<String>, ttl: Option<Duration>) {
        self.notification = Some(Notification {
            message: msg.into(),
            shown_at: Instant::now(),
            ttl,
        });
    }

    fn current_hint(&self) -> String {
        match self.screen {
            AppScreen::Updating => format!(
                "{}  q/Esc {}",
                self.t.t("hint.wait"),
                self.t.t("hint.cancel")
            ),
            AppScreen::UpdateConfirm => format!(
                "Enter {}  Esc {}",
                self.t.t("hint.confirm"),
                self.t.t("hint.back")
            ),
            AppScreen::UserDataPolicyConfirm => format!(
                "Enter {}  Esc {}",
                self.t.t("hint.confirm"),
                self.t.t("hint.back")
            ),
            AppScreen::Result => format!("Enter/Esc {}", self.t.t("hint.back")),
            AppScreen::SchemeSelector
            | AppScreen::SkinSelector
            | AppScreen::ThemePatchPresetSelector
            | AppScreen::ThemePatchDefaultSelector
            | AppScreen::Fcitx5LightThemeSelector
            | AppScreen::Fcitx5DarkThemeSelector => {
                format!(
                    "↑↓/jk {}  Enter {}  Esc {}",
                    self.t.t("hint.navigate"),
                    self.t.t("hint.confirm"),
                    self.t.t("hint.back")
                )
            }
            AppScreen::SkinRoundPrompt => format!(
                "←→/hj {}  Enter {}  Esc {}",
                self.t.t("hint.toggle"),
                self.t.t("hint.confirm"),
                self.t.t("hint.back")
            ),
            AppScreen::ConfigInput => format!(
                "{}  Enter {}  Esc {}",
                self.t.t("hint.input"),
                self.t.t("hint.confirm"),
                self.t.t("hint.back")
            ),
            AppScreen::ExcludeRules => format!(
                "↑↓/jk {}  Enter {}  a {}  d {}  r {}  Esc {}",
                self.t.t("hint.navigate"),
                self.t.t("hint.edit"),
                self.t.t("hint.add"),
                self.t.t("hint.delete"),
                self.t.t("hint.reset"),
                self.t.t("hint.back")
            ),
            AppScreen::WanxiangDiagnosis => format!("Esc {}", self.t.t("hint.back")),
            AppScreen::ConfigView => format!("Enter/Esc {}", self.t.t("hint.back")),
            AppScreen::Menu => format!(
                "↑↓/jk {}  Enter {}  q/Esc {}",
                self.t.t("hint.navigate"),
                self.t.t("hint.confirm"),
                self.t.t("hint.back")
            ),
        }
    }
}

fn model_update_supported(schema: Schema) -> bool {
    schema.supports_model_patch()
}

fn theme_patch_target_for_platform(
    rime_dir: &std::path::Path,
    installed_engines: &[String],
) -> Option<std::path::PathBuf> {
    if cfg!(target_os = "windows") && installed_engines.iter().any(|engine| engine == "weasel") {
        Some(rime_dir.join("weasel.custom.yaml"))
    } else if cfg!(target_os = "macos")
        && installed_engines.iter().any(|engine| engine == "squirrel")
    {
        Some(rime_dir.join("squirrel.custom.yaml"))
    } else {
        None
    }
}

fn skin_menu_target(app: &App) -> Option<SkinMenuTarget> {
    let installed_engines = config::detect_installed_engines();

    if cfg!(target_os = "linux")
        && crate::skin::fcitx5::builtin_themes_available()
        && crate::skin::fcitx5::theme_supported(&installed_engines)
    {
        return Some(SkinMenuTarget::Fcitx5Theme);
    }

    theme_patch_target_for_platform(std::path::Path::new(&app.rime_dir), &installed_engines)
        .map(SkinMenuTarget::ThemePatch)
}

fn resolve_update_context(app: &App, mode: &UpdateMode) -> anyhow::Result<ResolvedUpdateContext> {
    let manager = Manager::new()?;
    let schema = app.schema;
    if matches!(mode, UpdateMode::Model) && !model_update_supported(schema) {
        anyhow::bail!("{}", app.t.t("update.model_not_supported"));
    }

    Ok(ResolvedUpdateContext {
        schema,
        config: manager.config.clone(),
        cache_dir: manager.cache_dir.clone(),
        rime_dir: manager.rime_dir.clone(),
    })
}

// ── 主入口 ──
pub async fn run_tui() -> Result<()> {
    let manager = Manager::new()?;
    let mut app = App::new(&manager);
    crate::feedback::set_tui_active(true);

    // 终端设置
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, &mut app, &manager).await;

    // 恢复终端
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    crate::feedback::set_tui_active(false);

    result
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    manager: &Manager,
) -> Result<()> {
    refresh_config_status(app);
    loop {
        let mut progress_events = Vec::new();
        if let Some(rx) = &app.progress_rx {
            while let Ok(event) = rx.try_recv() {
                progress_events.push(event);
            }
        }
        for event in progress_events {
            app.update_msg = event.detail.clone();
            app.update_pct = event.progress.clamp(0.0, 1.0);
            upsert_stage_line(app, &event);
        }
        let mut finished_results = Vec::new();
        if let Some(rx) = &app.result_rx {
            while let Ok(event) = rx.try_recv() {
                finished_results.push(event.results);
            }
        }
        for results in finished_results {
            finish_update(app, results);
        }
        let mut config_snapshots = Vec::new();
        if let Some(rx) = &app.config_status_rx {
            while let Ok(snapshot) = rx.try_recv() {
                config_snapshots.push(snapshot);
            }
        }
        for snapshot in config_snapshots {
            app.config_status = snapshot;
            app.config_status_loading = false;
            app.config_status_rx = None;
        }

        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match app.screen {
                    AppScreen::Menu => handle_menu_key(app, key.code).await?,
                    AppScreen::Updating => handle_updating_key(app, key.code),
                    AppScreen::Result => handle_result_key(app, key.code),
                    AppScreen::UpdateConfirm => handle_update_confirm_key(app, key.code).await?,
                    AppScreen::UserDataPolicyConfirm => {
                        handle_user_data_policy_confirm_key(app, key.code)?
                    }
                    AppScreen::SchemeSelector => handle_scheme_key(app, key.code, manager)?,
                    AppScreen::SkinSelector => handle_skin_key(app, key.code, manager).await?,
                    AppScreen::ThemePatchPresetSelector => {
                        handle_theme_patch_preset_key(app, key.code)?
                    }
                    AppScreen::ThemePatchDefaultSelector => {
                        handle_theme_patch_default_key(app, key.code)?
                    }
                    AppScreen::Fcitx5LightThemeSelector => {
                        handle_fcitx5_theme_key(app, key.code, Fcitx5ThemePhase::Light).await?
                    }
                    AppScreen::Fcitx5DarkThemeSelector => {
                        handle_fcitx5_theme_key(app, key.code, Fcitx5ThemePhase::Dark).await?
                    }
                    AppScreen::SkinRoundPrompt => {
                        handle_skin_round_prompt_key(app, key.code).await?
                    }
                    AppScreen::ConfigView => handle_config_key(app, key.code),
                    AppScreen::ConfigInput => handle_config_input_key(app, key.code),
                    AppScreen::ExcludeRules => handle_exclude_rules_key(app, key.code),
                    AppScreen::WanxiangDiagnosis => handle_wanxiang_diagnosis_key(app, key.code),
                }
            }
        }

        // 清除过期通知
        if let Some(notification) = &app.notification {
            if notification
                .ttl
                .is_some_and(|ttl| notification.shown_at.elapsed() > ttl)
            {
                app.notification = None;
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

// ── 按键处理 ──

async fn handle_menu_key(app: &mut App, key: KeyCode) -> Result<()> {
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            app.menu_selected = app.menu_selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') if app.menu_selected < app.menu_items().len() - 1 => {
            app.menu_selected += 1;
        }
        KeyCode::Enter | KeyCode::Char('1'..='8') => {
            let idx = match key {
                KeyCode::Char(c) => c.to_digit(10).unwrap_or(0) as usize,
                _ => app.menu_selected + 1,
            };
            if let Some(reason) = menu_unavailable_reason(app, idx) {
                app.notify(reason);
                return Ok(());
            }
            match idx {
                1 => begin_update_flow(app, UpdateMode::All).await?,
                2 => begin_update_flow(app, UpdateMode::Scheme).await?,
                3 => start_update(app, UpdateMode::Dict).await?,
                4 => start_update(app, UpdateMode::Model).await?,
                5 => {
                    // Model patch toggle
                    app.screen = AppScreen::Result;
                    app.update_results.clear();
                    app.update_stage_lines.clear();
                    if app.schema.supports_model_patch() {
                        let patched = updater::model_patch::is_model_patched(
                            std::path::Path::new(&app.rime_dir),
                            &app.schema,
                            app.t.lang(),
                        );
                        if patched {
                            if let Err(e) = updater::model_patch::unpatch_model(
                                std::path::Path::new(&app.rime_dir),
                                &app.schema,
                                app.t.lang(),
                            ) {
                                app.update_results.push(format!("❌ {e}"));
                            } else {
                                app.update_results
                                    .push(format!("✅ {}", app.t.t("patch.model.disabled")));
                            }
                        } else {
                            if let Err(e) = updater::model_patch::patch_model(
                                std::path::Path::new(&app.rime_dir),
                                &app.schema,
                                app.t.lang(),
                            ) {
                                app.update_results.push(format!("❌ {e}"));
                            } else {
                                app.update_results
                                    .push(format!("✅ {}", app.t.t("patch.model.enabled")));
                            }
                        }
                    } else {
                        app.update_results
                            .push(app.t.t("patch.model.not_supported").into());
                    }
                    app.update_msg = app.t.t("menu.model_patch").into();
                    app.update_done = true;
                    app.update_outcome = Some(UpdateOutcome::Success);
                }
                6 => {
                    app.skin_selected = 0;
                    if matches!(skin_menu_target(app), Some(SkinMenuTarget::Fcitx5Theme)) {
                        let selection =
                            crate::skin::fcitx5::current_theme_selection().unwrap_or_default();
                        app.fcitx5_light_selected = selection.light;
                        app.fcitx5_dark_selected = selection.dark;
                        app.screen = AppScreen::Fcitx5DarkThemeSelector;
                    } else if let Some(SkinMenuTarget::ThemePatch(path)) = skin_menu_target(app) {
                        app.theme_patch_selections =
                            crate::skin::patch::read_skin_preset_selections(&path)
                                .unwrap_or_default();
                        app.theme_patch_default =
                            crate::skin::patch::read_default_skin(&path).unwrap_or(None);
                        app.screen = AppScreen::ThemePatchPresetSelector;
                    } else {
                        app.screen = AppScreen::SkinSelector;
                    }
                }
                7 => {
                    app.scheme_selected = current_schema_index(app.schema);
                    app.screen = AppScreen::SchemeSelector;
                }
                8 => enter_config_view(app),
                9 => app.should_quit = true,
                _ => {}
            }
        }
        KeyCode::Char('q') | KeyCode::Esc => {
            app.should_quit = true;
        }
        _ => {}
    }
    Ok(())
}

async fn handle_update_confirm_key(app: &mut App, key: KeyCode) -> Result<()> {
    match key {
        KeyCode::Enter => {
            if let Some(mode) = app.pending_update_mode.take() {
                start_update(app, mode).await?;
            } else {
                app.screen = AppScreen::Menu;
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.pending_update_mode = None;
            app.screen = AppScreen::Menu;
        }
        _ => {}
    }
    Ok(())
}

fn handle_user_data_policy_confirm_key(app: &mut App, key: KeyCode) -> Result<()> {
    match key {
        KeyCode::Enter => {
            if let Some(policy) = app.pending_user_data_policy.take() {
                if let Ok(mut manager) = Manager::new() {
                    manager.config.user_data_policy = policy;
                    let _ = manager.save();
                    app.notify(app.t.t("config.saved").to_string());
                }
            }
            app.screen = AppScreen::ConfigView;
            refresh_config_status(app);
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.pending_user_data_policy = None;
            app.screen = AppScreen::ConfigView;
        }
        _ => {}
    }
    Ok(())
}

async fn begin_update_flow(app: &mut App, mode: UpdateMode) -> Result<()> {
    let config = Manager::new()
        .map(|manager| manager.config)
        .unwrap_or_default();
    match config.user_data_policy.trim().to_ascii_lowercase().as_str() {
        "preserve" | "discard" => start_update(app, mode).await,
        _ => {
            app.pending_update_mode = Some(mode);
            app.screen = AppScreen::UpdateConfirm;
            Ok(())
        }
    }
}

fn handle_result_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
            app.screen = AppScreen::Menu;
            app.update_done = false;
            app.update_in_progress = false;
            app.update_pct = 0.0;
            app.update_stage_lines.clear();
            app.progress_rx = None;
            app.result_rx = None;
            app.update_task = None;
            app.cancel_signal = None;
            app.update_user_data_policy_summary = None;
        }
        _ => {}
    }
}

fn handle_updating_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Esc | KeyCode::Char('q') if app.update_in_progress => {
            if let Some(cancel) = &app.cancel_signal {
                cancel.cancel();
            }
            app.update_msg = app.t.t("update.cancelling").into();
            upsert_stage_line(
                app,
                &UpdateEvent {
                    component: UpdateComponent::Hook,
                    phase: UpdatePhase::Cancelling,
                    progress: app.update_pct,
                    detail: app.t.t("update.cancelling").into(),
                },
            );
        }
        _ => {}
    }
}

fn handle_scheme_key(app: &mut App, key: KeyCode, _manager: &Manager) -> Result<()> {
    let schemas = Schema::all();
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            app.scheme_selected = app.scheme_selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') if app.scheme_selected < schemas.len() - 1 => {
            app.scheme_selected += 1;
        }
        KeyCode::Enter => {
            if let Some(s) = schemas.get(app.scheme_selected) {
                app.schema = *s;
                if let Ok(mut manager) = Manager::new() {
                    manager.config.schema = *s;
                    let _ = manager.save();
                }
                refresh_config_status(app);
                app.notify(format!(
                    "{}: {}",
                    app.t.t("scheme.switched"),
                    s.display_name_lang(app.t.lang())
                ));
            }
            app.screen = AppScreen::Menu;
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.screen = AppScreen::Menu;
        }
        _ => {}
    }
    Ok(())
}

async fn handle_skin_key(app: &mut App, key: KeyCode, _manager: &Manager) -> Result<()> {
    let skins = available_skin_choices(app);
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            app.skin_selected = app.skin_selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') if app.skin_selected < skins.len() - 1 => {
            app.skin_selected += 1;
        }
        KeyCode::Enter => {
            if let Some((key, name)) = skins.get(app.skin_selected) {
                match skin_menu_target(app) {
                    Some(SkinMenuTarget::Fcitx5Theme)
                        if crate::skin::fcitx5::theme_supports_optional_rounding(key) =>
                    {
                        app.skin_round_choice =
                            crate::skin::fcitx5::installed_theme_rounding(key)?.unwrap_or(true);
                        app.pending_skin_selection = Some(PendingSkinSelection {
                            light_key: key.clone(),
                            dark_key: key.clone(),
                            target: SkinMenuTarget::Fcitx5Theme,
                        });
                        app.screen = AppScreen::SkinRoundPrompt;
                        return Ok(());
                    }
                    Some(target) => {
                        apply_skin_selection(app, target, key, name, None).await;
                    }
                    None => {
                        app.notify(app.t.t("skin.not_supported").to_string());
                    }
                }
            }
            app.screen = AppScreen::Menu;
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.screen = AppScreen::Menu;
        }
        _ => {}
    }
    Ok(())
}

fn handle_theme_patch_preset_key(app: &mut App, key: KeyCode) -> Result<()> {
    let skins = available_skin_choices(app);
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            app.skin_selected = app.skin_selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') if app.skin_selected < skins.len().saturating_sub(1) => {
            app.skin_selected += 1;
        }
        KeyCode::Char(' ') => {
            if let Some((key, _)) = skins.get(app.skin_selected) {
                if !app.theme_patch_selections.remove(key) {
                    app.theme_patch_selections.insert(key.clone());
                }
            }
        }
        KeyCode::Enter => {
            if app.theme_patch_selections.is_empty() {
                app.notify(app.t.t("skin.theme_patch_empty").to_string());
                return Ok(());
            }
            let selected = selected_theme_patch_choices(app);
            app.skin_selected = selected
                .iter()
                .position(|(key, _)| app.theme_patch_default.as_deref() == Some(key.as_str()))
                .unwrap_or(0);
            app.screen = AppScreen::ThemePatchDefaultSelector;
        }
        KeyCode::Esc | KeyCode::Char('q') => app.screen = AppScreen::Menu,
        _ => {}
    }
    Ok(())
}

fn handle_theme_patch_default_key(app: &mut App, key: KeyCode) -> Result<()> {
    let selected = selected_theme_patch_choices(app);
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            app.skin_selected = app.skin_selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j')
            if app.skin_selected < selected.len().saturating_sub(1) =>
        {
            app.skin_selected += 1;
        }
        KeyCode::Enter => {
            if let Some((key, name)) = selected.get(app.skin_selected) {
                if let Some(SkinMenuTarget::ThemePatch(path)) = skin_menu_target(app) {
                    let refs: Vec<&str> = app
                        .theme_patch_selections
                        .iter()
                        .map(String::as_str)
                        .collect();
                    if let Err(e) = crate::skin::patch::sync_skin_presets(&path, &refs) {
                        app.notify(format!("❌ {e}"));
                    } else if let Err(e) = crate::skin::patch::set_default_skin(&path, key) {
                        app.notify(format!("❌ {e}"));
                    } else if cfg!(target_os = "windows") {
                        if let Err(e) = crate::deployer::deploy_to("weasel", &app.t) {
                            app.notify(format!("❌ {e}"));
                        } else {
                            app.theme_patch_default = Some(key.clone());
                            app.notify(format!("✅ {}: {name}", app.t.t("skin.applied")));
                            app.screen = AppScreen::Menu;
                        }
                    } else if cfg!(target_os = "macos") {
                        if let Err(e) = crate::deployer::deploy_to("squirrel", &app.t) {
                            app.notify(format!("❌ {e}"));
                        } else {
                            app.theme_patch_default = Some(key.clone());
                            app.notify(format!("✅ {}: {name}", app.t.t("skin.applied")));
                            app.screen = AppScreen::Menu;
                        }
                    } else {
                        app.theme_patch_default = Some(key.clone());
                        app.notify(format!("✅ {}: {name}", app.t.t("skin.applied")));
                        app.screen = AppScreen::Menu;
                    }
                }
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => app.screen = AppScreen::ThemePatchPresetSelector,
        _ => {}
    }
    Ok(())
}

fn selected_theme_patch_choices(app: &App) -> Vec<(String, String)> {
    available_skin_choices(app)
        .into_iter()
        .filter(|(key, _)| app.theme_patch_selections.contains(key))
        .collect()
}

#[derive(Clone, Copy)]
enum Fcitx5ThemePhase {
    Dark,
    Light,
}

async fn handle_fcitx5_theme_key(
    app: &mut App,
    key: KeyCode,
    phase: Fcitx5ThemePhase,
) -> Result<()> {
    let skins = available_skin_choices(app);
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            app.skin_selected = app.skin_selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') if app.skin_selected < skins.len().saturating_sub(1) => {
            app.skin_selected += 1;
        }
        KeyCode::Enter => {
            if let Some((key, _name)) = skins.get(app.skin_selected) {
                match phase {
                    Fcitx5ThemePhase::Dark => {
                        app.fcitx5_dark_selected = Some(key.clone());
                        app.skin_selected =
                            current_skin_index(app, app.fcitx5_light_selected.as_deref());
                        app.screen = AppScreen::Fcitx5LightThemeSelector;
                    }
                    Fcitx5ThemePhase::Light => {
                        app.fcitx5_light_selected = Some(key.clone());
                        let light = app.fcitx5_light_selected.clone().unwrap_or_default();
                        let dark = app
                            .fcitx5_dark_selected
                            .clone()
                            .unwrap_or_else(|| light.clone());
                        let requires_round_prompt =
                            crate::skin::fcitx5::theme_supports_optional_rounding(&light)
                                || crate::skin::fcitx5::theme_supports_optional_rounding(&dark);
                        if requires_round_prompt {
                            let rounded = crate::skin::fcitx5::installed_theme_rounding(&light)
                                .or_else(|_| crate::skin::fcitx5::installed_theme_rounding(&dark))?
                                .unwrap_or(true);
                            app.skin_round_choice = rounded;
                            app.pending_skin_selection = Some(PendingSkinSelection {
                                light_key: light,
                                dark_key: dark,
                                target: SkinMenuTarget::Fcitx5Theme,
                            });
                            app.screen = AppScreen::SkinRoundPrompt;
                        } else {
                            apply_fcitx5_theme_pair(app, None).await;
                            app.screen = AppScreen::Menu;
                        }
                    }
                }
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => match phase {
            Fcitx5ThemePhase::Dark => {
                app.screen = AppScreen::Menu;
            }
            Fcitx5ThemePhase::Light => {
                app.screen = AppScreen::Fcitx5DarkThemeSelector;
            }
        },
        _ => {}
    }
    Ok(())
}

async fn apply_pending_skin_selection(app: &mut App) -> Result<()> {
    let Some(selection) = app.pending_skin_selection.clone() else {
        return Ok(());
    };

    match selection.target {
        SkinMenuTarget::Fcitx5Theme => {
            apply_fcitx5_theme_pair(app, Some(app.skin_round_choice)).await
        }
        _ => {
            apply_skin_selection(
                app,
                selection.target,
                &selection.light_key,
                &selection.dark_key,
                Some(app.skin_round_choice),
            )
            .await
        }
    }
    Ok(())
}

async fn apply_fcitx5_theme_pair(app: &mut App, rounded: Option<bool>) {
    let light = app.fcitx5_light_selected.clone().unwrap_or_default();
    let dark = app
        .fcitx5_dark_selected
        .clone()
        .unwrap_or_else(|| light.clone());
    if light.is_empty() || dark.is_empty() {
        app.notify(app.t.t("skin.not_supported").to_string());
        return;
    }

    if let Err(e) =
        crate::skin::fcitx5::apply_theme_pair(&light, &dark, rounded, rounded, app.t.lang()).await
    {
        app.notify(format!("❌ {e}"));
        return;
    }

    let suffix = if rounded.is_some() {
        let round_state = if rounded.unwrap_or(false) {
            app.t.t("skin.round_on")
        } else {
            app.t.t("skin.round_off")
        };
        format!(" ({round_state})")
    } else {
        String::new()
    };

    app.notify(format!(
        "✅ {}: {} / {}{}",
        app.t.t("skin.applied"),
        light,
        dark,
        suffix
    ));
}

fn current_skin_index(app: &App, key: Option<&str>) -> usize {
    let skins = available_skin_choices(app);
    key.and_then(|key| skins.iter().position(|(candidate, _)| candidate == key))
        .unwrap_or(0)
}

async fn apply_skin_selection(
    app: &mut App,
    target: SkinMenuTarget,
    key: &str,
    name: &str,
    rounded: Option<bool>,
) {
    match target {
        SkinMenuTarget::Fcitx5Theme => {
            if let Err(e) = crate::skin::fcitx5::apply_theme(key, rounded, app.t.lang()).await {
                app.notify(format!("❌ {e}"));
            } else if crate::skin::fcitx5::theme_supports_optional_rounding(key) {
                let round_state = if rounded.unwrap_or(false) {
                    app.t.t("skin.round_on")
                } else {
                    app.t.t("skin.round_off")
                };
                app.notify(format!(
                    "✅ {}: {} ({})",
                    app.t.t("skin.applied"),
                    name,
                    round_state
                ));
            } else {
                app.notify(format!("✅ {}: {name}", app.t.t("skin.applied")));
            }
        }
        SkinMenuTarget::ThemePatch(patch) => {
            if let Err(e) = crate::skin::patch::sync_skin_presets(&patch, &[key]) {
                app.notify(format!("❌ {e}"));
            } else if let Err(e) = crate::skin::patch::set_default_skin(&patch, key) {
                app.notify(format!("❌ {e}"));
            } else {
                app.notify(format!("✅ {}: {name}", app.t.t("skin.applied")));
            }
        }
    }
}

async fn handle_skin_round_prompt_key(app: &mut App, key: KeyCode) -> Result<()> {
    match key {
        KeyCode::Left | KeyCode::Char('h') => app.skin_round_choice = true,
        KeyCode::Right | KeyCode::Char('l') => app.skin_round_choice = false,
        KeyCode::Enter => {
            apply_pending_skin_selection(app).await?;
            app.pending_skin_selection = None;
            app.screen = AppScreen::Menu;
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.pending_skin_selection = None;
            app.screen = AppScreen::SkinSelector;
        }
        _ => {}
    }
    Ok(())
}

fn handle_config_key(app: &mut App, key: KeyCode) {
    let config = Manager::new().map(|m| m.config).unwrap_or_default();
    let actions = config_actions(&config);
    let effective_proxy = crate::api::effective_proxy(&config).ok().flatten();
    let env_proxy_active = matches!(
        effective_proxy.as_ref().map(|proxy| proxy.source),
        Some(crate::api::ProxySource::Environment)
    );
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            app.config_selected = app.config_selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') if app.config_selected + 1 < actions.len() => {
            app.config_selected += 1;
        }
        KeyCode::Left | KeyCode::Right | KeyCode::Enter => {
            if let Ok(mut manager) = Manager::new() {
                match actions
                    .get(app.config_selected)
                    .copied()
                    .unwrap_or(ConfigAction::Refresh)
                {
                    ConfigAction::TuiTheme => {
                        manager.config.tui_theme_mode = next_tui_theme_mode(&manager.config);
                    }
                    ConfigAction::UserDataPolicy => {
                        let next_policy = next_user_data_policy(&manager.config);
                        if next_policy == "discard" {
                            app.pending_user_data_policy = Some(next_policy);
                            app.screen = AppScreen::UserDataPolicyConfirm;
                            return;
                        }
                        manager.config.user_data_policy = next_policy;
                    }
                    ConfigAction::ExcludeRules => {
                        app.exclude_selected = 0;
                        app.screen = AppScreen::ExcludeRules;
                        return;
                    }
                    ConfigAction::WanxiangDiagnosis => {
                        app.screen = AppScreen::WanxiangDiagnosis;
                        return;
                    }
                    ConfigAction::Mirror => manager.config.use_mirror = !manager.config.use_mirror,
                    ConfigAction::DownloadThreads => {
                        app.config_input_field = Some(ConfigInputField::DownloadThreads);
                        app.config_input_value = manager.config.download_threads.to_string();
                        app.screen = AppScreen::ConfigInput;
                        return;
                    }
                    ConfigAction::Language => {
                        manager.config.language = next_language_value(&manager.config.language);
                        app.t = L10n::new(Lang::from_str(&manager.config.language));
                    }
                    ConfigAction::ProxyEnabled => {
                        if env_proxy_active {
                            app.notify(app.t.t("config.proxy_env_readonly").to_string());
                            return;
                        }
                        manager.config.proxy_enabled = !manager.config.proxy_enabled
                    }
                    ConfigAction::ProxyType => {
                        if env_proxy_active {
                            app.notify(app.t.t("config.proxy_env_readonly").to_string());
                            return;
                        }
                        manager.config.proxy_type =
                            next_proxy_type_value(&manager.config.proxy_type);
                    }
                    ConfigAction::ProxyAddress => {
                        if env_proxy_active {
                            app.notify(app.t.t("config.proxy_env_readonly").to_string());
                            return;
                        }
                        app.config_input_field = Some(ConfigInputField::ProxyAddress);
                        app.config_input_value = manager.config.proxy_address.clone();
                        app.screen = AppScreen::ConfigInput;
                        return;
                    }
                    ConfigAction::ModelPatch => {
                        manager.config.model_patch_enabled = !manager.config.model_patch_enabled
                    }
                    ConfigAction::CandidatePageSize => {
                        app.config_input_field = Some(ConfigInputField::CandidatePageSize);
                        app.config_input_value = match crate::custom::candidate_page_size(
                            std::path::Path::new(&app.rime_dir),
                            app.schema,
                        ) {
                            Ok(Some(value)) => value.to_string(),
                            _ => String::new(),
                        };
                        app.screen = AppScreen::ConfigInput;
                        return;
                    }
                    ConfigAction::EngineSync => {
                        manager.config.engine_sync_enabled = !manager.config.engine_sync_enabled
                    }
                    ConfigAction::SyncStrategy => {
                        manager.config.engine_sync_use_link = !manager.config.engine_sync_use_link
                    }
                    ConfigAction::Refresh => {}
                }
                let _ = manager.save();
                if matches!(
                    actions
                        .get(app.config_selected)
                        .copied()
                        .unwrap_or(ConfigAction::Refresh),
                    ConfigAction::Refresh
                ) {
                    refresh_config_status(app);
                } else {
                    refresh_config_status(app);
                    app.notify(app.t.t("config.saved").to_string());
                }
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.screen = AppScreen::Menu;
        }
        _ => {}
    }
}

fn handle_config_input_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Esc => {
            app.config_input_field = None;
            app.config_input_value.clear();
            app.screen = if app.exclude_edit_index.is_some()
                || matches!(
                    app.config_input_field,
                    Some(ConfigInputField::ExcludePattern)
                ) {
                AppScreen::ExcludeRules
            } else {
                AppScreen::ConfigView
            };
            app.exclude_edit_index = None;
        }
        KeyCode::Enter => {
            if let Ok(mut manager) = Manager::new() {
                match app.config_input_field {
                    Some(ConfigInputField::ProxyAddress) => {
                        manager.config.proxy_address = app.config_input_value.trim().to_string();
                        let _ = manager.save();
                        app.notify(app.t.t("config.saved").to_string());
                    }
                    Some(ConfigInputField::CandidatePageSize) => {
                        let trimmed = app.config_input_value.trim();
                        let value = if trimmed.is_empty() {
                            None
                        } else {
                            match trimmed.parse::<u8>() {
                                Ok(value @ 1..=9) => Some(value),
                                _ => {
                                    app.notify(app.t.t("config.invalid_page_size").to_string());
                                    return;
                                }
                            }
                        };
                        if crate::custom::set_candidate_page_size(
                            std::path::Path::new(&app.rime_dir),
                            app.schema,
                            value,
                        )
                        .is_ok()
                        {
                            app.notify(app.t.t("config.saved").to_string());
                        }
                    }
                    Some(ConfigInputField::DownloadThreads) => {
                        match app.config_input_value.trim().parse::<usize>() {
                            Ok(value @ 1..=8) => {
                                manager.config.download_threads = value;
                                let _ = manager.save();
                                app.notify(app.t.t("config.saved").to_string());
                            }
                            _ => {
                                app.notify(app.t.t("config.invalid_download_threads").to_string());
                                return;
                            }
                        }
                    }
                    Some(ConfigInputField::ExcludePattern) => {
                        let result = if let Some(index) = app.exclude_edit_index {
                            manager.update_exclude_pattern(index, app.config_input_value.clone())
                        } else {
                            manager.add_exclude_pattern(app.config_input_value.clone())
                        };
                        match result {
                            Ok(()) => app.notify(app.t.t("config.saved").to_string()),
                            Err(err) => {
                                app.notify(format!("❌ {err}"));
                                return;
                            }
                        }
                    }
                    None => {}
                }
            }
            app.exclude_edit_index = None;
            let return_screen = if matches!(
                app.config_input_field,
                Some(ConfigInputField::ExcludePattern)
            ) || app.exclude_edit_index.is_some()
            {
                AppScreen::ExcludeRules
            } else {
                AppScreen::ConfigView
            };
            app.config_input_field = None;
            app.config_input_value.clear();
            app.screen = return_screen;
            refresh_config_status(app);
        }
        KeyCode::Backspace => {
            app.config_input_value.pop();
        }
        KeyCode::Char(c) => {
            app.config_input_value.push(c);
        }
        _ => {}
    }
}

fn handle_exclude_rules_key(app: &mut App, key: KeyCode) {
    let manager = Manager::new();
    let patterns = manager
        .as_ref()
        .map(|m| &m.config.exclude_files)
        .cloned()
        .unwrap_or_default();
    let action_count = patterns.len() + 2;
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            app.exclude_selected = app.exclude_selected.saturating_sub(1)
        }
        KeyCode::Down | KeyCode::Char('j') if app.exclude_selected + 1 < action_count => {
            app.exclude_selected += 1
        }
        KeyCode::Char('a') => {
            app.exclude_edit_index = None;
            app.config_input_field = Some(ConfigInputField::ExcludePattern);
            app.config_input_value.clear();
            app.screen = AppScreen::ConfigInput;
        }
        KeyCode::Char('d') if app.exclude_selected < patterns.len() => {
            if let Ok(mut manager) = Manager::new() {
                if let Err(err) = manager.remove_exclude_pattern(app.exclude_selected) {
                    app.notify(format!("❌ {err}"));
                } else {
                    app.notify(app.t.t("config.saved").to_string());
                    app.exclude_selected = app
                        .exclude_selected
                        .min(manager.config.exclude_files.len().saturating_sub(1));
                }
            }
        }
        KeyCode::Char('r') => {
            if let Ok(mut manager) = Manager::new() {
                if let Err(err) = manager.reset_exclude_patterns() {
                    app.notify(format!("❌ {err}"));
                } else {
                    app.notify(app.t.t("config.saved").to_string());
                    app.exclude_selected = 0;
                }
            }
        }
        KeyCode::Enter => {
            if app.exclude_selected < patterns.len() {
                app.exclude_edit_index = Some(app.exclude_selected);
                app.config_input_field = Some(ConfigInputField::ExcludePattern);
                app.config_input_value = patterns[app.exclude_selected].clone();
                app.screen = AppScreen::ConfigInput;
            } else if app.exclude_selected == patterns.len() {
                app.exclude_edit_index = None;
                app.config_input_field = Some(ConfigInputField::ExcludePattern);
                app.config_input_value.clear();
                app.screen = AppScreen::ConfigInput;
            } else {
                if let Ok(mut manager) = Manager::new() {
                    if let Err(err) = manager.reset_exclude_patterns() {
                        app.notify(format!("❌ {err}"));
                    } else {
                        app.notify(app.t.t("config.saved").to_string());
                        app.exclude_selected = 0;
                    }
                }
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => app.screen = AppScreen::ConfigView,
        _ => {}
    }
}

fn handle_wanxiang_diagnosis_key(app: &mut App, key: KeyCode) {
    if matches!(key, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter) {
        app.screen = AppScreen::ConfigView;
    }
}

fn enter_config_view(app: &mut App) {
    app.config_selected = 0;
    app.config_input_field = None;
    app.config_input_value.clear();
    app.screen = AppScreen::ConfigView;
    refresh_config_status(app);
}

fn refresh_config_status(app: &mut App) {
    app.config_status_loading = true;
    let (tx, rx) = mpsc::channel();
    app.config_status_rx = Some(rx);
    let schema = app.schema;
    let lang = app.t.lang();
    let rime_dir = std::path::PathBuf::from(&app.rime_dir);
    tokio::spawn(async move {
        let snapshot = build_config_status_snapshot(schema, lang, rime_dir).await;
        let _ = tx.send(snapshot);
    });
}

fn available_skin_choices(app: &App) -> Vec<(String, String)> {
    match skin_menu_target(app) {
        Some(SkinMenuTarget::Fcitx5Theme) => crate::skin::fcitx5::builtin_theme_choices(),
        Some(SkinMenuTarget::ThemePatch(_)) => {
            crate::skin::builtin::list_available_skins(app.t.lang())
        }
        None => Vec::new(),
    }
}

// ── 更新调度 ──

async fn start_update(app: &mut App, mode: UpdateMode) -> Result<()> {
    app.screen = AppScreen::Updating;
    app.update_msg = app.t.t("update.checking").into();
    app.update_pct = 0.0;
    app.update_done = false;
    app.update_in_progress = true;
    app.update_outcome = None;
    app.update_results.clear();
    app.update_stage_lines.clear();
    app.update_task = None;
    app.cancel_signal = None;
    let context = match resolve_update_context(app, &mode) {
        Ok(context) => context,
        Err(e) => {
            app.update_results.push(format!("❌ {}", e));
            app.update_msg = app.t.t("update.failed").into();
            app.update_pct = 1.0;
            app.update_done = true;
            app.update_in_progress = false;
            app.update_outcome = Some(UpdateOutcome::Failure);
            app.screen = AppScreen::Result;
            return Ok(());
        }
    };
    app.update_user_data_policy_summary =
        Some(effective_user_data_policy_label(&context.config, app.t.lang()).to_string());

    let (progress_tx, progress_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    app.progress_rx = Some(progress_rx);
    app.result_rx = Some(result_rx);
    let lang = app.t.lang();
    let cancel_signal = crate::types::CancelSignal::new();
    app.cancel_signal = Some(cancel_signal.clone());

    let handle = tokio::spawn(async move {
        let results = run_update_task(context, mode, lang, cancel_signal.clone(), move |event| {
            let _ = progress_tx.send(event);
        })
        .await;
        let _ = result_tx.send(UpdateTaskResult {
            results: match results {
                Ok(value) => Ok(value),
                Err(e) if e.is::<crate::types::UpdateCancelled>() => {
                    Err(UpdateTaskError::Cancelled)
                }
                Err(e) => Err(UpdateTaskError::Failed(e.to_string())),
            },
        });
    });
    app.update_task = Some(handle);

    Ok(())
}

async fn run_update_task(
    context: ResolvedUpdateContext,
    mode: UpdateMode,
    lang: Lang,
    cancel: crate::types::CancelSignal,
    mut progress: impl FnMut(UpdateEvent) + Send + 'static,
) -> Result<Vec<updater::UpdateResult>> {
    let t = L10n::new(lang);
    match mode {
        UpdateMode::All => {
            updater::update_all(
                &context.schema,
                &context.config,
                context.cache_dir,
                context.rime_dir,
                cancel,
                &mut progress,
            )
            .await
        }
        UpdateMode::Scheme => {
            let base = updater::BaseUpdater::new(
                &context.config,
                context.cache_dir.clone(),
                context.rime_dir.clone(),
            )?;
            if context.schema.is_wanxiang() {
                updater::wanxiang::WanxiangUpdater { base }
                    .update_scheme(&context.schema, &context.config, Some(&cancel), |event| {
                        progress(event)
                    })
                    .await
                    .map(|r| vec![r])
            } else if context.schema == Schema::Ice {
                updater::ice::IceUpdater { base }
                    .update_scheme(&context.config, Some(&cancel), &mut progress)
                    .await
                    .map(|r| vec![r])
            } else if context.schema == Schema::Frost {
                updater::frost::FrostUpdater { base }
                    .update_scheme(&context.config, Some(&cancel), &mut progress)
                    .await
                    .map(|r| vec![r])
            } else {
                updater::mint::MintUpdater { base }
                    .update_scheme(&context.config, Some(&cancel), &mut progress)
                    .await
                    .map(|r| vec![r])
            }
        }
        UpdateMode::Dict => {
            if context.schema.dict_zip().is_none() {
                Ok(vec![updater::UpdateResult {
                    component: t.t("update.dict").into(),
                    old_version: "-".into(),
                    new_version: "-".into(),
                    success: false,
                    message: t.t("update.no_dict").into(),
                }])
            } else {
                let base = updater::BaseUpdater::new(
                    &context.config,
                    context.cache_dir,
                    context.rime_dir,
                )?;
                if context.schema.is_wanxiang() {
                    updater::wanxiang::WanxiangUpdater { base }
                        .update_dict(&context.schema, &context.config, Some(&cancel), |event| {
                            progress(event)
                        })
                        .await
                        .map(|r| vec![r])
                } else {
                    updater::ice::IceUpdater { base }
                        .update_dict(&context.config, Some(&cancel), &mut progress)
                        .await
                        .map(|r| vec![r])
                }
            }
        }
        UpdateMode::Model => {
            let base = updater::BaseUpdater::new(
                &context.config,
                context.cache_dir,
                context.rime_dir.clone(),
            )?;
            let wx = updater::wanxiang::WanxiangUpdater { base };
            let r = wx
                .update_model(&context.config, Some(&cancel), &mut progress)
                .await?;
            let mut v = vec![r];
            if context.config.model_patch_enabled && context.schema.supports_model_patch() {
                progress(UpdateEvent {
                    component: UpdateComponent::ModelPatch,
                    phase: UpdatePhase::Applying,
                    progress: 0.96,
                    detail: t.t("menu.model_patch").into(),
                });
                cancel.checkpoint()?;
                if let Err(e) =
                    updater::model_patch::patch_model(&context.rime_dir, &context.schema, lang)
                {
                    v.push(updater::UpdateResult {
                        component: t.t("update.component.model_patch").into(),
                        old_version: "?".into(),
                        new_version: "?".into(),
                        success: false,
                        message: e.to_string(),
                    });
                } else {
                    v.push(updater::UpdateResult {
                        component: t.t("update.component.model_patch").into(),
                        old_version: "-".into(),
                        new_version: t.t("patch.model.enabled").into(),
                        success: true,
                        message: t.t("patch.model.enabled").into(),
                    });
                    progress(UpdateEvent {
                        component: UpdateComponent::ModelPatch,
                        phase: UpdatePhase::Finished,
                        progress: 1.0,
                        detail: t.t("patch.model.enabled").into(),
                    });
                }
            }
            Ok(v)
        }
    }
}

fn finish_update(app: &mut App, results: Result<Vec<updater::UpdateResult>, UpdateTaskError>) {
    match results {
        Ok(rs) => {
            let all_success = rs.iter().all(|r| r.success);
            let any_success = rs.iter().any(|r| r.success);
            for r in &rs {
                let icon = if r.success { "✅" } else { "❌" };
                app.update_results
                    .push(format!("{icon} {} - {}", r.component, r.message));
            }
            app.update_msg = if all_success {
                app.t.t("update.complete").into()
            } else if any_success {
                app.t.t("update.partial").into()
            } else {
                app.t.t("update.failed").into()
            };
            app.update_outcome = Some(if all_success {
                UpdateOutcome::Success
            } else if any_success {
                UpdateOutcome::Partial
            } else {
                UpdateOutcome::Failure
            });
        }
        Err(UpdateTaskError::Cancelled) => {
            app.update_results
                .push(format!("⚠️ {}", app.t.t("update.cancelled")));
            app.update_msg = app.t.t("update.cancelled").into();
            app.update_outcome = Some(UpdateOutcome::Cancelled);
            upsert_stage_line(
                app,
                &UpdateEvent {
                    component: UpdateComponent::Hook,
                    phase: UpdatePhase::Cancelled,
                    progress: 1.0,
                    detail: app.t.t("update.cancelled").into(),
                },
            );
        }
        Err(UpdateTaskError::Failed(e)) => {
            app.update_results
                .push(format!("❌ {}: {e}", app.t.t("update.failed")));
            app.update_msg = app.t.t("update.failed").into();
            app.update_outcome = Some(UpdateOutcome::Failure);
        }
    }

    app.update_pct = 1.0;
    app.update_done = true;
    app.update_in_progress = false;
    app.screen = AppScreen::Result;
    app.progress_rx = None;
    app.result_rx = None;
    app.update_task = None;
    app.cancel_signal = None;
    refresh_config_status(app);
}

fn upsert_stage_line(app: &mut App, event: &UpdateEvent) {
    let label = format!(
        "{}: {}",
        component_label(app, event.component),
        phase_label(app, event.phase)
    );
    let line = format!("{label} - {}", event.detail);
    if let Some(existing) = app
        .update_stage_lines
        .iter_mut()
        .find(|entry| entry.starts_with(&label))
    {
        *existing = line;
    } else {
        app.update_stage_lines.push(line);
    }
}

fn component_label(app: &App, component: UpdateComponent) -> &str {
    match component {
        UpdateComponent::Scheme => app.t.t("update.scheme"),
        UpdateComponent::Dict => app.t.t("update.dict"),
        UpdateComponent::Model => app.t.t("update.model"),
        UpdateComponent::ModelPatch => app.t.t("update.component.model_patch"),
        UpdateComponent::Deploy => app.t.t("update.component.deploy"),
        UpdateComponent::Sync => app.t.t("update.component.sync"),
        UpdateComponent::Hook => app.t.t("update.component.hook"),
    }
}

fn phase_label(app: &App, phase: UpdatePhase) -> &str {
    match phase {
        UpdatePhase::Starting => app.t.t("update.checking"),
        UpdatePhase::Checking => app.t.t("update.checking"),
        UpdatePhase::Downloading => app.t.t("update.downloading"),
        UpdatePhase::Verifying => app.t.t("update.verifying"),
        UpdatePhase::Extracting => app.t.t("update.extracting"),
        UpdatePhase::Saving => app.t.t("update.saving"),
        UpdatePhase::Applying => app.t.t("menu.model_patch"),
        UpdatePhase::Deploying => app.t.t("update.deploying"),
        UpdatePhase::Syncing => app.t.t("update.syncing"),
        UpdatePhase::Running => app.t.t("hint.wait"),
        UpdatePhase::Cancelling => app.t.t("update.cancelling"),
        UpdatePhase::Cancelled => app.t.t("update.cancelled"),
        UpdatePhase::Finished => app.t.t("update.complete"),
    }
}

fn current_schema_index(schema: Schema) -> usize {
    Schema::all()
        .iter()
        .position(|candidate| *candidate == schema)
        .unwrap_or(0)
}

fn skin_menu_supported() -> bool {
    let manager = match Manager::new() {
        Ok(manager) => manager,
        Err(_) => return false,
    };
    let app = App::new(&manager);
    skin_menu_target(&app).is_some()
}

fn menu_unavailable_reason(app: &App, idx: usize) -> Option<String> {
    match idx {
        3 if app.schema.dict_zip().is_none() => Some(format!(
            "{}: {}",
            app.t.t("hint.unavailable"),
            app.t.t("update.no_dict")
        )),
        6 if !skin_menu_supported() => Some(format!(
            "{}: {}",
            app.t.t("hint.unavailable"),
            app.t.t("skin.not_supported")
        )),
        _ => None,
    }
}

// ── 渲染 ──

fn ui(f: &mut Frame, app: &App) {
    let size = f.area();
    f.render_widget(Clear, size);

    let header_height = 5;

    // 主布局: header + body + footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height), // header
            Constraint::Min(8),                // body
            Constraint::Length(3),             // footer
        ])
        .split(size);

    render_header(f, chunks[0], app);

    // Body - 根据屏幕渲染
    match app.screen {
        AppScreen::Menu => render_menu(f, chunks[1], app),
        AppScreen::Updating => crate::ui::update_view::render_updating(
            f,
            chunks[1],
            &app.t,
            crate::ui::update_view::UpdatingViewData {
                update_msg: &app.update_msg,
                update_pct: app.update_pct,
                update_stage_lines: &app.update_stage_lines,
            },
        ),
        AppScreen::Result => crate::ui::update_view::render_result(
            f,
            chunks[1],
            &app.t,
            crate::ui::update_view::ResultViewData {
                update_done: app.update_done,
                update_outcome: app.update_outcome,
                update_msg: &app.update_msg,
                update_user_data_policy_summary: app.update_user_data_policy_summary.as_deref(),
                update_results: &app.update_results,
            },
        ),
        AppScreen::UpdateConfirm => render_menu(f, chunks[1], app),
        AppScreen::UserDataPolicyConfirm => render_config_screen(f, chunks[1], app),
        AppScreen::SchemeSelector => render_scheme_selector(f, chunks[1], app),
        AppScreen::SkinSelector => render_skin_selector(f, chunks[1], app),
        AppScreen::ThemePatchPresetSelector => {
            render_theme_patch_preset_selector(f, chunks[1], app)
        }
        AppScreen::ThemePatchDefaultSelector => {
            render_theme_patch_default_selector(f, chunks[1], app)
        }
        AppScreen::Fcitx5DarkThemeSelector => {
            render_fcitx5_theme_selector(f, chunks[1], app, Fcitx5ThemePhase::Dark)
        }
        AppScreen::Fcitx5LightThemeSelector => {
            render_fcitx5_theme_selector(f, chunks[1], app, Fcitx5ThemePhase::Light)
        }
        AppScreen::ConfigView => render_config_screen(f, chunks[1], app),
        AppScreen::ConfigInput => crate::ui::config_view::render_config_input(
            f,
            chunks[1],
            build_config_input_view_data(app),
        ),
        AppScreen::ExcludeRules => crate::ui::config_view::render_exclude_rules(
            f,
            chunks[1],
            build_exclude_rules_view_data(app),
        ),
        AppScreen::WanxiangDiagnosis => crate::ui::config_view::render_wanxiang_diagnosis(
            f,
            chunks[1],
            build_wanxiang_diagnosis_view_data(app),
        ),
        AppScreen::SkinRoundPrompt => render_menu(f, chunks[1], app),
    }

    if let Some(notification) = &app.notification {
        render_notification_popup(f, size, &notification.message, app.t.t("hint.notice"));
    }

    if matches!(app.screen, AppScreen::SkinRoundPrompt) {
        render_skin_round_prompt(f, size, app);
    }

    if matches!(app.screen, AppScreen::UpdateConfirm) {
        render_update_confirmation_popup(f, size, app);
    }

    if matches!(app.screen, AppScreen::UserDataPolicyConfirm) {
        render_user_data_policy_confirmation_popup(f, size, app);
    }

    // Footer
    let footer_text = vec![Span::styled(
        format!(" {}", app.current_hint()),
        secondary_text(),
    )];
    let footer = Paragraph::new(Line::from(footer_text)).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(color_border())),
    );
    f.render_widget(footer, chunks[2]);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let engine_text = current_engine_label(app);
    let wide = area.width >= 120;
    let chunks = if wide {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(2)])
            .split(area)
    };

    let status_lines = if wide {
        vec![Line::from(vec![
            Span::styled(" snout ", accent_text().add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("v{}  ", env!("CARGO_PKG_VERSION")),
                secondary_text(),
            ),
            Span::styled(" / ", tertiary_text()),
            Span::styled(
                format!("{}: ", app.t.t("config.detected_engines")),
                secondary_text(),
            ),
            Span::styled(engine_text.clone(), primary_text()),
            Span::styled(" / ", tertiary_text()),
            Span::styled(
                format!("{}: ", app.t.t("config.current_scheme")),
                secondary_text(),
            ),
            Span::styled(app.schema.display_name_lang(app.t.lang()), primary_text()),
        ])]
    } else {
        vec![
            Line::from(vec![
                Span::styled(" snout ", accent_text().add_modifier(Modifier::BOLD)),
                Span::styled(format!("v{}", env!("CARGO_PKG_VERSION")), secondary_text()),
            ]),
            Line::from(vec![
                Span::styled(
                    format!("{}: ", app.t.t("config.detected_engines")),
                    secondary_text(),
                ),
                Span::styled(engine_text, primary_text()),
                Span::styled("  >>> ", tertiary_text()),
                Span::styled(
                    format!("{}: ", app.t.t("config.current_scheme")),
                    secondary_text(),
                ),
                Span::styled(app.schema.display_name_lang(app.t.lang()), primary_text()),
            ]),
        ]
    };

    let status = Paragraph::new(status_lines)
        .alignment(if wide {
            Alignment::Left
        } else {
            Alignment::Center
        })
        .block(if wide {
            panel_block(app.t.t("app.name"))
        } else {
            Block::default().borders(Borders::BOTTOM)
        });
    f.render_widget(status, chunks[0]);

    let breadcrumb = Paragraph::new(Line::from(vec![
        Span::styled(app.t.t("menu.title"), tertiary_text()),
        Span::styled("  /  ", tertiary_text()),
        Span::styled(current_screen_label(app), secondary_text()),
    ]))
    .alignment(if wide {
        Alignment::Right
    } else {
        Alignment::Center
    })
    .block(panel_block(app.t.t("menu.current_path")));
    f.render_widget(breadcrumb, chunks[1]);
}

fn current_engine_label(app: &App) -> String {
    let engines = config::detect_installed_engines();
    if engines.is_empty() {
        app.t.t("config.none").to_string()
    } else {
        engines.join(", ")
    }
}

fn current_screen_label(app: &App) -> &str {
    match app.screen {
        AppScreen::Menu => app
            .menu_items()
            .get(app.menu_selected)
            .map(|(_, label)| *label)
            .unwrap_or_else(|| app.t.t("menu.title")),
        AppScreen::Updating | AppScreen::Result | AppScreen::UpdateConfirm => {
            app.t.t("menu.update_all")
        }
        AppScreen::UserDataPolicyConfirm => app.t.t("menu.config"),
        AppScreen::ExcludeRules | AppScreen::WanxiangDiagnosis => app.t.t("menu.config"),
        AppScreen::SchemeSelector => app.t.t("menu.switch_scheme"),
        AppScreen::SkinSelector
        | AppScreen::ThemePatchPresetSelector
        | AppScreen::ThemePatchDefaultSelector
        | AppScreen::SkinRoundPrompt
        | AppScreen::Fcitx5DarkThemeSelector
        | AppScreen::Fcitx5LightThemeSelector => app.t.t("menu.skin_patch"),
        AppScreen::ConfigView | AppScreen::ConfigInput => app.t.t("menu.config"),
    }
}

fn render_notification_popup(f: &mut Frame, area: Rect, message: &str, title: &str) {
    let popup_width = (message.chars().count() as u16 + 8).clamp(24, area.width.saturating_sub(2));
    let popup_height = if area.height < 8 { 3 } else { 5 };
    let popup_area = centered_rect(popup_width, popup_height, area);
    f.render_widget(Clear, popup_area);
    let popup = Paragraph::new(Line::from(vec![Span::styled(message, primary_text())]))
        .alignment(Alignment::Center)
        .block(panel_block(title).border_style(Style::default().fg(color_warning())))
        .wrap(Wrap { trim: true });
    f.render_widget(popup, popup_area);
}

fn render_skin_round_prompt(f: &mut Frame, area: Rect, app: &App) {
    let Some(selection) = &app.pending_skin_selection else {
        return;
    };

    let yes_style = if app.skin_round_choice {
        Style::default()
            .fg(contrast_color(color_accent()))
            .bg(color_accent())
            .add_modifier(Modifier::BOLD)
    } else {
        accent_text()
    };
    let no_style = if !app.skin_round_choice {
        Style::default()
            .fg(contrast_color(color_selection_bg()))
            .bg(color_selection_bg())
            .add_modifier(Modifier::BOLD)
    } else {
        secondary_text()
    };
    let popup_width = area.width.saturating_sub(2).clamp(36, 72);
    let popup_height = area.height.saturating_sub(2).clamp(6, 9);
    let popup_area = centered_rect(popup_width, popup_height, area);
    f.render_widget(Clear, popup_area);
    let content = vec![
        Line::from(vec![Span::styled(
            format!(
                "{}: {} / {}",
                app.t.t("skin.round_prompt_theme"),
                selection.light_key,
                selection.dark_key
            ),
            primary_text(),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            app.t.t("skin.round_prompt_body"),
            secondary_text(),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("  {}  ", app.t.t("skin.round_on")), yes_style),
            Span::raw("  "),
            Span::styled(format!("  {}  ", app.t.t("skin.round_off")), no_style),
        ]),
    ];

    let popup = Paragraph::new(content)
        .wrap(Wrap { trim: true })
        .block(panel_block(app.t.t("skin.round_prompt_title")));
    f.render_widget(popup, popup_area);
}

fn render_update_confirmation_popup(f: &mut Frame, area: Rect, app: &App) {
    let mode = app.pending_update_mode.unwrap_or(UpdateMode::Scheme);
    let popup_width = area.width.saturating_sub(2).clamp(42, 88);
    let popup_height = area.height.saturating_sub(2).clamp(8, 12);
    let popup_area = centered_rect(popup_width, popup_height, area);
    f.render_widget(Clear, popup_area);

    let title = match mode {
        UpdateMode::All => app.t.t("update.confirm_all_title"),
        UpdateMode::Scheme => app.t.t("update.confirm_scheme_title"),
        UpdateMode::Dict => app.t.t("update.confirm_dict_title"),
        UpdateMode::Model => app.t.t("update.confirm_model_title"),
    };
    let notice = if let Ok(manager) = Manager::new() {
        update_notice_text(&manager.config, &app.t)
    } else {
        app.t.t("update.preserve_user_data_notice")
    };
    let detail = if let Ok(manager) = Manager::new() {
        update_detail_text(&manager.config, &app.t)
    } else {
        app.t.t("update.preserve_user_data_detail")
    };

    let content = vec![
        Line::from(vec![Span::styled(
            notice,
            primary_text().add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(detail, secondary_text())]),
        Line::from(""),
        Line::from(vec![Span::styled(
            app.t.t("update.preserve_user_data_scope"),
            tertiary_text(),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            app.t.t("update.confirm_continue"),
            accent_text(),
        )]),
    ];

    let popup = Paragraph::new(content)
        .wrap(Wrap { trim: true })
        .block(panel_block(title));
    f.render_widget(popup, popup_area);
}

fn render_user_data_policy_confirmation_popup(f: &mut Frame, area: Rect, app: &App) {
    let popup_width = area.width.saturating_sub(2).clamp(42, 88);
    let popup_height = area.height.saturating_sub(2).clamp(8, 12);
    let popup_area = centered_rect(popup_width, popup_height, area);
    f.render_widget(Clear, popup_area);

    let content = vec![
        Line::from(vec![Span::styled(
            app.t.t("config.user_data_policy_discard_notice"),
            Style::default()
                .fg(color_warning())
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            app.t.t("config.user_data_policy_discard_detail"),
            secondary_text(),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            app.t.t("update.confirm_continue"),
            accent_text(),
        )]),
    ];

    let popup = Paragraph::new(content)
        .wrap(Wrap { trim: true })
        .block(panel_block(
            app.t.t("config.user_data_policy_discard_title"),
        ));
    f.render_widget(popup, popup_area);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(area.width.saturating_sub(width) / 2),
            Constraint::Length(width.min(area.width)),
            Constraint::Min(0),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(area.height.saturating_sub(height) / 2),
            Constraint::Length(height.min(area.height)),
            Constraint::Min(0),
        ])
        .split(horizontal[1])[1]
}

fn table_lines(rows: Vec<(&str, &str)>) -> Vec<Line<'static>> {
    rows.into_iter()
        .map(|(label, value)| {
            Line::from(vec![
                Span::styled(format!("  {label} // "), secondary_text()),
                Span::styled(value.to_string(), primary_text()),
            ])
        })
        .collect()
}

fn selector_state(
    selected: usize,
    window_start: usize,
    visible_len: usize,
) -> ratatui::widgets::ListState {
    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(
        selected
            .saturating_sub(window_start)
            .min(visible_len.saturating_sub(1)),
    ));
    state
}

fn render_menu(f: &mut Frame, area: Rect, app: &App) {
    let menu_items = app.menu_items();
    let chunks = if area.width < 90 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(area)
    };
    let items: Vec<ListItem> = menu_items
        .iter()
        .enumerate()
        .map(|(i, (key, label))| {
            let idx = i + 1;
            let unavailable = menu_unavailable_reason(app, idx).is_some();
            let style = if unavailable {
                tertiary_text()
            } else {
                primary_text()
            };
            let line = vec![
                Span::styled(format!("  [{key}] "), accent_text()),
                Span::styled(*label, style),
            ];
            ListItem::new(Line::from(line))
        })
        .collect();

    let list = List::new(items)
        .block(panel_block(app.t.t("menu.title")))
        .highlight_style(selection_style())
        .highlight_symbol(selector_highlight_symbol());

    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(app.menu_selected));
    f.render_stateful_widget(list, chunks[0], &mut state);

    let selected_idx = app.menu_selected + 1;
    let install_message = config::rime_installation_message(app.t.lang());
    let detail = if !install_message.is_empty() {
        detail_lines(&install_message)
    } else if let Some(reason) = menu_unavailable_reason(app, selected_idx) {
        vec![
            Line::from(vec![Span::styled(
                app.t.t("hint.unavailable"),
                Style::default()
                    .fg(color_warning())
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(reason, secondary_text())]),
        ]
    } else {
        let mut lines = vec![
            Line::from(vec![Span::styled(
                menu_description(app, selected_idx),
                secondary_text(),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                app.t.t("menu.installed_versions"),
                section_title_text(),
            )]),
        ];
        lines.extend(table_lines(vec![
            (
                app.t.t("config.scheme_status_label"),
                app.config_status.installed_scheme_version.as_str(),
            ),
            (
                app.t.t("config.dict_status_label"),
                app.config_status.installed_dict_version.as_str(),
            ),
            (
                app.t.t("config.model_status_label"),
                app.config_status.installed_model_version.as_str(),
            ),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            app.t.t("menu.current_settings"),
            section_title_text(),
        )]));
        lines.extend(table_lines(vec![
            (
                app.t.t("config.candidate_page_size_label"),
                app.config_status.candidate_page_size.as_str(),
            ),
            (
                app.t.t("config.model_patch_status_label"),
                app.config_status.model_patch_status.as_str(),
            ),
        ]));
        if app.config_status_loading {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled(
                app.t.t("config.loading"),
                tertiary_text(),
            )]));
        }
        lines
    };
    let detail_panel = Paragraph::new(detail)
        .wrap(Wrap { trim: true })
        .block(panel_block(app.t.t("menu.result")));
    f.render_widget(detail_panel, chunks[1]);
}

fn detail_lines(message: &str) -> Vec<Line<'static>> {
    message
        .lines()
        .map(|line| {
            let style = if line.starts_with("⚠️") {
                Style::default()
                    .fg(color_warning())
                    .add_modifier(Modifier::BOLD)
            } else if line.trim_start().starts_with('•') {
                accent_text()
            } else if line.trim_start().starts_with('-') || line.trim_start().starts_with("http") {
                tertiary_text()
            } else {
                primary_text()
            };
            Line::from(vec![Span::styled(line.to_string(), style)])
        })
        .collect()
}

fn render_scheme_selector(f: &mut Frame, area: Rect, app: &App) {
    let schemas = Schema::all();
    let visible_rows = area.height.saturating_sub(2) as usize;
    let window = sliding_window(app.scheme_selected, schemas.len(), visible_rows.max(1));
    let visible_schemas = &schemas[window.start..window.end];
    let items: Vec<ListItem> = visible_schemas
        .iter()
        .map(|s| {
            let prefix = if *s == app.schema { " ● " } else { " ○ " };
            let style = if s.is_wanxiang() {
                accent_text()
            } else {
                primary_text()
            };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, secondary_text()),
                Span::styled(s.display_name_lang(app.t.lang()), style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(panel_block(&format!(
            "{} {}/{}",
            app.t.t("scheme.select_prompt"),
            window.current_index,
            window.total_items.max(1)
        )))
        .highlight_style(selection_style())
        .highlight_symbol(selector_highlight_symbol());

    let mut state = selector_state(app.scheme_selected, window.start, visible_schemas.len());
    f.render_stateful_widget(list, area, &mut state);
}

fn render_skin_selector(f: &mut Frame, area: Rect, app: &App) {
    let skins = available_skin_choices(app);
    let visible_rows = area.height.saturating_sub(2) as usize;
    let window = sliding_window(app.skin_selected, skins.len(), visible_rows.max(1));
    let visible_skins = &skins[window.start..window.end];
    let installed_linux_themes = match skin_menu_target(app) {
        Some(SkinMenuTarget::Fcitx5Theme) => {
            crate::skin::fcitx5::installed_theme_names().unwrap_or_default()
        }
        _ => std::collections::HashSet::new(),
    };
    let items: Vec<ListItem> = visible_skins
        .iter()
        .map(|(key, name)| {
            let mut spans = vec![
                Span::styled("  ", Style::default()),
                Span::styled(name.as_str(), primary_text()),
                Span::styled(format!(" ({key})"), tertiary_text()),
            ];
            if installed_linux_themes.contains(key) {
                spans.push(Span::styled(
                    format!(" [{}]", app.t.t("skin.installed_marker")),
                    accent_text(),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let title_key = match skin_menu_target(app) {
        Some(SkinMenuTarget::Fcitx5Theme) => "skin.fcitx5_select_prompt",
        _ => "skin.select_prompt",
    };
    let title = format!(
        "{} {}/{}",
        app.t.t(title_key),
        window.current_index,
        window.total_items.max(1)
    );

    let list = List::new(items)
        .block(panel_block(&title))
        .highlight_style(selection_style())
        .highlight_symbol(selector_highlight_symbol());

    let mut state = selector_state(app.skin_selected, window.start, visible_skins.len());
    f.render_stateful_widget(list, area, &mut state);
}

fn render_theme_patch_preset_selector(f: &mut Frame, area: Rect, app: &App) {
    let chunks = if area.width < 88 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(4)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
            .split(area)
    };
    let skins = available_skin_choices(app);
    let visible_rows = chunks[0].height.saturating_sub(2) as usize;
    let window = sliding_window(app.skin_selected, skins.len(), visible_rows.max(1));
    let visible_skins = &skins[window.start..window.end];
    let items: Vec<ListItem> = visible_skins
        .iter()
        .map(|(key, name)| {
            let marker = if app.theme_patch_selections.contains(key) {
                "[x]"
            } else {
                "[ ]"
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {marker} "), accent_text()),
                Span::styled(name.as_str(), primary_text()),
                Span::styled(format!(" ({key})"), tertiary_text()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(panel_block(app.t.t("skin.theme_patch_preset_prompt")))
        .highlight_style(selection_style())
        .highlight_symbol(selector_highlight_symbol());

    let mut state = selector_state(app.skin_selected, window.start, visible_skins.len());
    f.render_stateful_widget(list, chunks[0], &mut state);

    let summary = vec![
        Line::from(format!(
            "  {}: {}",
            app.t.t("menu.current_settings"),
            app.theme_patch_selections.len()
        )),
        Line::from(format!(
            "  {}: {}",
            app.t.t("skin.theme_patch_default_prompt"),
            app.theme_patch_default
                .clone()
                .unwrap_or_else(|| app.t.t("config.none").into())
        )),
    ];
    let summary_panel = Paragraph::new(summary)
        .wrap(Wrap { trim: true })
        .block(panel_block(app.t.t("menu.result")));
    f.render_widget(summary_panel, chunks[1]);
}

fn render_theme_patch_default_selector(f: &mut Frame, area: Rect, app: &App) {
    let chunks = if area.width < 88 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(4)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
            .split(area)
    };
    let skins = selected_theme_patch_choices(app);
    let visible_rows = chunks[0].height.saturating_sub(2) as usize;
    let window = sliding_window(app.skin_selected, skins.len(), visible_rows.max(1));
    let visible_skins = &skins[window.start..window.end];
    let items: Vec<ListItem> = visible_skins
        .iter()
        .map(|(key, name)| {
            let mut spans = vec![Span::styled("  ", Style::default())];
            if app.theme_patch_default.as_deref() == Some(key.as_str()) {
                spans.push(Span::styled("● ", accent_text()));
            } else {
                spans.push(Span::styled("○ ", tertiary_text()));
            }
            spans.push(Span::styled(name.as_str(), primary_text()));
            spans.push(Span::styled(format!(" ({key})"), tertiary_text()));
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(panel_block(app.t.t("skin.theme_patch_default_prompt")))
        .highlight_style(selection_style())
        .highlight_symbol(selector_highlight_symbol());

    let mut state = selector_state(app.skin_selected, window.start, visible_skins.len());
    f.render_stateful_widget(list, chunks[0], &mut state);

    let summary = vec![
        Line::from(format!(
            "  {}: {}",
            app.t.t("skin.theme_patch_preset_prompt"),
            app.theme_patch_selections.len()
        )),
        Line::from(format!(
            "  {}: {}",
            app.t.t("skin.theme_patch_default_prompt"),
            app.theme_patch_default
                .clone()
                .unwrap_or_else(|| app.t.t("config.none").into())
        )),
    ];
    let summary_panel = Paragraph::new(summary)
        .wrap(Wrap { trim: true })
        .block(panel_block(app.t.t("menu.result")));
    f.render_widget(summary_panel, chunks[1]);
}

fn render_fcitx5_theme_selector(f: &mut Frame, area: Rect, app: &App, phase: Fcitx5ThemePhase) {
    let chunks = if area.width < 88 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(4)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
            .split(area)
    };
    let skins = available_skin_choices(app);
    let visible_rows = chunks[0].height.saturating_sub(2) as usize;
    let window = sliding_window(app.skin_selected, skins.len(), visible_rows.max(1));
    let visible_skins = &skins[window.start..window.end];
    let current = crate::skin::fcitx5::current_theme_selection()
        .ok()
        .unwrap_or_default();

    let items: Vec<ListItem> = visible_skins
        .iter()
        .map(|(key, name)| {
            let mut spans = vec![
                Span::styled("  ", Style::default()),
                Span::styled(name.as_str(), primary_text()),
                Span::styled(format!(" ({key})"), tertiary_text()),
            ];
            if current.light.as_deref() == Some(key.as_str()) {
                spans.push(Span::styled(
                    format!(" [{}]", app.t.t("skin.current_light_marker")),
                    accent_text(),
                ));
            }
            if current.dark.as_deref() == Some(key.as_str()) {
                spans.push(Span::styled(
                    format!(" [{}]", app.t.t("skin.current_dark_marker")),
                    secondary_text(),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let title_key = match phase {
        Fcitx5ThemePhase::Dark => "skin.fcitx5_dark_select_prompt",
        Fcitx5ThemePhase::Light => "skin.fcitx5_light_select_prompt",
    };
    let list = List::new(items)
        .block(panel_block(&format!(
            "{} {}/{}",
            app.t.t(title_key),
            window.current_index,
            window.total_items.max(1)
        )))
        .highlight_style(selection_style())
        .highlight_symbol(selector_highlight_symbol());

    let mut state = selector_state(app.skin_selected, window.start, visible_skins.len());
    f.render_stateful_widget(list, chunks[0], &mut state);

    let summary = vec![
        Line::from(format!(
            "  {}: {}",
            app.t.t("skin.current_dark_marker"),
            current
                .dark
                .unwrap_or_else(|| app.t.t("config.none").into())
        )),
        Line::from(format!(
            "  {}: {}",
            app.t.t("skin.current_light_marker"),
            current
                .light
                .unwrap_or_else(|| app.t.t("config.none").into())
        )),
        Line::from(""),
        Line::from(format!(
            "  {}: {}",
            app.t.t("skin.selected_dark_marker"),
            app.fcitx5_dark_selected
                .clone()
                .unwrap_or_else(|| app.t.t("config.none").into())
        )),
        Line::from(format!(
            "  {}: {}",
            app.t.t("skin.selected_light_marker"),
            app.fcitx5_light_selected
                .clone()
                .unwrap_or_else(|| app.t.t("config.none").into())
        )),
    ];
    let summary_panel = Paragraph::new(summary)
        .wrap(Wrap { trim: true })
        .block(panel_block(app.t.t("menu.result")));
    f.render_widget(summary_panel, chunks[1]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SlidingWindow {
    start: usize,
    end: usize,
    current_index: usize,
    total_items: usize,
}

fn sliding_window(selected: usize, total_items: usize, window_size: usize) -> SlidingWindow {
    let safe_window_size = window_size.max(1);
    let bounded_selected = selected.min(total_items.saturating_sub(1));
    let start = if total_items <= safe_window_size {
        0
    } else {
        bounded_selected
            .saturating_add(1)
            .saturating_sub(safe_window_size)
    };
    let end = (start + safe_window_size).min(total_items);
    SlidingWindow {
        start,
        end,
        current_index: bounded_selected.saturating_add(1),
        total_items,
    }
}

fn render_config_screen(f: &mut Frame, area: Rect, app: &App) {
    let manager = Manager::new().ok();
    let detected_engines = manager
        .as_ref()
        .map(|_| crate::config::detect_installed_engines().join(", "))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| app.t.t("config.none").into());
    let config = manager.map(|m| m.config).unwrap_or_default();
    let effective_proxy = crate::api::effective_proxy(&config).ok().flatten();
    let env_proxy_active = matches!(
        effective_proxy.as_ref().map(|proxy| proxy.source),
        Some(crate::api::ProxySource::Environment)
    );

    crate::ui::config_view::render_config_screen(
        f,
        area,
        &app.t,
        crate::ui::config_view::ConfigScreenState {
            selected_index: app.config_selected,
            schema_name: app.schema.display_name_lang(app.t.lang()),
            config: &config,
            detected_engines,
            effective_proxy: effective_proxy.as_ref(),
            env_proxy_active,
            config_status: &app.config_status,
            is_loading: app.config_status_loading,
            rime_dir: &app.rime_dir,
            config_path: &app.config_path,
            lang: app.t.lang(),
        },
    );
}

fn build_exclude_rules_view_data(app: &App) -> crate::ui::config_view::ExcludeRulesViewData<'_> {
    let manager = Manager::new().ok();
    let patterns = manager
        .as_ref()
        .map(|m| crate::config::effective_exclude_patterns(&m.config))
        .unwrap_or_else(crate::config::default_exclude_patterns);
    let descriptions = manager
        .as_ref()
        .and_then(|m| {
            let effective = crate::config::effective_exclude_patterns(&m.config);
            let (parsed, errors) = crate::config::parse_exclude_patterns(&effective);
            if errors.is_empty() {
                Some(
                    parsed
                        .iter()
                        .map(crate::config::exclude_pattern_description)
                        .collect::<Vec<_>>(),
                )
            } else {
                None
            }
        })
        .unwrap_or_else(|| patterns.clone());

    crate::ui::config_view::ExcludeRulesViewData {
        help_text: app.t.t("config.exclude_help"),
        effective_count_label: app.t.t("config.exclude_effective_count"),
        patterns_len: patterns.len(),
        descriptions,
        selected_index: app.exclude_selected,
        add_label: app.t.t("config.exclude_add"),
        reset_label: app.t.t("config.exclude_reset"),
        examples_label: app.t.t("config.exclude_examples"),
        title: app.t.t("config.exclude_rules_title"),
    }
}

fn build_wanxiang_diagnosis_view_data(
    app: &App,
) -> crate::ui::config_view::WanxiangDiagnosisViewData<'_> {
    let manager = match Manager::new() {
        Ok(manager) => manager,
        Err(err) => {
            return crate::ui::config_view::WanxiangDiagnosisViewData {
                title: app.t.t("config.wanxiang_diagnosis_title"),
                failed_label: app.t.t("update.failed"),
                current_scheme_label: app.t.t("config.current_scheme"),
                markers_label: app.t.t("config.wanxiang_markers_label"),
                error_message: Some(err.to_string()),
                detected_schema: String::new(),
                record_schema: String::new(),
                config_schema: String::new(),
                custom_patch_schema: String::new(),
                marker_files: Vec::new(),
            };
        }
    };
    let diagnosis =
        crate::config::diagnose_wanxiang(&manager.config, &manager.cache_dir, &manager.rime_dir);
    crate::ui::config_view::WanxiangDiagnosisViewData {
        title: app.t.t("config.wanxiang_diagnosis_title"),
        failed_label: app.t.t("update.failed"),
        current_scheme_label: app.t.t("config.current_scheme"),
        markers_label: app.t.t("config.wanxiang_markers_label"),
        error_message: None,
        detected_schema: diagnosis
            .detected_schema
            .map(|s| s.display_name_lang(app.t.lang()))
            .unwrap_or_else(|| app.t.t("config.none").into()),
        record_schema: diagnosis
            .record_schema
            .map(|s| s.display_name_lang(app.t.lang()))
            .unwrap_or_else(|| app.t.t("config.none").into()),
        config_schema: diagnosis
            .config_schema
            .map(|s| s.display_name_lang(app.t.lang()))
            .unwrap_or_else(|| app.t.t("config.none").into()),
        custom_patch_schema: diagnosis
            .custom_patch_schema
            .map(|s| s.display_name_lang(app.t.lang()))
            .unwrap_or_else(|| app.t.t("config.none").into()),
        marker_files: diagnosis.marker_files,
    }
}

fn build_config_input_view_data(app: &App) -> crate::ui::config_view::ConfigInputViewData<'_> {
    let title = match app.config_input_field {
        Some(ConfigInputField::ProxyAddress) => app.t.t("config.proxy_address_label"),
        Some(ConfigInputField::CandidatePageSize) => app.t.t("config.candidate_page_size_label"),
        Some(ConfigInputField::DownloadThreads) => app.t.t("config.download_threads_label"),
        Some(ConfigInputField::ExcludePattern) => app.t.t("config.exclude_rules_label"),
        None => app.t.t("config.title"),
    };
    let hint = match app.config_input_field {
        Some(ConfigInputField::CandidatePageSize) => app.t.t("config.input_hint_page_size"),
        Some(ConfigInputField::DownloadThreads) => app.t.t("config.input_hint_download_threads"),
        Some(ConfigInputField::ExcludePattern) => app.t.t("config.input_hint_exclude_rule"),
        _ => app.t.t("config.input_hint"),
    };

    crate::ui::config_view::ConfigInputViewData {
        title,
        hint,
        value: &app.config_input_value,
        placeholder: app.t.t("config.input_placeholder"),
        edit_title: app.t.t("config.edit_title"),
    }
}

fn menu_description(app: &App, idx: usize) -> String {
    match idx {
        1 => app.t.t("menu.desc.update_all").into(),
        2 => app.t.t("menu.desc.update_scheme").into(),
        3 => app.t.t("menu.desc.update_dict").into(),
        4 => app.t.t("menu.desc.update_model").into(),
        5 => app.t.t("menu.desc.model_patch").into(),
        6 => app.t.t("menu.desc.skin_patch").into(),
        7 => app.t.t("menu.desc.switch_scheme").into(),
        8 => app.t.t("menu.desc.config").into(),
        9 => app.t.t("menu.desc.quit").into(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn model_update_supported_for_all_supported_schemas() {
        assert!(model_update_supported(Schema::WanxiangBase));
        assert!(model_update_supported(Schema::WanxiangMoqi));
        assert!(model_update_supported(Schema::Ice));
        assert!(model_update_supported(Schema::Frost));
        assert!(model_update_supported(Schema::Mint));
    }

    #[test]
    fn theme_patch_target_matches_platform_convention() {
        let base = std::path::Path::new("/tmp/rime");
        #[cfg(target_os = "windows")]
        assert_eq!(
            theme_patch_target_for_platform(base, &["weasel".to_string()]).unwrap(),
            base.join("weasel.custom.yaml")
        );

        #[cfg(target_os = "macos")]
        assert_eq!(
            theme_patch_target_for_platform(base, &["squirrel".to_string()]).unwrap(),
            base.join("squirrel.custom.yaml")
        );

        #[cfg(target_os = "linux")]
        assert!(theme_patch_target_for_platform(base, &[]).is_none());
    }

    #[test]
    fn current_schema_index_tracks_active_schema() {
        assert_eq!(current_schema_index(Schema::WanxiangBase), 0);
        assert_eq!(current_schema_index(Schema::Mint), Schema::all().len() - 1);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn skin_menu_supported_matches_skin_target_resolution() {
        let manager = Manager::new().expect("manager");
        let app = App::new(&manager);
        assert_eq!(skin_menu_supported(), skin_menu_target(&app).is_some());
    }

    #[test]
    fn menu_unavailable_reason_blocks_dict_for_schema_without_separate_dict() {
        let manager = Manager::new().expect("manager");
        let mut app = App::new(&manager);
        app.schema = Schema::Mint;
        assert!(menu_unavailable_reason(&app, 3).is_some());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn menu_unavailable_reason_uses_runtime_skin_support() {
        let manager = Manager::new().expect("manager");
        let app = App::new(&manager);

        assert_eq!(
            menu_unavailable_reason(&app, 6).is_some(),
            !skin_menu_supported()
        );
    }

    #[test]
    fn handle_updating_key_marks_update_as_cancelled() {
        let manager = Manager::new().expect("manager");
        let mut app = App::new(&manager);
        app.screen = AppScreen::Updating;
        app.update_in_progress = true;
        app.cancel_signal = Some(crate::types::CancelSignal::new());

        handle_updating_key(&mut app, KeyCode::Esc);

        assert!(matches!(app.screen, AppScreen::Updating));
        assert_eq!(app.update_msg, app.t.t("update.cancelling"));
    }

    #[tokio::test]
    async fn enter_on_quit_menu_item_exits() {
        let manager = Manager::new().expect("manager");
        let mut app = App::new(&manager);
        app.menu_selected = app.menu_items().len() - 1;

        handle_menu_key(&mut app, KeyCode::Enter)
            .await
            .expect("menu key");

        assert!(app.should_quit);
    }

    #[test]
    fn ui_clears_previous_screen_content_before_rendering() {
        let manager = Manager::new().expect("manager");
        let mut app = App::new(&manager);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");

        app.screen = AppScreen::Result;
        app.update_done = true;
        app.update_msg = "very-long-unique-result-line".into();
        app.update_results = vec!["result-detail-marker".into()];
        terminal.draw(|f| ui(f, &app)).expect("draw result");

        app.screen = AppScreen::Menu;
        app.update_results.clear();
        terminal.draw(|f| ui(f, &app)).expect("draw menu");

        let rendered = buffer_to_string(terminal.backend());
        assert!(!rendered.contains("very-long-unique-result-line"));
        assert!(!rendered.contains("result-detail-marker"));
    }

    #[test]
    fn terminal_theme_detection_prefers_explicit_override() {
        assert_eq!(
            crate::ui::style::detect_terminal_theme_from_env_values(Some("light"), Some("15;0")),
            Some(crate::ui::style::TerminalTheme::Light)
        );
        assert_eq!(
            crate::ui::style::detect_terminal_theme_from_env_values(Some("dark"), Some("0;15")),
            Some(crate::ui::style::TerminalTheme::Dark)
        );
    }

    #[test]
    fn terminal_theme_detection_uses_colorfgbg_background() {
        assert_eq!(
            crate::ui::style::detect_terminal_theme_from_env_values(None, Some("15;0")),
            Some(crate::ui::style::TerminalTheme::Dark)
        );
        assert_eq!(
            crate::ui::style::detect_terminal_theme_from_env_values(None, Some("0;15")),
            Some(crate::ui::style::TerminalTheme::Light)
        );
        assert_eq!(
            crate::ui::style::detect_terminal_theme_from_env_values(None, Some("default;default")),
            None
        );
    }

    #[test]
    fn palette_switches_to_dark_text_for_light_terminals() {
        let light = crate::ui::style::UiPalette::for_theme(crate::ui::style::TerminalTheme::Light);
        let dark = crate::ui::style::UiPalette::for_theme(crate::ui::style::TerminalTheme::Dark);

        assert_eq!(light.primary, ratatui::style::Color::Rgb(5, 5, 5));
        assert_eq!(dark.primary, ratatui::style::Color::Rgb(234, 234, 234));
        assert_eq!(light.accent, ratatui::style::Color::Rgb(176, 110, 12));
        assert_eq!(dark.accent, ratatui::style::Color::Rgb(224, 163, 46));
        assert_eq!(
            crate::ui::style::contrast_color(light.selection_bg),
            ratatui::style::Color::Black
        );
        assert_eq!(
            crate::ui::style::contrast_color(dark.selection_bg),
            ratatui::style::Color::White
        );
    }

    #[test]
    fn next_tui_theme_mode_cycles_expected_values() {
        let mut config = crate::types::Config::default();
        assert_eq!(next_tui_theme_mode(&config), "light");
        config.tui_theme_mode = "light".into();
        assert_eq!(next_tui_theme_mode(&config), "dark");
        config.tui_theme_mode = "dark".into();
        assert_eq!(next_tui_theme_mode(&config), "auto");
    }

    #[test]
    fn framed_title_and_selector_tokens_match_tactical_telemetry_style() {
        assert_eq!(crate::ui::style::framed_title("STATUS"), "[ STATUS ]");
        assert_eq!(selector_highlight_symbol(), ">>> ");
        assert_eq!(crate::ui::style::selector_prefix(true), ">>> ");
        assert_eq!(crate::ui::style::selector_prefix(false), " ·  ");
    }

    #[test]
    fn next_user_data_policy_cycles_expected_values() {
        let mut config = crate::types::Config::default();
        assert_eq!(next_user_data_policy(&config), "preserve");
        config.user_data_policy = "preserve".into();
        assert_eq!(next_user_data_policy(&config), "discard");
        config.user_data_policy = "discard".into();
        assert_eq!(next_user_data_policy(&config), "prompt");
    }

    #[test]
    fn config_actions_show_wanxiang_diagnosis_only_for_wanxiang() {
        let mut config = crate::types::Config::default();
        let actions = config_actions(&config);
        assert!(actions.contains(&ConfigAction::WanxiangDiagnosis));

        config.schema = Schema::Ice;
        let actions = config_actions(&config);
        assert!(!actions.contains(&ConfigAction::WanxiangDiagnosis));
    }

    #[test]
    fn resolve_update_context_reloads_latest_saved_config() {
        let manager = Manager::new().expect("manager");
        let original = manager.config.model_patch_enabled;
        let mut app = App::new(&manager);
        app.schema = manager.config.schema;

        let mut updated = Manager::new().expect("reload manager");
        updated.config.model_patch_enabled = !original;
        updated.save().expect("save updated config");

        let context = resolve_update_context(&app, &UpdateMode::All).expect("update context");
        assert_eq!(context.config.model_patch_enabled, !original);

        let mut restore = Manager::new().expect("restore manager");
        restore.config.model_patch_enabled = original;
        restore.save().expect("restore config");
    }

    fn buffer_to_string(backend: &TestBackend) -> String {
        backend
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<Vec<_>>()
            .join("")
    }
}
