use super::base::{BaseUpdater, UpdateResult};
use crate::i18n::{L10n, Lang};
use crate::types::*;
use crate::updater::{UpdateComponent, UpdateEvent, UpdatePhase};
use anyhow::{Context, Result};

/// 雾凇方案更新器
pub struct IceUpdater {
    pub base: BaseUpdater,
}

impl IceUpdater {
    /// 检查方案更新 (雾凇所有文件在一个 release 里)
    pub async fn check_scheme_update(&self, cancel: Option<&CancelSignal>) -> Result<UpdateInfo> {
        let t = L10n::new(self.base.lang);
        let releases = self
            .base
            .client
            .fetch_github_releases(ICE_OWNER, ICE_REPO, "", cancel)
            .await?;

        BaseUpdater::find_update_info(&releases, "full.zip", None)
            .context(format!("{}: full.zip", t.t("err.no_scheme_file")))
    }

    /// 更新方案
    pub async fn update_scheme(
        &self,
        config: &crate::types::Config,
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

        let info = self.check_scheme_update(cancel).await?;
        let record_path = self.base.cache_dir.join("scheme_record.json");
        let local = BaseUpdater::load_record(&record_path);

        // 方案切换检测
        let scheme_switched = local
            .as_ref()
            .map(|r| r.name != "full.zip")
            .unwrap_or(false);

        if !scheme_switched && scheme_is_current(&self.base.rime_dir, local.as_ref(), &info) {
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

        if scheme_switched {
            progress(UpdateEvent {
                component: UpdateComponent::Scheme,
                phase: UpdatePhase::Checking,
                progress: 0.05,
                detail: t.t("update.scheme_switched").into(),
            });
        }

        // 下载并解压
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
            name: "full.zip".into(),
            update_time: info.update_time.clone(),
            tag: info.tag.clone(),
            apply_time: chrono::Utc::now().to_rfc3339(),
            sha256: info.sha256.clone(),
        };
        BaseUpdater::save_record(&record_path, &record)?;
        crate::config::persist_installed_schema(Schema::Ice)?;

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
    pub async fn check_dict_update(&self, cancel: Option<&CancelSignal>) -> Result<UpdateInfo> {
        let t = L10n::new(self.base.lang);
        let releases = self
            .base
            .client
            .fetch_github_releases(ICE_OWNER, ICE_REPO, "", cancel)
            .await?;

        BaseUpdater::find_update_info(&releases, "all_dicts.zip", None)
            .context(format!("{}: all_dicts.zip", t.t("err.no_dict_file")))
    }

    /// 更新词库
    pub async fn update_dict(
        &self,
        config: &crate::types::Config,
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

        let info = self.check_dict_update(cancel).await?;
        let record_path = self.base.cache_dir.join("dict_record.json");
        let local = BaseUpdater::load_record(&record_path);

        if dict_is_current(&self.base.rime_dir, local.as_ref(), &info) {
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

        // 下载到 dicts 子目录
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
            name: "all_dicts.zip".into(),
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
}

fn scheme_is_current(
    rime_dir: &std::path::Path,
    local: Option<&UpdateRecord>,
    info: &UpdateInfo,
) -> bool {
    !BaseUpdater::needs_update(local, info)
        && [
            "rime_ice.schema.yaml",
            "rime_ice.dict.yaml",
            "cn_dicts/8105.dict.yaml",
            "cn_dicts/base.dict.yaml",
        ]
        .iter()
        .all(|file| rime_dir.join(file).is_file())
}

pub(crate) fn dict_is_current(
    rime_dir: &std::path::Path,
    local: Option<&UpdateRecord>,
    info: &UpdateInfo,
) -> bool {
    !BaseUpdater::needs_update(local, info)
        && ["cn_dicts/8105.dict.yaml", "cn_dicts/base.dict.yaml"]
            .iter()
            .all(|file| rime_dir.join("dicts").join(file).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const SCHEME_FILES: &[&str] = &[
        "rime_ice.schema.yaml",
        "rime_ice.dict.yaml",
        "cn_dicts/8105.dict.yaml",
        "cn_dicts/base.dict.yaml",
    ];
    const DICT_FILES: &[&str] = &[
        "dicts/cn_dicts/8105.dict.yaml",
        "dicts/cn_dicts/base.dict.yaml",
    ];

    fn cached_update(root: &Path, name: &str) -> (UpdateInfo, UpdateRecord) {
        let info = UpdateInfo {
            name: name.into(),
            url: String::new(),
            update_time: "2026-11-01T00:00:00Z".into(),
            tag: "v1".into(),
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
        let record_path = root.join("cache/record.json");
        BaseUpdater::save_record(&record_path, &record).expect("cached record");
        (
            info,
            BaseUpdater::load_record(&record_path).expect("read cached record"),
        )
    }

    fn install_files(root: &Path, files: &[&str]) {
        for file in files {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().expect("file parent")).expect("parent directory");
            std::fs::write(path, "installed").expect("installed file");
        }
    }

    #[test]
    fn scheme_freshness_requires_installed_files_despite_matching_cache() {
        let root = tempfile::tempdir().expect("temporary root");
        let rime_dir = root.path().join("rime");
        let (info, record) = cached_update(root.path(), "full.zip");
        install_files(&rime_dir, SCHEME_FILES);
        assert!(scheme_is_current(&rime_dir, Some(&record), &info));

        for file in SCHEME_FILES {
            let path = rime_dir.join(file);
            std::fs::remove_file(&path).expect("delete installed file");
            assert!(
                !scheme_is_current(&rime_dir, Some(&record), &info),
                "missing {file}"
            );
            std::fs::create_dir(&path).expect("directory instead of file");
            assert!(
                !scheme_is_current(&rime_dir, Some(&record), &info),
                "directory {file}"
            );
            std::fs::remove_dir(&path).expect("remove directory");
            std::fs::write(path, "installed").expect("restore installed file");
        }
    }

    #[test]
    fn scheme_freshness_rejects_the_same_cache_for_a_different_rime_root() {
        let root = tempfile::tempdir().expect("temporary root");
        let (info, record) = cached_update(root.path(), "full.zip");
        let installed = root.path().join("installed");
        install_files(&installed, SCHEME_FILES);
        assert!(scheme_is_current(&installed, Some(&record), &info));
        let other = root.path().join("other");
        install_files(
            &other,
            &[
                "ice.schema.yaml",
                "ice.dict.yaml",
                "cn_dicts/base.dict.yaml",
                "cn_dicts/8105.dict.yaml",
            ],
        );
        assert!(!scheme_is_current(&other, Some(&record), &info));
    }

    #[test]
    fn scheme_freshness_preserves_record_checks_when_files_exist() {
        let root = tempfile::tempdir().expect("temporary root");
        let (mut info, record) = cached_update(root.path(), "full.zip");
        install_files(root.path(), SCHEME_FILES);
        assert!(scheme_is_current(root.path(), Some(&record), &info));
        assert!(!scheme_is_current(root.path(), None, &info));
        info.tag = "v2".into();
        assert!(!scheme_is_current(root.path(), Some(&record), &info));
    }

    #[test]
    fn dict_freshness_requires_installed_data_despite_matching_cache() {
        let root = tempfile::tempdir().expect("temporary root");
        let rime_dir = root.path().join("rime");
        let (info, record) = cached_update(root.path(), "all_dicts.zip");
        install_files(&rime_dir, DICT_FILES);
        assert!(dict_is_current(&rime_dir, Some(&record), &info));

        for file in DICT_FILES {
            let path = rime_dir.join(file);
            std::fs::remove_file(&path).expect("delete installed dictionary");
            assert!(
                !dict_is_current(&rime_dir, Some(&record), &info),
                "missing {file}"
            );
            std::fs::create_dir(&path).expect("directory instead of dictionary");
            assert!(
                !dict_is_current(&rime_dir, Some(&record), &info),
                "directory {file}"
            );
            std::fs::remove_dir(&path).expect("remove directory");
            std::fs::write(path, "installed").expect("restore dictionary");
        }
    }

    #[test]
    fn dict_freshness_rejects_the_same_cache_for_a_different_rime_root() {
        let root = tempfile::tempdir().expect("temporary root");
        let (info, record) = cached_update(root.path(), "all_dicts.zip");
        let installed = root.path().join("installed");
        install_files(&installed, DICT_FILES);
        assert!(dict_is_current(&installed, Some(&record), &info));
        let other = root.path().join("other");
        install_files(&other, &["dicts/README.md", "rime_ice.dict.yaml"]);
        assert!(!dict_is_current(&other, Some(&record), &info));
    }

    #[test]
    fn dict_freshness_preserves_record_checks_when_files_exist() {
        let root = tempfile::tempdir().expect("temporary root");
        let (mut info, record) = cached_update(root.path(), "all_dicts.zip");
        install_files(root.path(), DICT_FILES);
        assert!(dict_is_current(root.path(), Some(&record), &info));
        assert!(!dict_is_current(root.path(), None, &info));
        info.tag = "v2".into();
        assert!(!dict_is_current(root.path(), Some(&record), &info));
    }
}
