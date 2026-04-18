use crate::api::Client;
use crate::fileutil;
use crate::fileutil::extract::UserDataBehavior;
use crate::i18n::{L10n, Lang};
use crate::types::*;
use crate::updater::{UpdateComponent, UpdateEvent, UpdatePhase};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// 更新结果
#[derive(Debug)]
pub struct UpdateResult {
    pub component: String,
    #[allow(dead_code)]
    pub old_version: String,
    #[allow(dead_code)]
    pub new_version: String,
    pub success: bool,
    pub message: String,
}

/// 基础更新器 - 共享逻辑
pub struct BaseUpdater {
    pub client: Client,
    pub cache_dir: PathBuf,
    pub rime_dir: PathBuf,
    pub lang: Lang,
}

impl BaseUpdater {
    pub fn new(config: &Config, cache_dir: PathBuf, rime_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            client: Client::new(config)?,
            cache_dir,
            rime_dir,
            lang: Lang::from_str(&config.language),
        })
    }

    /// 加载本地更新记录
    pub fn load_record(path: &Path) -> Option<UpdateRecord> {
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// 保存更新记录
    pub fn save_record(path: &Path, record: &UpdateRecord) -> Result<()> {
        let json = serde_json::to_string_pretty(record)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, json)?;
        Ok(())
    }

    /// 判断是否需要更新
    pub fn needs_update(local: Option<&UpdateRecord>, remote: &UpdateInfo) -> bool {
        match local {
            None => true,
            Some(rec) => {
                if rec.name != remote.name {
                    return true; // 方案切换
                }
                if !remote.tag.is_empty() && !rec.tag.is_empty() && rec.tag != remote.tag {
                    return true;
                }
                if !remote.sha256.is_empty()
                    && !rec.sha256.is_empty()
                    && rec.sha256 != remote.sha256
                {
                    return true;
                }

                match (
                    parse_update_time(&rec.update_time),
                    parse_update_time(&remote.update_time),
                ) {
                    (Some(local_time), Some(remote_time)) => remote_time > local_time,
                    _ => false,
                }
            }
        }
    }

    /// 查找 asset 并转为 UpdateInfo
    pub fn find_update_info(
        releases: &[GitHubRelease],
        filename: &str,
        skip_tag: Option<&str>,
    ) -> Option<UpdateInfo> {
        for release in releases {
            if let Some(skip) = skip_tag {
                if release.tag_name == skip {
                    continue;
                }
            }
            for asset in &release.assets {
                if asset.name == filename {
                    return Some(UpdateInfo {
                        name: asset.name.clone(),
                        url: asset.browser_download_url.clone(),
                        update_time: asset.updated_at.clone().unwrap_or_default(),
                        tag: release.tag_name.clone(),
                        description: release.body.clone(),
                        sha256: asset_sha256(asset),
                        size: asset.size,
                    });
                }
            }
        }
        None
    }

    /// 检查本地文件 SHA 是否匹配
    pub fn hash_matches(&self, expected_sha: &str, path: &Path) -> bool {
        if expected_sha.is_empty() {
            return false;
        }
        fileutil::hash::verify_sha256(path, expected_sha)
    }

    /// 通用下载+校验+解压流程
    pub async fn download_and_extract(
        &self,
        info: &UpdateInfo,
        config: &Config,
        extract_dest: &Path,
        component: UpdateComponent,
        cancel: Option<&CancelSignal>,
        progress: &mut impl FnMut(UpdateEvent),
    ) -> Result<()> {
        let t = L10n::new(self.lang);
        let zip_path = self.cache_dir.join(&info.name);
        let mut already_verified = false;

        // 缓存复用
        if zip_path.exists() && !info.sha256.is_empty() {
            if let Some(signal) = cancel {
                signal.checkpoint()?;
            }
            progress(UpdateEvent {
                component,
                phase: UpdatePhase::Checking,
                progress: 0.10,
                detail: t.t("update.cache.verify").into(),
            });
            if self.hash_matches(&info.sha256, &zip_path) {
                progress(UpdateEvent {
                    component,
                    phase: UpdatePhase::Checking,
                    progress: 0.70,
                    detail: t.t("update.cache.valid").into(),
                });
                already_verified = true;
            } else {
                self.do_download(info, config, &zip_path, component, cancel, progress)
                    .await?;
            }
        } else {
            self.do_download(info, config, &zip_path, component, cancel, progress)
                .await?;
        }

        // SHA256 校验 (仅当未在缓存阶段验证过)
        if !already_verified && !info.sha256.is_empty() {
            if let Some(signal) = cancel {
                signal.checkpoint()?;
            }
            progress(UpdateEvent {
                component,
                phase: UpdatePhase::Verifying,
                progress: 0.80,
                detail: t.t("update.verifying").into(),
            });
            if !fileutil::hash::verify_sha256(&zip_path, &info.sha256) {
                anyhow::bail!("{}", t.t("err.sha256_mismatch"));
            }
        }

        // 解压
        if let Some(signal) = cancel {
            signal.checkpoint()?;
        }
        progress(UpdateEvent {
            component,
            phase: UpdatePhase::Extracting,
            progress: 0.85,
            detail: t.t("update.extracting").into(),
        });
        crate::deployer::prepare_for_update(self.lang)?;
        std::fs::create_dir_all(extract_dest)?;
        fileutil::extract::extract_zip(
            &zip_path,
            extract_dest,
            user_data_behavior_for_config(config),
        )?;

        Ok(())
    }

    /// 内部下载方法
    pub(super) async fn do_download(
        &self,
        info: &UpdateInfo,
        config: &Config,
        zip_path: &Path,
        component: UpdateComponent,
        cancel: Option<&CancelSignal>,
        progress: &mut impl FnMut(UpdateEvent),
    ) -> Result<()> {
        let t = L10n::new(self.lang);
        progress(UpdateEvent {
            component,
            phase: UpdatePhase::Downloading,
            progress: 0.15,
            detail: t.t("update.downloading").into(),
        });
        let dl_client = Client::new_download_client(config)?;
        dl_client
            .download_file(&info.url, zip_path, config, cancel, |downloaded, total| {
                if let Some(t) = total {
                    let pct = 0.15 + (downloaded as f64 / t as f64) * 0.55;
                    progress(UpdateEvent {
                        component,
                        phase: UpdatePhase::Downloading,
                        progress: pct,
                        detail: format!(
                            "{} {:.0}%",
                            L10n::new(self.lang).t("update.download_progress"),
                            (downloaded as f64 / t as f64) * 100.0
                        ),
                    });
                }
            })
            .await
    }

    /// 构建成功结果
    pub fn success_result(component: &str, old: &str, new: &str, msg: &str) -> UpdateResult {
        UpdateResult {
            component: component.into(),
            old_version: old.into(),
            new_version: new.into(),
            success: true,
            message: msg.into(),
        }
    }

    /// 构建失败结果
    pub fn fail_result(component: &str, e: &anyhow::Error) -> UpdateResult {
        UpdateResult {
            component: component.into(),
            old_version: "?".into(),
            new_version: "?".into(),
            success: false,
            message: e.to_string(),
        }
    }

    /// 构建错误结果 (自定义消息)
    pub fn error_result(component: &str, msg: &str) -> UpdateResult {
        UpdateResult {
            component: component.into(),
            old_version: "-".into(),
            new_version: "-".into(),
            success: false,
            message: msg.into(),
        }
    }
}

