use crate::i18n::{L10n, Lang};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// 更新前准备（Windows 需要先停止占用 Rime 用户目录的 Weasel 进程）
pub fn prepare_for_update(_lang: Lang) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        stop_weasel_processes()?;
    }

    Ok(())
}

/// 跨平台部署 Rime
pub fn deploy(lang: Lang) -> Result<()> {
    let t = L10n::new(lang);
    let engines = detect_engines();
    ensure_deployable_engine_set(!engines.is_empty(), &t)?;

    let mut success_count = 0usize;
    let mut failures = Vec::new();
    for engine in &engines {
        match deploy_to(engine, &t) {
            Ok(()) => success_count += 1,
            Err(e) => {
                crate::feedback::warn(format!(
                    "⚠️  {} ({engine}): {e}",
                    t.t("deploy.target_failed")
                ));
                failures.push(format!("{engine}: {e}"));
            }
        }
    }

    finalize_deploy_result(success_count, failures, &t)
}

/// 部署到指定引擎
pub fn deploy_to(engine: &str, t: &L10n) -> Result<()> {
    match engine {
        #[cfg(target_os = "linux")]
        "fcitx5" => {
            if deploy_with_qdbus6().is_ok() {
                crate::feedback::info(format!("  ✅ {}", t.t("deploy.reloaded.fcitx5")));
                return Ok(());
            }
            if let Some(rime_dir) = linux_detect_rime_dir() {
                if run_rime_deployer(&rime_dir).is_ok() {
                    crate::feedback::info(format!("  ✅ {}", t.t("deploy.reloaded.fcitx5")));
                    return Ok(());
                }
            }
            let bin = find_binary("fcitx5-remote", t)?;
            std::process::Command::new(bin).arg("-r").spawn()?;
            crate::feedback::info(format!("  ✅ {}", t.t("deploy.reloaded.fcitx5")));
        }
        #[cfg(target_os = "linux")]
        "ibus" => {
            if let Some(rime_dir) = engine_data_dir("ibus") {
                let _ = run_rime_deployer(&rime_dir);
            }
            let bin = find_binary("ibus-daemon", t).or_else(|_| find_binary("ibus", t))?;
            std::process::Command::new(bin).args(["-drx"]).spawn()?;
            crate::feedback::info(format!("  ✅ {}", t.t("deploy.reloaded.ibus")));
        }
        #[cfg(target_os = "linux")]
        "fcitx" => {
            if let Some(rime_dir) = engine_data_dir("fcitx") {
                run_rime_deployer(&rime_dir)?;
            }
            crate::feedback::info(format!("  ✅ {}", t.t("deploy.reloaded.fcitx5")));
        }
        #[cfg(target_os = "macos")]
        "squirrel" => {
            if let Some(squirrel) = macos_squirrel_binary() {
                std::process::Command::new(squirrel)
                    .arg("--reload")
                    .spawn()?;
                crate::feedback::info(format!("  ✅ {}", t.t("deploy.reloaded.squirrel")));
            }
        }
        #[cfg(target_os = "macos")]
        "fcitx5" => {
            if let Some(fcitx5_curl) = macos_fcitx5_curl() {
                std::process::Command::new(fcitx5_curl)
                    .args(["/config/addon/rime/deploy", "-X", "POST", "-d", "{}"])
                    .spawn()?;
                crate::feedback::info(format!("  ✅ {}", t.t("deploy.reloaded.fcitx5")));
            }
        }
        #[cfg(target_os = "windows")]
        "weasel" => {
            if let Some(server) = windows_server_executable() {
                let _ = std::process::Command::new(&server).arg("/q").status();
                std::thread::sleep(std::time::Duration::from_millis(500));
                let _ = std::process::Command::new(&server).spawn();
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
            if let Some(weasel) = windows_deployer_executable() {
                let status = std::process::Command::new(weasel).arg("/deploy").status()?;
                if !status.success() {
                    anyhow::bail!("WeaselDeployer exited with status {status}");
                }
                crate::feedback::info(format!("  ✅ {}", t.t("deploy.reloaded.weasel")));
            }
        }
        _ => {}
    }
    Ok(())
}

/// 检测已安装的引擎
pub fn detect_engines() -> Vec<String> {
    let mut engines = Vec::new();

    #[cfg(target_os = "linux")]
    {
        if has_binary("fcitx5-remote") {
            engines.push("fcitx5".into());
        }
        if has_binary("ibus") {
            engines.push("ibus".into());
        }
        if has_binary("fcitx") {
            engines.push("fcitx".into());
        }
    }

    #[cfg(target_os = "macos")]
    {
        if macos_squirrel_binary().is_some() {
            engines.push("squirrel".into());
        }
        if macos_fcitx5_curl().is_some() {
            engines.push("fcitx5".into());
        }
    }

    #[cfg(target_os = "windows")]
    {
        if windows_deployer_executable().is_some() || windows_server_executable().is_some() {
            engines.push("weasel".into());
        }
    }

    engines
}

/// 获取主引擎数据目录
#[allow(dead_code)]
pub fn engine_data_dir(engine: &str) -> Option<PathBuf> {
    match engine {
        #[cfg(target_os = "linux")]
        "fcitx5" => Some(dirs::data_dir()?.join("fcitx5/rime")),
        #[cfg(target_os = "linux")]
        "ibus" => Some(dirs::home_dir()?.join(".config/ibus/rime")),
        #[cfg(target_os = "linux")]
        "fcitx" => Some(dirs::home_dir()?.join(".config/fcitx/rime")),
        #[cfg(target_os = "macos")]
        "squirrel" => Some(dirs::home_dir()?.join("Library/Rime")),
        #[cfg(target_os = "macos")]
        "fcitx5" => Some(dirs::home_dir()?.join(".local/share/fcitx5/rime")),
        #[cfg(target_os = "windows")]
        "weasel" => windows_rime_user_dir(),
        _ => None,
    }
}

/// 同步 Rime 目录到所有已安装引擎的数据目录
#[allow(dead_code)]
pub fn sync_to_engines(
    src_dir: &Path,
    exclude_files: &[String],
    use_link: bool,
    lang: Lang,
) -> Result<()> {
    let t = L10n::new(lang);
    let engines = detect_engines();
    if engines.len() <= 1 {
        return Ok(());
    }

    let primary = engines.first().cloned().unwrap_or_default();
    let mut errors = Vec::new();

    for engine in &engines {
        if *engine == primary {
            continue;
        }
        if let Some(target) = engine_data_dir(engine) {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            #[cfg(unix)]
            let sync_result = if use_link {
                sync_via_symlink(src_dir, &target)
            } else {
                sync_dir_filtered(src_dir, &target, exclude_files)
            };
            #[cfg(not(unix))]
            let sync_result = sync_dir_filtered(src_dir, &target, exclude_files);
            if let Err(e) = sync_result {
                errors.push(format!("{engine}: {e}"));
            }
        }
    }

    if !errors.is_empty() {
        crate::feedback::warn(format!(
            "⚠️ {}: {}",
            t.t("deploy.sync_partial_failed"),
            errors.join("; ")
        ));
    }
    Ok(())
}

#[allow(dead_code)]
fn sync_dir_filtered(src: &Path, dst: &Path, exclude_files: &[String]) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // 跳过排除的文件
        if exclude_files.iter().any(|e| name_str == *e) {
            continue;
        }
        // 跳过 build 目录
        if name_str == "build" {
            continue;
        }

        let src_path = entry.path();
        let dst_path = dst.join(&name);

        if src_path.is_dir() {
            sync_dir_filtered(&src_path, &dst_path, exclude_files)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// 执行 hook 脚本
pub fn run_hook(hook_path: &str, phase: &str, lang: Lang) -> Result<()> {
    if hook_path.is_empty() {
        return Ok(());
    }

    let t = L10n::new(lang);
    let path = Path::new(hook_path);
    if !path.exists() {
        crate::feedback::warn(format!(
            "  ⚠️ {phase} {}: {hook_path}",
            t.t("deploy.hook_missing")
        ));
        return Ok(());
    }

    crate::feedback::info(format!(
        "  🔧 {phase} {}: {hook_path}",
        t.t("deploy.hook_running")
    ));
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(hook_path)
        .status()?;

    if !status.success() {
        anyhow::bail!("{phase} {}: {hook_path}", t.t("deploy.hook_failed"));
    }
    Ok(())
}

// ── 辅助函数 ──

fn has_binary(name: &str) -> bool {
    which(name).is_some()
}

fn find_binary(name: &str, lang: &L10n) -> Result<PathBuf> {
    which(name).ok_or_else(|| anyhow::anyhow!("{}: {name}", lang.t("deploy.binary_not_found")))
}

fn which(name: &str) -> Option<PathBuf> {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| PathBuf::from(s.trim()))
}

#[cfg(target_os = "linux")]
fn linux_detect_rime_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    [
        home.join(".local/share/fcitx5/rime"),
        home.join(".config/fcitx5/rime"),
        home.join(".config/ibus/rime"),
        home.join(".config/fcitx/rime"),
    ]
    .into_iter()
    .find(|path| path.exists())
    .or_else(|| Some(home.join(".local/share/fcitx5/rime")))
}

