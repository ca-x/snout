use crate::config::Manager;
use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders},
};
use std::env;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

static QUERIED_THEME: OnceLock<TerminalTheme> = OnceLock::new();

// 终端主题判定在一次进程生命周期内是常量：探测在 Windows 上会 shell out 到 reg
// 并读配置，只应做一次，避免每个取色调用都重建 Manager。
const THEME_UNRESOLVED: u8 = 0;
const THEME_DARK: u8 = 1;
const THEME_LIGHT: u8 = 2;
static RESOLVED_THEME: AtomicU8 = AtomicU8::new(THEME_UNRESOLVED);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalTheme {
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UiPalette {
    pub(crate) primary: Color,
    pub(crate) secondary: Color,
    pub(crate) tertiary: Color,
    pub(crate) border: Color,
    pub(crate) accent: Color,
    pub(crate) selection_bg: Color,
    pub(crate) success: Color,
    pub(crate) warning: Color,
    pub(crate) danger: Color,
}

impl UiPalette {
    pub(crate) fn for_theme(theme: TerminalTheme) -> Self {
        match theme {
            TerminalTheme::Dark => Self {
                primary: Color::Rgb(234, 234, 234),
                secondary: Color::Rgb(168, 168, 168),
                tertiary: Color::Rgb(98, 98, 98),
                border: Color::Rgb(84, 84, 84),
                accent: Color::Rgb(224, 163, 46),
                selection_bg: Color::Rgb(38, 38, 38),
                success: Color::Rgb(74, 246, 38),
                warning: Color::Rgb(224, 163, 46),
                danger: Color::Rgb(255, 42, 42),
            },
            TerminalTheme::Light => Self {
                primary: Color::Rgb(5, 5, 5),
                secondary: Color::Rgb(58, 58, 60),
                tertiary: Color::Rgb(110, 110, 115),
                border: Color::Rgb(186, 181, 174),
                accent: Color::Rgb(176, 110, 12),
                selection_bg: Color::Rgb(231, 228, 221),
                success: Color::Rgb(36, 138, 61),
                warning: Color::Rgb(176, 110, 12),
                danger: Color::Rgb(192, 53, 43),
            },
        }
    }
}

pub(crate) fn framed_title(title: &str) -> String {
    format!("[ {title} ]")
}

pub(crate) fn selector_highlight_symbol() -> &'static str {
    ">>> "
}

pub(crate) fn selector_prefix(selected: bool) -> &'static str {
    if selected {
        ">>> "
    } else {
        " ·  "
    }
}

pub(crate) fn color_primary() -> Color {
    palette().primary
}

pub(crate) fn color_secondary() -> Color {
    palette().secondary
}

pub(crate) fn color_tertiary() -> Color {
    palette().tertiary
}

pub(crate) fn color_border() -> Color {
    palette().border
}

pub(crate) fn color_accent() -> Color {
    palette().accent
}

pub(crate) fn color_selection_bg() -> Color {
    palette().selection_bg
}

pub(crate) fn color_success() -> Color {
    palette().success
}

pub(crate) fn color_warning() -> Color {
    palette().warning
}

pub(crate) fn color_danger() -> Color {
    palette().danger
}

pub(crate) fn primary_text() -> Style {
    Style::default().fg(color_primary())
}

pub(crate) fn secondary_text() -> Style {
    Style::default().fg(color_secondary())
}

pub(crate) fn tertiary_text() -> Style {
    Style::default().fg(color_tertiary())
}

pub(crate) fn accent_text() -> Style {
    Style::default().fg(color_accent())
}

pub(crate) fn section_title_text() -> Style {
    Style::default()
        .fg(color_secondary())
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn selection_style() -> Style {
    Style::default()
        .bg(color_selection_bg())
        .fg(contrast_color(color_selection_bg()))
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn panel_block(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color_border()))
        .title(Span::styled(framed_title(title), section_title_text()))
}

fn palette() -> UiPalette {
    UiPalette::for_theme(detect_terminal_theme())
}

fn detect_terminal_theme() -> TerminalTheme {
    match RESOLVED_THEME.load(Ordering::Relaxed) {
        THEME_DARK => return TerminalTheme::Dark,
        THEME_LIGHT => return TerminalTheme::Light,
        _ => {}
    }

    let theme = resolve_terminal_theme();
    RESOLVED_THEME.store(
        match theme {
            TerminalTheme::Dark => THEME_DARK,
            TerminalTheme::Light => THEME_LIGHT,
        },
        Ordering::Relaxed,
    );
    theme
}

