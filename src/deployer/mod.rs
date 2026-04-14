use anyhow::Result;
use std::path::Path;

#[cfg(target_os = "windows")]
pub fn deploy() -> Result<()> {
    // 小狼毫: 运行 WeaselDeployer.exe 或重启服务
    let weasel_dir = Path::new(r"C:\Program Files\Rime");
    let deployer = weasel_dir.join("weaselDeployer.exe");
    if deployer.exists() {
        std::process::Command::new(deployer).spawn()?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn deploy() -> Result<()> {
    // 鼠须管: Squirrel --reload
    let squirrel = "/Library/Input Methods/Squirrel.app/Contents/MacOS/Squirrel";
    if Path::new(squirrel).exists() {
        std::process::Command::new(squirrel)
            .arg("--reload")
            .spawn()?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn deploy() -> Result<()> {
    // Fcitx5: fcitx5-remote -r
    if which("fcitx5-remote") {
        std::process::Command::new("fcitx5-remote")
            .arg("-r")
            .spawn()?;
    }
    // IBus: ibus engine Rime
    else if which("ibus") {
        std::process::Command::new("ibus")
            .args(["engine", "Rime"])
            .spawn()?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn which(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
