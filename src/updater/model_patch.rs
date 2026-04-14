use crate::types::Schema;
use anyhow::Result;
use serde_yaml;
use std::collections::HashMap;
use std::path::Path;

/// 为万象方案 patch 模型配置
///
/// 写入 `<schema_id>.custom.yaml`:
/// ```yaml
/// patch:
///   grammar/language_model: wanxiang-lts-zh-hans
/// ```
pub fn patch_model(rime_dir: &Path, schema: &Schema) -> Result<()> {
    let schema_id = schema.schema_id();
    let patch_file = rime_dir.join(format!("{schema_id}.custom.yaml"));

    // 读取现有文件
    let mut doc: HashMap<String, serde_yaml::Value> = if patch_file.exists() {
        let data = std::fs::read_to_string(&patch_file)?;
        serde_yaml::from_str(&data).unwrap_or_default()
    } else {
        HashMap::new()
    };

    // 获取或创建 patch section
    let patch = doc.entry("patch".into())
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));

    if let serde_yaml::Value::Mapping(ref mut m) = patch {
        m.insert(
            serde_yaml::Value::String("grammar/language_model".into()),
            serde_yaml::Value::String("wanxiang-lts-zh-hans".into()),
        );
    }

    // 写入
    if let Some(parent) = patch_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let yaml = serde_yaml::to_string(&doc)?;
    std::fs::write(&patch_file, yaml)?;

    println!("✅ 模型 patch 已写入: {}", patch_file.display());
    Ok(())
}

/// 移除模型 patch
pub fn unpatch_model(rime_dir: &Path, schema: &Schema) -> Result<()> {
    let schema_id = schema.schema_id();
    let patch_file = rime_dir.join(format!("{schema_id}.custom.yaml"));

    if !patch_file.exists() {
        return Ok(());
    }

    let mut doc: HashMap<String, serde_yaml::Value> = {
        let data = std::fs::read_to_string(&patch_file)?;
        serde_yaml::from_str(&data).unwrap_or_default()
    };

    if let Some(serde_yaml::Value::Mapping(ref mut m)) = doc.get_mut("patch") {
        m.remove(&serde_yaml::Value::String("grammar/language_model".into()));
    }

    let yaml = serde_yaml::to_string(&doc)?;
    std::fs::write(&patch_file, yaml)?;

    println!("✅ 模型 patch 已移除");
    Ok(())
}

/// 检查模型 patch 是否已启用
pub fn is_model_patched(rime_dir: &Path, schema: &Schema) -> bool {
    let schema_id = schema.schema_id();
    let patch_file = rime_dir.join(format!("{schema_id}.custom.yaml"));

    if !patch_file.exists() {
        return false;
    }

    let data = std::fs::read_to_string(&patch_file).unwrap_or_default();
    let doc: HashMap<String, serde_yaml::Value> =
        serde_yaml::from_str(&data).unwrap_or_default();

    if let Some(serde_yaml::Value::Mapping(m)) = doc.get("patch") {
        m.contains_key(&serde_yaml::Value::String("grammar/language_model".into()))
    } else {
        false
    }
}
