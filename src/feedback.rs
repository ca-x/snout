use std::sync::atomic::{AtomicBool, Ordering};

pub const TUI_ACTIVE_ENV: &str = "SNOUT_TUI_ACTIVE";
static JSON_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn set_tui_active(active: bool) {
    if active {
        std::env::set_var(TUI_ACTIVE_ENV, "1");
    } else {
        std::env::remove_var(TUI_ACTIVE_ENV);
    }
}

pub fn set_json_active(active: bool) {
    JSON_ACTIVE.store(active, Ordering::SeqCst);
}

pub fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    if JSON_ACTIVE.load(Ordering::SeqCst) {
        command.stdout(std::process::Stdio::null());
    }
    command
}

pub fn info(message: impl AsRef<str>) {
    if JSON_ACTIVE.load(Ordering::SeqCst) {
        eprintln!("{}", message.as_ref());
    } else if std::env::var_os(TUI_ACTIVE_ENV).is_none() {
        println!("{}", message.as_ref());
    }
}

pub fn warn(message: impl AsRef<str>) {
    if std::env::var_os(TUI_ACTIVE_ENV).is_none() {
        eprintln!("{}", message.as_ref());
    }
}
