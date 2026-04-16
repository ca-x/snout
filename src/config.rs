use crate::i18n::{L10n, Lang};
use crate::types::Config;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Manager {
    pub config_path: PathBuf,
    pub config: Config,
    pub rime_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl Manager {
    pub fn new() -> Result<Self> {
        let config_path = get_config_path()?;
        let config = load_or_create_config(&config_path)?;
        let rime_dir = detect_rime_dir();
        let cache_dir = get_cache_dir();

        // 确保目录存在
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            config_path,
            config,
            rime_dir,
            cache_dir,
        })
    }

    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.config)?;
        fs::write(&self.config_path, json)?;
        Ok(())
    }

    /// 方案记录路径
    #[allow(dead_code)]
    pub fn scheme_record_path(&self) -> PathBuf {
        self.cache_dir.join("scheme_record.json")
    }

    /// 词库记录路径
    #[allow(dead_code)]
    pub fn dict_record_path(&self) -> PathBuf {
        self.cache_dir.join("dict_record.json")
    }

    /// 模型记录路径
    #[allow(dead_code)]
    pub fn model_record_path(&self) -> PathBuf {
        self.cache_dir.join("model_record.json")
    }

    /// 方案解压路径 (就是 Rime 用户目录)
    #[allow(dead_code)]
    pub fn extract_path(&self) -> &Path {
        &self.rime_dir
    }

    /// 词库解压路径
    #[allow(dead_code)]
    pub fn dict_extract_path(&self) -> PathBuf {
        self.rime_dir.join("dicts")
    }
}

fn get_config_path() -> Result<PathBuf> {
    let t = L10n::new(Lang::Zh);
    let dir = if cfg!(target_os = "windows") {
        let appdata =
            std::env::var("APPDATA").with_context(|| t.t("config.appdata_missing").to_string())?;
        PathBuf::from(appdata)
    } else if cfg!(target_os = "macos") {
        dirs::home_dir()
            .with_context(|| t.t("config.home_missing").to_string())?
            .join("Library/Application Support")
    } else {
        dirs::config_dir().with_context(|| t.t("config.config_dir_missing").to_string())?
    };
    Ok(dir.join("snout/config.json"))
}

fn load_or_create_config(path: &Path) -> Result<Config> {
    let t = L10n::new(Lang::Zh);
    if path.exists() {
        let data = fs::read_to_string(path)?;
        match serde_json::from_str::<Config>(&data) {
            Ok(cfg) => return Ok(cfg),
            Err(e) => {
                eprintln!("⚠️ {} ({e})", t.t("config.parse_failed_defaulting"));
            }
        }
    }
    Ok(Config::default())
}

fn detect_rime_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        windows_rime_user_dir().unwrap_or_else(|| {
            let appdata = std::env::var("APPDATA").unwrap_or_default();
            PathBuf::from(appdata).join("Rime")
        })
    } else if cfg!(target_os = "macos") {
        let squirrel = macos_squirrel_rime_dir();
        let fcitx5 = macos_fcitx5_rime_dir();

        if squirrel.exists() {
            return squirrel;
        }
        if fcitx5.exists() {
            return fcitx5;
        }

        if macos_squirrel_installed() {
            squirrel
        } else if macos_fcitx5_installed() {
            fcitx5
        } else {
            squirrel
        }
    } else {
        // Linux: 按已知优先级覆盖 fcitx5/ibus/fcitx 的常见目录。
        for candidate in linux_rime_dir_candidates() {
            if candidate.exists() {
                return candidate;
            }
        }

        if which_exists("fcitx5-remote") {
            linux_fcitx5_rime_dir()
        } else if which_exists("ibus") {
            linux_ibus_rime_dir()
        } else if which_exists("fcitx") {
            linux_fcitx_rime_dir()
        } else {
            linux_fcitx5_rime_dir()
        }
    }
}

fn get_cache_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        PathBuf::from(appdata).join("snout/cache")
    } else if cfg!(target_os = "macos") {
        dirs::home_dir()
            .unwrap_or_default()
            .join("Library/Caches/snout")
    } else {
        dirs::cache_dir().unwrap_or_default().join("snout")
    }
}

