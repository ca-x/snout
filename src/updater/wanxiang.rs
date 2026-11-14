use super::base::{BaseUpdater, UpdateResult};
use crate::i18n::{L10n, Lang};
use crate::types::*;
use crate::updater::{UpdateComponent, UpdateEvent, UpdatePhase};
use anyhow::{Context, Result};
use std::path::Path;

/// 万象方案更新器
pub struct WanxiangUpdater {
    pub base: BaseUpdater,
}

impl WanxiangUpdater {
    /// 检查方案更新
    pub async fn check_scheme_update(
        &self,
        schema: &Schema,
        cancel: Option<&CancelSignal>,
    ) -> Result<UpdateInfo> {
        let t = L10n::new(self.base.lang);
        let info = if self.base.client.use_mirror() {
            match self
                .base
                .client
                .find_latest_cnb_asset_info(
                    WX_OWNER,
                    WX_CNB_REPO,
                    |name| name == schema.scheme_zip(),
                    Some(WX_CNB_DICT_TAG),
                    cancel,
                )
                .await
            {
                Ok(info) => Ok(info),
                Err(_) => self.fetch_github_scheme_info(schema, cancel).await,
            }
        } else {
            self.fetch_github_scheme_info(schema, cancel).await
        };

        info.context(format!(
            "{}: {}",
            t.t("err.no_scheme_file"),
            schema.scheme_zip()
        ))
    }

    async fn fetch_github_scheme_info(
        &self,
        schema: &Schema,
        cancel: Option<&CancelSignal>,
    ) -> Result<UpdateInfo> {
        let t = L10n::new(self.base.lang);
        let releases = self
            .base
            .client
            .fetch_github_releases(schema.owner(), schema.repo(), "", cancel)
            .await?;
        BaseUpdater::find_update_info(&releases, schema.scheme_zip(), None).context(format!(
            "{}: {}",
            t.t("err.no_scheme_file"),
            schema.scheme_zip()
        ))
    }

    /// 更新方案
    pub async fn update_scheme(
        &self,
        schema: &Schema,
        config: &Config,
        cancel: Option<&CancelSignal>,
        mut progress: impl FnMut(UpdateEvent),
    ) -> Result<UpdateResult> {
        let t = L10n::new(Lang::from_str(&config.language));
        progress(UpdateEvent {
            component: UpdateComponent::Scheme,
            phase: UpdatePhase::Checking,
            progress: 0.05,
            detail: t.t("update.scheme.checking").into(),
        });

        let info = self.check_scheme_update(schema, cancel).await?;
        let record_path = self.base.cache_dir.join("scheme_record.json");
        let local = BaseUpdater::load_record(&record_path);

        // 关键文件检测
        let key_file_missing = !self.base.rime_dir.join("lua/wanxiang.lua").exists();

        // 方案切换检测
        let scheme_switched = local
            .as_ref()
            .map(|r| r.name != schema.scheme_zip())
            .unwrap_or(false);

        if !key_file_missing
            && !scheme_switched
            && !BaseUpdater::needs_update(local.as_ref(), &info)
        {
            progress(UpdateEvent {
                component: UpdateComponent::Scheme,
                phase: UpdatePhase::Finished,
                progress: 1.0,
                detail: t.t("update.up_to_date").into(),
            });
            return Ok(BaseUpdater::success_result(
                UpdateComponent::Scheme,
                t.t("update.scheme"),
                &info.tag,
                &info.tag,
                t.t("update.up_to_date"),
            ));
        }

        if key_file_missing {
            progress(UpdateEvent {
                component: UpdateComponent::Scheme,
                phase: UpdatePhase::Checking,
                progress: 0.05,
                detail: t.t("update.key_file_missing").into(),
            });
        } else if scheme_switched {
            progress(UpdateEvent {
                component: UpdateComponent::Scheme,
                phase: UpdatePhase::Checking,
                progress: 0.05,
                detail: t.t("update.scheme_switched").into(),
            });
        }

        // 下载
        self.base
            .download_and_extract(
                &info,
                config,
                &self.base.rime_dir,
                UpdateComponent::Scheme,
                cancel,
                &mut progress,
            )
            .await?;

        // 保存记录
        if let Some(signal) = cancel {
            signal.checkpoint()?;
        }
        progress(UpdateEvent {
            component: UpdateComponent::Scheme,
            phase: UpdatePhase::Saving,
            progress: 0.95,
            detail: t.t("update.saving").into(),
        });
        let record = UpdateRecord {
            name: info.name.clone(),
            update_time: info.update_time.clone(),
            tag: info.tag.clone(),
            apply_time: chrono::Utc::now().to_rfc3339(),
            sha256: info.sha256.clone(),
        };
        BaseUpdater::save_record(&record_path, &record)?;
        crate::config::persist_installed_schema(*schema)?;

        // 清理 build 目录
        let build_dir = self.base.rime_dir.join("build");
        if build_dir.exists() {
            let _ = std::fs::remove_dir_all(&build_dir);
        }

        progress(UpdateEvent {
            component: UpdateComponent::Scheme,
            phase: UpdatePhase::Finished,
            progress: 1.0,
            detail: t.t("update.scheme_done").into(),
        });
        Ok(UpdateResult {
            kind: UpdateComponent::Scheme,
            component: t.t("update.scheme").into(),
            old_version: local
                .map(|r| r.tag)
                .unwrap_or_else(|| t.t("status.not_installed").into()),
            new_version: info.tag,
            success: true,
            message: t.t("update.complete").into(),
        })
    }

