use crate::config::Manager;
use crate::i18n::{L10n, Lang};
use crate::types::Schema;
use crate::updater;
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
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::time::{Duration, Instant};

// ── 应用状态 ──
pub enum AppScreen {
    Menu,
    Updating,
    Result,
    SchemeSelector,
    SkinSelector,
    ConfigView,
}

pub struct App {
    pub should_quit: bool,
    pub screen: AppScreen,
    pub selected: usize,
    pub schema: Schema,
    pub rime_dir: String,
    pub config_path: String,
    pub t: L10n,
    // 更新状态
    pub update_msg: String,
    pub update_pct: f64,
    pub update_done: bool,
    pub update_results: Vec<String>,
    // 通知
    pub notification: Option<(String, Instant)>,
}

#[derive(Clone)]
struct ResolvedUpdateContext {
    schema: Schema,
    config: crate::types::Config,
    cache_dir: std::path::PathBuf,
    rime_dir: std::path::PathBuf,
}

impl App {
    pub fn new(manager: &Manager) -> Self {
        let lang = Lang::from_str(&manager.config.language);
        Self {
            should_quit: false,
            screen: AppScreen::Menu,
            selected: 0,
            schema: manager.config.schema,
            rime_dir: manager.rime_dir.display().to_string(),
            config_path: manager.config_path.display().to_string(),
            t: L10n::new(lang),
            update_msg: String::new(),
            update_pct: 0.0,
            update_done: false,
            update_results: Vec::new(),
            notification: None,
        }
    }

    /// 动态菜单项 (i18n)
    pub fn menu_items(&self) -> Vec<(&str, &str)> {
        vec![
            ("1", self.t.t("menu.update_all")),
            ("2", self.t.t("menu.update_scheme")),
            ("3", self.t.t("menu.update_dict")),
            ("4", self.t.t("menu.update_model")),
            ("5", self.t.t("menu.model_patch")),
            ("6", self.t.t("menu.skin_patch")),
            ("7", self.t.t("menu.switch_scheme")),
            ("8", self.t.t("menu.config")),
            ("Q", self.t.t("menu.quit")),
        ]
    }

    pub fn notify(&mut self, msg: impl Into<String>) {
        self.notification = Some((msg.into(), Instant::now()));
    }
}

fn model_update_supported(schema: Schema) -> bool {
    schema.supports_model_patch()
}