/// 检测已安装的 Rime 引擎
pub fn detect_installed_engines() -> Vec<String> {
    let mut engines = Vec::new();

    #[cfg(target_os = "linux")]
    {
        if which_exists("fcitx5-remote") || fcitx5_rime_installed() {
            engines.push("fcitx5".into());
        }
        if which_exists("ibus") {
            engines.push("ibus".into());
        }
        if which_exists("fcitx") || fcitx_rime_installed() {
            engines.push("fcitx".into());
        }
    }

    #[cfg(target_os = "macos")]
    {
        if macos_squirrel_installed() {
            engines.push("squirrel".into());
        }
        if macos_fcitx5_installed() {
            engines.push("fcitx5".into());
        }
    }

    #[cfg(target_os = "windows")]
    {
        if windows_weasel_detected() {
            engines.push("weasel".into());
        }
    }

    engines
}

#[cfg(target_os = "linux")]
fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn fcitx5_rime_installed() -> bool {
    linux_fcitx5_rime_dir().exists() || linux_fcitx5_config_rime_dir().exists()
}

#[cfg(target_os = "linux")]
fn fcitx_rime_installed() -> bool {
    linux_fcitx_rime_dir().exists()
}

#[cfg(target_os = "linux")]
fn linux_fcitx5_rime_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_default().join("fcitx5/rime")
}

#[cfg(target_os = "linux")]
fn linux_fcitx5_config_rime_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/fcitx5/rime")
}

#[cfg(target_os = "linux")]
fn linux_ibus_rime_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/ibus/rime")
}

#[cfg(target_os = "linux")]
fn linux_fcitx_rime_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/fcitx/rime")
}

#[cfg(target_os = "linux")]
fn linux_rime_dir_candidates() -> Vec<PathBuf> {
    vec![
        linux_fcitx5_rime_dir(),
        linux_fcitx5_config_rime_dir(),
        linux_ibus_rime_dir(),
        linux_fcitx_rime_dir(),
    ]
}

#[cfg(target_os = "macos")]
fn macos_squirrel_rime_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join("Library/Rime")
}

#[cfg(not(target_os = "macos"))]
fn macos_squirrel_rime_dir() -> PathBuf {
    PathBuf::new()
}

#[cfg(target_os = "macos")]
fn macos_fcitx5_rime_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".local/share/fcitx5/rime")
}

#[cfg(not(target_os = "macos"))]
fn macos_fcitx5_rime_dir() -> PathBuf {
    PathBuf::new()
}

#[cfg(target_os = "macos")]
fn macos_squirrel_installed() -> bool {
    let home = dirs::home_dir().unwrap_or_default();
    [
        PathBuf::from("/Library/Input Methods/Squirrel.app"),
        home.join("Library/Input Methods/Squirrel.app"),
    ]
    .iter()
    .any(|path| path.exists())
}

#[cfg(not(target_os = "macos"))]
fn macos_squirrel_installed() -> bool {
    false
}

#[cfg(target_os = "macos")]
fn macos_fcitx5_installed() -> bool {
    let home = dirs::home_dir().unwrap_or_default();
    [
        PathBuf::from("/Library/Input Methods/Fcitx5.app"),
        home.join("Library/Input Methods/Fcitx5.app"),
    ]
    .iter()
    .any(|path| path.exists())
}

#[cfg(not(target_os = "macos"))]
fn macos_fcitx5_installed() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn windows_weasel_detected() -> bool {
    windows_rime_user_dir().is_some()
        || windows_registry_query(r"HKLM\SOFTWARE\WOW6432Node\Rime\Weasel", "ServerExecutable")
            .is_some()
        || windows_registry_query(r"HKCU\Software\Rime\Weasel", "RimeUserDir").is_some()
        || Path::new(r"C:\Program Files\Rime").exists()
}

#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
fn windows_weasel_detected() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn windows_rime_user_dir() -> Option<PathBuf> {
    windows_registry_query(r"HKCU\Software\Rime\Weasel", "RimeUserDir")
        .or_else(|| windows_registry_query(r"HKLM\SOFTWARE\WOW6432Node\Rime\Weasel", "RimeUserDir"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(not(target_os = "windows"))]
fn windows_rime_user_dir() -> Option<PathBuf> {
    None
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

#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
fn windows_registry_query(_key: &str, _value: &str) -> Option<String> {
    None
}
