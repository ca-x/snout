use crate::i18n::{L10n, Lang};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use zbus::blocking::{Connection, Proxy};

include!(concat!(env!("OUT_DIR"), "/fcitx5_theme_manifest.rs"));

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemeSelection {
    pub light: Option<String>,
    pub dark: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FcitxThemeConfig {
    theme: Option<String>,
    dark_theme: Option<String>,
    use_dark_theme: Option<bool>,
    follow_system_dark_mode: Option<bool>,
}

impl FcitxThemeConfig {
    fn for_pair(light_theme: &str, dark_theme: &str) -> Self {
        Self {
            theme: Some(light_theme.to_string()),
            dark_theme: Some(dark_theme.to_string()),
            use_dark_theme: Some(true),
            follow_system_dark_mode: Some(true),
        }
    }

    fn to_json_payload(&self) -> String {
        serde_json::json!({
            "Theme": self.theme,
            "DarkTheme": self.dark_theme,
            "UseDarkTheme": self.use_dark_theme,
            "FollowSystemDarkMode": self.follow_system_dark_mode,
        })
        .to_string()
    }
}

pub fn builtin_theme_choices() -> Vec<(String, String)> {
    FCITX5_THEME_NAMES
        .iter()
        .map(|name| ((*name).to_string(), (*name).to_string()))
        .collect()
}

pub fn builtin_themes_available() -> bool {
    !FCITX5_THEME_NAMES.is_empty()
}

pub fn theme_supported(installed_engines: &[String]) -> bool {
    #[cfg(target_os = "linux")]
    {
        installed_engines.iter().any(|engine| engine == "fcitx5")
            || which_exists("fcitx5")
            || which_exists("fcitx5-remote")
            || fcitx_theme_root_path()
                .map(|path| path.exists())
                .unwrap_or(false)
            || fcitx_classicui_config_path()
                .map(|path| path.exists())
                .unwrap_or(false)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = installed_engines;
        false
    }
}

pub fn installed_theme_names() -> Result<HashSet<String>> {
    let root = fcitx_theme_root_path()?;
    if !root.exists() {
        return Ok(HashSet::new());
    }

    let mut themes = HashSet::new();
    for entry in std::fs::read_dir(root).context("read fcitx5 theme root")? {
        let entry = entry.context("read fcitx5 theme entry")?;
        if entry.path().is_dir() {
            themes.insert(entry.file_name().to_string_lossy().to_string());
        }
    }
    Ok(themes)
}

pub fn current_theme_selection() -> Result<ThemeSelection> {
    let config_path = fcitx_classicui_config_path()?;
    if !config_path.exists() {
        return Ok(ThemeSelection::default());
    }

    let content = std::fs::read_to_string(config_path).context("read fcitx5 classicui config")?;
    let config = read_theme_config(&content);
    Ok(ThemeSelection {
        light: config.theme,
        dark: config.dark_theme,
    })
}

pub fn apply_theme_pair(
    light_theme: &str,
    dark_theme: &str,
    light_rounded: Option<bool>,
    dark_rounded: Option<bool>,
    lang: Lang,
) -> Result<()> {
    install_theme(light_theme, light_rounded)?;
    if dark_theme != light_theme || dark_rounded != light_rounded {
        install_theme(dark_theme, dark_rounded)?;
    }
    write_theme_setting_pair(light_theme, dark_theme)?;
    reload_theme(lang)
}

pub fn apply_theme(theme_name: &str, rounded: Option<bool>, lang: Lang) -> Result<()> {
    apply_theme_pair(theme_name, theme_name, rounded, rounded, lang)
}

pub fn theme_supports_optional_rounding(theme_name: &str) -> bool {
    if !theme_name.starts_with("catppuccin-") {
        return false;
    }
    theme_file_text(theme_name, "theme.conf")
        .map(|content| optional_rounding_state(&content).is_some())
        .unwrap_or(false)
}

pub fn installed_theme_rounding(theme_name: &str) -> Result<Option<bool>> {
    let path = fcitx_theme_root_path()?.join(theme_name).join("theme.conf");
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path).context("read installed theme.conf")?;
    Ok(optional_rounding_state(&content))
}

fn install_theme(theme_name: &str, rounded: Option<bool>) -> Result<()> {
    if !FCITX5_THEME_NAMES.contains(&theme_name) {
        anyhow::bail!("unknown fcitx5 theme: {theme_name}");
    }

    let root = fcitx_theme_root_path()?;
    let target_dir = root.join(theme_name);
    if target_dir.exists() {
        std::fs::remove_dir_all(&target_dir).context("remove existing fcitx5 theme")?;
    }
    std::fs::create_dir_all(&target_dir).context("create fcitx5 theme directory")?;

    let mut installed_any = false;
    for (theme, rel_path, bytes) in FCITX5_THEME_FILES {
        if *theme != theme_name {
            continue;
        }
        installed_any = true;
        let path = target_dir.join(rel_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("create fcitx5 theme parent")?;
        }
        if *rel_path == "theme.conf" {
            let content = String::from_utf8_lossy(bytes);
            let rendered = render_theme_conf(&content, rounded);
            std::fs::write(path, rendered).context("write fcitx5 theme file")?;
        } else {
            std::fs::write(path, bytes).context("write fcitx5 theme file")?;
        }
    }

    if !installed_any {
        anyhow::bail!("embedded fcitx5 theme has no files: {theme_name}");
    }

    Ok(())
}

