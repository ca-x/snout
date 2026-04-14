pub mod model_patch;

use crate::api::Client;
use crate::fileutil;
use crate::types::*;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// 更新结果
pub struct UpdateResult {
    pub component: String,
    pub old_version: String,
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

    /// 从 GitHub releases 中查找匹配的 asset
    pub fn find_asset<'a>(
        releases: &'a [GitHubRelease],
        filename: &str,
        skip_tag: Option<&str>,
    ) -> Option<&'a GitHubAsset> {
        for release in releases {
            if let Some(skip) = skip_tag {
                if release.tag_name == skip {
                    continue;
                }
            }
            for asset in &release.assets {
                if asset.name == filename {
                    return Some(asset);
                }
            }
        }
        None
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
}

// ── 方案更新器 ──
pub struct SchemeUpdater {
    pub base: BaseUpdater,
}

impl SchemeUpdater {
    pub async fn check_update(&self, schema: &Schema) -> Result<UpdateInfo> {
        let releases = if self.base.client.use_mirror() && schema.is_wanxiang() {
            let release = self.base.client.fetch_cnb_release(
                WX_OWNER,
                WX_CNB_REPO,
                &format!("latest/{}", schema.scheme_zip()),
            ).await?;
            vec![release]
        } else {
            self.base.client.fetch_github_releases(
                schema.owner(),
                schema.repo(),
                "",
            ).await?
        };

        BaseUpdater::find_update_info(&releases, schema.scheme_zip(), None)
            .context(format!("未找到方案文件: {}", schema.scheme_zip()))
    }

    pub async fn run(
        &self,
        schema: &Schema,
        config: &Config,
        mut progress: impl FnMut(&str, f64),
    ) -> Result<UpdateResult> {
        progress("检查方案更新...", 0.05);

        let info = self.check_update(schema).await?;
        let record_path = self.base.cache_dir.join("scheme_record.json");
        let local = BaseUpdater::load_record(&record_path);

        if !BaseUpdater::needs_update(local.as_ref(), &info) {
            progress("方案已是最新", 1.0);
            return Ok(UpdateResult {
                component: "方案".into(),
                old_version: info.tag.clone(),
                new_version: info.tag.clone(),
                success: true,
                message: "已是最新版本".into(),
            });
        }

        progress("下载方案...", 0.15);
        let zip_path = self.base.cache_dir.join(&info.name);

        let dl_client = Client::new_download_client(config)?;
        let url = info.url.clone();
        dl_client.download_file(&url, &zip_path, |downloaded, total| {
            if let Some(t) = total {
                let pct = 0.15 + (downloaded as f64 / t as f64) * 0.60;
                progress(&format!("下载中... {:.0}%", (downloaded as f64 / t as f64) * 100.0), pct);
            }
        }).await?;

        // SHA256 校验
        if !info.sha256.is_empty() {
            progress("校验文件...", 0.80);
            if !fileutil::hash::verify_sha256(&zip_path, &info.sha256) {
                anyhow::bail!("SHA256 校验失败");
            }
        }

        progress("解压方案...", 0.85);
        fileutil::extract::extract_zip(&zip_path, &self.base.rime_dir)?;

        // 处理 CNB 嵌套目录
        if self.base.client.use_mirror() {
            let _ = fileutil::extract::handle_nested_dir(
                &self.base.rime_dir,
                &info.name,
            );
        }

        progress("保存记录...", 0.95);
        let record = UpdateRecord {
            name: info.name.clone(),
            update_time: info.update_time.clone(),
            tag: info.tag.clone(),
            apply_time: chrono::Utc::now().to_rfc3339(),
            sha256: info.sha256.clone(),
        };
        BaseUpdater::save_record(&record_path, &record)?;

        // 清理 build 目录
        let build_dir = self.base.rime_dir.join("build");
        if build_dir.exists() {
            let _ = std::fs::remove_dir_all(&build_dir);
        }

        progress("方案更新完成", 1.0);
        Ok(UpdateResult {
            component: "方案".into(),
            old_version: local.map(|r| r.tag).unwrap_or_else(|| "未安装".into()),
            new_version: info.tag,
            success: true,
            message: "更新成功".into(),
        })
    }
}

