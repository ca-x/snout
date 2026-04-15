use crate::api::Client;
use crate::fileutil;
use crate::types::*;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// 更新结果
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
}

impl BaseUpdater {
    pub fn new(config: &Config, cache_dir: PathBuf, rime_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            client: Client::new(config)?,
            cache_dir,
            rime_dir,
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
                rec.tag != remote.tag || rec.sha256 != remote.sha256
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
                        sha256: asset.sha256.clone().unwrap_or_default(),
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
        progress: &mut impl FnMut(&str, f64),
    ) -> Result<()> {
        let zip_path = self.cache_dir.join(&info.name);
        let mut already_verified = false;

        // 缓存复用
        if zip_path.exists() && !info.sha256.is_empty() {
            progress("校验本地缓存...", 0.10);
            if self.hash_matches(&info.sha256, &zip_path) {
                progress("缓存有效，跳过下载", 0.70);
                already_verified = true;
            } else {
                self.do_download(info, config, &zip_path, progress).await?;
            }
        } else {
            self.do_download(info, config, &zip_path, progress).await?;
        }

        // SHA256 校验 (仅当未在缓存阶段验证过)
        if !already_verified && !info.sha256.is_empty() {
            progress("校验文件...", 0.80);
            if !fileutil::hash::verify_sha256(&zip_path, &info.sha256) {
                anyhow::bail!("SHA256 校验失败");
            }
        }

        // 解压
        progress("解压中...", 0.85);
        std::fs::create_dir_all(extract_dest)?;
        fileutil::extract::extract_zip(&zip_path, extract_dest)?;

        Ok(())
    }

    /// 内部下载方法
    pub(super) async fn do_download(
        &self,
        info: &UpdateInfo,
        config: &Config,
        zip_path: &Path,
        progress: &mut impl FnMut(&str, f64),
    ) -> Result<()> {
        progress("下载中...", 0.15);
        let dl_client = Client::new_download_client(config)?;
        dl_client
            .download_file(&info.url, zip_path, |downloaded, total| {
                if let Some(t) = total {
                    let pct = 0.15 + (downloaded as f64 / t as f64) * 0.55;
                    progress(
                        &format!("下载中... {:.0}%", (downloaded as f64 / t as f64) * 100.0),
                        pct,
                    );
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