    /// 检查词库更新
    pub async fn check_dict_update(
        &self,
        schema: &Schema,
        cancel: Option<&CancelSignal>,
    ) -> Result<UpdateInfo> {
        let t = L10n::new(self.base.lang);
        let dict_zip = schema
            .dict_zip()
            .with_context(|| t.t("update.no_dict").to_string())?;

        if self.base.client.use_mirror() {
            match self.fetch_mirror_dict_info(dict_zip, cancel).await {
                Ok(info) => Some(info),
                Err(_) => {
                    self.fetch_github_dict_info(schema, dict_zip, cancel)
                        .await?
                }
            }
        } else {
            self.fetch_github_dict_info(schema, dict_zip, cancel)
                .await?
        }
        .context(format!("{}: {dict_zip}", t.t("err.no_dict_file")))
    }

    async fn fetch_mirror_dict_info(
        &self,
        dict_zip: &str,
        cancel: Option<&CancelSignal>,
    ) -> Result<UpdateInfo> {
        let latest_tag = self
            .base
            .client
            .fetch_cnb_latest_tag(WX_OWNER, WX_CNB_REPO, cancel)
            .await?;
        let latest_release = self
            .base
            .client
            .fetch_cnb_release(WX_OWNER, WX_CNB_REPO, &latest_tag, cancel)
            .await?;
        if let Some(info) = find_matching_release_asset(&latest_release, dict_zip) {
            return Ok(info);
        }
        if latest_tag == WX_CNB_DICT_TAG {
            anyhow::bail!("mirror asset not found: {dict_zip}");
        }

        let fallback_release = self
            .base
            .client
            .fetch_cnb_release(WX_OWNER, WX_CNB_REPO, WX_CNB_DICT_TAG, cancel)
            .await?;
        find_matching_release_asset(&fallback_release, dict_zip)
            .ok_or_else(|| anyhow::anyhow!("mirror asset not found: {dict_zip}"))
    }

    async fn fetch_github_dict_info(
        &self,
        schema: &Schema,
        dict_zip: &str,
        cancel: Option<&CancelSignal>,
    ) -> Result<Option<UpdateInfo>> {
        let releases = self
            .base
            .client
            .fetch_github_releases(schema.owner(), schema.repo(), schema.dict_tag(), cancel)
            .await?;
        Ok(BaseUpdater::find_update_info(&releases, dict_zip, None))
    }