// ── 词库更新器 ──
pub struct DictUpdater {
    pub base: BaseUpdater,
}

impl DictUpdater {
    pub async fn check_update(&self, schema: &Schema) -> Result<UpdateInfo> {
        let dict_zip = schema.dict_zip()
            .context("此方案无独立词库")?;

        if self.base.client.use_mirror() && schema.is_wanxiang() {
            let tag = if dict_zip.starts_with("base") {
                WX_CNB_DICT_TAG
            } else {
                "latest"
            };
            let release = self.base.client.fetch_cnb_release(
                WX_OWNER, WX_CNB_REPO, tag,
            ).await?;
            BaseUpdater::find_update_info(&[release], dict_zip, None)
        } else {
            let releases = self.base.client.fetch_github_releases(
                schema.owner(),
                schema.repo(),
                schema.dict_tag(),
            ).await?;
            BaseUpdater::find_update_info(&releases, dict_zip, None)
        }.context(format!("未找到词库: {dict_zip}"))
    }

    pub async fn run(
        &self,
        schema: &Schema,
        config: &Config,
        mut progress: impl FnMut(&str, f64),
    ) -> Result<UpdateResult> {
        progress("检查词库更新...", 0.05);

        let info = self.check_update(schema).await?;
        let record_path = self.base.cache_dir.join("dict_record.json");
        let local = BaseUpdater::load_record(&record_path);

        if !BaseUpdater::needs_update(local.as_ref(), &info) {
            progress("词库已是最新", 1.0);
            return Ok(UpdateResult {
                component: "词库".into(),
                old_version: info.tag.clone(),
                new_version: info.tag.clone(),
                success: true,
                message: "已是最新版本".into(),
            });
        }

        progress("下载词库...", 0.15);
        let zip_path = self.base.cache_dir.join(&info.name);
        let dl_client = Client::new_download_client(config)?;
        dl_client.download_file(&info.url, &zip_path, |dl, total| {
            if let Some(t) = total {
                let pct = 0.15 + (dl as f64 / t as f64) * 0.60;
                progress(&format!("下载中... {:.0}%", (dl as f64 / t as f64) * 100.0), pct);
            }
        }).await?;

        if !info.sha256.is_empty() {
            progress("校验文件...", 0.80);
            if !fileutil::hash::verify_sha256(&zip_path, &info.sha256) {
                anyhow::bail!("SHA256 校验失败");
            }
        }

        progress("解压词库...", 0.85);
        let dict_dir = self.base.rime_dir.join("dicts");
        std::fs::create_dir_all(&dict_dir)?;
        fileutil::extract::extract_zip(&zip_path, &dict_dir)?;

        if self.base.client.use_mirror() && schema.is_wanxiang() {
            let _ = fileutil::extract::handle_nested_dir(&dict_dir, &info.name);
        }

        progress("保存记录...", 0.95);
        let record = UpdateRecord {
            name: info.name.clone(),
            update_time: info.update_time.clone(),
            tag: info.tag.clone(),
            apply_time: chrono::Utc::now().to_rfc3339(),
            sha256: info.sha256.clone(),
        };
        BaseUpdater::save_record(&record_path, &record)?;

        progress("词库更新完成", 1.0);
        Ok(UpdateResult {
            component: "词库".into(),
            old_version: local.map(|r| r.tag).unwrap_or_else(|| "未安装".into()),
            new_version: info.tag,
            success: true,
            message: "更新成功".into(),
        })
    }
}

// ── 模型更新器 ──
pub struct ModelUpdater {
    pub base: BaseUpdater,
}

impl ModelUpdater {
    pub async fn check_update(&self) -> Result<UpdateInfo> {
        let releases = self.base.client.fetch_github_releases(
            WX_OWNER, MODEL_REPO, MODEL_TAG,
        ).await?;

        BaseUpdater::find_update_info(&releases, MODEL_FILE, None)
            .context(format!("未找到模型: {MODEL_FILE}"))
    }

