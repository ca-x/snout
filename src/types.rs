use serde::{Deserialize, Serialize};
use std::fmt;

// ── GitHub/CNB 常量 ──
pub const GITHUB_API: &str = "https://api.github.com";
pub const CNB_BASE: &str = "https://cnb.cool";

// 万象
pub const WX_OWNER: &str = "amzxyz";
pub const WX_REPO: &str = "rime_wanxiang";
pub const WX_CNB_REPO: &str = "rime-wanxiang";
pub const WX_DICT_TAG: &str = "dict-nightly";
pub const WX_CNB_DICT_TAG: &str = "v1.0.0";

// 模型
pub const MODEL_REPO: &str = "RIME-LMDG";
pub const MODEL_TAG: &str = "LTS";
pub const MODEL_FILE: &str = "wanxiang-lts-zh-hans.gram";

// 雾凇
pub const ICE_OWNER: &str = "iDvel";
pub const ICE_REPO: &str = "rime-ice";

// 白霜
pub const FROST_OWNER: &str = "gaboolic";
pub const FROST_REPO: &str = "rime-frost";

// ── 方案类型 ──
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum Schema {
    WanxiangBase,
    WanxiangMoqi,
    WanxiangFlypy,
    WanxiangZrm,
    WanxiangTiger,
    WanxiangWubi,
    WanxiangHanxin,
    WanxiangShouyou,
    WanxiangShyplus,
    WanxiangWx,
    Ice,     // 雾凇
    Frost,   // 白霜
}

impl Schema {
    pub fn all() -> &'static [Schema] {
        &[
            Schema::WanxiangBase,
            Schema::WanxiangMoqi,
            Schema::WanxiangFlypy,
            Schema::WanxiangZrm,
            Schema::WanxiangTiger,
            Schema::WanxiangWubi,
            Schema::WanxiangHanxin,
            Schema::WanxiangShouyou,
            Schema::WanxiangShyplus,
            Schema::WanxiangWx,
            Schema::Ice,
            Schema::Frost,
        ]
    }

    /// 显示名称
    pub fn display_name(&self) -> &'static str {
        match self {
            Schema::WanxiangBase => "万象拼音 (标准版)",
            Schema::WanxiangMoqi => "万象拼音 Pro (墨奇辅助)",
            Schema::WanxiangFlypy => "万象拼音 Pro (小鹤辅助)",
            Schema::WanxiangZrm => "万象拼音 Pro (自然码辅助)",
            Schema::WanxiangTiger => "万象拼音 Pro (虎码辅助)",
            Schema::WanxiangWubi => "万象拼音 Pro (五笔辅助)",
            Schema::WanxiangHanxin => "万象拼音 Pro (汉心辅助)",
            Schema::WanxiangShouyou => "万象拼音 Pro (首右辅助)",
            Schema::WanxiangShyplus => "万象拼音 Pro (首右+辅助)",
            Schema::WanxiangWx => "万象拼音 Pro (万象辅助)",
            Schema::Ice => "雾凇拼音",
            Schema::Frost => "白霜拼音",
        }
    }

    /// 所属仓库 owner
    pub fn owner(&self) -> &'static str {
        match self {
            Schema::Ice => ICE_OWNER,
            Schema::Frost => FROST_OWNER,
            _ => WX_OWNER,
        }
    }

    /// 所属仓库名
    pub fn repo(&self) -> &'static str {
        match self {
            Schema::Ice => ICE_REPO,
            Schema::Frost => FROST_REPO,
            _ => WX_REPO,
        }
    }

    /// GitHub release 中的方案 zip 文件名
    pub fn scheme_zip(&self) -> &'static str {
        match self {
            Schema::WanxiangBase => "rime-wanxiang-base.zip",
            Schema::WanxiangMoqi => "rime-wanxiang-moqi-fuzhu.zip",
            Schema::WanxiangFlypy => "rime-wanxiang-flypy-fuzhu.zip",
            Schema::WanxiangZrm => "rime-wanxiang-zrm-fuzhu.zip",
            Schema::WanxiangTiger => "rime-wanxiang-tiger-fuzhu.zip",
            Schema::WanxiangWubi => "rime-wanxiang-wubi-fuzhu.zip",
            Schema::WanxiangHanxin => "rime-wanxiang-hanxin-fuzhu.zip",
            Schema::WanxiangShouyou => "rime-wanxiang-shouyou-fuzhu.zip",
            Schema::WanxiangShyplus => "rime-wanxiang-shyplus-fuzhu.zip",
            Schema::WanxiangWx => "rime-wanxiang-wx-fuzhu.zip",
            Schema::Ice => "full.zip",
            Schema::Frost => "rime-frost-schemas.zip",
        }
    }

    /// 词库 zip 文件名 (万象共用，雾凇/白霜有各自的)
    pub fn dict_zip(&self) -> Option<&'static str> {
        match self {
            Schema::WanxiangBase => Some("base-dicts.zip"),
            Schema::WanxiangMoqi
            | Schema::WanxiangFlypy
            | Schema::WanxiangZrm
            | Schema::WanxiangTiger
            | Schema::WanxiangWubi
            | Schema::WanxiangHanxin
            | Schema::WanxiangShouyou
            | Schema::WanxiangShyplus
            | Schema::WanxiangWx => Some("pro-dicts.zip"),
            Schema::Ice => Some("all_dicts.zip"),
            Schema::Frost => None, // 白霜词库内嵌在方案 zip 中
        }
    }

    /// 词库 release tag
    pub fn dict_tag(&self) -> &'static str {
        match self {
            Schema::Ice => "", // 雾凇用最新 tag，和方案一起
            Schema::Frost => "1.0.0",
            _ => WX_DICT_TAG,
        }
    }

    /// 是否为万象系方案
    pub fn is_wanxiang(&self) -> bool {
        !matches!(self, Schema::Ice | Schema::Frost)
    }

    /// 是否支持模型 patch
    pub fn supports_model_patch(&self) -> bool {
        self.is_wanxiang()
    }

    /// Rime schema id (用于 patch 文件名)
    pub fn schema_id(&self) -> &'static str {
        match self {
            Schema::WanxiangBase => "wanxiang",
            Schema::WanxiangMoqi
            | Schema::WanxiangFlypy
            | Schema::WanxiangZrm
            | Schema::WanxiangTiger
            | Schema::WanxiangWubi
            | Schema::WanxiangHanxin
            | Schema::WanxiangShouyou
            | Schema::WanxiangShyplus
            | Schema::WanxiangWx => "wanxiang_pro",
            Schema::Ice => "rime_ice",
            Schema::Frost => "frost",
        }
    }

    /// 方案 zip 内含的主目录 (用于 CNB 镜像嵌套目录处理)
    pub fn extract_subdir(&self) -> Option<&'static str> {
        match self {
            Schema::Frost => None,
            _ => None, // 需要实际检测
        }
    }
}

