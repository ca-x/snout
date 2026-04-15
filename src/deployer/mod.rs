use anyhow::Result;
use std::path::{Path, PathBuf};

/// 跨平台部署 Rime
pub fn deploy() -> Result<()> {
    let engines = detect_engines();
    if engines.is_empty() {
        anyhow::bail!("未检测到 Rime 引擎");
    }

    for engine in &engines {
        if let Err(e) = deploy_to(engine) {
            eprintln!("⚠️  部署到 {engine} 失败: {e}");
        }
    }
    Ok(())
}

/// 部署到指定引擎
pub fn deploy_to(engine: &str) -> Result<()> {
    match engine {
        #[cfg(target_os = "linux")]
        "fcitx5" => {
            let bin = find_binary("fcitx5-remote")?;
            std::process::Command::new(bin).arg("-r").spawn()?;
            println!("  ✅ Fcitx5 已重载");
        }
        #[cfg(target_os = "linux")]
        "ibus" => {
            let bin = find_binary("ibus")?;
            std::process::Command::new(bin).args(["engine", "Rime"]).spawn()?;
            println!("  ✅ IBus 已重载");
        }
        #[cfg(target_os = "macos")]
        "squirrel" => {
            let squirrel = "/Library/Input Methods/Squirrel.app/Contents/MacOS/Squirrel";
            if Path::new(squirrel).exists() {
                std::process::Command::new(squirrel).arg("--reload").spawn()?;
                println!("  ✅ 鼠须管已重载");
            }
        }
        #[cfg(target_os = "windows")]
        "weasel" => {
            let weasel = Path::new(r"C:\Program Files\Rime\weaselDeployer.exe");
            if weasel.exists() {
                std::process::Command::new(weasel).spawn()?;
                println!("  ✅ 小狼毫已重载");
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
    }

    #[cfg(target_os = "macos")]
    {
        if Path::new("/Library/Input Methods/Squirrel.app").exists() {
            engines.push("squirrel".into());
        }
    }

    #[cfg(target_os = "windows")]
    {
        if Path::new(r"C:\Program Files\Rime").exists() {
            engines.push("weasel".into());
        }
    }

    engines
}

/// 获取主引擎数据目录
pub fn engine_data_dir(engine: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    match engine {
        #[cfg(target_os = "linux")]
        "fcitx5" => Some(dirs::data_dir()?.join("fcitx5/rime")),
        #[cfg(target_os = "linux")]
        "ibus" => Some(home.join(".config/ibus/rime")),
        #[cfg(target_os = "macos")]
        "squirrel" => Some(home.join("Library/Rime")),
        #[cfg(target_os = "windows")]
        "weasel" => {
            let appdata = std::env::var("APPDATA").ok()?;
            Some(PathBuf::from(appdata).join("Rime"))
        }
        _ => None,
    }
}

/// 同步 Rime 目录到所有已安装引擎的数据目录
pub fn sync_to_engines(src_dir: &Path, exclude_files: &[String]) -> Result<()> {
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
            std::fs::create_dir_all(&target)?;
            if let Err(e) = sync_dir_filtered(src_dir, &target, exclude_files) {
                errors.push(format!("{engine}: {e}"));
            }
        }
    }

    if !errors.is_empty() {
        eprintln!("⚠️ 部分引擎同步失败: {}", errors.join("; "));
    }
    Ok(())
}

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

/// 同步 Rime 目录到其他引擎 (Fcitx 兼容模式)
pub fn sync_to_fcitx(rime_dir: &Path, use_link: bool) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let fcitx_rime = dirs::home_dir()
            .unwrap_or_default()
            .join(".config/fcitx/rime");

        if !fcitx_rime.parent().map(|p| p.exists()).unwrap_or(false) {
            return Ok(()); // fcitx 未安装
        }

        std::fs::create_dir_all(&fcitx_rime)?;

        if use_link {
            // 软链接模式: 删除旧目录，创建符号链接
            if fcitx_rime.exists() {
                // 备份旧目录
                let backup = fcitx_rime.with_extension("bak");
                if fcitx_rime.is_symlink() {
                    std::fs::remove_file(&fcitx_rime)?;
                } else if fcitx_rime.is_dir() {
                    if backup.exists() {
                        std::fs::remove_dir_all(&backup)?;
                    }
                    std::fs::rename(&fcitx_rime, &backup)?;
                }
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(rime_dir, &fcitx_rime)?;
            println!("  ✅ 已创建软链接: {} -> {}", fcitx_rime.display(), rime_dir.display());
        } else {
            // 复制模式
            copy_dir_recursive(rime_dir, &fcitx_rime)?;
            println!("  ✅ 已同步到: {}", fcitx_rime.display());
        }
    }

    Ok(())
}

/// 执行 hook 脚本
pub fn run_hook(hook_path: &str, phase: &str) -> Result<()> {
    if hook_path.is_empty() {
        return Ok(());
    }

    let path = Path::new(hook_path);
    if !path.exists() {
        eprintln!("  ⚠️ {phase} hook 不存在: {hook_path}");
        return Ok(());
    }

    println!("  🔧 执行 {phase} hook: {hook_path}");
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(hook_path)
        .status()?;

    if !status.success() {
        anyhow::bail!("{phase} hook 执行失败: {hook_path}");
    }
    Ok(())
}

// ── 辅助函数 ──

fn has_binary(name: &str) -> bool {
    which(name).is_some()
}

fn find_binary(name: &str) -> Result<PathBuf> {
    which(name).ok_or_else(|| anyhow::anyhow!("未找到: {name}"))
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

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        // 跳过 build 目录
        if entry.file_name() == "build" {
            continue;
        }

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
