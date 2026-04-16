pub const TUI_ACTIVE_ENV: &str = "SNOUT_TUI_ACTIVE";

pub fn set_tui_active(active: bool) {
    if active {
        std::env::set_var(TUI_ACTIVE_ENV, "1");
    } else {
        std::env::remove_var(TUI_ACTIVE_ENV);
    }
}

pub fn info(message: impl AsRef<str>) {
    if std::env::var_os(TUI_ACTIVE_ENV).is_none() {
        println!("{}", message.as_ref());
    }
}

pub fn warn(message: impl AsRef<str>) {
    if std::env::var_os(TUI_ACTIVE_ENV).is_none() {
        eprintln!("{}", message.as_ref());
    }
}
