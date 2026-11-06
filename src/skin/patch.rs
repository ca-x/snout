use crate::i18n::Lang;
use crate::skin::builtin::{builtin_skins, SkinPreset};
use anyhow::Result;
use serde_yaml;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// 读取现有的 YAML patch 文件
fn read_patch(path: &Path) -> Result<HashMap<String, serde_yaml::Value>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let data = std::fs::read_to_string(path)?;
    let doc: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(&data)?;
    Ok(doc)
}

/// 写入 YAML patch 文件
fn write_patch(path: &Path, doc: &HashMap<String, serde_yaml::Value>) -> Result<()> {
    let yaml = serde_yaml::to_string(doc)?;
    crate::fileutil::atomic_write(path, yaml.as_bytes())
}

/// 获取 patch section
fn get_patch(
    doc: &mut HashMap<String, serde_yaml::Value>,
) -> Result<HashMap<String, serde_yaml::Value>> {
    match doc.remove("patch") {
        None => Ok(HashMap::new()),
        Some(serde_yaml::Value::Mapping(mapping)) => mapping
            .into_iter()
            .map(|(key, value)| match key {
                serde_yaml::Value::String(key) => Ok((key, value)),
                _ => anyhow::bail!("patch section keys must be strings"),
            })
            .collect(),
        Some(_) => anyhow::bail!("patch section must be a mapping"),
    }
}

/// 设置 patch section
fn set_patch(
    doc: &mut HashMap<String, serde_yaml::Value>,
    patch: HashMap<String, serde_yaml::Value>,
) {
    let mut mapping = serde_yaml::Mapping::new();
    for (k, v) in patch {
        mapping.insert(serde_yaml::Value::String(k), v);
    }
    doc.insert("patch".into(), serde_yaml::Value::Mapping(mapping));
}

/// 设置默认主题
pub fn set_default_skin(path: &Path, theme_key: &str) -> Result<()> {
    let mut doc = read_patch(path)?;
    let mut patch = get_patch(&mut doc)?;

    patch.insert("style/color_scheme".into(), theme_key.into());
    patch.insert("style/color_scheme_dark".into(), theme_key.into());

    set_patch(&mut doc, patch);
    write_patch(path, &doc)
}

pub fn sync_skin_presets(path: &Path, keys: &[&str]) -> Result<()> {
    let mut doc = read_patch(path)?;
    let mut patch = get_patch(&mut doc)?;
    sync_preset_entries(&mut patch, keys, builtin_skins(Lang::Zh));

    set_patch(&mut doc, patch);
    write_patch(path, &doc)
}

/// Applies builtin presets and their selected default in a single atomic write.
pub fn apply_skin_presets(path: &Path, keys: &[&str], default: &str) -> Result<()> {
    let skins = builtin_skins(Lang::Zh);
    for key in keys {
        anyhow::ensure!(
            skins.iter().any(|skin| skin.key == *key),
            "unknown builtin skin preset: {key}"
        );
    }
    anyhow::ensure!(
        keys.contains(&default),
        "default skin preset must be included in the selected presets: {default}"
    );

    let mut doc = read_patch(path)?;
    let mut patch = get_patch(&mut doc)?;
    sync_preset_entries(&mut patch, keys, skins);
    patch.insert("style/color_scheme".into(), default.into());
    patch.insert("style/color_scheme_dark".into(), default.into());

    set_patch(&mut doc, patch);
    write_patch(path, &doc)
}

fn sync_preset_entries(
    patch: &mut HashMap<String, serde_yaml::Value>,
    keys: &[&str],
    skins: Vec<SkinPreset>,
) {
    for skin in skins {
        let patch_key = format!("preset_color_schemes/{}", skin.key);
        if keys.contains(&skin.key.as_str()) {
            let mapping = skin
                .values
                .into_iter()
                .map(|(key, value)| (serde_yaml::Value::String(key), value))
                .collect();
            patch.insert(patch_key, serde_yaml::Value::Mapping(mapping));
        } else {
            patch.remove(&patch_key);
        }
    }
}

