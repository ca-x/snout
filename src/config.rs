use crate::i18n::{L10n, Lang};
use crate::types::{Config, Schema, UpdateRecord};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Manager {
    pub config_path: PathBuf,
    pub config: Config,
    pub rime_dir: PathBuf,
    pub cache_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExcludePatternType {
    Wildcard,
    Regex,
    Exact,
}

#[derive(Debug, Clone)]
pub struct ExcludePattern {
    pub original: String,
    pub kind: ExcludePatternType,
    regex: regex::Regex,
}

#[derive(Debug, Clone)]
pub struct WanxiangDiagnosis {
    pub detected_schema: Option<Schema>,
    pub record_schema: Option<Schema>,
    pub custom_patch_schema: Option<Schema>,
    pub config_schema: Option<Schema>,
    pub marker_files: Vec<(String, bool)>,
}

impl Manager {
    pub fn new() -> Result<Self> {
        let config_path = get_config_path()?;
        let mut config = load_or_create_config(&config_path)?;
        let rime_dir = detect_rime_dir();
        let cache_dir = get_cache_dir();

        // 确保目录存在
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(&cache_dir)?;

        let authoritative_schema = detect_authoritative_schema(&config, &cache_dir, &rime_dir);
        let schema_changed = authoritative_schema.is_some_and(|schema| schema != config.schema);
        if let Some(schema) = authoritative_schema {
            config.schema = schema;
        }

        let manager = Self {
            config_path,
            config,
            rime_dir,
            cache_dir,
        };

        if schema_changed {
            manager.save()?;
        }

        Ok(manager)
    }

    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.config)?;
        fs::write(&self.config_path, json)?;
        Ok(())
    }

    pub fn add_exclude_pattern(&mut self, pattern: String) -> Result<()> {
        let pattern = pattern.trim().to_string();
        if pattern.is_empty() {
            anyhow::bail!("exclude pattern cannot be empty");
        }
        parse_exclude_pattern(&pattern)?;
        if self.config.exclude_files.iter().any(|p| p == &pattern) {
            anyhow::bail!("exclude pattern already exists");
        }
        self.config.exclude_files.push(pattern);
        self.save()
    }

    pub fn update_exclude_pattern(&mut self, index: usize, pattern: String) -> Result<()> {
        if index >= self.config.exclude_files.len() {
            anyhow::bail!("exclude pattern index out of range");
        }
        let pattern = pattern.trim().to_string();
        if pattern.is_empty() {
            anyhow::bail!("exclude pattern cannot be empty");
        }
        parse_exclude_pattern(&pattern)?;
        self.config.exclude_files[index] = pattern;
        self.save()
    }

    pub fn remove_exclude_pattern(&mut self, index: usize) -> Result<()> {
        if index >= self.config.exclude_files.len() {
            anyhow::bail!("exclude pattern index out of range");
        }
        self.config.exclude_files.remove(index);
        self.save()
    }

    pub fn reset_exclude_patterns(&mut self) -> Result<()> {
        self.config.exclude_files = default_exclude_patterns();
        self.save()
    }

    #[allow(dead_code)]
    pub fn exclude_pattern_descriptions(&self) -> Result<Vec<String>> {
        let (patterns, errors) = parse_exclude_patterns(&self.config.exclude_files);
        if let Some(err) = errors.into_iter().next() {
            return Err(err);
        }
        Ok(patterns.iter().map(exclude_pattern_description).collect())
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

pub fn persist_installed_schema(schema: Schema) -> Result<()> {
    let config_path = get_config_path()?;
    let mut config = load_or_create_config(&config_path)?;
    if config.schema == schema {
        return Ok(());
    }

    config.schema = schema;
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(config_path, serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

pub fn rime_installation_message(lang: Lang) -> String {
    let t = L10n::new(lang);
    if !detect_installed_engines().is_empty() {
        return String::new();
    }

    #[cfg(target_os = "linux")]
    {
        format!(
            "⚠️  {}\n\n{}\n  • {}\n    - Debian/Ubuntu: sudo apt install fcitx5-rime\n    - Fedora: sudo dnf install fcitx5-rime\n    - Arch Linux: sudo pacman -S fcitx5-rime\n  • {}\n    - Debian/Ubuntu: sudo apt install ibus-rime\n    - Fedora: sudo dnf install ibus-rime\n    - Arch Linux: sudo pacman -S ibus-rime\n  • {}\n{}\n",
            t.t("wizard.no_engine"),
            t.t("wizard.install_one_of"),
            t.t("wizard.install.fcitx5"),
            "IBus + Rime - Linux",
            "Fcitx + Rime - Linux",
            t.t("install.hint.after_engine")
        )
    }

    #[cfg(target_os = "macos")]
    {
        format!(
            "⚠️  {}\n\n{}\n  • {}\n    brew install --cask squirrel\n  • Fcitx5 + Rime - macOS\n    brew install --cask tinypkg/tap/fcitx5-rime\n{}\n",
            t.t("wizard.no_engine"),
            t.t("wizard.install_one_of"),
            t.t("wizard.install.squirrel"),
            t.t("install.hint.after_engine")
        )
    }

    #[cfg(target_os = "windows")]
    {
        format!(
            "⚠️  {}\n\n{}\n  • {}\n    https://rime.im\n  • Rabbit - Windows\n    https://github.com/amorphobia/rabbit\n{}\n",
            t.t("wizard.no_engine"),
            t.t("wizard.install_one_of"),
            t.t("wizard.install.weasel"),
            t.t("install.hint.after_engine")
        )
    }
}

pub fn default_exclude_patterns() -> Vec<String> {
    vec![
        "*.userdb*".into(),
        "*.custom.yaml".into(),
        "installation.yaml".into(),
        "user.yaml".into(),
        "custom_phrase.txt".into(),
    ]
}

pub fn parse_exclude_pattern(pattern: &str) -> Result<Option<ExcludePattern>> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Ok(None);
    }

    let (kind, regex_source) = if has_regex_chars(pattern) {
        (ExcludePatternType::Regex, pattern.to_string())
    } else if pattern.contains('*') || pattern.contains('?') {
        (ExcludePatternType::Wildcard, wildcard_to_regex(pattern))
    } else {
        (
            ExcludePatternType::Exact,
            format!("^{}$", regex::escape(pattern)),
        )
    };

    let regex = regex::Regex::new(&regex_source)
        .with_context(|| format!("invalid exclude pattern: {pattern}"))?;

    Ok(Some(ExcludePattern {
        original: pattern.into(),
        kind,
        regex,
    }))
}

pub fn parse_exclude_patterns(patterns: &[String]) -> (Vec<ExcludePattern>, Vec<anyhow::Error>) {
    let mut parsed = Vec::new();
    let mut errors = Vec::new();

    for pattern in patterns {
        match parse_exclude_pattern(pattern) {
            Ok(Some(item)) => parsed.push(item),
            Ok(None) => {}
            Err(err) => errors.push(err),
        }
    }

    (parsed, errors)
}

pub fn matches_any_exclude_pattern(path: &Path, patterns: &[ExcludePattern]) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    patterns
        .iter()
        .any(|pattern| pattern.regex.is_match(&normalized) || pattern.regex.is_match(name))
}

