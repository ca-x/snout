use crate::i18n::{L10n, Lang};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

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

// 薄荷
pub const MINT_OWNER: &str = "Mintimate";
pub const MINT_REPO: &str = "oh-my-rime";
pub const MINT_BRANCH: &str = "main";
pub const MINT_ARCHIVE: &str = "oh-my-rime.zip";

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
    Ice,   // 雾凇
    Frost, // 白霜
    Mint,  // 薄荷
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
            Schema::Mint,
        ]
    }

    pub fn i18n_key(&self) -> &'static str {
        match self {
            Schema::WanxiangBase => "schema.wanxiang_base",
            Schema::WanxiangMoqi => "schema.wanxiang_moqi",
            Schema::WanxiangFlypy => "schema.wanxiang_flypy",
            Schema::WanxiangZrm => "schema.wanxiang_zrm",
            Schema::WanxiangTiger => "schema.wanxiang_tiger",
            Schema::WanxiangWubi => "schema.wanxiang_wubi",
            Schema::WanxiangHanxin => "schema.wanxiang_hanxin",
            Schema::WanxiangShouyou => "schema.wanxiang_shouyou",
            Schema::WanxiangShyplus => "schema.wanxiang_shyplus",
            Schema::WanxiangWx => "schema.wanxiang_wx",
            Schema::Ice => "schema.ice",
            Schema::Frost => "schema.frost",
            Schema::Mint => "schema.mint",
        }
    }

    pub fn display_name(&self) -> String {
        self.display_name_lang(Lang::Zh)
    }

    /// 多语言显示名称
    pub fn display_name_lang(&self, lang: Lang) -> String {
        L10n::new(lang).t(self.i18n_key()).to_string()
    }

    /// 所属仓库 owner
    pub fn owner(&self) -> &'static str {
        match self {
            Schema::Ice => ICE_OWNER,
            Schema::Frost => FROST_OWNER,
            Schema::Mint => MINT_OWNER,
            _ => WX_OWNER,
        }
    }

    /// 所属仓库名
    pub fn repo(&self) -> &'static str {
        match self {
            Schema::Ice => ICE_REPO,
            Schema::Frost => FROST_REPO,
            Schema::Mint => MINT_REPO,
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
            Schema::Mint => MINT_ARCHIVE,
        }
    }

    /// 词库 zip 文件名
    pub fn dict_zip(&self) -> Option<&'static str> {
        match self {
            Schema::WanxiangBase => Some("base-dicts.zip"),
            Schema::WanxiangMoqi => Some("pro-moqi-fuzhu-dicts.zip"),
            Schema::WanxiangFlypy => Some("pro-flypy-fuzhu-dicts.zip"),
            Schema::WanxiangZrm => Some("pro-zrm-fuzhu-dicts.zip"),
            Schema::WanxiangTiger => Some("pro-tiger-fuzhu-dicts.zip"),
            Schema::WanxiangWubi => Some("pro-wubi-fuzhu-dicts.zip"),
            Schema::WanxiangHanxin => Some("pro-hanxin-fuzhu-dicts.zip"),
            Schema::WanxiangShouyou => Some("pro-shouyou-fuzhu-dicts.zip"),
            Schema::WanxiangShyplus => Some("pro-shyplus-fuzhu-dicts.zip"),
            Schema::WanxiangWx => Some("pro-wx-fuzhu-dicts.zip"),
            Schema::Ice => Some("all_dicts.zip"),
            Schema::Frost => None, // 白霜词库内嵌在方案 zip 中
            Schema::Mint => None,  // 薄荷随方案仓库一起分发
        }
    }

    /// 词库 release tag
    pub fn dict_tag(&self) -> &'static str {
        match self {
            Schema::Ice => "", // 雾凇用最新 tag，和方案一起
            Schema::Frost => "1.0.0",
            Schema::Mint => "",
            _ => WX_DICT_TAG,
        }
    }

    /// 是否为万象系方案
    pub fn is_wanxiang(&self) -> bool {
        !matches!(self, Schema::Ice | Schema::Frost | Schema::Mint)
    }

    /// 是否支持将万象模型 patch 到当前方案
    pub fn supports_model_patch(&self) -> bool {
        true
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
            Schema::Frost => "rime_frost",
            Schema::Mint => "rime_mint",
        }
    }

    /// 方案 zip 内含的主目录 (用于 CNB 镜像嵌套目录处理)
    #[allow(dead_code)]
    pub fn extract_subdir(&self) -> Option<&'static str> {
        match self {
            Schema::Frost => None,
            _ => None, // 需要实际检测
        }
    }

    pub fn parse_with_lang(s: &str, lang: Lang) -> anyhow::Result<Self> {
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
            "mint" | "bohe" | "薄荷" => Ok(Schema::Mint),
            _ => anyhow::bail!("{}: {}", L10n::new(lang).t("schema.unknown"), s),
        }
    }

    pub fn from_scheme_archive_name(name: &str) -> Option<Self> {
        match name {
            "rime-wanxiang-base.zip" => Some(Schema::WanxiangBase),
            "rime-wanxiang-moqi-fuzhu.zip" => Some(Schema::WanxiangMoqi),
            "rime-wanxiang-flypy-fuzhu.zip" => Some(Schema::WanxiangFlypy),
            "rime-wanxiang-zrm-fuzhu.zip" => Some(Schema::WanxiangZrm),
            "rime-wanxiang-tiger-fuzhu.zip" => Some(Schema::WanxiangTiger),
            "rime-wanxiang-wubi-fuzhu.zip" => Some(Schema::WanxiangWubi),
            "rime-wanxiang-hanxin-fuzhu.zip" => Some(Schema::WanxiangHanxin),
            "rime-wanxiang-shouyou-fuzhu.zip" => Some(Schema::WanxiangShouyou),
            "rime-wanxiang-shyplus-fuzhu.zip" => Some(Schema::WanxiangShyplus),
            "rime-wanxiang-wx-fuzhu.zip" => Some(Schema::WanxiangWx),
            "full.zip" => Some(Schema::Ice),
            "rime-frost-schemas.zip" => Some(Schema::Frost),
            MINT_ARCHIVE => Some(Schema::Mint),
            _ => None,
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
        Schema::parse_with_lang(s, Lang::En)
    }
}