    /// 更新词库
    pub async fn update_dict(
        &self,
        schema: &Schema,
        config: &Config,
        cancel: Option<&CancelSignal>,
        mut progress: impl FnMut(UpdateEvent),
    ) -> Result<UpdateResult> {
        let t = L10n::new(Lang::from_str(&config.language));
        progress(UpdateEvent {
            component: UpdateComponent::Dict,
            phase: UpdatePhase::Checking,
            progress: 0.05,
            detail: t.t("update.dict.checking").into(),
        });

        let info = self.check_dict_update(schema, cancel).await?;
        let record_path = self.base.cache_dir.join("dict_record.json");
        let local = BaseUpdater::load_record(&record_path);

        if dict_is_current(&self.base.rime_dir, schema, local.as_ref(), &info) {
            progress(UpdateEvent {
                component: UpdateComponent::Dict,
                phase: UpdatePhase::Finished,
                progress: 1.0,
                detail: t.t("update.up_to_date").into(),
            });
            return Ok(BaseUpdater::success_result(
                UpdateComponent::Dict,
                t.t("update.dict"),
                &info.tag,
                &info.tag,
                t.t("update.up_to_date"),
            ));
        }

        // 下载
        let dict_dir = self.base.rime_dir.join("dicts");
        self.base
            .download_and_extract(
                &info,
                config,
                &dict_dir,
                UpdateComponent::Dict,
                cancel,
                &mut progress,
            )
            .await?;

        // 保存记录
        if let Some(signal) = cancel {
            signal.checkpoint()?;
        }
        progress(UpdateEvent {
            component: UpdateComponent::Dict,
            phase: UpdatePhase::Saving,
            progress: 0.95,
            detail: t.t("update.saving").into(),
        });
        let record = UpdateRecord {
            name: info.name.clone(),
            update_time: info.update_time.clone(),
            tag: info.tag.clone(),
            apply_time: chrono::Utc::now().to_rfc3339(),
            sha256: info.sha256.clone(),
        };
        BaseUpdater::save_record(&record_path, &record)?;

        progress(UpdateEvent {
            component: UpdateComponent::Dict,
            phase: UpdatePhase::Finished,
            progress: 1.0,
            detail: t.t("update.dict_done").into(),
        });
        Ok(UpdateResult {
            kind: UpdateComponent::Dict,
            component: t.t("update.dict").into(),
            old_version: local
                .map(|r| r.tag)
                .unwrap_or_else(|| t.t("status.not_installed").into()),
            new_version: info.tag,
            success: true,
            message: t.t("update.complete").into(),
        })
    }

    /// 检查模型更新
    pub async fn check_model_update(&self, cancel: Option<&CancelSignal>) -> Result<UpdateInfo> {
        let t = L10n::new(self.base.lang);
        let info = if self.base.client.use_mirror() {
            match self.fetch_mirror_model_info(cancel).await {
                Ok(info) => Some(info),
                Err(_) => self.fetch_github_model_info(cancel).await?,
            }
        } else {
            self.fetch_github_model_info(cancel).await?
        };

        info.context(format!("{}: {MODEL_FILE}", t.t("err.no_model_file")))
    }

    async fn fetch_mirror_model_info(&self, cancel: Option<&CancelSignal>) -> Result<UpdateInfo> {
        let release = self
            .base
            .client
            .fetch_cnb_release(WX_OWNER, WX_CNB_REPO, "model", cancel)
            .await?;
        find_matching_release_asset(&release, MODEL_FILE)
            .ok_or_else(|| anyhow::anyhow!("mirror asset not found: {MODEL_FILE}"))
    }

    async fn fetch_github_model_info(
        &self,
        cancel: Option<&CancelSignal>,
    ) -> Result<Option<UpdateInfo>> {
        let releases = self
            .base
            .client
            .fetch_github_releases(WX_OWNER, MODEL_REPO, MODEL_TAG, cancel)
            .await?;
        Ok(BaseUpdater::find_update_info(&releases, MODEL_FILE, None))
    }

