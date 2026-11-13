use std::process::{Command, Output};

fn run(arguments: &[&str]) -> Output {
    let home = tempfile::tempdir().expect("isolated CLI home");
    Command::new(env!("CARGO_BIN_EXE_snout"))
        .args(arguments)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("APPDATA", home.path().join("appdata"))
        .env("LOCALAPPDATA", home.path().join("localappdata"))
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_CACHE_HOME", home.path().join("cache"))
        .output()
        .expect("run CLI")
}

#[test]
fn parse_errors_keep_the_json_envelope_and_exit_code() {
    let output = run(&["--json", "--unknown-option"]);
    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON envelope");
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["ok"], false);
    assert_eq!(report["command"], "parse");
    assert!(report["error"]
        .as_str()
        .expect("parse error")
        .contains("--unknown-option"));
    assert_eq!(report["results"], serde_json::json!([]));
}

#[test]
fn unsupported_dictionary_keeps_the_structured_result_and_failure_exit() {
    let output = run(&["--json", "--dict", "--schema", "mint", "--lang", "en"]);
    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON envelope");
    assert_eq!(report["ok"], false);
    assert_eq!(report["command"], "dict");
    assert_eq!(report["schema"], "Mint");
    assert_eq!(report["results"][0]["component"], "dict");
    assert_eq!(report["results"][0]["success"], false);
    assert!(report["error"].is_null());
    assert!(output.stderr.is_empty());
}

#[test]
fn completions_and_help_remain_available_without_initialization() {
    assert!(run(&["--help"]).status.success());
    for shell in ["zsh", "fish"] {
        let output = run(&["completions", shell]);
        assert!(output.status.success());
        let script = String::from_utf8(output.stdout).expect("UTF-8 completions");
        assert_eq!(
            script,
            snout::cli::completion_script(match shell {
                "zsh" => snout::cli::CompletionShell::Zsh,
                _ => snout::cli::CompletionShell::Fish,
            })
        );
    }
}

#[test]
fn program_update_is_distinct_from_data_updates_and_keeps_json_errors() {
    let help = run(&["self-update", "--help"]);
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout).unwrap();
    assert!(text.contains("--check"));
    assert!(text.contains("--yes"));
    for arguments in [
        vec!["self-update", "--check", "--json"],
        vec!["self-update", "--yes", "--json"],
    ] {
        let output = run(&arguments);
        assert_eq!(output.status.code(), Some(1));
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["command"], "self-update");
        assert_eq!(report["ok"], false);
        assert!(report["error"].as_str().unwrap().contains("development"));
    }
}

#[test]
fn package_marker_disables_terminal_updates_before_configuration_or_network() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let executable = bin.join(if cfg!(windows) { "snout.exe" } else { "snout" });
    std::fs::copy(env!("CARGO_BIN_EXE_snout"), &executable).unwrap();
    let marker = root.path().join("share/snout/package-managed");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(marker, "pacman\n").unwrap();
    let config = root.path().join("config");
    let output = Command::new(executable)
        .args(["self-update", "--yes", "--json"])
        .env("XDG_CONFIG_HOME", &config)
        .env("HOME", root.path())
        .env("USERPROFILE", root.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["command"], "self-update");
    assert!(report["error"]
        .as_str()
        .unwrap()
        .contains("Package-managed"));
    assert!(!config.exists());
}