pub fn exclude_pattern_description(pattern: &ExcludePattern) -> String {
    let label = match pattern.kind {
        ExcludePatternType::Wildcard => "通配符",
        ExcludePatternType::Regex => "正则",
        ExcludePatternType::Exact => "精确",
    };
    format!("{label}: {}", pattern.original)
}

fn wildcard_to_regex(pattern: &str) -> String {
    let mut result = String::from("^");
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    result.push_str(".*");
                    i += 1;
                } else {
                    result.push_str("[^/\\\\]*");
                }
            }
            '?' => result.push_str("[^/\\\\]"),
            c if ".+^$()[]{}|\\".contains(c) => {
                result.push('\\');
                result.push(c);
            }
            c => result.push(c),
        }
        i += 1;
    }
    result.push('$');
    result
}

fn has_regex_chars(pattern: &str) -> bool {
    [
        "^", "$", "[", "]", "(", ")", "{", "}", "|", "+", "\\", "\\.",
    ]
    .iter()
    .any(|token| pattern.contains(token))
}

pub fn effective_exclude_patterns(config: &Config) -> Vec<String> {
    let mut patterns = default_exclude_patterns();
    for item in &config.exclude_files {
        if !patterns.iter().any(|p| p == item) {
            patterns.push(item.clone());
        }
    }
    patterns
}

