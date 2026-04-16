use crate::i18n::Lang;
use crate::skin::builtin::builtin_skins;
use anyhow::Result;
use serde_yaml;
use std::collections::HashMap;
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
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let yaml = serde_yaml::to_string(doc)?;
    std::fs::write(path, yaml)?;
    Ok(())
}

/// 获取 patch section
fn get_patch(doc: &mut HashMap<String, serde_yaml::Value>) -> HashMap<String, serde_yaml::Value> {
    doc.remove("patch")
        .and_then(|v| {
            if let serde_yaml::Value::Mapping(m) = v {
                let mut result = HashMap::new();
                for (k, v) in m {
                    if let serde_yaml::Value::String(key) = k {
                        result.insert(key, v);
                    }
                }
                Some(result)
            } else {
                None
            }
        })
        .unwrap_or_default()
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
    let mut patch = get_patch(&mut doc);

    patch.insert("style/color_scheme".into(), theme_key.into());
    patch.insert("style/color_scheme_dark".into(), theme_key.into());

    set_patch(&mut doc, patch);
    write_patch(path, &doc)
}

pub fn sync_skin_presets(path: &Path, keys: &[&str]) -> Result<()> {
    let mut doc = read_patch(path)?;
    let mut patch = get_patch(&mut doc);
    let selected: std::collections::HashSet<&str> = keys.iter().copied().collect();

    for skin in builtin_skins(Lang::Zh) {
        let patch_key = format!("preset_color_schemes/{}", skin.key);
        if selected.contains(skin.key.as_str()) {
            let mut mapping = serde_yaml::Mapping::new();
            for (k, v) in &skin.values {
                mapping.insert(serde_yaml::Value::String(k.clone()), v.clone());
            }
            patch.insert(patch_key, serde_yaml::Value::Mapping(mapping));
        } else {
            patch.remove(&patch_key);
        }
    }

    set_patch(&mut doc, patch);
    write_patch(path, &doc)
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
}
