use crate::config::{self, Manager};
use crate::i18n::{L10n, Lang};
use crate::types::Schema;
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
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap},
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
    SchemeSelector,
    SkinSelector,
    SkinRoundPrompt,
    Fcitx5LightThemeSelector,
    Fcitx5DarkThemeSelector,
    ConfigView,
    ConfigInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateOutcome {
    Success,
    Partial,
    Failure,
    Cancelled,
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
}

#[derive(Debug)]
enum UpdateTaskError {
    Cancelled,
    Failed(String),
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

#[derive(Debug, Clone, Default)]
struct ConfigStatusSnapshot {
    scheme_status: String,
    dict_status: String,
    model_status: String,
    model_patch_status: String,
    candidate_page_size: String,
    installed_scheme_version: String,
    installed_dict_version: String,
    installed_model_version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfigAction {
    Mirror,
    Language,
    ProxyEnabled,
    ProxyType,
    ProxyAddress,
    ModelPatch,
    CandidatePageSize,
    EngineSync,
    SyncStrategy,
    Refresh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfigInputField {
    ProxyAddress,
    CandidatePageSize,
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
            AppScreen::Result => format!("Enter/Esc {}", self.t.t("hint.back")),
            AppScreen::SchemeSelector
            | AppScreen::SkinSelector
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

fn resolve_update_context(
    app: &App,
    manager: &Manager,
    mode: &UpdateMode,
) -> anyhow::Result<ResolvedUpdateContext> {
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
                    AppScreen::Menu => handle_menu_key(app, key.code, manager).await?,
                    AppScreen::Updating => handle_updating_key(app, key.code),
                    AppScreen::Result => handle_result_key(app, key.code),
                    AppScreen::SchemeSelector => handle_scheme_key(app, key.code, manager)?,
                    AppScreen::SkinSelector => handle_skin_key(app, key.code, manager)?,
                    AppScreen::Fcitx5LightThemeSelector => {
                        handle_fcitx5_theme_key(app, key.code, Fcitx5ThemePhase::Light)?
                    }
                    AppScreen::Fcitx5DarkThemeSelector => {
                        handle_fcitx5_theme_key(app, key.code, Fcitx5ThemePhase::Dark)?
                    }
                    AppScreen::SkinRoundPrompt => handle_skin_round_prompt_key(app, key.code)?,
                    AppScreen::ConfigView => handle_config_key(app, key.code),
                    AppScreen::ConfigInput => handle_config_input_key(app, key.code),
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

async fn handle_menu_key(app: &mut App, key: KeyCode, manager: &Manager) -> Result<()> {
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
                1 => start_update(app, manager, UpdateMode::All).await?,
                2 => start_update(app, manager, UpdateMode::Scheme).await?,
                3 => start_update(app, manager, UpdateMode::Dict).await?,
                4 => start_update(app, manager, UpdateMode::Model).await?,
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

fn handle_skin_key(app: &mut App, key: KeyCode, _manager: &Manager) -> Result<()> {
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
                        apply_skin_selection(app, target, key, name, None);
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

#[derive(Clone, Copy)]
enum Fcitx5ThemePhase {
    Dark,
    Light,
}

fn handle_fcitx5_theme_key(app: &mut App, key: KeyCode, phase: Fcitx5ThemePhase) -> Result<()> {
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
                            apply_fcitx5_theme_pair(app, None);
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

fn apply_pending_skin_selection(app: &mut App) -> Result<()> {
    let Some(selection) = app.pending_skin_selection.clone() else {
        return Ok(());
    };

    match selection.target {
        SkinMenuTarget::Fcitx5Theme => apply_fcitx5_theme_pair(app, Some(app.skin_round_choice)),
        _ => apply_skin_selection(
            app,
            selection.target,
            &selection.light_key,
            &selection.dark_key,
            Some(app.skin_round_choice),
        ),
    }
    Ok(())
}

fn apply_fcitx5_theme_pair(app: &mut App, rounded: Option<bool>) {
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
        crate::skin::fcitx5::apply_theme_pair(&light, &dark, rounded, rounded, app.t.lang())
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

fn apply_skin_selection(
    app: &mut App,
    target: SkinMenuTarget,
    key: &str,
    name: &str,
    rounded: Option<bool>,
) {
    match target {
        SkinMenuTarget::Fcitx5Theme => {
            if let Err(e) = crate::skin::fcitx5::apply_theme(key, rounded, app.t.lang()) {
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

fn handle_skin_round_prompt_key(app: &mut App, key: KeyCode) -> Result<()> {
    match key {
        KeyCode::Left | KeyCode::Char('h') => app.skin_round_choice = true,
        KeyCode::Right | KeyCode::Char('l') => app.skin_round_choice = false,
        KeyCode::Enter => {
            apply_pending_skin_selection(app)?;
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
                    ConfigAction::Mirror => manager.config.use_mirror = !manager.config.use_mirror,
                    ConfigAction::Language => {
                        manager.config.language = if manager.config.language.starts_with("zh") {
                            "en".into()
                        } else {
                            "zh".into()
                        };
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
                        manager.config.proxy_type = if manager.config.proxy_type == "http" {
                            "socks5".into()
                        } else {
                            "http".into()
                        };
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
            app.screen = AppScreen::ConfigView;
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
                    None => {}
                }
            }
            app.config_input_field = None;
            app.config_input_value.clear();
            app.screen = AppScreen::ConfigView;
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

fn enter_config_view(app: &mut App) {
    app.config_selected = 0;
    app.config_input_field = None;
    app.config_input_value.clear();
    app.screen = AppScreen::ConfigView;
    refresh_config_status(app);
}

fn config_actions(config: &crate::types::Config) -> Vec<ConfigAction> {
    let mut actions = vec![
        ConfigAction::Mirror,
        ConfigAction::Language,
        ConfigAction::ProxyEnabled,
    ];
    if config.proxy_enabled {
        actions.push(ConfigAction::ProxyType);
        actions.push(ConfigAction::ProxyAddress);
    }
    actions.extend([
        ConfigAction::ModelPatch,
        ConfigAction::CandidatePageSize,
        ConfigAction::EngineSync,
    ]);
    if config.engine_sync_enabled {
        actions.push(ConfigAction::SyncStrategy);
    }
    actions.push(ConfigAction::Refresh);
    actions
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

async fn build_config_status_snapshot(
    schema: Schema,
    lang: Lang,
    rime_dir: std::path::PathBuf,
) -> ConfigStatusSnapshot {
    let t = L10n::new(lang);
    let manager = match Manager::new() {
        Ok(manager) => manager,
        Err(_) => {
            return ConfigStatusSnapshot {
                scheme_status: t.t("update.failed").into(),
                dict_status: t.t("update.failed").into(),
                model_status: t.t("update.failed").into(),
                model_patch_status: t.t("update.failed").into(),
                candidate_page_size: t.t("update.failed").into(),
                installed_scheme_version: t.t("config.unknown").into(),
                installed_dict_version: t.t("config.unknown").into(),
                installed_model_version: t.t("config.unknown").into(),
            };
        }
    };

    let scheme_local = updater::BaseUpdater::load_record(&manager.scheme_record_path());
    let dict_local = updater::BaseUpdater::load_record(&manager.dict_record_path());
    let model_local = updater::BaseUpdater::load_record(&manager.model_record_path());
    let model_patch_applied = updater::model_patch::is_model_patched(&rime_dir, &schema, lang);

    let base = match updater::BaseUpdater::new(
        &manager.config,
        manager.cache_dir.clone(),
        manager.rime_dir.clone(),
    ) {
        Ok(base) => base,
        Err(_) => {
            return ConfigStatusSnapshot {
                scheme_status: local_status_text(&t, scheme_local.as_ref(), None),
                dict_status: local_status_text(&t, dict_local.as_ref(), None),
                model_status: local_status_text(&t, model_local.as_ref(), None),
                model_patch_status: format!(
                    "{} / {}",
                    if manager.config.model_patch_enabled {
                        t.t("config.enabled")
                    } else {
                        t.t("config.disabled")
                    },
                    if model_patch_applied {
                        t.t("patch.model.enabled")
                    } else {
                        t.t("patch.model.disabled")
                    }
                ),
                candidate_page_size: candidate_page_size_text(&rime_dir, schema, &t),
                installed_scheme_version: installed_version_text(&t, scheme_local.as_ref()),
                installed_dict_version: installed_dict_version_text(
                    schema,
                    &t,
                    dict_local.as_ref(),
                ),
                installed_model_version: installed_version_text(&t, model_local.as_ref()),
            };
        }
    };

    let scheme_remote = if schema.is_wanxiang() {
        updater::wanxiang::WanxiangUpdater { base }
            .check_scheme_update(&schema, None)
            .await
            .ok()
    } else if schema == Schema::Ice {
        updater::ice::IceUpdater { base }
            .check_scheme_update(None)
            .await
            .ok()
    } else if schema == Schema::Frost {
        updater::frost::FrostUpdater { base }
            .check_scheme_update(None)
            .await
            .ok()
    } else {
        updater::mint::MintUpdater { base }
            .check_scheme_update(None)
            .await
            .ok()
    };

    let base = updater::BaseUpdater::new(
        &manager.config,
        manager.cache_dir.clone(),
        manager.rime_dir.clone(),
    )
    .ok();
    let dict_remote = if let Some(base) = base {
        if schema.is_wanxiang() {
            updater::wanxiang::WanxiangUpdater { base }
                .check_dict_update(&schema, None)
                .await
                .ok()
        } else if schema == Schema::Ice {
            updater::ice::IceUpdater { base }
                .check_dict_update(None)
                .await
                .ok()
        } else {
            None
        }
    } else {
        None
    };

    let base = updater::BaseUpdater::new(
        &manager.config,
        manager.cache_dir.clone(),
        manager.rime_dir.clone(),
    )
    .ok();
    let model_remote = if let Some(base) = base {
        updater::wanxiang::WanxiangUpdater { base }
            .check_model_update(None)
            .await
            .ok()
    } else {
        None
    };

    ConfigStatusSnapshot {
        scheme_status: local_status_text(&t, scheme_local.as_ref(), scheme_remote.as_ref()),
        dict_status: if schema.dict_zip().is_none() {
            t.t("config.na").into()
        } else {
            local_status_text(&t, dict_local.as_ref(), dict_remote.as_ref())
        },
        model_status: local_status_text(&t, model_local.as_ref(), model_remote.as_ref()),
        model_patch_status: format!(
            "{} / {}",
            if manager.config.model_patch_enabled {
                t.t("config.enabled")
            } else {
                t.t("config.disabled")
            },
            if model_patch_applied {
                t.t("patch.model.enabled")
            } else {
                t.t("patch.model.disabled")
            }
        ),
        candidate_page_size: candidate_page_size_text(&rime_dir, schema, &t),
        installed_scheme_version: installed_version_text(&t, scheme_local.as_ref()),
        installed_dict_version: installed_dict_version_text(schema, &t, dict_local.as_ref()),
        installed_model_version: installed_version_text(&t, model_local.as_ref()),
    }
}

fn local_status_text(
    t: &L10n,
    local: Option<&crate::types::UpdateRecord>,
    remote: Option<&crate::types::UpdateInfo>,
) -> String {
    match (local, remote) {
        (None, _) => t.t("status.not_installed").into(),
        (Some(local), Some(remote)) => {
            if crate::updater::BaseUpdater::needs_update(Some(local), remote) {
                format!("{} -> {}", local.tag, remote.tag)
            } else {
                format!("{} ({})", local.tag, t.t("config.latest"))
            }
        }
        (Some(local), None) => format!("{} ({})", local.tag, t.t("config.unknown")),
    }
}

fn candidate_page_size_text(rime_dir: &std::path::Path, schema: Schema, t: &L10n) -> String {
    match crate::custom::candidate_page_size(rime_dir, schema) {
        Ok(Some(value)) => value.to_string(),
        Ok(None) => t.t("config.default_value").into(),
        Err(_) => t.t("config.unknown").into(),
    }
}

fn installed_version_text(t: &L10n, local: Option<&crate::types::UpdateRecord>) -> String {
    local
        .map(|record| record.tag.clone())
        .unwrap_or_else(|| t.t("status.not_installed").into())
}

fn installed_dict_version_text(
    schema: Schema,
    t: &L10n,
    local: Option<&crate::types::UpdateRecord>,
) -> String {
    if schema.dict_zip().is_none() {
        t.t("config.na").into()
    } else {
        installed_version_text(t, local)
    }
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

enum UpdateMode {
    All,
    Scheme,
    Dict,
    Model,
}

async fn start_update(app: &mut App, manager: &Manager, mode: UpdateMode) -> Result<()> {
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

    let context = match resolve_update_context(app, manager, &mode) {
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

    // 主布局: header + body + footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // header
            Constraint::Min(8),    // body
            Constraint::Length(3), // footer
        ])
        .split(size);

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " snout ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("v{}  ", env!("CARGO_PKG_VERSION")),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            app.schema.display_name_lang(app.t.lang()),
            Style::default().fg(Color::Green),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    )
    .alignment(Alignment::Center);
    f.render_widget(header, chunks[0]);

    // Body - 根据屏幕渲染
    match app.screen {
        AppScreen::Menu => render_menu(f, chunks[1], app),
        AppScreen::Updating => render_updating(f, chunks[1], app),
        AppScreen::Result => render_result(f, chunks[1], app),
        AppScreen::SchemeSelector => render_scheme_selector(f, chunks[1], app),
        AppScreen::SkinSelector => render_skin_selector(f, chunks[1], app),
        AppScreen::Fcitx5DarkThemeSelector => {
            render_fcitx5_theme_selector(f, chunks[1], app, Fcitx5ThemePhase::Dark)
        }
        AppScreen::Fcitx5LightThemeSelector => {
            render_fcitx5_theme_selector(f, chunks[1], app, Fcitx5ThemePhase::Light)
        }
        AppScreen::ConfigView => render_config(f, chunks[1], app),
        AppScreen::ConfigInput => render_config_input(f, chunks[1], app),
        AppScreen::SkinRoundPrompt => render_menu(f, chunks[1], app),
    }

    if let Some(notification) = &app.notification {
        render_notification_popup(f, size, &notification.message, app.t.t("hint.notice"));
    }

    if matches!(app.screen, AppScreen::SkinRoundPrompt) {
        render_skin_round_prompt(f, size, app);
    }

    // Footer
    let footer_text = vec![Span::styled(
        format!(" {}", app.current_hint()),
        Style::default().fg(Color::White),
    )];
    let footer =
        Paragraph::new(Line::from(footer_text)).block(Block::default().borders(Borders::TOP));
    f.render_widget(footer, chunks[2]);
}

fn render_notification_popup(f: &mut Frame, area: Rect, message: &str, title: &str) {
    let popup_width = (message.chars().count() as u16 + 8).clamp(24, area.width.saturating_sub(2));
    let popup_height = if area.height < 8 { 3 } else { 5 };
    let popup_area = centered_rect(popup_width, popup_height, area);
    f.render_widget(Clear, popup_area);
    let popup = Paragraph::new(Line::from(vec![Span::styled(
        message,
        Style::default().fg(Color::White),
    )]))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(Span::styled(
                format!(" {} ", title),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
    )
    .wrap(Wrap { trim: true });
    f.render_widget(popup, popup_area);
}

fn render_skin_round_prompt(f: &mut Frame, area: Rect, app: &App) {
    let Some(selection) = &app.pending_skin_selection else {
        return;
    };

    let yes_style = if app.skin_round_choice {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };
    let no_style = if !app.skin_round_choice {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Yellow)
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
            Style::default().fg(Color::White),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            app.t.t("skin.round_prompt_body"),
            Style::default().fg(Color::White),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("  {}  ", app.t.t("skin.round_on")), yes_style),
            Span::raw("  "),
            Span::styled(format!("  {}  ", app.t.t("skin.round_off")), no_style),
        ]),
    ];

    let popup = Paragraph::new(content).wrap(Wrap { trim: true }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(Span::styled(
                format!(" {} ", app.t.t("skin.round_prompt_title")),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
    );
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
                Style::default().fg(Color::Gray)
            } else if i == 5 || i == 6 {
                Style::default().fg(Color::Magenta)
            } else {
                Style::default().fg(Color::White)
            };
            let line = vec![
                Span::styled(format!("  {key}. "), Style::default().fg(Color::Cyan)),
                Span::styled(*label, style),
            ];
            ListItem::new(Line::from(line))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    format!(" {} ", app.t.t("menu.title")),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

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
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(reason),
        ]
    } else {
        let mut lines = vec![
            Line::from(vec![Span::styled(
                app.t.t("hint.confirm"),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                app.t.t("menu.action_ready"),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(menu_description(app, selected_idx)),
            Line::from(""),
            Line::from(vec![Span::styled(
                app.t.t("menu.installed_versions"),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(format!(
                "  {}: {}",
                app.t.t("config.scheme_status_label"),
                app.config_status.installed_scheme_version
            )),
            Line::from(format!(
                "  {}: {}",
                app.t.t("config.dict_status_label"),
                app.config_status.installed_dict_version
            )),
            Line::from(format!(
                "  {}: {}",
                app.t.t("config.model_status_label"),
                app.config_status.installed_model_version
            )),
            Line::from(""),
            Line::from(vec![Span::styled(
                app.t.t("menu.current_settings"),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(format!(
                "  {}: {}",
                app.t.t("config.candidate_page_size_label"),
                app.config_status.candidate_page_size
            )),
            Line::from(format!(
                "  {}: {}",
                app.t.t("config.model_patch_status_label"),
                app.config_status.model_patch_status
            )),
        ];
        if app.config_status_loading {
            lines.push(Line::from(""));
            lines.push(Line::from(app.t.t("config.loading")));
        }
        lines
    };
    let detail_panel = Paragraph::new(detail).wrap(Wrap { trim: true }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Span::styled(
                format!(" {} ", app.t.t("menu.result")),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
    );
    f.render_widget(detail_panel, chunks[1]);
}

fn detail_lines(message: &str) -> Vec<Line<'static>> {
    message
        .lines()
        .map(|line| {
            let style = if line.starts_with("⚠️") {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if line.trim_start().starts_with('•') {
                Style::default().fg(Color::Cyan)
            } else if line.trim_start().starts_with('-') || line.trim_start().starts_with("http") {
                Style::default().fg(Color::Gray)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(vec![Span::styled(line.to_string(), style)])
        })
        .collect()
}

fn render_updating(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(3),
        ])
        .split(area);

    let msg = Paragraph::new(Line::from(vec![
        Span::styled("  ⏳ ", Style::default().fg(Color::Yellow)),
        Span::styled(&app.update_msg, Style::default().fg(Color::White)),
    ]))
    .block(Block::default().borders(Borders::ALL).title(Span::styled(
        format!(" {} ", app.t.t("update.checking")),
        Style::default().fg(Color::Yellow),
    )));
    f.render_widget(msg, chunks[0]);

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", app.t.t("update.progress"))),
        )
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
        .ratio(app.update_pct)
        .label(format!("{:.0}%", app.update_pct * 100.0));
    f.render_widget(gauge, chunks[1]);

    let stage_lines = if app.update_stage_lines.is_empty() {
        vec![Line::from(vec![Span::styled(
            format!("  {}", app.t.t("hint.wait")),
            Style::default().fg(Color::Gray),
        )])]
    } else {
        app.update_stage_lines
            .iter()
            .map(|stage| {
                Line::from(vec![
                    Span::styled("  • ", Style::default().fg(Color::Gray)),
                    Span::styled(stage, Style::default().fg(Color::White)),
                ])
            })
            .collect()
    };
    let stage_panel = Paragraph::new(stage_lines).wrap(Wrap { trim: true }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Span::styled(
                format!(" {} ", app.t.t("update.status_section")),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
    );
    f.render_widget(stage_panel, chunks[2]);
}

fn render_result(f: &mut Frame, area: Rect, app: &App) {
    let title = if app.update_done {
        format!(" {} ", app.t.t("menu.done"))
    } else {
        format!(" {} ", app.t.t("menu.result"))
    };
    let (accent, status_color) = match app.update_outcome {
        Some(UpdateOutcome::Success) => (Color::Green, Color::Green),
        Some(UpdateOutcome::Partial) => (Color::Yellow, Color::Yellow),
        Some(UpdateOutcome::Failure) => (Color::Red, Color::Red),
        Some(UpdateOutcome::Cancelled) => (Color::DarkGray, Color::Yellow),
        None => (Color::Yellow, Color::Yellow),
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                &app.update_msg,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
    ];

    for r in &app.update_results {
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(r, Style::default().fg(Color::White)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        format!("  {}", app.t.t("result.back_to_menu")),
        Style::default().fg(Color::Gray),
    )]));

    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(accent))
                .title(Span::styled(&title, Style::default().fg(accent))),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
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
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::Green)
            };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Yellow)),
                Span::styled(s.display_name_lang(app.t.lang()), style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(Span::styled(
                    format!(
                        " {} {}/{} ",
                        app.t.t("scheme.select_prompt"),
                        window.current_index,
                        window.total_items.max(1)
                    ),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(
        app.scheme_selected
            .saturating_sub(window.start)
            .min(visible_schemas.len().saturating_sub(1)),
    ));
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
                Span::styled(name.as_str(), Style::default().fg(Color::White)),
                Span::styled(format!(" ({key})"), Style::default().fg(Color::Gray)),
            ];
            if installed_linux_themes.contains(key) {
                spans.push(Span::styled(
                    format!(" [{}]", app.t.t("skin.installed_marker")),
                    Style::default().fg(Color::Cyan),
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta))
                .title(Span::styled(
                    format!(" {} ", title),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(
        app.skin_selected
            .saturating_sub(window.start)
            .min(visible_skins.len().saturating_sub(1)),
    ));
    f.render_stateful_widget(list, area, &mut state);
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
                Span::styled(name.as_str(), Style::default().fg(Color::White)),
                Span::styled(format!(" ({key})"), Style::default().fg(Color::Gray)),
            ];
            if current.light.as_deref() == Some(key.as_str()) {
                spans.push(Span::styled(
                    format!(" [{}]", app.t.t("skin.current_light_marker")),
                    Style::default().fg(Color::Cyan),
                ));
            }
            if current.dark.as_deref() == Some(key.as_str()) {
                spans.push(Span::styled(
                    format!(" [{}]", app.t.t("skin.current_dark_marker")),
                    Style::default().fg(Color::Yellow),
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta))
                .title(Span::styled(
                    format!(
                        " {} {}/{} ",
                        app.t.t(title_key),
                        window.current_index,
                        window.total_items.max(1)
                    ),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(
        app.skin_selected
            .saturating_sub(window.start)
            .min(visible_skins.len().saturating_sub(1)),
    ));
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
    let summary_panel = Paragraph::new(summary).wrap(Wrap { trim: true }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Span::styled(
                format!(" {} ", app.t.t("menu.result")),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
    );
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

fn render_config(f: &mut Frame, area: Rect, app: &App) {
    let manager = Manager::new().ok();
    let engines = manager
        .as_ref()
        .map(|_| crate::config::detect_installed_engines().join(", "))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| app.t.t("config.none").into());
    let language = if app.t.lang() == Lang::Zh {
        app.t.t("config.lang.zh")
    } else {
        app.t.t("config.lang.en")
    };
    let config = manager.map(|m| m.config).unwrap_or_default();
    let effective_proxy = crate::api::effective_proxy(&config).ok().flatten();
    let env_proxy_active = matches!(
        effective_proxy.as_ref().map(|proxy| proxy.source),
        Some(crate::api::ProxySource::Environment)
    );
    let selected_style = |selected: bool| {
        if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        }
    };
    let actions = config_actions(&config);
    let action_line = |index: usize, label: String, value: String| -> Line {
        let selected = app.config_selected == index;
        Line::from(vec![
            Span::styled(
                format!("{} ", if selected { "▸" } else { " " }),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(label, selected_style(selected)),
            Span::styled(value, Style::default().fg(Color::White)),
        ])
    };
    let chunks = if area.width < 110 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
            .split(area)
    };

    let left_lines = vec![
        Line::from(vec![Span::styled(
            format!("  {}:", app.t.t("config.features_section")),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        action_line(
            actions
                .iter()
                .position(|action| *action == ConfigAction::Mirror)
                .unwrap_or(0),
            format!("{}: ", app.t.t("config.mirror_label")),
            if config.use_mirror {
                app.t.t("config.enabled").into()
            } else {
                app.t.t("config.disabled").into()
            },
        ),
        action_line(
            actions
                .iter()
                .position(|action| *action == ConfigAction::Language)
                .unwrap_or(1),
            format!("{}: ", app.t.t("config.language_label")),
            language.to_string(),
        ),
        action_line(
            actions
                .iter()
                .position(|action| *action == ConfigAction::ProxyEnabled)
                .unwrap_or(2),
            format!("{}: ", app.t.t("config.proxy_label")),
            if config.proxy_enabled || effective_proxy.is_some() {
                app.t.t("config.enabled").into()
            } else {
                app.t.t("config.disabled").into()
            },
        ),
        if config.proxy_enabled {
            action_line(
                actions
                    .iter()
                    .position(|action| *action == ConfigAction::ProxyType)
                    .unwrap_or(3),
                format!("{}: ", app.t.t("config.proxy_type_label")),
                if config.proxy_type == "http" {
                    app.t.t("config.proxy_type_http").into()
                } else {
                    app.t.t("config.proxy_type_socks5").into()
                },
            )
        } else {
            Line::from("")
        },
        if config.proxy_enabled {
            action_line(
                actions
                    .iter()
                    .position(|action| *action == ConfigAction::ProxyAddress)
                    .unwrap_or(4),
                format!("{}: ", app.t.t("config.proxy_address_label")),
                if config.proxy_address.trim().is_empty() {
                    app.t.t("config.none").into()
                } else {
                    config.proxy_address.clone()
                },
            )
        } else {
            Line::from("")
        },
        if env_proxy_active {
            Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(
                    app.t.t("config.proxy_env_readonly"),
                    Style::default().fg(Color::Gray),
                ),
            ])
        } else {
            Line::from("")
        },
        action_line(
            actions
                .iter()
                .position(|action| *action == ConfigAction::ModelPatch)
                .unwrap_or(5),
            format!("{}: ", app.t.t("config.model_patch_label")),
            if config.model_patch_enabled {
                app.t.t("config.enabled").into()
            } else {
                app.t.t("config.disabled").into()
            },
        ),
        action_line(
            actions
                .iter()
                .position(|action| *action == ConfigAction::CandidatePageSize)
                .unwrap_or(6),
            format!("{}: ", app.t.t("config.candidate_page_size_label")),
            if app.config_status_loading {
                app.t.t("config.loading").into()
            } else {
                app.config_status.candidate_page_size.clone()
            },
        ),
        action_line(
            actions
                .iter()
                .position(|action| *action == ConfigAction::EngineSync)
                .unwrap_or(7),
            format!("{}: ", app.t.t("config.engine_sync_label")),
            if config.engine_sync_enabled {
                app.t.t("config.enabled").into()
            } else {
                app.t.t("config.disabled").into()
            },
        ),
        action_line(
            actions
                .iter()
                .position(|action| *action == ConfigAction::SyncStrategy)
                .unwrap_or(8),
            format!("{}: ", app.t.t("config.sync_strategy_label")),
            if config.engine_sync_use_link {
                app.t.t("config.sync_link").into()
            } else {
                app.t.t("config.sync_copy").into()
            },
        ),
        Line::from(""),
        action_line(
            actions
                .iter()
                .position(|action| *action == ConfigAction::Refresh)
                .unwrap_or(9),
            format!("{}: ", app.t.t("hint.refresh")),
            app.t.t("hint.confirm").into(),
        ),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("  {}", app.t.t("config.back")),
            Style::default().fg(Color::Gray),
        )]),
    ];

    let right_lines = vec![
        Line::from(vec![Span::styled(
            format!("  {}:", app.t.t("config.runtime_section")),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::styled(
                format!("  {}: ", app.t.t("config.current_scheme")),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(
                app.schema.display_name_lang(app.t.lang()),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {}: ", app.t.t("config.detected_engines")),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(engines, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {}: ", app.t.t("config.proxy_source_label")),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(
                match effective_proxy.as_ref().map(|proxy| proxy.source) {
                    Some(crate::api::ProxySource::Config) => app.t.t("config.proxy_source_config"),
                    Some(crate::api::ProxySource::Environment) => {
                        app.t.t("config.proxy_source_env")
                    }
                    None => app.t.t("config.none"),
                },
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {}: ", app.t.t("config.proxy_value_label")),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(
                effective_proxy
                    .as_ref()
                    .map(|proxy| proxy.url.as_str())
                    .unwrap_or_else(|| app.t.t("config.none")),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("  {}:", app.t.t("config.status_section")),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::styled(
                format!("  {}: ", app.t.t("config.scheme_status_label")),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(
                if app.config_status_loading {
                    app.t.t("config.loading")
                } else {
                    &app.config_status.scheme_status
                },
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {}: ", app.t.t("config.dict_status_label")),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(
                if app.config_status_loading {
                    app.t.t("config.loading")
                } else {
                    &app.config_status.dict_status
                },
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {}: ", app.t.t("config.model_status_label")),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(
                if app.config_status_loading {
                    app.t.t("config.loading")
                } else {
                    &app.config_status.model_status
                },
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {}: ", app.t.t("config.model_patch_status_label")),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(
                if app.config_status_loading {
                    app.t.t("config.loading")
                } else {
                    &app.config_status.model_patch_status
                },
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {}: ", app.t.t("config.candidate_page_size_label")),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(
                if app.config_status_loading {
                    app.t.t("config.loading")
                } else {
                    &app.config_status.candidate_page_size
                },
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("  {}:", app.t.t("config.paths_section")),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::styled(
                format!("  {}: ", app.t.t("config.rime_dir")),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(&app.rime_dir, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {}: ", app.t.t("config.config_file")),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(&app.config_path, Style::default().fg(Color::White)),
        ]),
    ];

    let left = Paragraph::new(left_lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue))
            .title(Span::styled(
                format!(" {} ", app.t.t("config.title")),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
    );
    let right = Paragraph::new(right_lines)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    format!(" {} ", app.t.t("menu.result")),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )),
        );
    f.render_widget(left, chunks[0]);
    f.render_widget(right, chunks[1]);
}

fn render_config_input(f: &mut Frame, area: Rect, app: &App) {
    let title = match app.config_input_field {
        Some(ConfigInputField::ProxyAddress) => app.t.t("config.proxy_address_label"),
        Some(ConfigInputField::CandidatePageSize) => app.t.t("config.candidate_page_size_label"),
        None => app.t.t("config.title"),
    };
    let hint = match app.config_input_field {
        Some(ConfigInputField::CandidatePageSize) => app.t.t("config.input_hint_page_size"),
        _ => app.t.t("config.input_hint"),
    };
    let text = vec![
        Line::from(vec![Span::styled(
            format!("{}:", title),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(hint, Style::default().fg(Color::Gray))]),
        Line::from(""),
        Line::from(vec![Span::styled(
            if app.config_input_value.is_empty() {
                app.t.t("config.input_placeholder")
            } else {
                &app.config_input_value
            },
            Style::default().fg(Color::White),
        )]),
    ];

    let p = Paragraph::new(text).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(Span::styled(
                format!(" {} ", app.t.t("config.edit_title")),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
    );
    f.render_widget(p, area);
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

        handle_menu_key(&mut app, KeyCode::Enter, &manager)
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