fn skin_patch_target(rime_dir: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
    if cfg!(target_os = "windows") {
        Ok(rime_dir.join("weasel.custom.yaml"))
    } else if cfg!(target_os = "macos") {
        Ok(rime_dir.join("squirrel.custom.yaml"))
    } else {
        anyhow::bail!("当前平台不支持皮肤 Patch")
    }
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

    result
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    manager: &Manager,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match app.screen {
                    AppScreen::Menu => handle_menu_key(app, key.code, manager).await?,
                    AppScreen::Updating => {} // 更新中不响应按键
                    AppScreen::Result => handle_result_key(app, key.code),
                    AppScreen::SchemeSelector => handle_scheme_key(app, key.code, manager)?,
                    AppScreen::SkinSelector => handle_skin_key(app, key.code, manager)?,
                    AppScreen::ConfigView => handle_config_key(app, key.code),
                }
            }
        }

        // 清除过期通知
        if let Some((_, t)) = &app.notification {
            if t.elapsed() > Duration::from_secs(3) {
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
            app.selected = app.selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.selected < app.menu_items().len() - 1 {
                app.selected += 1;
            }
        }
        KeyCode::Enter | KeyCode::Char('1'..='8') => {
            let idx = match key {
                KeyCode::Char(c) => c.to_digit(10).unwrap_or(0) as usize,
                _ => app.selected + 1,
            };
            match idx {
                1 => start_update(app, manager, UpdateMode::All).await?,
                2 => start_update(app, manager, UpdateMode::Scheme).await?,
                3 => start_update(app, manager, UpdateMode::Dict).await?,
                4 => start_update(app, manager, UpdateMode::Model).await?,
                5 => {
                    // Model patch toggle
                    app.screen = AppScreen::Result;
                    app.update_results.clear();
                    if app.schema.supports_model_patch() {
                        let patched = updater::model_patch::is_model_patched(
                            std::path::Path::new(&app.rime_dir),
                            &app.schema,
                        );
                        if patched {
                            if let Err(e) = updater::model_patch::unpatch_model(
                                std::path::Path::new(&app.rime_dir),
                                &app.schema,
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
                }
                6 => app.screen = AppScreen::SkinSelector,
                7 => app.screen = AppScreen::SchemeSelector,
                8 => app.screen = AppScreen::ConfigView,
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
            app.update_pct = 0.0;
        }
        _ => {}
    }
}

fn handle_scheme_key(app: &mut App, key: KeyCode, _manager: &Manager) -> Result<()> {
    let schemas = Schema::all();
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            app.selected = app.selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.selected < schemas.len() - 1 {
                app.selected += 1;
            }
        }
        KeyCode::Enter => {
            if let Some(s) = schemas.get(app.selected) {
                app.schema = *s;
                let mut m = Manager::new()?;
                m.config.schema = *s;
                m.save()?;
                app.notify(format!(
                    "{}: {}",
                    app.t.t("scheme.switched"),
                    s.display_name()
                ));
            }
            app.screen = AppScreen::Menu;
            app.selected = 0;
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.screen = AppScreen::Menu;
            app.selected = 0;
        }
        _ => {}
    }
    Ok(())
}

fn handle_skin_key(app: &mut App, key: KeyCode, _manager: &Manager) -> Result<()> {
    let skins = crate::skin::list_available_skins();
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            app.selected = app.selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.selected < skins.len() - 1 {
                app.selected += 1;
            }
        }
        KeyCode::Enter => {
            if let Some((key, name)) = skins.get(app.selected) {
                let rime_dir = std::path::Path::new(&app.rime_dir);
                match skin_patch_target(rime_dir) {
                    Ok(patch) => {
                        if let Err(e) =
                            crate::skin::patch::write_skin_presets(&patch, &[key.as_str()])
                        {
                            app.notify(format!("❌ {e}"));
                        } else if let Err(e) = crate::skin::patch::set_default_skin(&patch, key) {
                            app.notify(format!("❌ {e}"));
                        } else {
                            app.notify(format!("✅ {}: {name}", app.t.t("skin.applied")));
                        }
                    }
                    Err(_) => {
                        let msg = app.t.t("skin.not_supported").to_string();
                        app.notify(msg);
                    }
                }
            }
            app.screen = AppScreen::Menu;
            app.selected = 0;
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.screen = AppScreen::Menu;
            app.selected = 0;
        }
        _ => {}
    }
    Ok(())
}

