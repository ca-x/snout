//! Updates for Rime schemes distributed as repository snapshots.
use super::{BaseUpdater, UpdateComponent, UpdateEvent, UpdateResult};
use crate::fileutil::extract::{merge_directory, UserDataBehavior};
use crate::i18n::{L10n, Lang};
use crate::types::{CancelSignal, Config, Schema, UpdateInfo, UpdateRecord};
use anyhow::{ensure, Context, Result};
use std::path::{Path, PathBuf};

pub struct RepositoryUpdater {
    pub base: BaseUpdater,
}
impl RepositoryUpdater {
    pub async fn check_scheme_update(
        &self,
        schema: &Schema,
        cancel: Option<&CancelSignal>,
    ) -> Result<UpdateInfo> {
        ensure!(
            matches!(schema, Schema::Wubi86Jidian | Schema::Abcm),
            "Not a repository-distributed scheme"
        );
        self.base
            .client
            .fetch_github_branch_archive(
                schema.owner(),
                schema.repo(),
                "master",
                schema.scheme_zip(),
                cancel,
            )
            .await
    }
    pub async fn update_scheme(
        &self,
        schema: &Schema,
        config: &Config,
        cancel: Option<&CancelSignal>,
        mut progress: impl FnMut(UpdateEvent),
    ) -> Result<UpdateResult> {
        let t = L10n::new(Lang::from_str(&config.language));
        let info = self.check_scheme_update(schema, cancel).await?;
        let record_path = self.base.cache_dir.join("scheme_record.json");
        let old = BaseUpdater::load_record(&record_path);
        if old
            .as_ref()
            .is_some_and(|record| record.name == info.name && record.tag == info.tag)
            && distribution_present(&self.base.rime_dir, *schema)
        {
            return Ok(BaseUpdater::success_result(
                UpdateComponent::Scheme,
                t.t("update.scheme"),
                &info.tag,
                &info.tag,
                t.t("update.up_to_date"),
            ));
        }
        let staging = tempfile::tempdir()?;
        self.base
            .download_and_extract(
                &info,
                config,
                staging.path(),
                UpdateComponent::Scheme,
                cancel,
                &mut progress,
            )
            .await?;
        if let Some(cancel) = cancel {
            cancel.checkpoint()?;
        }
        let distribution = find_distribution(staging.path(), schema.schema_id())?
            .context("Scheme file missing from upstream archive / 上游归档缺少方案文件")?;
        let filtered = tempfile::tempdir()?;
        stage_distribution(&distribution, filtered.path())?;
        // ABC's translator uses the standard luna_pinyin dictionary.
        if *schema == Schema::Abcm && !self.base.rime_dir.join("luna_pinyin.dict.yaml").is_file() {
            let dependency = self
                .base
                .client
                .fetch_github_branch_archive(
                    "rime",
                    "rime-luna-pinyin",
                    "master",
                    "luna-pinyin.zip",
                    cancel,
                )
                .await?;
            let dictionaries = tempfile::tempdir()?;
            self.base
                .download_and_extract(
                    &dependency,
                    config,
                    dictionaries.path(),
                    UpdateComponent::Scheme,
                    cancel,
                    &mut progress,
                )
                .await?;
            let source = find_file(dictionaries.path(), "luna_pinyin.dict.yaml")?
                .context("Missing luna_pinyin dictionary")?;
            std::fs::copy(source, filtered.path().join("luna_pinyin.dict.yaml"))?;
        }
        if let Some(cancel) = cancel {
            cancel.checkpoint()?;
        }
        merge_directory(
            filtered.path(),
            &self.base.rime_dir,
            if config.user_data_policy == "discard" {
                UserDataBehavior::Discard
            } else {
                UserDataBehavior::Preserve
            },
            &repository_exclude_patterns(config),
        )?;
        crate::custom::enable_schema(&self.base.rime_dir, *schema)?;
        BaseUpdater::save_record(
            &record_path,
            &UpdateRecord {
                name: info.name.clone(),
                tag: info.tag.clone(),
                update_time: info.update_time,
                sha256: info.sha256,
                apply_time: chrono::Utc::now().to_rfc3339(),
            },
        )?;
        crate::config::persist_installed_schema(*schema)?;
        Ok(BaseUpdater::success_result(
            UpdateComponent::Scheme,
            t.t("update.scheme"),
            old.as_ref()
                .map(|record| record.tag.as_str())
                .unwrap_or("-"),
            &info.tag,
            t.t("update.scheme_done"),
        ))
    }
}