    pub async fn run(
        &self,
        config: &Config,
        mut progress: impl FnMut(&str, f64),
    ) -> Result<UpdateResult> {
        progress("检查模型更新...", 0.05);

        let info = self.check_update().await?;
        let record_path = self.base.cache_dir.join("model_record.json");
        let local = BaseUpdater::load_record(&record_path);

        let target = self.base.rime_dir.join(MODEL_FILE);

        // 已有相同文件则跳过
        if target.exists() {
            if let Some(ref rec) = local {
                if rec.name == MODEL_FILE && !info.sha256.is_empty() {
                    if self.base.hash_matches(&info.sha256, &target) {
                        progress("模型已是最新", 1.0);
                        return Ok(UpdateResult {
                            component: "模型".into(),
                            old_version: rec.tag.clone(),
                            new_version: info.tag.clone(),
                            success: true,
                            message: "已是最新版本".into(),
                        });
                    }
                }
            }
        }

        progress("下载模型...", 0.15);
        let dl_client = Client::new_download_client(config)?;
        dl_client.download_file(&info.url, &target, |dl, total| {
            if let Some(t) = total {
                let pct = 0.15 + (dl as f64 / t as f64) * 0.75;
                progress(&format!("下载中... {:.0}%", (dl as f64 / t as f64) * 100.0), pct);
            }
        }).await?;

        if !info.sha256.is_empty() {
            progress("校验模型...", 0.92);
            if !fileutil::hash::verify_sha256(&target, &info.sha256) {
                anyhow::bail!("SHA256 校验失败");
            }
        }

        progress("保存记录...", 0.95);
        let record = UpdateRecord {
            name: MODEL_FILE.into(),
            update_time: info.update_time.clone(),
            tag: info.tag.clone(),
            apply_time: chrono::Utc::now().to_rfc3339(),
            sha256: info.sha256.clone(),
        };
        BaseUpdater::save_record(&record_path, &record)?;

        progress("模型更新完成", 1.0);
        Ok(UpdateResult {
            component: "模型".into(),
            old_version: local.map(|r| r.tag).unwrap_or_else(|| "未安装".into()),
            new_version: info.tag,
            success: true,
            message: "更新成功".into(),
        })
    }
}

// ── 组合更新 ──

pub async fn update_all(
    schema: &Schema,
    config: &Config,
    cache_dir: PathBuf,
    rime_dir: PathBuf,
    mut progress: impl FnMut(&str, f64),
) -> Result<Vec<UpdateResult>> {
    let base = BaseUpdater::new(config, cache_dir, rime_dir.clone())?;
    let mut results = Vec::new();

    // 1. 方案
    progress("更新方案...", 0.0);
    let scheme = SchemeUpdater { base: BaseUpdater::new(config, Default::default(), rime_dir.clone())? };
    // 复用 client 避免重复创建
    match scheme.run(schema, config, |msg, pct| {
        progress(msg, pct * 0.40);
    }).await {
        Ok(r) => results.push(r),
        Err(e) => results.push(UpdateResult {
            component: "方案".into(),
            old_version: "?".into(),
            new_version: "?".into(),
            success: false,
            message: e.to_string(),
        }),
    }

    // 2. 词库 (如果有独立词库)
    if schema.dict_zip().is_some() {
        progress("更新词库...", 0.40);
        let dict = DictUpdater { base: BaseUpdater::new(config, Default::default(), rime_dir.clone())? };
        match dict.run(schema, config, |msg, pct| {
            progress(msg, 0.40 + pct * 0.30);
        }).await {
            Ok(r) => results.push(r),
            Err(e) => results.push(UpdateResult {
                component: "词库".into(),
                old_version: "?".into(),
                new_version: "?".into(),
                success: false,
                message: e.to_string(),
            }),
        }
    }

    // 3. 模型 (仅万象)
    if schema.supports_model_patch() && config.model_patch_enabled {
        progress("更新模型...", 0.70);
        let model = ModelUpdater { base: BaseUpdater::new(config, Default::default(), rime_dir.clone())? };
        match model.run(config, |msg, pct| {
            progress(msg, 0.70 + pct * 0.30);
        }).await {
            Ok(r) => results.push(r),
            Err(e) => results.push(UpdateResult {
                component: "模型".into(),
                old_version: "?".into(),
                new_version: "?".into(),
                success: false,
                message: e.to_string(),
            }),
        }
    }

    progress("更新完成", 1.0);
    Ok(results)
}
