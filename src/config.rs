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
    let dir = if cfg!(target_os = "windows") {
        let appdata = std::env::var("APPDATA").context("APPDATA 未设置")?;
        PathBuf::from(appdata)
    } else if cfg!(target_os = "macos") {
        dirs::home_dir()
            .context("无法获取 HOME")?
            .join("Library/Application Support")
    } else {
        dirs::config_dir().context("无法获取 config 目录")?
    };
    Ok(dir.join("snout/config.json"))
}

fn load_or_create_config(path: &Path) -> Result<Config> {
    if path.exists() {
        let data = fs::read_to_string(path)?;
        match serde_json::from_str::<Config>(&data) {
            Ok(cfg) => return Ok(cfg),
            Err(e) => {
                eprintln!("⚠️ 配置文件解析失败 ({e})，使用默认配置");
            }
        }
    }
    Ok(Config::default())
}

fn detect_rime_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        PathBuf::from(appdata).join("Rime")
    } else if cfg!(target_os = "macos") {
        dirs::home_dir().unwrap_or_default().join("Library/Rime")
    } else {
        // Linux: 优先 Fcitx5, 然后 IBus
        let fcitx5 = dirs::data_dir().unwrap_or_default().join("fcitx5/rime");
        if fcitx5.parent().map(|p| p.exists()).unwrap_or(false) {
            fcitx5
        } else {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".config/ibus/rime")
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
        if which_exists("fcitx5") || fcitx5_rime_installed() {
            engines.push("fcitx5".into());
        }
        if which_exists("ibus-daemon") {
            engines.push("ibus".into());
        }
    }

    #[cfg(target_os = "macos")]
    {
        let squirrel = Path::new("/Library/Input Methods/Squirrel.app");
        if squirrel.exists() {
            engines.push("squirrel".into());
        }
    }

    #[cfg(target_os = "windows")]
    {
        let weasel_reg = Path::new(r"C:\Program Files\Rime");
        if weasel_reg.exists() {
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
    let data_dir = dirs::data_dir().unwrap_or_default();
    data_dir.join("fcitx5/rime").exists()
}