#[cfg(target_os = "linux")]
fn deploy_with_qdbus6() -> Result<()> {
    if which("qdbus6").is_none() {
        anyhow::bail!("qdbus6 unavailable");
    }
    std::process::Command::new("qdbus6")
        .args([
            "org.fcitx.Fcitx5",
            "/controller",
            "org.fcitx.Fcitx.Controller1.SetConfig",
            "fcitx://config/addon/rime/deploy",
            "",
        ])
        .status()?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn run_rime_deployer(rime_dir: &Path) -> Result<()> {
    for candidate in [
        "/usr/lib/rime/rime_deployer",
        "/usr/lib64/rime/rime_deployer",
        "/usr/local/lib/rime/rime_deployer",
        "rime_deployer",
    ] {
        let command = if candidate == "rime_deployer" {
            which(candidate).unwrap_or_else(|| PathBuf::from(candidate))
        } else {
            PathBuf::from(candidate)
        };
        if !command.exists() && candidate != "rime_deployer" {
            continue;
        }
        let status = std::process::Command::new(&command)
            .args(["--build", &rime_dir.display().to_string()])
            .status();
        if matches!(status, Ok(s) if s.success()) {
            return Ok(());
        }
    }
    anyhow::bail!("rime_deployer unavailable")
}

#[cfg(target_os = "macos")]
fn macos_squirrel_binary() -> Option<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    [
        PathBuf::from("/Library/Input Methods/Squirrel.app/Contents/MacOS/Squirrel"),
        home.join("Library/Input Methods/Squirrel.app/Contents/MacOS/Squirrel"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

#[cfg(target_os = "macos")]
fn macos_fcitx5_curl() -> Option<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    [
        PathBuf::from("/Library/Input Methods/Fcitx5.app/Contents/bin/fcitx5-curl"),
        home.join("Library/Input Methods/Fcitx5.app/Contents/bin/fcitx5-curl"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

#[cfg(target_os = "windows")]
fn windows_registry_query(key: &str, value: &str) -> Option<String> {
    let output = std::process::Command::new("reg")
        .args(["query", key, "/v", value])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    for line in stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with(value) {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let data = parts[2..].join(" ");
        if !data.is_empty() {
            return Some(data);
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn windows_rime_user_dir() -> Option<PathBuf> {
    windows_registry_query(r"HKCU\Software\Rime\Weasel", "RimeUserDir")
        .or_else(|| windows_registry_query(r"HKLM\SOFTWARE\WOW6432Node\Rime\Weasel", "RimeUserDir"))
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("APPDATA")
                .ok()
                .map(|v| PathBuf::from(v).join("Rime"))
        })
}

#[cfg(target_os = "windows")]
fn windows_server_executable() -> Option<PathBuf> {
    if let (Some(root), Some(exe)) = (
        windows_registry_query(r"HKLM\SOFTWARE\WOW6432Node\Rime\Weasel", "WeaselRoot"),
        windows_registry_query(r"HKLM\SOFTWARE\WOW6432Node\Rime\Weasel", "ServerExecutable"),
    ) {
        return Some(PathBuf::from(root).join(exe));
    }
    let local = std::env::var("LOCALAPPDATA").ok()?;
    let candidate = PathBuf::from(local).join("Programs/Rime/weasel-x64/WeaselServer.exe");
    if candidate.exists() {
        return Some(candidate);
    }
    None
}

#[cfg(target_os = "windows")]
fn windows_deployer_executable() -> Option<PathBuf> {
    if let Some(server) = windows_server_executable() {
        let deployer = server.with_file_name("WeaselDeployer.exe");
        if deployer.exists() {
            return Some(deployer);
        }
    }
    let local = std::env::var("LOCALAPPDATA").ok()?;
    let candidate = PathBuf::from(local).join("Programs/Rime/weasel-x64/WeaselDeployer.exe");
    if candidate.exists() {
        return Some(candidate);
    }
    None
}

#[cfg(target_os = "windows")]
fn stop_weasel_processes() -> Result<()> {
    if !graceful_stop_weasel() {
        hard_stop_weasel();
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn graceful_stop_weasel() -> bool {
    let Some(server) = windows_server_executable() else {
        return false;
    };

    match std::process::Command::new(server).arg("/q").status() {
        Ok(status) if status.success() => {
            std::thread::sleep(std::time::Duration::from_millis(500));
            true
        }
        _ => false,
    }
}

#[cfg(target_os = "windows")]
fn hard_stop_weasel() {
    for _ in 0..3 {
        let _ = std::process::Command::new("taskkill")
            .args(["/IM", "WeaselServer.exe", "/F"])
            .status();
        let _ = std::process::Command::new("taskkill")
            .args(["/IM", "WeaselDeployer.exe", "/F"])
            .status();
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

#[cfg(unix)]
fn sync_via_symlink(src: &Path, target: &Path) -> Result<()> {
    if target.exists() || target.is_symlink() {
        let backup = target.with_extension("bak");
        if target.is_symlink() || target.is_file() {
            std::fs::remove_file(target)?;
        } else if target.is_dir() {
            if backup.exists() {
                if backup.is_dir() {
                    std::fs::remove_dir_all(&backup)?;
                } else {
                    std::fs::remove_file(&backup)?;
                }
            }
            std::fs::rename(target, &backup)?;
        }
    }

    std::os::unix::fs::symlink(src, target)?;
    Ok(())
}

fn ensure_deployable_engine_set(has_engines: bool, t: &L10n) -> Result<()> {
    if has_engines {
        Ok(())
    } else {
        anyhow::bail!("{}", t.t("deploy.no_engine_detected"));
    }
}

fn finalize_deploy_result(success_count: usize, failures: Vec<String>, t: &L10n) -> Result<()> {
    if success_count == 0 {
        anyhow::bail!(
            "{}: {}",
            t.t("deploy.all_engines_failed"),
            failures.join("; ")
        );
    }

    if !failures.is_empty() {
        crate::feedback::warn(format!(
            "⚠️  {}: {}",
            t.t("deploy.partial_engines_failed"),
            failures.join("; ")
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("snout-{name}-{nanos}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn rejects_empty_engine_set() {
        let t = L10n::new(Lang::Zh);
        assert!(ensure_deployable_engine_set(false, &t).is_err());
        assert!(ensure_deployable_engine_set(true, &t).is_ok());
    }

    #[test]
    fn fails_when_all_deployments_fail() {
        let t = L10n::new(Lang::Zh);
        let err = finalize_deploy_result(0, vec!["fcitx5: boom".into()], &t).unwrap_err();
        assert!(err.to_string().contains(t.t("deploy.all_engines_failed")));
    }

    #[test]
    fn succeeds_when_at_least_one_deployment_succeeds() {
        let t = L10n::new(Lang::Zh);
        assert!(finalize_deploy_result(1, Vec::new(), &t).is_ok());
        assert!(finalize_deploy_result(1, vec!["ibus: failed".into()], &t).is_ok());
    }

    #[test]
    fn prepare_for_update_returns_ok() {
        assert!(prepare_for_update(Lang::En).is_ok());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn fcitx_engine_data_dir_uses_fcitx5_data_path() {
        let path = engine_data_dir("fcitx5").expect("fcitx5 dir");
        assert!(path.ends_with("fcitx5/rime"));
    }

    #[cfg(unix)]
    #[test]
    fn sync_via_symlink_does_not_create_backup_for_missing_target() {
        let base = temp_dir("deployer-link");
        let src = base.join("src");
        let target = base.join("target");
        std::fs::create_dir_all(&src).expect("create src dir");

        sync_via_symlink(&src, &target).expect("create symlink");

        assert!(target.is_symlink());
        assert!(!target.with_extension("bak").exists());

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn sync_dir_filtered_copies_files_and_skips_exclusions() {
        let base = temp_dir("deployer-copy");
        let src = base.join("src");
        let dst = base.join("dst");
        std::fs::create_dir_all(src.join("nested")).expect("create nested src dir");
        std::fs::create_dir_all(src.join("build")).expect("create build dir");
        std::fs::write(src.join("keep.txt"), "keep").expect("write keep");
        std::fs::write(src.join("skip.txt"), "skip").expect("write skip");
        std::fs::write(src.join("nested").join("child.txt"), "child").expect("write child");
        std::fs::write(src.join("build").join("artifact.txt"), "artifact").expect("write artifact");
        std::fs::create_dir_all(&dst).expect("create dst");
        std::fs::write(dst.join("preexisting.txt"), "stay").expect("write preexisting");

        sync_dir_filtered(&src, &dst, &["skip.txt".into()]).expect("sync dir");

        assert_eq!(
            std::fs::read_to_string(dst.join("keep.txt")).unwrap(),
            "keep"
        );
        assert_eq!(
            std::fs::read_to_string(dst.join("nested").join("child.txt")).unwrap(),
            "child"
        );
        assert!(!dst.join("skip.txt").exists());
        assert!(!dst.join("build").exists());
        assert_eq!(
            std::fs::read_to_string(dst.join("preexisting.txt")).unwrap(),
            "stay"
        );

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn run_hook_accepts_empty_and_missing_paths() {
        assert!(run_hook("", "pre-update", Lang::En).is_ok());
        assert!(run_hook("/definitely/missing/hook.sh", "pre-update", Lang::En).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn run_hook_reports_failure_for_nonzero_exit() {
        let base = temp_dir("hook-fail");
        let script = base.join("fail.sh");
        std::fs::write(&script, "#!/bin/sh\nexit 7\n").expect("write script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script, perms).unwrap();
        }

        let err = run_hook(
            script.to_str().expect("script path"),
            "post-update",
            Lang::En,
        )
        .unwrap_err();
        assert!(err.to_string().contains("Hook execution failed"));

        std::fs::remove_dir_all(&base).ok();
    }

    #[cfg(unix)]
    #[test]
    fn run_hook_runs_successful_commands() {
        let base = temp_dir("hook-ok");
        let script = base.join("ok.sh");
        std::fs::write(&script, "#!/bin/sh\nexit 0\n").expect("write script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script, perms).unwrap();
        }

        assert!(run_hook(
            script.to_str().expect("script path"),
            "post-update",
            Lang::En
        )
        .is_ok());

        std::fs::remove_dir_all(&base).ok();
    }
}