pub fn read_skin_preset_selections(path: &Path) -> Result<HashSet<String>> {
    let mut doc = read_patch(path)?;
    let patch = get_patch(&mut doc)?;
    let builtin_keys: HashSet<String> = builtin_skins(Lang::Zh)
        .into_iter()
        .map(|skin| skin.key)
        .collect();

    Ok(patch
        .keys()
        .filter_map(|key| key.strip_prefix("preset_color_schemes/"))
        .filter(|key| builtin_keys.contains(*key))
        .map(str::to_string)
        .collect())
}

pub fn read_default_skin(path: &Path) -> Result<Option<String>> {
    let mut doc = read_patch(path)?;
    let patch = get_patch(&mut doc)?;
    Ok(patch
        .get("style/color_scheme")
        .and_then(|value| match value {
            serde_yaml::Value::String(value) if !value.trim().is_empty() => Some(value.clone()),
            _ => None,
        }))
}

/// 列出所有可用的内置主题
#[allow(dead_code)]
pub fn list_available_skins() -> Vec<(String, String)> {
    builtin_skins(Lang::Zh)
        .into_iter()
        .map(|s| (s.key, s.display_name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_patch_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("snout-skin-{name}-{nanos}.yaml"))
    }

    fn rejects_invalid_patch_without_writing(operation: impl Fn(&Path) -> Result<()>) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("squirrel.custom.yaml");
        for original in [
            "patch: [keep-this-user-value]\nother: untouched\n",
            "patch: keep-this-user-value\nother: untouched\n",
            "patch: null\nother: untouched\n",
            "patch:\n  42: keep-this-user-value\nother: untouched\n",
            "patch: [invalid yaml\n",
        ] {
            std::fs::write(&path, original).unwrap();
            assert!(
                operation(&path).is_err(),
                "accepted invalid patch: {original}"
            );
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn regression_sync_rejects_invalid_patch_without_writing() {
        rejects_invalid_patch_without_writing(|path| sync_skin_presets(path, &["wechat"]));
    }

    #[test]
    fn regression_default_rejects_invalid_patch_without_writing() {
        rejects_invalid_patch_without_writing(|path| set_default_skin(path, "wechat"));
    }

    #[test]
    fn apply_skin_presets_rejects_invalid_patch_without_writing() {
        rejects_invalid_patch_without_writing(|path| {
            apply_skin_presets(path, &["wechat"], "wechat")
        });
    }

    #[test]
    fn apply_skin_presets_preserves_unrelated_entries_and_sets_both_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("squirrel.custom.yaml");
        std::fs::write(&path, "metadata: keep\npatch:\n  menu/page_size: 9\n  preset_color_schemes/custom_theme:\n    name: custom\n  preset_color_schemes/jianchun:\n    name: old\n  preset_color_schemes/wechat:\n    name: stale\n  style/color_scheme: jianchun\n  style/color_scheme_dark: jianchun\n").unwrap();

        apply_skin_presets(&path, &["wechat", "win11_dark"], "wechat").unwrap();

        let mut doc = read_patch(&path).unwrap();
        assert_eq!(doc["metadata"], serde_yaml::Value::String("keep".into()));
        let patch = get_patch(&mut doc).unwrap();
        assert_eq!(patch["menu/page_size"], serde_yaml::Value::Number(9.into()));
        assert_eq!(patch["preset_color_schemes/custom_theme"]["name"], "custom");
        assert!(!patch.contains_key("preset_color_schemes/jianchun"));
        assert_ne!(patch["preset_color_schemes/wechat"]["name"], "stale");
        assert_eq!(patch["style/color_scheme"], "wechat");
        assert_eq!(patch["style/color_scheme_dark"], "wechat");
        assert_eq!(
            read_skin_preset_selections(&path).unwrap(),
            HashSet::from(["wechat".into(), "win11_dark".into()])
        );
        assert_eq!(read_default_skin(&path).unwrap().as_deref(), Some("wechat"));
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn apply_skin_presets_validates_before_any_disk_changes() {
        let invalid: &[(&[&str], &str)] = &[
            (&["unknown"], "unknown"),
            (&["wechat", "unknown"], "wechat"),
            (&["wechat"], "unknown"),
            (&["wechat"], "jianchun"),
            (&["wechat"], ""),
            (&[], ""),
        ];
        for (keys, default) in invalid {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("squirrel.custom.yaml");
            let original = "patch:\n  style/color_scheme: jianchun\n";
            std::fs::write(&path, original).unwrap();

            assert!(apply_skin_presets(&path, keys, default).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
            let missing = directory.path().join("missing/theme.custom.yaml");
            assert!(apply_skin_presets(&missing, keys, default).is_err());
            assert!(!missing.parent().unwrap().exists());
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn apply_skin_presets_creates_missing_parent_and_accepts_duplicate_keys() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("new/squirrel.custom.yaml");

        apply_skin_presets(&path, &["wechat", "wechat"], "wechat").unwrap();

        assert_eq!(
            read_skin_preset_selections(&path).unwrap(),
            HashSet::from(["wechat".into()])
        );
        assert_eq!(read_default_skin(&path).unwrap().as_deref(), Some("wechat"));
    }

    #[test]
    fn read_skin_helpers_reject_invalid_patch_without_writing() {
        rejects_invalid_patch_without_writing(|path| read_default_skin(path).map(|_| ()));
        rejects_invalid_patch_without_writing(|path| read_skin_preset_selections(path).map(|_| ()));
    }

    #[test]
    fn failed_atomic_replace_cleans_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("existing-directory");
        std::fs::create_dir(&path).unwrap();

        let result = write_patch(&path, &HashMap::new());

        assert!(result.is_err());
        assert!(path.is_dir());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn apply_skin_presets_preserves_existing_symlink() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("actual.custom.yaml");
        let path = directory.path().join("squirrel.custom.yaml");
        std::fs::write(&target, "patch: {}\n").unwrap();
        std::os::unix::fs::symlink("actual.custom.yaml", &path).unwrap();

        apply_skin_presets(&path, &["wechat"], "wechat").unwrap();

        assert_eq!(
            std::fs::read_link(&path).unwrap(),
            Path::new("actual.custom.yaml")
        );
        assert_eq!(
            read_default_skin(&target).unwrap().as_deref(),
            Some("wechat")
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn regression_write_replaces_file_without_modifying_open_snapshot() {
        use std::io::Read;
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("squirrel.custom.yaml");
        let original = "patch:\n  style/color_scheme: jianchun\n";
        std::fs::write(&path, original).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        let mut snapshot = std::fs::File::open(&path).unwrap();

        set_default_skin(&path, "wechat").unwrap();

        let mut previous = String::new();
        snapshot.read_to_string(&mut previous).unwrap();
        assert_eq!(previous, original);
        assert_eq!(read_default_skin(&path).unwrap().as_deref(), Some("wechat"));
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn sync_skin_presets_preserves_custom_entries() {
        let path = temp_patch_path("preserve-custom");
        std::fs::write(
            &path,
            "patch:\n  preset_color_schemes/custom_theme:\n    name: custom\n  preset_color_schemes/jianchun:\n    name: old\n",
        )
        .expect("write patch");

        sync_skin_presets(&path, &["wechat"]).expect("sync presets");

        let data = std::fs::read_to_string(&path).expect("read patch");
        assert!(data.contains("preset_color_schemes/custom_theme"));
        assert!(data.contains("preset_color_schemes/wechat"));
        assert!(!data.contains("preset_color_schemes/jianchun:\n    name: old"));

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn read_skin_helpers_extract_builtin_selections_and_default() {
        let path = temp_patch_path("read-selections");
        std::fs::write(
            &path,
            "patch:\n  preset_color_schemes/jianchun:\n    name: x\n  preset_color_schemes/custom_theme:\n    name: y\n  style/color_scheme: jianchun\n",
        )
        .expect("write patch");

        let selections = read_skin_preset_selections(&path).expect("read selections");
        let default_skin = read_default_skin(&path).expect("read default");

        assert!(selections.contains("jianchun"));
        assert!(!selections.contains("custom_theme"));
        assert_eq!(default_skin.as_deref(), Some("jianchun"));

        std::fs::remove_file(path).ok();
    }
}