fn user_data_behavior_for_config(config: &Config) -> UserDataBehavior {
    match config.user_data_policy.trim().to_ascii_lowercase().as_str() {
        "discard" => UserDataBehavior::Discard,
        _ => UserDataBehavior::Preserve,
    }
}

fn parse_update_time(value: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    if value.trim().is_empty() {
        return None;
    }
    chrono::DateTime::parse_from_rfc3339(value).ok()
}

fn asset_sha256(asset: &GitHubAsset) -> String {
    if let Some(sha256) = asset.sha256.as_deref() {
        if !sha256.trim().is_empty() {
            return sha256.trim().to_string();
        }
    }

    asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_record() -> UpdateRecord {
        UpdateRecord {
            name: "asset.zip".into(),
            update_time: "2026-01-01T00:00:00+00:00".into(),
            tag: "v1.0.0".into(),
            apply_time: "2026-01-01T00:00:00+00:00".into(),
            sha256: "abc".into(),
        }
    }

    fn sample_info() -> UpdateInfo {
        UpdateInfo {
            name: "asset.zip".into(),
            url: "https://example.invalid/asset.zip".into(),
            update_time: "2026-01-01T00:00:00+00:00".into(),
            tag: "v1.0.0".into(),
            description: String::new(),
            sha256: "abc".into(),
            size: 1,
        }
    }

    #[test]
    fn needs_update_ignores_missing_remote_sha_when_tag_matches() {
        let local = sample_record();
        let mut remote = sample_info();
        remote.sha256.clear();

        assert!(!BaseUpdater::needs_update(Some(&local), &remote));
    }

    #[test]
    fn needs_update_uses_remote_time_when_tag_and_sha_match() {
        let local = sample_record();
        let mut remote = sample_info();
        remote.update_time = "2026-01-02T00:00:00+00:00".into();

        assert!(BaseUpdater::needs_update(Some(&local), &remote));
    }

    #[test]
    fn needs_update_detects_tag_change_without_sha() {
        let local = sample_record();
        let mut remote = sample_info();
        remote.tag = "v1.1.0".into();
        remote.sha256.clear();

        assert!(BaseUpdater::needs_update(Some(&local), &remote));
    }

    #[test]
    fn asset_sha256_reads_digest_fallback() {
        let asset = GitHubAsset {
            name: "asset.zip".into(),
            browser_download_url: "https://example.invalid/asset.zip".into(),
            updated_at: None,
            size: 1,
            sha256: None,
            digest: Some("sha256:deadbeef".into()),
        };

        assert_eq!(asset_sha256(&asset), "deadbeef");
    }
}
