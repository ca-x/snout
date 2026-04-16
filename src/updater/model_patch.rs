use crate::types::Schema;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

type PatchDoc = HashMap<String, serde_yaml::Value>;

const PATCH_KEY: &str = "patch";
const MODEL_KEY: &str = "grammar/language_model";
const MODEL_VALUE: &str = "wanxiang-lts-zh-hans";

/// 为当前方案写入万象模型 patch 配置
///
/// 写入 `<schema_id>.custom.yaml`:
/// ```yaml
/// patch:
///   # Wanxiang:
///   grammar/language_model: wanxiang-lts-zh-hans
///
///   # Ice / Frost / Mint:
///   grammar/language: wanxiang-lts-zh-hans
///   grammar/collocation_max_length: 5
///   grammar/collocation_min_length: 2
///   translator/contextual_suggestions: true
///   translator/max_homophones: 7
///   translator/max_homographs: 7
/// ```
pub fn patch_model(rime_dir: &Path, schema: &Schema) -> Result<()> {
    let patch_file = patch_file_path(rime_dir, schema);
    let mut doc = load_patch_doc(&patch_file)?;

    let patch = doc
        .entry(PATCH_KEY.into())
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));

    if let serde_yaml::Value::Mapping(mapping) = patch {
        apply_patch_values(mapping, schema);
    } else {
        anyhow::bail!("模型 patch 文件中的 `{PATCH_KEY}` 节不是映射类型");
    }

    write_patch_doc(&patch_file, &doc)?;

    println!("✅ 模型 patch 已写入: {}", patch_file.display());
    Ok(())
}

/// 移除模型 patch
pub fn unpatch_model(rime_dir: &Path, schema: &Schema) -> Result<()> {
    let patch_file = patch_file_path(rime_dir, schema);

    if !patch_file.exists() {
        return Ok(());
    }

    let mut doc = load_patch_doc(&patch_file)?;

    if let Some(patch) = doc.get_mut(PATCH_KEY) {
        if let serde_yaml::Value::Mapping(mapping) = patch {
            remove_patch_values(mapping, schema);
        } else {
            anyhow::bail!("模型 patch 文件中的 `{PATCH_KEY}` 节不是映射类型");
        }
    }

    write_patch_doc(&patch_file, &doc)?;

    println!("✅ 模型 patch 已移除");
    Ok(())
}

/// 检查模型 patch 是否已启用
pub fn is_model_patched(rime_dir: &Path, schema: &Schema) -> bool {
    let patch_file = patch_file_path(rime_dir, schema);

    match load_patch_doc(&patch_file) {
        Ok(doc) => has_model_patch(&doc, schema),
        Err(e) => {
            eprintln!("⚠️ 读取模型 patch 状态失败: {e}");
            false
        }
    }
}

fn patch_file_path(rime_dir: &Path, schema: &Schema) -> std::path::PathBuf {
    rime_dir.join(format!("{}.custom.yaml", schema.schema_id()))
}

fn load_patch_doc(path: &Path) -> Result<PatchDoc> {
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let data = std::fs::read_to_string(path)
        .with_context(|| format!("读取模型 patch 文件失败: {}", path.display()))?;
    serde_yaml::from_str(&data)
        .with_context(|| format!("解析模型 patch 文件失败: {}", path.display()))
}

fn write_patch_doc(path: &Path, doc: &PatchDoc) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let yaml = serde_yaml::to_string(doc)?;
    std::fs::write(path, yaml)?;
    Ok(())
}

fn has_model_patch(doc: &PatchDoc, schema: &Schema) -> bool {
    match doc.get(PATCH_KEY) {
        Some(serde_yaml::Value::Mapping(mapping)) => {
            has_expected_patch(mapping, patch_spec_for_schema(schema))
        }
        _ => false,
    }
}

fn patch_spec_for_schema(schema: &Schema) -> Vec<(&'static str, serde_yaml::Value)> {
    match schema {
        Schema::Ice | Schema::Frost | Schema::Mint => vec![
            (
                "grammar/language",
                serde_yaml::Value::String(MODEL_VALUE.into()),
            ),
            (
                "grammar/collocation_max_length",
                serde_yaml::Value::Number(5.into()),
            ),
            (
                "grammar/collocation_min_length",
                serde_yaml::Value::Number(2.into()),
            ),
            (
                "translator/contextual_suggestions",
                serde_yaml::Value::Bool(true),
            ),
            (
                "translator/max_homophones",
                serde_yaml::Value::Number(7.into()),
            ),
            (
                "translator/max_homographs",
                serde_yaml::Value::Number(7.into()),
            ),
        ],
        _ => vec![(MODEL_KEY, serde_yaml::Value::String(MODEL_VALUE.into()))],
    }
}

fn apply_patch_values(mapping: &mut serde_yaml::Mapping, schema: &Schema) {
    for (key, value) in patch_spec_for_schema(schema) {
        mapping.insert(serde_yaml::Value::String(key.into()), value);
    }
}

fn remove_patch_values(mapping: &mut serde_yaml::Mapping, schema: &Schema) {
    for (key, _) in patch_spec_for_schema(schema) {
        mapping.remove(serde_yaml::Value::String(key.into()));
    }
}

