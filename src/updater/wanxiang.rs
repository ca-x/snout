use super::base::{BaseUpdater, UpdateResult};
use crate::i18n::{L10n, Lang};
use crate::types::*;
use crate::updater::{UpdateComponent, UpdateEvent, UpdatePhase};
use anyhow::{Context, Result};

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

        // 处理 CNB 嵌套目录
        let mut warnings = Vec::new();
        if self.base.client.use_mirror() {
            if let Err(e) =
                crate::fileutil::extract::handle_nested_dir(&self.base.rime_dir, &info.name)
            {
                let msg = format!("{}: {e}", t.t("update.nested_dir_failed"));
                crate::feedback::warn(format!("⚠️ {msg}"));
                warnings.push(msg);
            }
        }

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
        let msg = if warnings.is_empty() {
            t.t("update.complete").into()
        } else {
            format!("{} ({})", t.t("update.complete"), warnings.join("; "))
        };
        Ok(UpdateResult {
            component: t.t("update.scheme").into(),
            old_version: local
                .map(|r| r.tag)
                .unwrap_or_else(|| t.t("status.not_installed").into()),
            new_version: info.tag,
            success: true,
            message: msg,
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

        let mut warnings = Vec::new();
        if self.base.client.use_mirror() {
            if let Err(e) = crate::fileutil::extract::handle_nested_dir(&dict_dir, &info.name) {
                let msg = format!("{}: {e}", t.t("update.nested_dir_failed"));
                crate::feedback::warn(format!("⚠️ {msg}"));
                warnings.push(msg);
            }
        }

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
        let msg = if warnings.is_empty() {
            t.t("update.complete").into()
        } else {
            format!("{} ({})", t.t("update.complete"), warnings.join("; "))
        };
        Ok(UpdateResult {
            component: t.t("update.dict").into(),
            old_version: local
                .map(|r| r.tag)
                .unwrap_or_else(|| t.t("status.not_installed").into()),
            new_version: info.tag,
            success: true,
            message: msg,
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
        if target.exists()
            && local.as_ref().is_some_and(|rec| rec.name == MODEL_FILE)
            && ((!info.sha256.is_empty() && self.base.hash_matches(&info.sha256, &target))
                || !BaseUpdater::needs_update(local.as_ref(), &info))
        {
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
        let dl_client = crate::api::Client::new_download_client(config)?;
        dl_client
            .download_file(&info.url, &target, cancel, |dl, total| {
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
        let record = UpdateRecord {
            name: MODEL_FILE.into(),
            update_time: info.update_time.clone(),
            tag: info.tag.clone(),
            apply_time: chrono::Utc::now().to_rfc3339(),
            sha256: info.sha256.clone(),
        };
        BaseUpdater::save_record(&record_path, &record)?;

        progress(UpdateEvent {
            component: UpdateComponent::Model,
            phase: UpdatePhase::Finished,
            progress: 1.0,
            detail: t.t("update.model_done").into(),
        });
        Ok(UpdateResult {
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

fn find_matching_release_asset(release: &GitHubRelease, file_name: &str) -> Option<UpdateInfo> {
    BaseUpdater::find_update_info(std::slice::from_ref(release), file_name, None)
}