    /// 更新模型
    pub async fn update_model(
        &self,
        config: &Config,
        cancel: Option<&CancelSignal>,
        mut progress: impl FnMut(UpdateEvent),
    ) -> Result<UpdateResult> {
        let t = L10n::new(Lang::from_str(&config.language));
        progress(UpdateEvent {
            component: UpdateComponent::Model,
            phase: UpdatePhase::Checking,
            progress: 0.05,
            detail: t.t("update.model.checking").into(),
        });

        let info = self.check_model_update(cancel).await?;
        let record_path = self.base.cache_dir.join("model_record.json");
        let local = BaseUpdater::load_record(&record_path);

        let target = self.base.rime_dir.join(MODEL_FILE);

        // 已有相同文件则跳过
        if adopt_current_model(&target, &record_path, local.as_ref(), &info, cancel)? {
            let current_tag = local
                .as_ref()
                .map(|rec| rec.tag.clone())
                .unwrap_or_else(|| info.tag.clone());
            progress(UpdateEvent {
                component: UpdateComponent::Model,
                phase: UpdatePhase::Finished,
                progress: 1.0,
                detail: t.t("update.up_to_date").into(),
            });
            return Ok(UpdateResult {
                kind: UpdateComponent::Model,
                component: t.t("update.model").into(),
                old_version: current_tag.clone(),
                new_version: info.tag.clone(),
                success: true,
                message: t.t("update.up_to_date").into(),
            });
        }

        // 下载
        progress(UpdateEvent {
            component: UpdateComponent::Model,
            phase: UpdatePhase::Downloading,
            progress: 0.15,
            detail: t.t("update.download_model").into(),
        });
        crate::deployer::prepare_for_update(Lang::from_str(&config.language))?;
        let dl_client = crate::api::Client::new_download_client(config)?;
        dl_client
            .download_file(&info.url, &target, config, cancel, |dl, total| {
                if let Some(t) = total {
                    let pct = 0.15 + (dl as f64 / t as f64) * 0.75;
                    progress(UpdateEvent {
                        component: UpdateComponent::Model,
                        phase: UpdatePhase::Downloading,
                        progress: pct,
                        detail: format!(
                            "{} {:.0}%",
                            L10n::new(Lang::from_str(&config.language))
                                .t("update.download_progress"),
                            (dl as f64 / t as f64) * 100.0
                        ),
                    });
                }
            })
            .await?;

        if !info.sha256.is_empty() {
            if let Some(signal) = cancel {
                signal.checkpoint()?;
            }
            progress(UpdateEvent {
                component: UpdateComponent::Model,
                phase: UpdatePhase::Verifying,
                progress: 0.92,
                detail: t.t("update.verifying").into(),
            });
            if !crate::fileutil::hash::verify_sha256(&target, &info.sha256) {
                anyhow::bail!("{}", t.t("err.sha256_mismatch"));
            }
        }

        // 保存记录
        if let Some(signal) = cancel {
            signal.checkpoint()?;
        }
        progress(UpdateEvent {
            component: UpdateComponent::Model,
            phase: UpdatePhase::Saving,
            progress: 0.95,
            detail: t.t("update.saving").into(),
        });
        save_model_record(&record_path, &info)?;

        progress(UpdateEvent {
            component: UpdateComponent::Model,
            phase: UpdatePhase::Finished,
            progress: 1.0,
            detail: t.t("update.model_done").into(),
        });
        Ok(UpdateResult {
            kind: UpdateComponent::Model,
            component: t.t("update.model").into(),
            old_version: local
                .map(|r| r.tag)
                .unwrap_or_else(|| t.t("status.not_installed").into()),
            new_version: info.tag,
            success: true,
            message: t.t("update.complete").into(),
        })
    }
}

pub(crate) fn dict_is_current(
    rime_dir: &Path,
    schema: &Schema,
    local: Option<&UpdateRecord>,
    info: &UpdateInfo,
) -> bool {
    let required_files = match schema {
        Schema::WanxiangBase => ["zi.dict.yaml", "jichu.dict.yaml"],
        _ if schema.is_wanxiang() => ["zi.pro.dict.yaml", "jichu.pro.dict.yaml"],
        _ => return false,
    };
    let dict_dir = rime_dir.join("dicts");
    !BaseUpdater::needs_update(local, info)
        && required_files
            .iter()
            .all(|file| dict_dir.join(file).is_file())
}