fn theme_file_text(theme_name: &str, rel_path: &str) -> Option<String> {
    FCITX5_THEME_FILES
        .iter()
        .find(|(theme, path, _)| *theme == theme_name && *path == rel_path)
        .map(|(_, _, bytes)| String::from_utf8_lossy(bytes).into_owned())
}

fn render_theme_conf(content: &str, rounded: Option<bool>) -> String {
    if rounded != Some(true) {
        if content.ends_with('\n') {
            return content.to_string();
        }
        return format!("{content}\n");
    }

    let mut rendered = content
        .lines()
        .map(|line| {
            if line.trim() == "# Image=panel.svg" {
                line.replacen("# ", "", 1)
            } else if line.trim() == "#Image=panel.svg" {
                line.replacen('#', "", 1)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    rendered.push('\n');
    rendered
}

fn optional_rounding_state(content: &str) -> Option<bool> {
    let mut saw_enabled = false;
    let mut saw_disabled = false;

    for line in content.lines() {
        match line.trim() {
            "Image=panel.svg" => saw_enabled = true,
            "# Image=panel.svg" | "#Image=panel.svg" => saw_disabled = true,
            _ => {}
        }
    }

    if saw_disabled {
        Some(saw_enabled)
    } else {
        None
    }
}

fn write_theme_setting_pair(light_theme: &str, dark_theme: &str) -> Result<()> {
    let config = FcitxThemeConfig::for_pair(light_theme, dark_theme);
    let config_path = fcitx_classicui_config_path()?;
    let content = if config_path.exists() {
        std::fs::read_to_string(&config_path).context("read classicui config before update")?
    } else {
        String::new()
    };
    let updated = upsert_theme_values(&content, &config);

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).context("create classicui config dir")?;
    }
    std::fs::write(config_path, updated).context("write classicui config")?;

    let _ = set_theme_via_dbus(&config);
    Ok(())
}

fn reload_theme(lang: Lang) -> Result<()> {
    if reload_via_dbus().is_ok() {
        return Ok(());
    }
    if reload_via_fcitx5_remote().is_ok() {
        return Ok(());
    }

    let t = L10n::new(lang);
    crate::deployer::deploy_to("fcitx5", &t)
}

#[cfg(target_os = "linux")]
fn fcitx5_proxy<'a>(connection: &'a Connection) -> Result<Proxy<'a>> {
    Proxy::new(
        connection,
        "org.fcitx.Fcitx5",
        "/controller",
        "org.fcitx.Fcitx.Controller1",
    )
    .context("create fcitx5 dbus proxy")
}