impl fmt::Display for Schema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

impl std::str::FromStr for Schema {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "wanxiang" | "base" => Ok(Schema::WanxiangBase),
            "moqi" => Ok(Schema::WanxiangMoqi),
            "flypy" => Ok(Schema::WanxiangFlypy),
            "zrm" => Ok(Schema::WanxiangZrm),
            "tiger" => Ok(Schema::WanxiangTiger),
            "wubi" => Ok(Schema::WanxiangWubi),
            "hanxin" => Ok(Schema::WanxiangHanxin),
            "shouyou" => Ok(Schema::WanxiangShouyou),
            "shyplus" => Ok(Schema::WanxiangShyplus),
            "wx" => Ok(Schema::WanxiangWx),
            "ice" | "wusong" | "雾凇" => Ok(Schema::Ice),
            "frost" | "baishuang" | "白霜" => Ok(Schema::Frost),
            _ => anyhow::bail!("未知方案: {}", s),
        }
    }
}

// ── 引擎类型 ──
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Engine {
    Weasel,  // 小狼毫 (Windows)
    Squirrel, // 鼠须管 (macOS)
    Fcitx5,  // Linux Fcitx5
    IBus,    // Linux IBus
}

impl Engine {
    pub fn display_name(&self) -> &'static str {
        match self {
            Engine::Weasel => "小狼毫 (Weasel)",
            Engine::Squirrel => "鼠须管 (Squirrel)",
            Engine::Fcitx5 => "Fcitx5",
            Engine::IBus => "IBus",
        }
    }
}

// ── 配置 ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub schema: Schema,
    pub use_mirror: bool,
    pub github_token: String,
    pub proxy_enabled: bool,
    pub proxy_type: String,       // "socks5" | "http"
    pub proxy_address: String,    // "127.0.0.1:1080"
    pub exclude_files: Vec<String>,
    pub auto_update: bool,
    pub auto_update_countdown: i32,
    pub pre_update_hook: String,
    pub post_update_hook: String,
    pub language: String,         // "zh" | "en"
    pub fcitx_compat: bool,       // Linux: 同步到 ~/.config/fcitx/rime/
    pub fcitx_use_link: bool,     // 使用软链接还是复制
    pub model_patch_enabled: bool, // 是否自动 patch 模型
    pub skin_patch_key: String,   // 内置皮肤 key, 为空表示不 patch
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: Schema::WanxiangBase,
            use_mirror: false,
            github_token: String::new(),
            proxy_enabled: false,
            proxy_type: "socks5".into(),
            proxy_address: "127.0.0.1:1080".into(),
            exclude_files: vec![".DS_Store".into(), ".git".into()],
            auto_update: false,
            auto_update_countdown: 10,
            pre_update_hook: String::new(),
            post_update_hook: String::new(),
            language: "zh".into(),
            fcitx_compat: false,
            fcitx_use_link: true,
            model_patch_enabled: false,
            skin_patch_key: String::new(),
        }
    }
}

// ── 更新信息 ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub name: String,
    pub url: String,
    pub update_time: String, // ISO 8601
    pub tag: String,
    pub description: String,
    pub sha256: String,
    pub size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRecord {
    pub name: String,
    pub update_time: String,
    pub tag: String,
    pub apply_time: String,
    pub sha256: String,
}

// ── GitHub API 类型 ──
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub body: String,
    pub assets: Vec<GitHubAsset>,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    pub updated_at: Option<String>,
    pub size: i64,
    pub sha256: Option<String>,
}

// ── 更新状态 ──
#[derive(Debug, Clone)]
pub struct UpdateStatus {
    pub local_version: String,
    pub remote_version: String,
    pub needs_update: bool,
    pub message: String,
}