fn adopt_current_model(
    target: &Path,
    record_path: &Path,
    local: Option<&UpdateRecord>,
    info: &UpdateInfo,
    cancel: Option<&CancelSignal>,
) -> Result<bool> {
    if !model_is_current(target, local, info) {
        return Ok(false);
    }
    let metadata_matches = local.is_some_and(|record| {
        record.name == MODEL_FILE
            && record.tag == info.tag
            && record.update_time == info.update_time
            && record.sha256 == info.sha256
    });
    if !info.sha256.is_empty() && !metadata_matches {
        if let Some(signal) = cancel {
            signal.checkpoint()?;
        }
        save_model_record(record_path, info)?;
    }
    Ok(true)
}

fn save_model_record(record_path: &Path, info: &UpdateInfo) -> Result<()> {
    let record = UpdateRecord {
        name: MODEL_FILE.into(),
        update_time: info.update_time.clone(),
        tag: info.tag.clone(),
        apply_time: chrono::Utc::now().to_rfc3339(),
        sha256: info.sha256.clone(),
    };
    BaseUpdater::save_record(record_path, &record)
}

pub(crate) fn model_is_current(
    target: &Path,
    local: Option<&UpdateRecord>,
    info: &UpdateInfo,
) -> bool {
    if !target.is_file() {
        return false;
    }

    if info.sha256.is_empty() {
        local.is_some_and(|record| record.name == MODEL_FILE)
            && !BaseUpdater::needs_update(local, info)
    } else {
        crate::fileutil::hash::verify_sha256(target, &info.sha256)
    }
}

