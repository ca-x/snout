use crate::config::Manager;
use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders},
};
use std::env;

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
    let configured_mode = Manager::new()
        .ok()
        .and_then(|manager| parse_theme_override(Some(&manager.config.tui_theme_mode)));
    detect_terminal_theme_from_env_values(
        env::var("SNOUT_TUI_THEME").ok().as_deref(),
        env::var("COLORFGBG").ok().as_deref(),
    )
    .or(configured_mode)
    .unwrap_or(TerminalTheme::Dark)
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
                Color::Black
            } else {
                Color::White
            }
        }
        _ => Color::White,
    }
}
