use crate::config::Manager;
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

// ── 菜单项 ──
const MENU_ITEMS: &[(&str, &str)] = &[
    ("1", "一键更新"),
    ("2", "更新方案"),
    ("3", "更新词库"),
    ("4", "更新模型"),
    ("5", "模型 Patch"),
    ("6", "皮肤 Patch"),
    ("7", "切换方案"),
    ("8", "配置"),
    ("Q", "退出"),
];

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
    // 更新状态
    pub update_msg: String,
    pub update_pct: f64,
    pub update_done: bool,
    pub update_results: Vec<String>,
    // 通知
    pub notification: Option<(String, Instant)>,
}

impl App {
    pub fn new(manager: &Manager) -> Self {
        Self {
            should_quit: false,
            screen: AppScreen::Menu,
            selected: 0,
            schema: manager.config.schema,
            rime_dir: manager.rime_dir.display().to_string(),
            config_path: manager.config_path.display().to_string(),
            update_msg: String::new(),
            update_pct: 0.0,
            update_done: false,
            update_results: Vec::new(),
            notification: None,
        }
    }

    pub fn notify(&mut self, msg: impl Into<String>) {
        self.notification = Some((msg.into(), Instant::now()));
    }
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
            if app.selected < MENU_ITEMS.len() - 1 {
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
                                app.update_results.push("✅ 模型 patch 已移除".into());
                            }
                        } else {
                            if let Err(e) = updater::model_patch::patch_model(
                                std::path::Path::new(&app.rime_dir),
                                &app.schema,
                            ) {
                                app.update_results.push(format!("❌ {e}"));
                            } else {
                                app.update_results.push("✅ 模型 patch 已启用".into());
                            }
                        }
                    } else {
                        app.update_results.push("此方案不支持模型 patch".into());
                    }
                    app.update_msg = "模型 Patch".into();
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
                app.notify(format!("方案已切换: {}", s.display_name()));
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
                // 尝试 weasel.custom.yaml 或 squirrel.custom.yaml
                let patch = rime_dir.join("weasel.custom.yaml");
                let patch = if !patch.exists() {
                    rime_dir.join("squirrel.custom.yaml")
                } else {
                    patch
                };
                if let Err(e) = crate::skin::patch::write_skin_presets(&patch, &[key.as_str()]) {
                    app.notify(format!("❌ {e}"));
                } else if let Err(e) = crate::skin::patch::set_default_skin(&patch, key) {
                    app.notify(format!("❌ {e}"));
                } else {
                    app.notify(format!("✅ 皮肤已设置: {name}"));
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
    app.update_msg = "准备中...".into();
    app.update_pct = 0.0;
    app.update_done = false;
    app.update_results.clear();

    let schema = app.schema;
    let cache_dir = manager.cache_dir.clone();
    let rime_dir = manager.rime_dir.clone();
    let config = manager.config.clone();

    // 使用 channel 报告进度 (简化版：直接在终端显示)
    // 实际更新在后台执行，这里用 tokio::spawn

    let results = match mode {
        UpdateMode::All => {
            updater::update_all(&schema, &config, cache_dir, rime_dir, |_msg, _pct| {
                // 注意: 在真实 TUI 中这里应该用 channel 更新 app 状态
                // 简化版直接打印到 stdout (会被 TUI 覆盖)
            })
            .await
        }
        UpdateMode::Scheme => {
            let base = updater::BaseUpdater::new(&config, cache_dir, rime_dir).unwrap();
            let scheme = updater::SchemeUpdater { base };
            scheme
                .run(&schema, &config, |_, _| {})
                .await
                .map(|r| vec![r])
        }
        UpdateMode::Dict => {
            let base = updater::BaseUpdater::new(&config, cache_dir, rime_dir).unwrap();
            let dict = updater::DictUpdater { base };
            dict.run(&schema, &config, |_, _| {}).await.map(|r| vec![r])
        }
        UpdateMode::Model => {
            let base = updater::BaseUpdater::new(&config, cache_dir, rime_dir.clone()).unwrap();
            let model = updater::ModelUpdater { base };
            let r = model.run(&config, |_, _| {}).await?;
            let mut v = vec![r];
            if config.model_patch_enabled && schema.supports_model_patch() {
                if let Err(e) = updater::model_patch::patch_model(&rime_dir, &schema) {
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
                        new_version: "已启用".into(),
                        success: true,
                        message: "patch 成功".into(),
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
            app.update_msg = "更新完成".into();
        }
        Err(e) => {
            app.update_results.push(format!("❌ 错误: {e}"));
            app.update_msg = "更新失败".into();
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
            Span::styled(" 导航", Style::default().fg(Color::White)),
            Span::styled("  Enter", Style::default().fg(Color::DarkGray)),
            Span::styled(" 确认", Style::default().fg(Color::White)),
            Span::styled("  q/Esc", Style::default().fg(Color::DarkGray)),
            Span::styled(" 返回/退出", Style::default().fg(Color::White)),
        ]
    };
    let footer =
        Paragraph::new(Line::from(footer_text)).block(Block::default().borders(Borders::TOP));
    f.render_widget(footer, chunks[2]);
}

fn render_menu(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = MENU_ITEMS
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
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(" 更新中 ", Style::default().fg(Color::Yellow))),
    );
    f.render_widget(msg, chunks[0]);

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" 进度 "))
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