// ── 引擎类型 ──
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Engine {
    Weasel,   // 小狼毫 (Windows)
    Squirrel, // 鼠须管 (macOS)
    Fcitx5,   // Linux Fcitx5
    IBus,     // Linux IBus
}

impl Engine {
    #[allow(dead_code)]
    pub fn display_name(&self) -> String {
        match self {
            Engine::Weasel => "Weasel".into(),
            Engine::Squirrel => "Squirrel".into(),
            Engine::Fcitx5 => "Fcitx5".into(),
            Engine::IBus => "IBus".into(),
        }
    }
}

// ── 配置 ──
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub schema: Schema,
    pub use_mirror: bool,
    pub github_token: String,
    pub proxy_enabled: bool,
    pub proxy_type: String,    // "socks5" | "http"
    pub proxy_address: String, // "127.0.0.1:1080"
    pub exclude_files: Vec<String>,
    pub auto_update: bool,
    pub auto_update_countdown: i32,
    pub pre_update_hook: String,
    pub post_update_hook: String,
    pub language: String,           // "zh" | "en"
    pub engine_sync_enabled: bool,  // 是否同步到其他已安装引擎目录
    pub engine_sync_use_link: bool, // 使用软链接还是复制
    pub model_patch_enabled: bool,  // 是否自动 patch 模型
    pub skin_patch_key: String,     // 内置皮肤 key, 为空表示不 patch
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
            engine_sync_enabled: false,
            engine_sync_use_link: true,
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

#[derive(Clone, Debug, Default)]
pub struct CancelSignal {
    cancelled: Arc<AtomicBool>,
}

#[derive(Debug)]
pub struct UpdateCancelled;

impl fmt::Display for UpdateCancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "update cancelled")
    }
}

impl std::error::Error for UpdateCancelled {}

impl CancelSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn checkpoint(&self) -> Result<()> {
        if self.is_cancelled() {
            return Err(anyhow::Error::new(UpdateCancelled));
        }
        Ok(())
    }
}