fn resolve_terminal_theme() -> TerminalTheme {
    let configured_mode = Manager::new()
        .ok()
        .and_then(|manager| parse_theme_override(Some(&manager.config.tui_theme_mode)));
    parse_theme_override(env::var("SNOUT_TUI_THEME").ok().as_deref())
        .or(configured_mode)
        .or_else(|| QUERIED_THEME.get().copied())
        .or_else(|| {
            detect_terminal_theme_from_env_values(None, env::var("COLORFGBG").ok().as_deref())
        })
        .unwrap_or(TerminalTheme::Dark)
}

/// TUI 内切换主题（light/dark/auto）后调用，让下一次取色重新判定。
pub(crate) fn reset_terminal_theme_cache() {
    RESOLVED_THEME.store(THEME_UNRESOLVED, Ordering::Relaxed);
}

pub(crate) fn query_terminal_background() {
    #[cfg(unix)]
    if let Some(theme) = query_osc_background() {
        let _ = QUERIED_THEME.set(theme);
        reset_terminal_theme_cache();
    }
}

#[cfg(unix)]
fn query_osc_background() -> Option<TerminalTheme> {
    use std::io::{self, Write};
    use std::os::fd::AsRawFd;
    use std::time::{Duration, Instant};

    let stdin = io::stdin();
    let fd = stdin.as_raw_fd();
    if unsafe { libc::isatty(fd) } != 1 {
        return None;
    }
    let mut stdout = io::stdout();
    stdout.write_all(b"\x1b]11;?\x07").ok()?;
    stdout.flush().ok()?;

    let deadline = Instant::now() + Duration::from_millis(150);
    let mut response = Vec::with_capacity(64);
    while response.len() < 128 {
        let timeout = deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as i32;
        if timeout == 0 {
            break;
        }
        let mut pollfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&mut pollfd, 1, timeout) } <= 0 {
            break;
        }
        let mut byte = 0u8;
        if unsafe { libc::read(fd, (&mut byte as *mut u8).cast(), 1) } != 1 {
            break;
        }
        response.push(byte);
        if byte == b'\x07' || response.ends_with(b"\x1b\\") {
            if let Some(theme) = parse_osc_background(&response) {
                return Some(theme);
            }
            response.clear();
        }
    }
    None
}

pub(crate) fn parse_osc_background(response: &[u8]) -> Option<TerminalTheme> {
    let start = response.windows(5).position(|part| part == b"\x1b]11;")? + 5;
    let end = response[start..]
        .iter()
        .position(|byte| *byte == b'\x07' || *byte == b'\x1b')?
        + start;
    let color = std::str::from_utf8(&response[start..end]).ok()?;
    let rgb = color.strip_prefix("rgb:")?;
    let channels: Vec<u8> = rgb
        .split('/')
        .map(|part| {
            if part.is_empty() || part.len() > 4 {
                return None;
            }
            let value = u32::from_str_radix(part, 16).ok()?;
            let max = (1u32 << (4 * part.len())) - 1;
            Some((value * 255 / max) as u8)
        })
        .collect::<Option<Vec<_>>>()?;
    let [r, g, b] = channels.as_slice() else {
        return None;
    };
    let luma = (299u32 * *r as u32 + 587u32 * *g as u32 + 114u32 * *b as u32) / 1000;
    Some(if luma >= 140 {
        TerminalTheme::Light
    } else {
        TerminalTheme::Dark
    })
}

pub(crate) fn detect_terminal_theme_from_env_values(
    theme_override: Option<&str>,
    colorfgbg: Option<&str>,
) -> Option<TerminalTheme> {
    parse_theme_override(theme_override).or_else(|| parse_colorfgbg_theme(colorfgbg))
}

fn parse_theme_override(value: Option<&str>) -> Option<TerminalTheme> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "light" => Some(TerminalTheme::Light),
        "dark" => Some(TerminalTheme::Dark),
        _ => None,
    }
}

fn parse_colorfgbg_theme(value: Option<&str>) -> Option<TerminalTheme> {
    let bg = value?
        .split([';', ':', ','])
        .filter_map(|part| part.trim().parse::<u8>().ok())
        .next_back()?;

    Some(match bg {
        0..=6 | 8 => TerminalTheme::Dark,
        _ => TerminalTheme::Light,
    })
}

pub(crate) fn contrast_color(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            let luma = (299u32 * r as u32 + 587u32 * g as u32 + 114u32 * b as u32) / 1000;
            if luma >= 140 {
                Color::Rgb(0, 0, 0)
            } else {
                Color::Rgb(255, 255, 255)
            }
        }
        _ => Color::Rgb(255, 255, 255),
    }
}
