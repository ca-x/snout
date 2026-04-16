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

        if !scheme_switched && !BaseUpdater::needs_update(local.as_ref(), &info) {
            progress(UpdateEvent {
                component: UpdateComponent::Scheme,
                phase: UpdatePhase::Finished,
                progress: 1.0,
                detail: t.t("update.up_to_date").into(),
            });
            return Ok(BaseUpdater::success_result(
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

        if !BaseUpdater::needs_update(local.as_ref(), &info) {
            progress(UpdateEvent {
                component: UpdateComponent::Dict,
                phase: UpdatePhase::Finished,
                progress: 1.0,
                detail: t.t("update.up_to_date").into(),
            });
            return Ok(BaseUpdater::success_result(
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