fn find_matching_release_asset(release: &GitHubRelease, file_name: &str) -> Option<UpdateInfo> {
    BaseUpdater::find_update_info(std::slice::from_ref(release), file_name, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE_DICT_FILES: [&str; 2] = ["dicts/zi.dict.yaml", "dicts/jichu.dict.yaml"];
    const PRO_DICT_FILES: [&str; 2] = ["dicts/zi.pro.dict.yaml", "dicts/jichu.pro.dict.yaml"];
    const DICT_CASES: &[(Schema, [&str; 2])] = &[
        (Schema::WanxiangBase, BASE_DICT_FILES),
        (Schema::WanxiangMoqi, PRO_DICT_FILES),
        (Schema::WanxiangFlypy, PRO_DICT_FILES),
        (Schema::WanxiangZrm, PRO_DICT_FILES),
        (Schema::WanxiangTiger, PRO_DICT_FILES),
        (Schema::WanxiangWubi, PRO_DICT_FILES),
        (Schema::WanxiangHanxin, PRO_DICT_FILES),
        (Schema::WanxiangShouyou, PRO_DICT_FILES),
        (Schema::WanxiangShyplus, PRO_DICT_FILES),
        (Schema::WanxiangWx, PRO_DICT_FILES),
    ];

    fn cached_dict_update(root: &Path, schema: &Schema) -> (UpdateInfo, UpdateRecord) {
        let info = UpdateInfo {
            name: schema.dict_zip().expect("dictionary archive").into(),
            url: String::new(),
            update_time: "2026-11-01T00:00:00Z".into(),
            tag: "dict-v1".into(),
            description: String::new(),
            sha256: String::new(),
            size: 0,
        };
        let record = UpdateRecord {
            name: info.name.clone(),
            update_time: info.update_time.clone(),
            tag: info.tag.clone(),
            apply_time: String::new(),
            sha256: String::new(),
        };
        let record_path = root.join("cache/dict_record.json");
        BaseUpdater::save_record(&record_path, &record).expect("cached dictionary record");
        let local = BaseUpdater::load_record(&record_path).expect("read cached dictionary record");
        (info, local)
    }

    fn install_dict_files(rime_dir: &Path, files: &[&str]) {
        for file in files {
            let path = rime_dir.join(file);
            std::fs::create_dir_all(path.parent().expect("dictionary parent"))
                .expect("dictionary directory");
            std::fs::write(path, "installed dictionary").expect("dictionary file");
        }
    }

    #[test]
    fn dict_freshness_rejects_missing_and_empty_roots_for_all_variants() {
        for (schema, files) in DICT_CASES {
            let root = tempfile::tempdir().expect("temporary root");
            let (info, local) = cached_dict_update(root.path(), schema);
            let installed = root.path().join("installed");
            install_dict_files(&installed, files);
            assert!(dict_is_current(&installed, schema, Some(&local), &info));

            let other = root.path().join("other");
            assert!(
                !dict_is_current(&other, schema, Some(&local), &info),
                "missing root: {schema:?}"
            );
            std::fs::create_dir_all(other.join("dicts")).expect("empty dictionary directory");
            assert!(
                !dict_is_current(&other, schema, Some(&local), &info),
                "empty root: {schema:?}"
            );
        }
    }

    #[test]
    fn dict_freshness_rejects_deleted_sentinels_for_all_variants() {
        for (schema, files) in DICT_CASES {
            let root = tempfile::tempdir().expect("temporary root");
            let (info, local) = cached_dict_update(root.path(), schema);
            let rime_dir = root.path().join("rime");
            install_dict_files(&rime_dir, files);
            assert!(dict_is_current(&rime_dir, schema, Some(&local), &info));

            for file in files {
                let path = rime_dir.join(file);
                std::fs::remove_file(&path).expect("delete dictionary");
                assert!(
                    !dict_is_current(&rime_dir, schema, Some(&local), &info),
                    "missing {file}: {schema:?}"
                );
                std::fs::create_dir(&path).expect("directory instead of dictionary");
                assert!(
                    !dict_is_current(&rime_dir, schema, Some(&local), &info),
                    "directory {file}: {schema:?}"
                );
                std::fs::remove_dir(&path).expect("remove placeholder directory");
                std::fs::write(path, "installed dictionary").expect("restore dictionary");
            }
        }
    }

    #[test]
    fn dict_freshness_rejects_wrong_variant_and_nested_layouts_for_all_variants() {
        for (schema, files) in DICT_CASES {
            let root = tempfile::tempdir().expect("temporary root");
            let (info, local) = cached_dict_update(root.path(), schema);
            let rime_dir = root.path().join("rime");
            let wrong_files = if *schema == Schema::WanxiangBase {
                &PRO_DICT_FILES
            } else {
                &BASE_DICT_FILES
            };
            install_dict_files(&rime_dir, wrong_files);
            install_dict_files(&rime_dir.join("dicts"), files);
            assert!(
                !dict_is_current(&rime_dir, schema, Some(&local), &info),
                "wrong layout or variant: {schema:?}"
            );
        }
    }

    #[test]
    fn dict_freshness_preserves_valid_install_and_metadata_checks_for_all_variants() {
        assert_eq!(
            DICT_CASES.len(),
            Schema::all()
                .iter()
                .filter(|schema| schema.is_wanxiang())
                .count()
        );
        for (schema, files) in DICT_CASES {
            let root = tempfile::tempdir().expect("temporary root");
            let (mut info, local) = cached_dict_update(root.path(), schema);
            let rime_dir = root.path().join("rime");
            install_dict_files(&rime_dir, files);
            assert!(
                dict_is_current(&rime_dir, schema, Some(&local), &info),
                "installed: {schema:?}"
            );
            assert!(!dict_is_current(&rime_dir, schema, None, &info));
            info.tag = "dict-v2".into();
            assert!(!dict_is_current(&rime_dir, schema, Some(&local), &info));
            info.tag = local.tag.clone();
            info.name = "other-dicts.zip".into();
            assert!(!dict_is_current(&rime_dir, schema, Some(&local), &info));
        }
    }

    fn model_info() -> UpdateInfo {
        UpdateInfo {
            name: MODEL_FILE.into(),
            url: String::new(),
            update_time: "2026-11-01T00:00:00Z".into(),
            tag: "model-v1".into(),
            description: String::new(),
            sha256: "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9".into(),
            size: 11,
        }
    }

    fn model_record(info: &UpdateInfo) -> UpdateRecord {
        UpdateRecord {
            name: info.name.clone(),
            update_time: info.update_time.clone(),
            tag: info.tag.clone(),
            apply_time: String::new(),
            sha256: info.sha256.clone(),
        }
    }

    #[test]
    fn model_adoption_persists_authoritative_metadata_without_a_record() {
        let root = tempfile::tempdir().expect("temporary root");
        let target = root.path().join(MODEL_FILE);
        let record_path = root.path().join("cache/model_record.json");
        std::fs::write(&target, "hello world").expect("existing valid model");
        let info = model_info();

        assert!(
            adopt_current_model(&target, &record_path, None, &info, None).expect("adopt model")
        );
        let saved = BaseUpdater::load_record(&record_path).expect("persisted model record");
        assert_eq!(saved.name, MODEL_FILE);
        assert_eq!(saved.tag, info.tag);
        assert_eq!(saved.update_time, info.update_time);
        assert_eq!(saved.sha256, info.sha256);
        assert!(!saved.apply_time.is_empty());
        assert_eq!(
            std::fs::read(target).expect("unchanged model"),
            b"hello world"
        );
    }

    #[test]
    fn model_adoption_replaces_stale_metadata_for_a_checksum_valid_file() {
        let root = tempfile::tempdir().expect("temporary root");
        let target = root.path().join(MODEL_FILE);
        let record_path = root.path().join("model_record.json");
        std::fs::write(&target, "hello world").expect("existing valid model");
        let info = model_info();
        let mut stale = model_record(&info);
        stale.name = "old-model.gram".into();
        stale.tag = "model-v0".into();
        stale.update_time = "2026-10-01T00:00:00Z".into();
        stale.sha256.clear();
        BaseUpdater::save_record(&record_path, &stale).expect("stale model record");
        let local = BaseUpdater::load_record(&record_path).expect("read stale record");

        assert!(
            adopt_current_model(&target, &record_path, Some(&local), &info, None)
                .expect("adopt model")
        );
        let saved = BaseUpdater::load_record(&record_path).expect("refreshed model record");
        assert_eq!(saved.name, MODEL_FILE);
        assert_eq!(saved.tag, info.tag);
        assert_eq!(saved.update_time, info.update_time);
        assert_eq!(saved.sha256, info.sha256);
        assert_eq!(
            std::fs::read(target).expect("unchanged model"),
            b"hello world"
        );
    }

    #[test]
    fn model_adoption_repairs_a_missing_checksum_in_an_otherwise_matching_record() {
        let root = tempfile::tempdir().expect("temporary root");
        let target = root.path().join(MODEL_FILE);
        let record_path = root.path().join("model_record.json");
        std::fs::write(&target, "hello world").expect("existing valid model");
        let info = model_info();
        let mut local = model_record(&info);
        local.sha256.clear();
        BaseUpdater::save_record(&record_path, &local).expect("legacy model record");

        assert!(
            adopt_current_model(&target, &record_path, Some(&local), &info, None)
                .expect("adopt model")
        );
        assert_eq!(
            BaseUpdater::load_record(&record_path)
                .expect("repaired record")
                .sha256,
            info.sha256
        );
    }

    #[test]
    fn model_adoption_does_not_rewrite_matching_metadata() {
        let root = tempfile::tempdir().expect("temporary root");
        let target = root.path().join(MODEL_FILE);
        let record_path = root.path().join("model_record.json");
        std::fs::write(&target, "hello world").expect("existing valid model");
        let info = model_info();
        let mut local = model_record(&info);
        local.apply_time = "2026-10-01T00:00:00Z".into();
        BaseUpdater::save_record(&record_path, &local).expect("matching model record");
        let original = std::fs::read(&record_path).expect("original record");

        assert!(
            adopt_current_model(&target, &record_path, Some(&local), &info, None)
                .expect("current model")
        );
        assert_eq!(
            std::fs::read(record_path).expect("unchanged record"),
            original
        );
    }

    #[test]
    fn model_adoption_does_not_record_unverified_files() {
        let root = tempfile::tempdir().expect("temporary root");
        let target = root.path().join(MODEL_FILE);
        let record_path = root.path().join("model_record.json");
        std::fs::write(&target, "invalid model").expect("corrupt model");
        let mut info = model_info();

        assert!(
            !adopt_current_model(&target, &record_path, None, &info, None).expect("not current")
        );
        info.sha256.clear();
        assert!(
            !adopt_current_model(&target, &record_path, None, &info, None)
                .expect("unverified model")
        );
        assert!(!record_path.exists());
    }

    #[test]
    fn model_adoption_honors_cancellation_before_record_repair() {
        let root = tempfile::tempdir().expect("temporary root");
        let target = root.path().join(MODEL_FILE);
        let record_path = root.path().join("model_record.json");
        std::fs::write(&target, "hello world").expect("existing valid model");
        let cancel = CancelSignal::new();
        cancel.cancel();

        let error = adopt_current_model(&target, &record_path, None, &model_info(), Some(&cancel))
            .expect_err("cancelled adoption");
        assert!(error.is::<UpdateCancelled>());
        assert!(!record_path.exists());
        assert_eq!(
            std::fs::read(target).expect("unchanged model"),
            b"hello world"
        );
    }

    #[test]
    fn model_adoption_reports_record_write_failure_without_changing_the_model() {
        let root = tempfile::tempdir().expect("temporary root");
        let target = root.path().join(MODEL_FILE);
        std::fs::write(&target, "hello world").expect("existing valid model");

        assert!(adopt_current_model(&target, root.path(), None, &model_info(), None).is_err());
        assert_eq!(
            std::fs::read(target).expect("unchanged model"),
            b"hello world"
        );
    }

    #[test]
    fn model_freshness_rejects_corruption_despite_matching_record() {
        let directory = tempfile::tempdir().expect("create model directory");
        let target = directory.path().join(MODEL_FILE);
        let info = model_info();
        let local = model_record(&info);

        for contents in ["corrupted model", ""] {
            std::fs::write(&target, contents).expect("write corrupted model");

            assert!(
                !model_is_current(&target, Some(&local), &info),
                "matching metadata must not bypass checksum failure for {contents:?}"
            );
        }
    }

    #[test]
    fn model_freshness_accepts_matching_checksum_without_record() {
        let directory = tempfile::tempdir().expect("create model directory");
        let target = directory.path().join(MODEL_FILE);
        std::fs::write(&target, "hello world").expect("write valid model");

        assert!(model_is_current(&target, None, &model_info()));
    }

    #[test]
    fn model_freshness_uses_checksum_instead_of_stale_metadata() {
        let directory = tempfile::tempdir().expect("create model directory");
        let target = directory.path().join(MODEL_FILE);
        std::fs::write(&target, "hello world").expect("write valid model");
        let info = model_info();
        let mut local = model_record(&info);
        local.tag = "model-v0".into();
        local.update_time = "2026-10-01T00:00:00Z".into();
        local.sha256 = "outdated checksum".into();

        assert!(model_is_current(&target, Some(&local), &info));
    }

    #[test]
    fn model_freshness_uses_metadata_only_without_checksum() {
        let directory = tempfile::tempdir().expect("create model directory");
        let target = directory.path().join(MODEL_FILE);
        std::fs::write(&target, "model without checksum").expect("write model");
        let mut info = model_info();
        info.sha256.clear();
        let local = model_record(&info);

        assert!(model_is_current(&target, Some(&local), &info));
        assert!(!model_is_current(&target, None, &info));

        info.tag = "model-v2".into();
        assert!(!model_is_current(&target, Some(&local), &info));
    }

    #[test]
    fn model_freshness_rejects_missing_file_despite_matching_record() {
        let directory = tempfile::tempdir().expect("create model directory");
        let target = directory.path().join(MODEL_FILE);
        let mut info = model_info();
        info.sha256.clear();
        let local = model_record(&info);

        assert!(!model_is_current(&target, Some(&local), &info));
    }

    #[test]
    fn model_freshness_rejects_directory_without_checksum() {
        let directory = tempfile::tempdir().expect("create model directory");
        let target = directory.path().join(MODEL_FILE);
        std::fs::create_dir(&target).expect("create directory instead of model file");
        let mut info = model_info();
        info.sha256.clear();
        let local = model_record(&info);

        assert!(!model_is_current(&target, Some(&local), &info));
    }
}