pub fn diagnose_wanxiang(config: &Config, cache_dir: &Path, rime_dir: &Path) -> WanxiangDiagnosis {
    let record_schema = detect_schema_from_record(cache_dir, rime_dir);
    let custom_patch_schema = detect_wanxiang_pro_variant(rime_dir);
    let detected_schema = detect_authoritative_schema(config, cache_dir, rime_dir);
    let config_schema = config.schema.is_wanxiang().then_some(config.schema);
    let marker_files = wanxiang_pro_marker_paths(rime_dir)
        .into_iter()
        .map(|path| {
            (
                path.strip_prefix(rime_dir)
                    .unwrap_or(&path)
                    .display()
                    .to_string(),
                path.exists(),
            )
        })
        .collect();
    WanxiangDiagnosis {
        detected_schema,
        record_schema,
        custom_patch_schema,
        config_schema,
        marker_files,
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

fn detect_authoritative_schema(
    config: &Config,
    cache_dir: &Path,
    rime_dir: &Path,
) -> Option<Schema> {
    detect_schema_from_record(cache_dir, rime_dir)
        .or_else(|| detect_schema_from_files(config, rime_dir))
}

fn detect_schema_from_record(cache_dir: &Path, rime_dir: &Path) -> Option<Schema> {
    let record_path = cache_dir.join("scheme_record.json");
    let data = fs::read_to_string(record_path).ok()?;
    let record = serde_json::from_str::<UpdateRecord>(&data).ok()?;
    let schema = Schema::from_scheme_archive_name(&record.name)?;
    schema_record_matches_files(schema, rime_dir).then_some(schema)
}

fn detect_schema_from_files(config: &Config, rime_dir: &Path) -> Option<Schema> {
    for schema in [Schema::Ice, Schema::Frost, Schema::Mint] {
        if schema_record_matches_files(schema, rime_dir) {
            return Some(schema);
        }
    }

    if let Some(schema) = detect_wanxiang_schema_from_files(config, rime_dir) {
        return Some(schema);
    }

    schema_record_matches_files(Schema::WanxiangBase, rime_dir).then_some(Schema::WanxiangBase)
}

fn detect_wanxiang_schema_from_files(config: &Config, rime_dir: &Path) -> Option<Schema> {
    if wanxiang_pro_markers_exist(rime_dir) {
        return detect_wanxiang_pro_variant(rime_dir).or_else(|| {
            matches!(
                config.schema,
                Schema::WanxiangMoqi
                    | Schema::WanxiangFlypy
                    | Schema::WanxiangZrm
                    | Schema::WanxiangTiger
                    | Schema::WanxiangWubi
                    | Schema::WanxiangHanxin
                    | Schema::WanxiangShouyou
                    | Schema::WanxiangShyplus
                    | Schema::WanxiangWx
            )
            .then_some(config.schema)
        });
    }

    if config.schema.is_wanxiang() && schema_record_matches_files(config.schema, rime_dir) {
        return Some(config.schema);
    }

    None
}

fn wanxiang_pro_markers_exist(rime_dir: &Path) -> bool {
    wanxiang_pro_marker_paths(rime_dir)
        .into_iter()
        .any(|path| path.exists())
}

fn wanxiang_pro_marker_paths(rime_dir: &Path) -> Vec<PathBuf> {
    vec![
        rime_dir.join("wanxiang_pro.schema.yaml"),
        rime_dir.join("wanxiang_pro.custom.yaml"),
        rime_dir.join("wanxiang_pro.dict.yaml"),
        rime_dir.join("custom").join("wanxiang_pro.schema.yaml"),
        rime_dir.join("custom").join("wanxiang_pro.custom.yaml"),
        rime_dir.join("custom").join("wanxiang_pro.dict.yaml"),
    ]
}

fn detect_wanxiang_pro_variant(rime_dir: &Path) -> Option<Schema> {
    wanxiang_pro_custom_paths(rime_dir)
        .into_iter()
        .find_map(|path| {
            let content = fs::read_to_string(path).ok()?;
            parse_wanxiang_pro_variant_from_text(&content)
        })
}

fn wanxiang_pro_custom_paths(rime_dir: &Path) -> Vec<PathBuf> {
    vec![
        rime_dir.join("wanxiang_pro.custom.yaml"),
        rime_dir.join("custom").join("wanxiang_pro.custom.yaml"),
    ]
}

fn parse_wanxiang_pro_variant_from_text(content: &str) -> Option<Schema> {
    let lowered = content.to_ascii_lowercase();
    for (schema, ascii_markers, unicode_markers) in [
        (Schema::WanxiangShyplus, &["shyplus"][..], &["首右+"][..]),
        (Schema::WanxiangShouyou, &["shouyou"][..], &["首右"][..]),
        (Schema::WanxiangHanxin, &["hanxin"][..], &["汉心"][..]),
        (Schema::WanxiangWubi, &["wubi"][..], &["五笔"][..]),
        (Schema::WanxiangTiger, &["tiger"][..], &["虎码"][..]),
        (Schema::WanxiangZrm, &["zrm"][..], &["自然码"][..]),
        (Schema::WanxiangFlypy, &["flypy"][..], &["小鹤"][..]),
        (Schema::WanxiangMoqi, &["moqi"][..], &["墨奇"][..]),
        (
            Schema::WanxiangWx,
            &["/pro/wx", "wx_chaifen"][..],
            &["万象辅助"][..],
        ),
    ] {
        if ascii_markers.iter().any(|marker| lowered.contains(marker))
            || unicode_markers
                .iter()
                .any(|marker| content.contains(marker))
        {
            return Some(schema);
        }
    }

    None
}

fn schema_record_matches_files(schema: Schema, rime_dir: &Path) -> bool {
    match schema {
        Schema::WanxiangBase => rime_dir.join("wanxiang.schema.yaml").exists(),
        Schema::WanxiangMoqi
        | Schema::WanxiangFlypy
        | Schema::WanxiangZrm
        | Schema::WanxiangTiger
        | Schema::WanxiangWubi
        | Schema::WanxiangHanxin
        | Schema::WanxiangShouyou
        | Schema::WanxiangShyplus
        | Schema::WanxiangWx => wanxiang_pro_markers_exist(rime_dir),
        Schema::Ice => rime_dir.join("rime_ice.schema.yaml").exists(),
        Schema::Frost => rime_dir.join("rime_frost.schema.yaml").exists(),
        Schema::Mint => rime_dir.join("rime_mint.schema.yaml").exists(),
    }
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

#[cfg(not(target_os = "linux"))]
fn which_exists(_cmd: &str) -> bool {
    false
}

#[cfg(target_os = "linux")]
fn fcitx5_rime_installed() -> bool {
    linux_fcitx5_rime_dir().exists() || linux_fcitx5_config_rime_dir().exists()
}

#[cfg(not(target_os = "linux"))]
fn fcitx5_rime_installed() -> bool {
    false
}

#[cfg(target_os = "linux")]
fn fcitx_rime_installed() -> bool {
    linux_fcitx_rime_dir().exists()
}

#[cfg(not(target_os = "linux"))]
fn fcitx_rime_installed() -> bool {
    false
}

#[cfg(target_os = "linux")]
fn linux_fcitx5_rime_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_default().join("fcitx5/rime")
}

#[cfg(not(target_os = "linux"))]
fn linux_fcitx5_rime_dir() -> PathBuf {
    PathBuf::new()
}

#[cfg(target_os = "linux")]
fn linux_fcitx5_config_rime_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/fcitx5/rime")
}

#[cfg(not(target_os = "linux"))]
fn linux_fcitx5_config_rime_dir() -> PathBuf {
    PathBuf::new()
}

#[cfg(target_os = "linux")]
fn linux_ibus_rime_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/ibus/rime")
}