fn has_expected_patch(
    mapping: &serde_yaml::Mapping,
    expected: Vec<(&'static str, serde_yaml::Value)>,
) -> bool {
    expected
        .into_iter()
        .all(|(key, value)| mapping.get(serde_yaml::Value::String(key.into())) == Some(&value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_rime_dir(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("snout-{name}-{nanos}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn patch_model_fails_fast_on_invalid_yaml() {
        let dir = temp_rime_dir("model-patch-invalid");
        let file = patch_file_path(&dir, &Schema::WanxiangBase);
        std::fs::write(&file, "patch: [broken").expect("write invalid yaml");

        let result = patch_model(&dir, &Schema::WanxiangBase);

        assert!(result.is_err());
        let err = result.expect_err("invalid yaml should fail");
        assert!(err.to_string().contains("解析模型 patch 文件失败"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn is_model_patched_returns_false_on_invalid_yaml() {
        let dir = temp_rime_dir("model-patch-detect-invalid");
        let file = patch_file_path(&dir, &Schema::WanxiangBase);
        std::fs::write(&file, "patch: [broken").expect("write invalid yaml");

        assert!(!is_model_patched(&dir, &Schema::WanxiangBase));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn patch_and_unpatch_model_round_trip() {
        let dir = temp_rime_dir("model-patch-roundtrip");

        patch_model(&dir, &Schema::WanxiangBase).expect("patch model");
        assert!(is_model_patched(&dir, &Schema::WanxiangBase));

        unpatch_model(&dir, &Schema::WanxiangBase).expect("unpatch model");
        assert!(!is_model_patched(&dir, &Schema::WanxiangBase));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn patch_round_trip_supports_non_wanxiang_schemas() {
        let dir = temp_rime_dir("model-patch-cross-schema");

        patch_model(&dir, &Schema::Ice).expect("patch ice model");
        assert!(is_model_patched(&dir, &Schema::Ice));

        patch_model(&dir, &Schema::Frost).expect("patch frost model");
        assert!(is_model_patched(&dir, &Schema::Frost));

        unpatch_model(&dir, &Schema::Ice).expect("unpatch ice model");
        unpatch_model(&dir, &Schema::Frost).expect("unpatch frost model");
        assert!(!is_model_patched(&dir, &Schema::Ice));
        assert!(!is_model_patched(&dir, &Schema::Frost));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ice_patch_matches_reference_integrator_shape() {
        let dir = temp_rime_dir("model-patch-ice-shape");

        patch_model(&dir, &Schema::Ice).expect("patch ice model");

        let patch_file = patch_file_path(&dir, &Schema::Ice);
        let doc = load_patch_doc(&patch_file).expect("load patch doc");
        let patch = match doc.get(PATCH_KEY) {
            Some(serde_yaml::Value::Mapping(mapping)) => mapping,
            other => panic!("unexpected patch mapping: {other:?}"),
        };

        assert_eq!(
            patch.get(serde_yaml::Value::String("grammar/language".into())),
            Some(&serde_yaml::Value::String(MODEL_VALUE.into()))
        );
        assert_eq!(
            patch.get(serde_yaml::Value::String(
                "translator/contextual_suggestions".into()
            )),
            Some(&serde_yaml::Value::Bool(true))
        );
        assert_eq!(
            patch.get(serde_yaml::Value::String(
                "translator/max_homophones".into()
            )),
            Some(&serde_yaml::Value::Number(7.into()))
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn frost_patch_matches_ice_shape() {
        let dir = temp_rime_dir("model-patch-frost-shape");

        patch_model(&dir, &Schema::Frost).expect("patch frost model");

        let patch_file = patch_file_path(&dir, &Schema::Frost);
        let doc = load_patch_doc(&patch_file).expect("load patch doc");
        let patch = match doc.get(PATCH_KEY) {
            Some(serde_yaml::Value::Mapping(mapping)) => mapping,
            other => panic!("unexpected patch mapping: {other:?}"),
        };

        assert_eq!(
            patch.get(serde_yaml::Value::String("grammar/language".into())),
            Some(&serde_yaml::Value::String(MODEL_VALUE.into()))
        );
        assert_eq!(
            patch.get(serde_yaml::Value::String(
                "translator/contextual_suggestions".into()
            )),
            Some(&serde_yaml::Value::Bool(true))
        );
        assert_eq!(
            patch.get(serde_yaml::Value::String(
                "translator/max_homographs".into()
            )),
            Some(&serde_yaml::Value::Number(7.into()))
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mint_patch_matches_documented_shape() {
        let dir = temp_rime_dir("model-patch-mint-shape");

        patch_model(&dir, &Schema::Mint).expect("patch mint model");

        let patch_file = patch_file_path(&dir, &Schema::Mint);
        let doc = load_patch_doc(&patch_file).expect("load patch doc");
        let patch = match doc.get(PATCH_KEY) {
            Some(serde_yaml::Value::Mapping(mapping)) => mapping,
            other => panic!("unexpected patch mapping: {other:?}"),
        };

        assert_eq!(
            patch.get(serde_yaml::Value::String("grammar/language".into())),
            Some(&serde_yaml::Value::String(MODEL_VALUE.into()))
        );
        assert_eq!(
            patch.get(serde_yaml::Value::String(
                "translator/contextual_suggestions".into()
            )),
            Some(&serde_yaml::Value::Bool(true))
        );
        assert_eq!(
            patch.get(serde_yaml::Value::String(
                "translator/max_homophones".into()
            )),
            Some(&serde_yaml::Value::Number(7.into()))
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