fn repository_exclude_patterns(config: &Config) -> Vec<String> {
    let mut patterns = crate::config::effective_exclude_patterns(config);
    patterns.extend([
        "wubi86_jidian_user.dict.yaml".into(),
        "wubi86_jidian_user_hamster.dict.yaml".into(),
    ]);
    patterns
}

fn distribution_present(root: &Path, schema: Schema) -> bool {
    let required: &[&str] = match schema {
        Schema::Wubi86Jidian => &[
            "wubi86_jidian.schema.yaml",
            "wubi86_jidian.dict.yaml",
            "wubi86_jidian_extra.dict.yaml",
            "wubi86_jidian_extra_district.dict.yaml",
            "pinyin_simp.dict.yaml",
        ],
        Schema::Abcm => &[
            "abcm.schema.yaml",
            "luna_pinyin.dict.yaml",
            "lua/abcm_processor.lua",
            "lua/abcm_translator.lua",
            "lua/abcm_filter.lua",
            "lua/abcm_stroke.lua",
            "lua/abcm_keywords.lua",
        ],
        _ => return false,
    };
    required.iter().all(|name| root.join(name).is_file())
}

fn find_file(root: &Path, name: &str) -> Result<Option<PathBuf>> {
    if root.join(name).is_file() {
        return Ok(Some(root.join(name)));
    }
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(found) = find_file(&entry.path(), name)? {
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}
fn find_distribution(root: &Path, id: &str) -> Result<Option<PathBuf>> {
    Ok(find_file(root, &format!("{id}.schema.yaml"))?
        .and_then(|file| file.parent().map(Path::to_owned)))
}
fn stage_distribution(source: &Path, target: &Path) -> Result<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_file()
            && !name.ends_with(".custom.yaml")
            && (name.ends_with(".yaml") || name.ends_with(".ico"))
        {
            std::fs::copy(entry.path(), target.join(name))?;
        } else if name == "lua" && entry.file_type()?.is_dir() {
            merge_directory(
                &entry.path(),
                &target.join(name),
                UserDataBehavior::Discard,
                &[],
            )?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jidian_user_dictionaries_survive_preserving_updates() {
        let incoming = tempfile::tempdir().unwrap();
        let installed = tempfile::tempdir().unwrap();
        for name in [
            "wubi86_jidian_user.dict.yaml",
            "wubi86_jidian_user_hamster.dict.yaml",
        ] {
            std::fs::write(incoming.path().join(name), "upstream template").unwrap();
            std::fs::write(installed.path().join(name), "my words").unwrap();
        }
        merge_directory(
            incoming.path(),
            installed.path(),
            UserDataBehavior::Preserve,
            &repository_exclude_patterns(&Config::default()),
        )
        .unwrap();
        for name in [
            "wubi86_jidian_user.dict.yaml",
            "wubi86_jidian_user_hamster.dict.yaml",
        ] {
            assert_eq!(
                std::fs::read_to_string(installed.path().join(name)).unwrap(),
                "my words"
            );
        }
    }

    #[test]
    fn a_scheme_marker_alone_does_not_hide_missing_dependencies() {
        let root = tempfile::tempdir().unwrap();
        for schema in [Schema::Wubi86Jidian, Schema::Abcm] {
            std::fs::write(
                root.path()
                    .join(format!("{}.schema.yaml", schema.schema_id())),
                "schema",
            )
            .unwrap();
            assert!(!distribution_present(root.path(), schema));
        }
    }

    #[test]
    fn nested_rime_distribution_keeps_lua_and_omits_platform_customizations() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("abcm-master/rime");
        std::fs::create_dir_all(source.join("lua")).unwrap();
        for (name, content) in [
            ("abcm.schema.yaml", "schema"),
            ("lua/abcm_processor.lua", "lua"),
            ("default.custom.yaml", "user preferences"),
            ("weasel.custom.yaml", "skin"),
            ("README.md", "docs"),
        ] {
            std::fs::write(source.join(name), content).unwrap();
        }
        let found = find_distribution(root.path(), "abcm").unwrap().unwrap();
        assert_eq!(found, source);
        let output = tempfile::tempdir().unwrap();
        stage_distribution(&found, output.path()).unwrap();
        assert!(output.path().join("abcm.schema.yaml").is_file());
        assert!(output.path().join("lua/abcm_processor.lua").is_file());
        assert!(!output.path().join("default.custom.yaml").exists());
        assert!(!output.path().join("weasel.custom.yaml").exists());
    }
}
