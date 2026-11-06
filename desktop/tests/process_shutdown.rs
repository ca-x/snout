#![cfg(target_os = "linux")]

use snout::application::UpdateAction;
use snout::types::Config;
use snout_desktop::controller::{Command, Controller, ProcessShutdown};
use std::path::Path;
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn wait_for_file(path: &Path) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    path.exists()
}

fn wait_for_process(child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().expect("subprocess status").is_none() {
        if Instant::now() >= deadline {
            child.kill().expect("stop only stuck fixture process");
            child.wait().expect("reap stuck fixture");
            panic!("fixture did not finish after hook was released");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn close_fixture(root: &Path, mode: &str) {
    let shutdown = ProcessShutdown::install().expect("production process shutdown owner");
    let mut controller =
        Controller::start(None).expect("production controller with temporary HOME");
    let deadline = Instant::now() + Duration::from_secs(5);
    while controller.snapshot.busy && Instant::now() < deadline {
        controller.poll();
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        controller.snapshot.config.is_some(),
        "temporary configuration loaded"
    );
    controller
        .submit(Command::Update {
            action: UpdateAction::All,
            preserve: true,
        })
        .expect("start real shared pre-update hook");
    assert!(
        wait_for_file(&root.join("hook-started")),
        "real hook started"
    );
    match mode {
        "retained-root" => {
            std::fs::write(root.join("ui-closed"), "closed")
                .expect("event loop returned before root cleanup");
            shutdown
                .wait()
                .expect("process owner cancels a retained controller");
            drop(controller);
        }
        "explicit-shutdown" => {
            std::fs::write(root.join("ui-closed"), "closed").expect("event loop returned");
            controller
                .shutdown()
                .expect("explicit shutdown after the event loop");
            drop(controller);
            shutdown
                .wait()
                .expect("the same worker is not joined twice");
        }
        "controller-drop" | "process-owner-drop" => {
            controller.request_shutdown();
            drop(controller);
            std::fs::write(root.join("ui-closed"), "closed")
                .expect("UI close returned without waiting");
            if mode == "process-owner-drop" {
                drop(shutdown);
            } else {
                shutdown
                    .wait()
                    .expect("safe worker completion after the event loop");
            }
        }
        _ => panic!("unknown close fixture mode: {mode}"),
    }
    assert!(
        Controller::start(None).is_err(),
        "late root cleanup cannot start another worker after process shutdown"
    );
}

#[test]
fn controller_close_waits_for_running_hook_before_process_exit() {
    if let Some(root) = std::env::var_os("SNOUT_SHUTDOWN_FIXTURE_ROOT") {
        let mode = std::env::var("SNOUT_SHUTDOWN_FIXTURE_MODE").expect("close fixture mode");
        close_fixture(Path::new(&root), &mode);
        return;
    }
    for mode in [
        "controller-drop",
        "retained-root",
        "explicit-shutdown",
        "process-owner-drop",
    ] {
        check_close_case(mode);
    }
}

fn check_close_case(mode: &str) {
    let root = tempfile::tempdir().expect("temporary HOME");
    let directory = root.path();
    for name in ["home", "config/snout", "data", "cache", "runtime", "bin"] {
        std::fs::create_dir_all(directory.join(name)).expect("isolated directory");
    }
    std::os::unix::fs::symlink("/bin/sh", directory.join("bin/sh")).expect("fixture shell");
    std::os::unix::fs::symlink("/bin/sleep", directory.join("bin/sleep")).expect("fixture sleep");
    let hook = directory.join("blocking hook.sh");
    std::fs::write(&hook, format!(
        "root='{}'\nprintf '%s' \"$$\" > \"$root/hook-pid\"\nprintf started > \"$root/hook-started\"\nwhile [ ! -f \"$root/release-hook\" ]; do sleep 0.02; done\nprintf finished > \"$root/hook-finished\"\nexit 1\n", directory.display(),
    )).expect("blocking hook fixture");
    let config = Config {
        language: "en".into(),
        auto_update: false,
        engine_sync_enabled: false,
        rime_auto_setup: Some(false),
        pre_update_hook: hook.to_string_lossy().into_owned(),
        ..Config::default()
    };
    std::fs::write(
        directory.join("config/snout/config.json"),
        serde_json::to_vec(&config).expect("config JSON"),
    )
    .expect("isolated configuration");
    let mut child = ProcessCommand::new(std::env::current_exe().expect("fixture executable"))
        .args([
            "--exact",
            "controller_close_waits_for_running_hook_before_process_exit",
            "--nocapture",
        ])
        .env_clear()
        .env("HOME", directory.join("home"))
        .env("XDG_CONFIG_HOME", directory.join("config"))
        .env("XDG_DATA_HOME", directory.join("data"))
        .env("XDG_CACHE_HOME", directory.join("cache"))
        .env("XDG_RUNTIME_DIR", directory.join("runtime"))
        .env("PATH", directory.join("bin"))
        .env("SNOUT_SHUTDOWN_FIXTURE_ROOT", directory)
        .env("SNOUT_SHUTDOWN_FIXTURE_MODE", mode)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("production controller subprocess");
    let ui_closed = wait_for_file(&directory.join("ui-closed"));
    thread::sleep(Duration::from_millis(150));
    let exited_before_hook_release = child.try_wait().expect("fixture still running").is_some();
    std::fs::write(directory.join("release-hook"), "release")
        .expect("release hook even when regression fails");
    wait_for_process(&mut child);
    let output = child.wait_with_output().expect("fixture output");
    let finished_at_exit = directory.join("hook-finished").exists();
    assert!(
        wait_for_file(&directory.join("hook-finished")),
        "hook cleaned up even on RED"
    );
    assert!(
        ui_closed,
        "window/controller drop must not wait for the hook on the UI thread: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !exited_before_hook_release,
        "{mode}: parent exited while the real hook was still running"
    );
    assert!(
        output.status.success(),
        "fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        finished_at_exit,
        "hook effects must finish before parent exit"
    );
    let hook_pid = std::fs::read_to_string(directory.join("hook-pid")).expect("real hook PID");
    assert!(
        !Path::new("/proc").join(hook_pid.trim()).exists(),
        "hook must be reaped before parent exit"
    );
    let completed = std::fs::read(directory.join("hook-finished")).expect("completed hook");
    thread::sleep(Duration::from_millis(100));
    assert_eq!(
        std::fs::read(directory.join("hook-finished")).expect("stable effects"),
        completed
    );
}