#[cfg(not(target_os = "linux"))]
fn linux_ibus_rime_dir() -> PathBuf {
    PathBuf::new()
}

#[cfg(target_os = "linux")]
fn linux_fcitx_rime_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/fcitx/rime")
}

#[cfg(not(target_os = "linux"))]
fn linux_fcitx_rime_dir() -> PathBuf {
    PathBuf::new()
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

#[cfg(not(target_os = "linux"))]
fn linux_rime_dir_candidates() -> Vec<PathBuf> {
    Vec::new()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("snout-config-{name}-{nanos}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn authoritative_schema_prefers_successful_scheme_record() {
        let cache_dir = temp_dir("record-cache");
        let rime_dir = temp_dir("record-rime");
        std::fs::write(rime_dir.join("rime_ice.schema.yaml"), "").expect("write ice schema");
        std::fs::write(rime_dir.join("wanxiang.schema.yaml"), "").expect("write wanxiang schema");
        std::fs::write(
            cache_dir.join("scheme_record.json"),
            serde_json::to_string(&UpdateRecord {
                name: "full.zip".into(),
                update_time: String::new(),
                tag: "v1".into(),
                apply_time: String::new(),
                sha256: String::new(),
            })
            .expect("serialize record"),
        )
        .expect("write record");

        let schema = detect_authoritative_schema(&Config::default(), &cache_dir, &rime_dir);

        assert_eq!(schema, Some(Schema::Ice));
        std::fs::remove_dir_all(cache_dir).ok();
        std::fs::remove_dir_all(rime_dir).ok();
    }

    #[test]
    fn authoritative_schema_falls_back_to_existing_files() {
        let cache_dir = temp_dir("files-cache");
        let rime_dir = temp_dir("files-rime");
        std::fs::write(rime_dir.join("rime_frost.schema.yaml"), "").expect("write frost schema");

        let schema = detect_authoritative_schema(&Config::default(), &cache_dir, &rime_dir);

        assert_eq!(schema, Some(Schema::Frost));
        std::fs::remove_dir_all(cache_dir).ok();
        std::fs::remove_dir_all(rime_dir).ok();
    }

    #[test]
    fn authoritative_schema_prefers_configured_wanxiang_pro_over_base_file_probe() {
        let cache_dir = temp_dir("pro-preferred-cache");
        let rime_dir = temp_dir("pro-preferred-rime");
        std::fs::write(rime_dir.join("wanxiang.schema.yaml"), "").expect("write base schema");
        std::fs::write(rime_dir.join("wanxiang_pro.schema.yaml"), "").expect("write pro schema");

        let config = Config {
            schema: Schema::WanxiangMoqi,
            ..Config::default()
        };

        let schema = detect_authoritative_schema(&config, &cache_dir, &rime_dir);

        assert_eq!(schema, Some(Schema::WanxiangMoqi));
        std::fs::remove_dir_all(cache_dir).ok();
        std::fs::remove_dir_all(rime_dir).ok();
    }

    #[test]
    fn authoritative_schema_detects_wanxiang_pro_variant_from_custom_patch() {
        let cache_dir = temp_dir("pro-custom-cache");
        let rime_dir = temp_dir("pro-custom-rime");
        std::fs::create_dir_all(rime_dir.join("custom")).expect("create custom dir");
        std::fs::write(rime_dir.join("wanxiang.schema.yaml"), "").expect("write base schema");
        std::fs::write(
            rime_dir.join("custom").join("wanxiang_pro.custom.yaml"),
            "patch:
  speller/algebra:
    __patch:
      - wanxiang_algebra:/pro/自然码
      - wanxiang_algebra:/pro/直接辅助
",
        )
        .expect("write pro custom");

        let schema = detect_authoritative_schema(&Config::default(), &cache_dir, &rime_dir);

        assert_eq!(schema, Some(Schema::WanxiangZrm));
        std::fs::remove_dir_all(cache_dir).ok();
        std::fs::remove_dir_all(rime_dir).ok();
    }

    #[test]
    fn authoritative_schema_detects_wanxiang_pro_wx_variant_from_custom_patch() {
        let cache_dir = temp_dir("pro-wx-cache");
        let rime_dir = temp_dir("pro-wx-rime");
        std::fs::write(rime_dir.join("wanxiang.schema.yaml"), "").expect("write base schema");
        std::fs::write(rime_dir.join("wanxiang_pro.dict.yaml"), "").expect("write pro dict");
        std::fs::write(
            rime_dir.join("wanxiang_pro.custom.yaml"),
            "patch:
  custom_phrase/user_dict: custom
  comment_format:
    - xform/^.*$/万象辅助/
  aux_code: wx_chaifen
",
        )
        .expect("write wx custom");

        let schema = detect_authoritative_schema(&Config::default(), &cache_dir, &rime_dir);

        assert_eq!(schema, Some(Schema::WanxiangWx));
        std::fs::remove_dir_all(cache_dir).ok();
        std::fs::remove_dir_all(rime_dir).ok();
    }
    #[test]
    fn exclude_pattern_parser_supports_wildcard_regex_and_exact() {
        let wildcard = parse_exclude_pattern("*.userdb*")
            .expect("parse wildcard")
            .expect("pattern");
        assert!(matches!(wildcard.kind, ExcludePatternType::Wildcard));
        assert!(matches_any_exclude_pattern(
            Path::new("user_flypyzc.userdb"),
            &[wildcard]
        ));

        let regex = parse_exclude_pattern(r"^sync/.*$")
            .expect("parse regex")
            .expect("pattern");
        assert!(matches!(regex.kind, ExcludePatternType::Regex));
        assert!(matches_any_exclude_pattern(
            Path::new("sync/data.yaml"),
            &[regex]
        ));

        let exact = parse_exclude_pattern("installation.yaml")
            .expect("parse exact")
            .expect("pattern");
        assert!(matches!(exact.kind, ExcludePatternType::Exact));
        assert!(matches_any_exclude_pattern(
            Path::new("installation.yaml"),
            &[exact]
        ));
    }
}