fn handle_config_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
            app.screen = AppScreen::Menu;
        }
        _ => {}
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
    app.update_results.clear();

    let context = match resolve_update_context(app, manager, &mode) {
        Ok(context) => context,
        Err(e) => {
            app.update_results.push(format!("❌ {}", e));
            app.update_msg = app.t.t("update.failed").into();
            app.update_pct = 1.0;
            app.update_done = true;
            app.screen = AppScreen::Result;
            return Ok(());
        }
    };

    // 使用 channel 报告进度 (简化版：直接在终端显示)
    // 实际更新在后台执行，这里用 tokio::spawn

    let results = match mode {
        UpdateMode::All => {
            updater::update_all(
                &context.schema,
                &context.config,
                context.cache_dir,
                context.rime_dir,
                |_msg, _pct| {},
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
                    .update_scheme(&context.schema, &context.config, |_, _| {})
                    .await
                    .map(|r| vec![r])
            } else if context.schema == Schema::Ice {
                updater::ice::IceUpdater { base }
                    .update_scheme(&context.config, |_, _| {})
                    .await
                    .map(|r| vec![r])
            } else {
                updater::frost::FrostUpdater { base }
                    .update_scheme(&context.config, |_, _| {})
                    .await
                    .map(|r| vec![r])
            }
        }
        UpdateMode::Dict => {
            if context.schema.dict_zip().is_none() {
                Ok(vec![updater::UpdateResult {
                    component: "词库".into(),
                    old_version: "-".into(),
                    new_version: "-".into(),
                    success: false,
                    message: app.t.t("update.no_dict").into(),
                }])
            } else {
                let base = updater::BaseUpdater::new(
                    &context.config,
                    context.cache_dir,
                    context.rime_dir,
                )?;
                if context.schema.is_wanxiang() {
                    updater::wanxiang::WanxiangUpdater { base }
                        .update_dict(&context.schema, &context.config, |_, _| {})
                        .await
                        .map(|r| vec![r])
                } else {
                    updater::ice::IceUpdater { base }
                        .update_dict(&context.config, |_, _| {})
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
            let r = wx.update_model(&context.config, |_, _| {}).await?;
            let mut v = vec![r];
            if context.config.model_patch_enabled && context.schema.supports_model_patch() {
                if let Err(e) =
                    updater::model_patch::patch_model(&context.rime_dir, &context.schema)
                {
                    v.push(updater::UpdateResult {
                        component: "模型patch".into(),
                        old_version: "?".into(),
                        new_version: "?".into(),
                        success: false,
                        message: e.to_string(),
                    });
                } else {
                    v.push(updater::UpdateResult {
                        component: "模型patch".into(),
                        old_version: "-".into(),
                        new_version: app.t.t("patch.model.enabled").into(),
                        success: true,
                        message: app.t.t("patch.model.enabled").into(),
                    });
                }
            }
            Ok(v)
        }
    };

    match results {
        Ok(rs) => {
            for r in &rs {
                let icon = if r.success { "✅" } else { "❌" };
                app.update_results
                    .push(format!("{icon} {} - {}", r.component, r.message));
            }
            app.update_msg = app.t.t("update.complete").into();
        }
        Err(e) => {
            app.update_results.push(format!("❌ 错误: {e}"));
            app.update_msg = app.t.t("update.failed").into();
        }
    }

    app.update_pct = 1.0;
    app.update_done = true;
    app.screen = AppScreen::Result;
    Ok(())
}

// ── 渲染 ──

fn ui(f: &mut Frame, app: &App) {
    let size = f.area();

    // 主布局: header + body + footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // header
            Constraint::Min(8),    // body
            Constraint::Length(3), // footer
        ])
        .split(size);

    // Header
    let header_text = vec![
        Line::from(vec![Span::styled(
            "╔══════════════════════════════════════╗",
            Style::default().fg(Color::Cyan),
        )]),
        Line::from(vec![
            Span::styled("║  ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "snout",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" v0.1.0  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {}", app.schema.display_name()),
                Style::default().fg(Color::Green),
            ),
            Span::styled("  ║", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![Span::styled(
            "╚══════════════════════════════════════╝",
            Style::default().fg(Color::Cyan),
        )]),
    ];
    let header = Paragraph::new(header_text).alignment(Alignment::Center);
    f.render_widget(header, chunks[0]);

    // Body - 根据屏幕渲染
    match app.screen {
        AppScreen::Menu => render_menu(f, chunks[1], app),
        AppScreen::Updating => render_updating(f, chunks[1], app),
        AppScreen::Result => render_result(f, chunks[1], app),
        AppScreen::SchemeSelector => render_scheme_selector(f, chunks[1], app),
        AppScreen::SkinSelector => render_skin_selector(f, chunks[1], app),
        AppScreen::ConfigView => render_config(f, chunks[1], app),
    }

    // Footer / 通知
    let footer_text = if let Some((msg, _)) = &app.notification {
        vec![Span::styled(
            format!(" 💡 {msg}"),
            Style::default().fg(Color::Yellow),
        )]
    } else {
        vec![
            Span::styled(" ↑↓/jk", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {}", app.t.t("hint.navigate")),
                Style::default().fg(Color::White),
            ),
            Span::styled("  Enter", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {}", app.t.t("hint.confirm")),
                Style::default().fg(Color::White),
            ),
            Span::styled("  q/Esc", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {}", app.t.t("hint.back")),
                Style::default().fg(Color::White),
            ),
        ]
    };
    let footer =
        Paragraph::new(Line::from(footer_text)).block(Block::default().borders(Borders::TOP));
    f.render_widget(footer, chunks[2]);
}

fn render_menu(f: &mut Frame, area: Rect, app: &App) {
    let menu_items = app.menu_items();
    let items: Vec<ListItem> = menu_items
        .iter()
        .enumerate()
        .map(|(i, (key, label))| {
            let style = if i == 5 || i == 6 {
                Style::default().fg(Color::Magenta)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {key}. "), Style::default().fg(Color::Cyan)),
                Span::styled(*label, style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    " 主菜单 ",
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
    state.select(Some(app.selected));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_updating(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
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
}

fn render_result(f: &mut Frame, area: Rect, app: &App) {
    let title = if app.update_done {
        " 完成 "
    } else {
        " 结果 "
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                &app.update_msg,
                Style::default()
                    .fg(Color::Green)
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
        "  按 Enter 返回主菜单",
        Style::default().fg(Color::DarkGray),
    )]));

    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green))
                .title(Span::styled(title, Style::default().fg(Color::Green))),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn render_scheme_selector(f: &mut Frame, area: Rect, app: &App) {
    let schemas = Schema::all();
    let items: Vec<ListItem> = schemas
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
                Span::styled(s.display_name(), style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(Span::styled(
                    " 选择方案 (Enter确认/Esc返回) ",
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
    state.select(Some(app.selected.min(schemas.len() - 1)));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_skin_selector(f: &mut Frame, area: Rect, app: &App) {
    let skins = crate::skin::list_available_skins();
    let items: Vec<ListItem> = skins
        .iter()
        .map(|(key, name)| {
            ListItem::new(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(name.as_str(), Style::default().fg(Color::White)),
                Span::styled(format!(" ({key})"), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta))
                .title(Span::styled(
                    " 选择皮肤 (Enter确认/Esc返回) ",
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
    state.select(Some(app.selected.min(skins.len().saturating_sub(1))));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_config(f: &mut Frame, area: Rect, app: &App) {
    let lines = vec![
        Line::from(vec![
            Span::styled("  当前方案: ", Style::default().fg(Color::DarkGray)),
            Span::styled(app.schema.display_name(), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("  Rime 目录: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&app.rime_dir, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  配置文件: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&app.config_path, Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  支持方案:",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            "  • 万象拼音: amzxyz/rime_wanxiang",
            Style::default().fg(Color::White),
        )]),
        Line::from(vec![Span::styled(
            "  • 雾凇拼音: iDvel/rime-ice",
            Style::default().fg(Color::White),
        )]),
        Line::from(vec![Span::styled(
            "  • 白霜拼音: gaboolic/rime-frost",
            Style::default().fg(Color::White),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  按 Esc 返回",
            Style::default().fg(Color::DarkGray),
        )]),
    ];

    let p = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue))
            .title(Span::styled(
                " 配置信息 ",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
    );
    f.render_widget(p, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_update_supported_for_all_supported_schemas() {
        assert!(model_update_supported(Schema::WanxiangBase));
        assert!(model_update_supported(Schema::WanxiangMoqi));
        assert!(model_update_supported(Schema::Ice));
        assert!(model_update_supported(Schema::Frost));
    }

    #[test]
    fn skin_patch_target_matches_platform_convention() {
        let base = std::path::Path::new("/tmp/rime");
        #[cfg(target_os = "windows")]
        assert_eq!(
            skin_patch_target(base).unwrap(),
            base.join("weasel.custom.yaml")
        );

        #[cfg(target_os = "macos")]
        assert_eq!(
            skin_patch_target(base).unwrap(),
            base.join("squirrel.custom.yaml")
        );

        #[cfg(target_os = "linux")]
        assert!(skin_patch_target(base).is_err());
    }
}
