use crate::types::Schema;
use anyhow::Result;
use serde_yaml::{Mapping, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

type PatchDoc = HashMap<String, Value>;

const PATCH_KEY: &str = "patch";
const MENU_PAGE_SIZE_KEY: &str = "menu/page_size";

fn custom_patch_path(rime_dir: &Path, schema: Schema) -> PathBuf {
    rime_dir.join(format!("{}.custom.yaml", schema.schema_id()))
}

fn load_patch_doc(path: &Path) -> Result<PatchDoc> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    Ok(serde_yaml::from_str(&std::fs::read_to_string(path)?)?)
}

fn write_patch_doc(path: &Path, doc: &PatchDoc) -> Result<()> {
    crate::fileutil::atomic_write(path, serde_yaml::to_string(doc)?.as_bytes())
}

fn patch_mapping_mut(doc: &mut PatchDoc) -> Result<&mut Mapping> {
    let patch = doc
        .entry(PATCH_KEY.into())
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    match patch {
        Value::Mapping(mapping) => Ok(mapping),
        _ => anyhow::bail!("patch section is not a mapping"),
    }
}

pub fn candidate_page_size(rime_dir: &Path, schema: Schema) -> Result<Option<u8>> {
    let path = custom_patch_path(rime_dir, schema);
    let doc = load_patch_doc(&path)?;
    let Some(Value::Mapping(mapping)) = doc.get(PATCH_KEY) else {
        return Ok(None);
    };
    let Some(value) = mapping.get(Value::String(MENU_PAGE_SIZE_KEY.into())) else {
        return Ok(None);
    };
    match value {
        Value::Number(number) => Ok(number.as_u64().and_then(|value| u8::try_from(value).ok())),
        _ => Ok(None),
    }
}

pub fn set_candidate_page_size(
    rime_dir: &Path,
    schema: Schema,
    page_size: Option<u8>,
) -> Result<()> {
    if page_size.is_some_and(|value| !(1..=9).contains(&value)) {
        anyhow::bail!("candidate page size must be between 1 and 9");
    }
    let path = custom_patch_path(rime_dir, schema);
    let mut doc = load_patch_doc(&path)?;
    let mapping = patch_mapping_mut(&mut doc)?;
    let key = Value::String(MENU_PAGE_SIZE_KEY.into());

    if let Some(page_size) = page_size {
        mapping.insert(key, Value::Number(u64::from(page_size).into()));
    } else {
        mapping.remove(&key);
    }

    write_patch_doc(&path, &doc)
}

/// Add the installed scheme to the switcher while retaining existing user patches.
pub fn enable_schema(rime_dir: &Path, schema: Schema) -> Result<()> {
    let path = rime_dir.join("default.custom.yaml");
    let mut doc = load_patch_doc(&path)?;
    let patch = patch_mapping_mut(&mut doc)?;
    let key = Value::String("schema_list".into());
    let list = patch
        .entry(key)
        .or_insert_with(|| Value::Sequence(Vec::new()));
    let Value::Sequence(list) = list else {
        anyhow::bail!("schema_list is not a sequence");
    };
    let entry = serde_yaml::to_value(HashMap::from([("schema", schema.schema_id())]))?;
    list.retain(|value| value.get("schema").and_then(Value::as_str) != Some(schema.schema_id()));
    list.insert(0, entry);
    write_patch_doc(&path, &doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_rime_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("snout-custom-{name}-{nanos}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn enabling_a_scheme_preserves_preferences_and_existing_choices() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("default.custom.yaml"),
            "patch:\n  menu/page_size: 9\n  schema_list:\n    - schema: rime_ice\n",
        )
        .unwrap();
        enable_schema(root.path(), Schema::Wubi86Jidian).unwrap();
        enable_schema(root.path(), Schema::Abcm).unwrap();
        enable_schema(root.path(), Schema::Abcm).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(
            &std::fs::read_to_string(root.path().join("default.custom.yaml")).unwrap(),
        )
        .unwrap();
        assert_eq!(doc["patch"]["menu/page_size"].as_u64(), Some(9));
        let list = doc["patch"]["schema_list"].as_sequence().unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0]["schema"].as_str(), Some("abcm"));
        assert_eq!(list[2]["schema"].as_str(), Some("rime_ice"));
    }

    #[test]
    fn candidate_page_size_round_trip() {
        let dir = temp_rime_dir("candidate-page-size");

        set_candidate_page_size(&dir, Schema::Ice, Some(9)).expect("write");
        assert_eq!(
            candidate_page_size(&dir, Schema::Ice).expect("read"),
            Some(9)
        );

        set_candidate_page_size(&dir, Schema::Ice, None).expect("clear");
        assert_eq!(candidate_page_size(&dir, Schema::Ice).expect("read"), None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn invalid_candidate_page_size_does_not_change_the_patch() {
        let root = tempfile::tempdir().expect("temporary Rime directory");
        let path = custom_patch_path(root.path(), Schema::Ice);
        set_candidate_page_size(root.path(), Schema::Ice, Some(5)).expect("valid size");
        let original = std::fs::read(&path).expect("original patch");
        for invalid in [0, 10, u8::MAX] {
            assert!(set_candidate_page_size(root.path(), Schema::Ice, Some(invalid)).is_err());
            assert_eq!(std::fs::read(&path).expect("unchanged patch"), original);
        }
    }
}