// ── GitHub API 类型 ──
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub body: String,
    pub assets: Vec<GitHubAsset>,
    #[allow(dead_code)]
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    pub updated_at: Option<String>,
    pub size: i64,
    pub sha256: Option<String>,
    pub digest: Option<String>,
}

// ── 更新状态 ──
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct UpdateStatus {
    pub local_version: String,
    pub remote_version: String,
    pub needs_update: bool,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_display() {
        assert_eq!(Schema::WanxiangBase.display_name(), "万象拼音 (标准版)");
        assert_eq!(Schema::Ice.display_name(), "雾凇拼音");
        assert_eq!(Schema::Frost.display_name(), "白霜拼音");
        assert_eq!(Schema::Mint.display_name(), "薄荷输入法");
        assert_eq!(
            Schema::WanxiangBase.display_name_lang(Lang::Zh),
            "万象拼音 (标准版)"
        );
        assert_eq!(Schema::Ice.display_name_lang(Lang::Zh), "雾凇拼音");
        assert_eq!(Schema::Frost.display_name_lang(Lang::Zh), "白霜拼音");
        assert_eq!(Schema::Mint.display_name_lang(Lang::Zh), "薄荷输入法");
        assert_eq!(
            Schema::WanxiangBase.display_name_lang(Lang::En),
            "Wanxiang (Base)"
        );
        assert_eq!(
            Schema::WanxiangMoqi.display_name_lang(Lang::En),
            "Wanxiang Pro (Moqi)"
        );
        assert_eq!(Schema::Ice.display_name_lang(Lang::En), "Rime Ice");
        assert_eq!(Schema::Frost.display_name_lang(Lang::En), "Rime Frost");
        assert_eq!(Schema::Mint.display_name_lang(Lang::En), "Mint Input");
    }

    #[test]
    fn test_schema_all_includes_mint() {
        assert!(Schema::all().contains(&Schema::Mint));
    }

    #[test]
    fn test_schema_is_wanxiang() {
        assert!(Schema::WanxiangBase.is_wanxiang());
        assert!(Schema::WanxiangMoqi.is_wanxiang());
        assert!(!Schema::Ice.is_wanxiang());
        assert!(!Schema::Frost.is_wanxiang());
        assert!(!Schema::Mint.is_wanxiang());
    }

    #[test]
    fn test_schema_supports_model_patch() {
        assert!(Schema::WanxiangBase.supports_model_patch());
        assert!(Schema::Ice.supports_model_patch());
        assert!(Schema::Frost.supports_model_patch());
        assert!(Schema::Mint.supports_model_patch());
    }

    #[test]
    fn test_schema_from_str() {
        assert_eq!("wanxiang".parse::<Schema>().unwrap(), Schema::WanxiangBase);
        assert_eq!("ice".parse::<Schema>().unwrap(), Schema::Ice);
        assert_eq!("雾凇".parse::<Schema>().unwrap(), Schema::Ice);
        assert_eq!("frost".parse::<Schema>().unwrap(), Schema::Frost);
        assert_eq!("白霜".parse::<Schema>().unwrap(), Schema::Frost);
        assert_eq!("mint".parse::<Schema>().unwrap(), Schema::Mint);
        assert_eq!("薄荷".parse::<Schema>().unwrap(), Schema::Mint);
        assert!("unknown".parse::<Schema>().is_err());
        assert!(Schema::parse_with_lang("unknown", Lang::Zh)
            .unwrap_err()
            .to_string()
            .contains("未知方案"));
    }

    #[test]
    fn test_schema_zip_names() {
        assert_eq!(Schema::WanxiangBase.scheme_zip(), "rime-wanxiang-base.zip");
        assert_eq!(Schema::Ice.scheme_zip(), "full.zip");
        assert_eq!(Schema::Frost.scheme_zip(), "rime-frost-schemas.zip");
        assert_eq!(Schema::Mint.scheme_zip(), "oh-my-rime.zip");
    }

    #[test]
    fn test_schema_dict_zip() {
        assert_eq!(Schema::WanxiangBase.dict_zip(), Some("base-dicts.zip"));
        assert_eq!(
            Schema::WanxiangMoqi.dict_zip(),
            Some("pro-moqi-fuzhu-dicts.zip")
        );
        assert_eq!(
            Schema::WanxiangFlypy.dict_zip(),
            Some("pro-flypy-fuzhu-dicts.zip")
        );
        assert_eq!(
            Schema::WanxiangZrm.dict_zip(),
            Some("pro-zrm-fuzhu-dicts.zip")
        );
        assert_eq!(
            Schema::WanxiangTiger.dict_zip(),
            Some("pro-tiger-fuzhu-dicts.zip")
        );
        assert_eq!(
            Schema::WanxiangWubi.dict_zip(),
            Some("pro-wubi-fuzhu-dicts.zip")
        );
        assert_eq!(
            Schema::WanxiangHanxin.dict_zip(),
            Some("pro-hanxin-fuzhu-dicts.zip")
        );
        assert_eq!(
            Schema::WanxiangShouyou.dict_zip(),
            Some("pro-shouyou-fuzhu-dicts.zip")
        );
        assert_eq!(
            Schema::WanxiangShyplus.dict_zip(),
            Some("pro-shyplus-fuzhu-dicts.zip")
        );
        assert_eq!(
            Schema::WanxiangWx.dict_zip(),
            Some("pro-wx-fuzhu-dicts.zip")
        );
        assert_eq!(Schema::Ice.dict_zip(), Some("all_dicts.zip"));
        assert_eq!(Schema::Frost.dict_zip(), None);
        assert_eq!(Schema::Mint.dict_zip(), None);
    }

    #[test]
    fn test_schema_owner_repo() {
        assert_eq!(Schema::WanxiangBase.owner(), "amzxyz");
        assert_eq!(Schema::WanxiangBase.repo(), "rime_wanxiang");
        assert_eq!(Schema::Ice.owner(), "iDvel");
        assert_eq!(Schema::Ice.repo(), "rime-ice");
        assert_eq!(Schema::Frost.owner(), "gaboolic");
        assert_eq!(Schema::Frost.repo(), "rime-frost");
        assert_eq!(Schema::Mint.owner(), "Mintimate");
        assert_eq!(Schema::Mint.repo(), "oh-my-rime");
    }

    #[test]
    fn test_schema_id() {
        assert_eq!(Schema::WanxiangBase.schema_id(), "wanxiang");
        assert_eq!(Schema::WanxiangMoqi.schema_id(), "wanxiang_pro");
        assert_eq!(Schema::Ice.schema_id(), "rime_ice");
        assert_eq!(Schema::Frost.schema_id(), "rime_frost");
        assert_eq!(Schema::Mint.schema_id(), "rime_mint");
    }

    #[test]
    fn test_config_defaults() {
        let config = Config::default();
        assert_eq!(config.schema, Schema::WanxiangBase);
        assert!(!config.use_mirror);
        assert!(!config.proxy_enabled);
        assert!(!config.model_patch_enabled);
        assert_eq!(config.language, "zh");
    }

    #[test]
    fn test_config_serialization_uses_engine_sync_field_names() {
        let json = serde_json::to_string(&Config::default()).expect("serialize config");
        assert!(json.contains("engine_sync_enabled"));
        assert!(json.contains("engine_sync_use_link"));
        assert!(!json.contains("fcitx_compat"));
        assert!(!json.contains("fcitx_use_link"));
    }

    #[test]
    fn test_schema_from_scheme_archive_name() {
        assert_eq!(
            Schema::from_scheme_archive_name("rime-wanxiang-base.zip"),
            Some(Schema::WanxiangBase)
        );
        assert_eq!(
            Schema::from_scheme_archive_name("full.zip"),
            Some(Schema::Ice)
        );
        assert_eq!(
            Schema::from_scheme_archive_name("rime-frost-schemas.zip"),
            Some(Schema::Frost)
        );
        assert_eq!(
            Schema::from_scheme_archive_name(MINT_ARCHIVE),
            Some(Schema::Mint)
        );
        assert_eq!(Schema::from_scheme_archive_name("unknown.zip"), None);
    }
}
