use nsh_base::theme;
use nsh_core::prelude::*;

pub fn install() {
    let theme = nsh_base::Theme::from_json(include_str!("theme.json"))
        .expect("the bundled desktop theme is valid");
    nsh_core::element::set_tokens(theme.into_tokens());
}

pub fn canvas() -> AdaptiveColor {
    theme().background
}
pub fn surface() -> AdaptiveColor {
    theme().card
}
pub fn ink() -> AdaptiveColor {
    theme().foreground
}
pub fn muted() -> AdaptiveColor {
    theme().muted_foreground
}
pub fn line() -> AdaptiveColor {
    theme().border
}
pub fn accent() -> AdaptiveColor {
    theme().primary
}
pub fn on_accent() -> AdaptiveColor {
    theme().primary_foreground
}
pub fn selection() -> AdaptiveColor {
    theme().accent
}
pub fn sidebar_surface() -> AdaptiveColor {
    theme().sidebar
}
pub fn danger() -> AdaptiveColor {
    theme().destructive
}

pub fn overlay() -> AdaptiveColor {
    theme()
        .color("overlay")
        .expect("the desktop theme defines overlay")
}