#[cfg(target_os = "linux")]
fn set_theme_via_dbus(config: &FcitxThemeConfig) -> Result<()> {
    let connection = Connection::session().context("connect session dbus")?;
    let proxy = fcitx5_proxy(&connection)?;
    let payload = config.to_json_payload();
    let _: () = proxy
        .call(
            "SetConfig",
            &("fcitx://config/addon/classicui/classicui", payload),
        )
        .context("set fcitx theme via dbus")?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn set_theme_via_dbus(_config: &FcitxThemeConfig) -> Result<()> {
    anyhow::bail!("dbus unavailable")
}

#[cfg(target_os = "linux")]
fn reload_via_dbus() -> Result<()> {
    let connection = Connection::session().context("connect session dbus")?;
    let proxy = fcitx5_proxy(&connection)?;
    let _: () = proxy
        .call("ReloadAddonConfig", &("classicui",))
        .context("reload classicui via dbus")?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn reload_via_dbus() -> Result<()> {
    anyhow::bail!("dbus unavailable")
}

fn reload_via_fcitx5_remote() -> Result<()> {
    if !which_exists("fcitx5-remote") {
        anyhow::bail!("fcitx5-remote unavailable");
    }

    let status = std::process::Command::new("fcitx5-remote")
        .arg("-r")
        .status()
        .context("run fcitx5-remote")?;
    if !status.success() {
        anyhow::bail!("fcitx5-remote reload failed");
    }
    Ok(())
}

fn read_theme_config(content: &str) -> FcitxThemeConfig {
    let mut config = FcitxThemeConfig::default();
    for line in content.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match key.trim() {
            "Theme" => config.theme = Some(value.to_string()),
            "DarkTheme" => config.dark_theme = Some(value.to_string()),
            "UseDarkTheme" => config.use_dark_theme = parse_bool(value),
            "FollowSystemDarkMode" => config.follow_system_dark_mode = parse_bool(value),
            _ => {}
        }
    }
    config
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn format_bool(value: bool) -> &'static str {
    if value {
        "True"
    } else {
        "False"
    }
}

fn upsert_key_value(content: &str, key: &str, value: &str) -> String {
    let mut lines = Vec::new();
    let mut found = false;

    for line in content.lines() {
        if let Some((line_key, _)) = line.split_once('=') {
            if line_key.trim() == key {
                lines.push(format!("{key}={value}"));
                found = true;
                continue;
            }
        }
        lines.push(line.to_string());
    }

    if !found {
        lines.push(format!("{key}={value}"));
    }

    let mut output = lines.join("\n");
    if !output.is_empty() {
        output.push('\n');
    }
    output
}

fn upsert_theme_values(content: &str, config: &FcitxThemeConfig) -> String {
    let content = if let Some(theme) = &config.theme {
        upsert_key_value(content, "Theme", theme)
    } else {
        content.to_string()
    };
    let content = if let Some(dark_theme) = &config.dark_theme {
        upsert_key_value(&content, "DarkTheme", dark_theme)
    } else {
        content
    };
    let content = if let Some(use_dark_theme) = config.use_dark_theme {
        upsert_key_value(&content, "UseDarkTheme", format_bool(use_dark_theme))
    } else {
        content
    };

    if let Some(follow_system_dark_mode) = config.follow_system_dark_mode {
        upsert_key_value(
            &content,
            "FollowSystemDarkMode",
            format_bool(follow_system_dark_mode),
        )
    } else {
        content
    }
}

fn fcitx_theme_root_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("missing home")?;
    Ok(home.join(".local/share/fcitx5/themes"))
}

fn fcitx_classicui_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("missing home")?;
    Ok(home.join(".config/fcitx5/conf/classicui.conf"))
}

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_theme_manifest_is_not_empty() {
        assert!(builtin_themes_available());
    }

    #[test]
    fn read_theme_config_extracts_light_and_dark_values() {
        let config = read_theme_config(
            "[Groups/0]\nTheme=OriLight\nDarkTheme=OriDark\nUseDarkTheme=True\nFollowSystemDarkMode=True\n",
        );
        assert_eq!(config.theme, Some("OriLight".into()));
        assert_eq!(config.dark_theme, Some("OriDark".into()));
        assert_eq!(config.use_dark_theme, Some(true));
        assert_eq!(config.follow_system_dark_mode, Some(true));
    }

    #[test]
    fn upsert_key_value_replaces_existing_theme() {
        let output = upsert_key_value("Theme=Old\nUseDarkTheme=False\n", "Theme", "New");
        assert_eq!(output, "Theme=New\nUseDarkTheme=False\n");
    }

    #[test]
    fn upsert_key_value_appends_missing_theme() {
        let output = upsert_key_value("UseDarkTheme=False\n", "Theme", "OriDark");
        assert_eq!(output, "UseDarkTheme=False\nTheme=OriDark\n");
    }

    #[test]
    fn upsert_theme_values_updates_light_dark_and_mode_flags() {
        let output = upsert_theme_values(
            "Theme=Old\nUseDarkTheme=False\n",
            &FcitxThemeConfig::for_pair("Latte", "Mocha"),
        );
        assert!(output.contains("Theme=Latte"));
        assert!(output.contains("DarkTheme=Mocha"));
        assert!(output.contains("UseDarkTheme=True"));
        assert!(output.contains("FollowSystemDarkMode=True"));
    }

    #[test]
    fn optional_rounding_detection_finds_disabled_toggle() {
        assert_eq!(optional_rounding_state("# Image=panel.svg\n"), Some(false));
    }

    #[test]
    fn optional_rounding_detection_finds_enabled_toggle() {
        assert_eq!(
            optional_rounding_state("Image=panel.svg\n# Image=panel.svg\n"),
            Some(true)
        );
    }

    #[test]
    fn optional_rounding_detection_returns_none_for_fixed_theme() {
        assert_eq!(
            optional_rounding_state("Theme=OriDark\nImage=panel.svg\n"),
            None
        );
    }

    #[test]
    fn render_theme_conf_enables_optional_rounding() {
        let rendered = render_theme_conf("# Image=panel.svg\nTheme=Test\n", Some(true));
        assert!(rendered.contains("Image=panel.svg"));
        assert!(!rendered.contains("# Image=panel.svg"));
    }

    #[test]
    fn catppuccin_theme_reports_optional_rounding() {
        assert!(theme_supports_optional_rounding("catppuccin-latte-sky"));
        assert!(!theme_supports_optional_rounding("OriDark"));
        assert!(!theme_supports_optional_rounding("inflex-wechat"));
    }
}
