use crate::i18n::{L10n, Lang};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::PathBuf;

include!(concat!(env!("OUT_DIR"), "/fcitx5_theme_manifest.rs"));

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

pub fn current_theme() -> Result<Option<String>> {
    let config_path = fcitx_classicui_config_path()?;
    if !config_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(config_path).context("read fcitx5 classicui config")?;
    Ok(read_theme_key(&content))
}

pub fn apply_theme(theme_name: &str, rounded: Option<bool>, lang: Lang) -> Result<()> {
    install_theme(theme_name, rounded)?;
    write_theme_setting(theme_name)?;
    reload_theme(lang)
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
        return content.to_string();
    }

    content
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
        .join("\n")
        + "\n"
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

fn write_theme_setting(theme_name: &str) -> Result<()> {
    let config_path = fcitx_classicui_config_path()?;
    let content = if config_path.exists() {
        std::fs::read_to_string(&config_path).context("read classicui config before update")?
    } else {
        String::new()
    };
    let updated = upsert_key_value(&content, "Theme", theme_name);

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).context("create classicui config dir")?;
    }
    std::fs::write(config_path, updated).context("write classicui config")?;
    Ok(())
}

fn reload_theme(lang: Lang) -> Result<()> {
    if reload_via_qdbus6().is_ok() {
        return Ok(());
    }
    if reload_via_fcitx5_remote().is_ok() {
        return Ok(());
    }

    let t = L10n::new(lang);
    crate::deployer::deploy_to("fcitx5", &t)
}

fn reload_via_qdbus6() -> Result<()> {
    if !which_exists("qdbus6") {
        anyhow::bail!("qdbus6 unavailable");
    }

    let status = std::process::Command::new("qdbus6")
        .args([
            "org.fcitx.Fcitx5",
            "/controller",
            "org.fcitx.Fcitx.Controller1.ReloadAddonConfig",
            "classicui",
        ])
        .status()
        .context("run qdbus6")?;
    if !status.success() {
        anyhow::bail!("qdbus6 reload failed");
    }
    Ok(())
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

fn read_theme_key(content: &str) -> Option<String> {
    for line in content.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "Theme" {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
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
    fn read_theme_key_extracts_theme_value() {
        assert_eq!(
            read_theme_key("[Groups/0]\nTheme=OriLight\nUseDarkTheme=False\n"),
            Some("OriLight".into())
        );
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
