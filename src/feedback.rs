use std::sync::atomic::{AtomicBool, Ordering};

pub const TUI_ACTIVE_ENV: &str = "SNOUT_TUI_ACTIVE";
static JSON_ACTIVE: AtomicBool = AtomicBool::new(false);
static TUI_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn set_tui_active(active: bool) {
    TUI_ACTIVE.store(active, Ordering::Relaxed);
}

pub fn set_json_active(active: bool) {
    JSON_ACTIVE.store(active, Ordering::SeqCst);
}

pub fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    if TUI_ACTIVE.load(Ordering::Relaxed) {
        command.env(TUI_ACTIVE_ENV, "1");
    }
    if JSON_ACTIVE.load(Ordering::SeqCst) {
        command.stdout(std::process::Stdio::null());
    }
    command
}

pub fn info(message: impl AsRef<str>) {
    if JSON_ACTIVE.load(Ordering::SeqCst) {
        eprintln!("{}", message.as_ref());
    } else if !tui_active() {
        println!("{}", message.as_ref());
    }
}

pub fn warn(message: impl AsRef<str>) {
    if !tui_active() {
        eprintln!("{}", message.as_ref());
    }
}

fn tui_active() -> bool {
    TUI_ACTIVE.load(Ordering::Relaxed) || std::env::var_os(TUI_ACTIVE_ENV).is_some()
}
