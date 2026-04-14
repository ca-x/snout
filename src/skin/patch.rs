use crate::skin::builtin::{find_skin, builtin_skins};
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
fn set_patch(doc: &mut HashMap<String, serde_yaml::Value>, patch: HashMap<String, serde_yaml::Value>) {
    let mut mapping = serde_yaml::Mapping::new();
    for (k, v) in patch {
        mapping.insert(serde_yaml::Value::String(k), v);
    }
    doc.insert("patch".into(), serde_yaml::Value::Mapping(mapping));
}

/// 将内置主题写入 skin patch 文件
pub fn write_skin_presets(path: &Path, keys: &[&str]) -> Result<()> {
    let mut doc = read_patch(path)?;
    let mut patch = get_patch(&mut doc);

    for key in keys {
        if let Some(skin) = find_skin(key) {
            let patch_key = format!("preset_color_schemes/{}", key);
            let mut mapping = serde_yaml::Mapping::new();
            for (k, v) in &skin.values {
                mapping.insert(serde_yaml::Value::String(k.clone()), v.clone());
            }
            patch.insert(patch_key, serde_yaml::Value::Mapping(mapping));
        }
    }

    set_patch(&mut doc, patch);
    write_patch(path, &doc)
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

/// 列出所有可用的内置主题
pub fn list_available_skins() -> Vec<(String, String)> {
    builtin_skins()
        .into_iter()
        .map(|s| (s.key, s.display_name))
        .collect()
}
