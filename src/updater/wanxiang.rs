use super::base::{BaseUpdater, UpdateResult};
use crate::types::*;
use anyhow::{Context, Result};

/// 万象方案更新器
pub struct WanxiangUpdater {
    pub base: BaseUpdater,
}

impl WanxiangUpdater {
    /// 检查方案更新
    pub async fn check_scheme_update(&self, schema: &Schema) -> Result<UpdateInfo> {
        let releases = if self.base.client.use_mirror() {
            let release = self
                .base
                .client
                .fetch_cnb_release(
                    WX_OWNER,
                    WX_CNB_REPO,
                    &format!("latest/{}", schema.scheme_zip()),
                )
                .await?;
            vec![release]
        } else {
            self.base
                .client
                .fetch_github_releases(schema.owner(), schema.repo(), "")
                .await?
        };

        BaseUpdater::find_update_info(&releases, schema.scheme_zip(), None)
            .context(format!("未找到方案文件: {}", schema.scheme_zip()))
    }

    /// 更新方案
    pub async fn update_scheme(
        &self,
        schema: &Schema,
        config: &Config,
        mut progress: impl FnMut(&str, f64),
    ) -> Result<UpdateResult> {
        progress("检查万象方案更新...", 0.05);

        let info = self.check_scheme_update(schema).await?;
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
            progress("方案已是最新", 1.0);
            return Ok(BaseUpdater::success_result(
                "方案",
                &info.tag,
                &info.tag,
                "已是最新版本",
            ));
        }

        if key_file_missing {
            progress("关键文件缺失，强制更新...", 0.05);
        } else if scheme_switched {
            progress("检测到方案切换，重新下载...", 0.05);
        }

        // 下载
        self.base
            .download_and_extract(&info, config, &self.base.rime_dir, &mut progress)
            .await?;

        // 处理 CNB 嵌套目录
        if self.base.client.use_mirror() {
            if let Err(e) =
                crate::fileutil::extract::handle_nested_dir(&self.base.rime_dir, &info.name)
            {
                eprintln!("⚠️ 嵌套目录处理失败: {e}");
            }
        }

        // 保存记录
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

    /// 检查词库更新
    pub async fn check_dict_update(&self, schema: &Schema) -> Result<UpdateInfo> {
        let dict_zip = schema.dict_zip().context("此方案无独立词库")?;

        if self.base.client.use_mirror() {
            let tag = if dict_zip.starts_with("base") {
                WX_CNB_DICT_TAG
            } else {
                "latest"
            };
            let release = self
                .base
                .client
                .fetch_cnb_release(WX_OWNER, WX_CNB_REPO, tag)
                .await?;
            BaseUpdater::find_update_info(&[release], dict_zip, None)
        } else {
            let releases = self
                .base
                .client
                .fetch_github_releases(schema.owner(), schema.repo(), schema.dict_tag())
                .await?;
            BaseUpdater::find_update_info(&releases, dict_zip, None)
        }
        .context(format!("未找到词库: {dict_zip}"))
    }

    /// 更新词库
    pub async fn update_dict(
        &self,
        schema: &Schema,
        config: &Config,
        mut progress: impl FnMut(&str, f64),
    ) -> Result<UpdateResult> {
        progress("检查万象词库更新...", 0.05);

        let info = self.check_dict_update(schema).await?;
        let record_path = self.base.cache_dir.join("dict_record.json");
        let local = BaseUpdater::load_record(&record_path);

        if !BaseUpdater::needs_update(local.as_ref(), &info) {
            progress("词库已是最新", 1.0);
            return Ok(BaseUpdater::success_result(
                "词库",
                &info.tag,
                &info.tag,
                "已是最新版本",
            ));
        }

        // 下载
        let dict_dir = self.base.rime_dir.join("dicts");
        self.base
            .download_and_extract(&info, config, &dict_dir, &mut progress)
            .await?;

        if self.base.client.use_mirror() {
            if let Err(e) = crate::fileutil::extract::handle_nested_dir(&dict_dir, &info.name) {
                eprintln!("⚠️ 嵌套目录处理失败: {e}");
            }
        }

        // 保存记录
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

    /// 检查模型更新
    pub async fn check_model_update(&self) -> Result<UpdateInfo> {
        let releases = self
            .base
            .client
            .fetch_github_releases(WX_OWNER, MODEL_REPO, MODEL_TAG)
            .await?;

        BaseUpdater::find_update_info(&releases, MODEL_FILE, None)
            .context(format!("未找到模型: {MODEL_FILE}"))
    }

    /// 更新模型
    pub async fn update_model(
        &self,
        config: &Config,
        mut progress: impl FnMut(&str, f64),
    ) -> Result<UpdateResult> {
        progress("检查万象模型更新...", 0.05);

        let info = self.check_model_update().await?;
        let record_path = self.base.cache_dir.join("model_record.json");
        let local = BaseUpdater::load_record(&record_path);

        let target = self.base.rime_dir.join(MODEL_FILE);

        // 已有相同文件则跳过
        if target.exists() {
            if let Some(ref rec) = local {
                if rec.name == MODEL_FILE
                    && !info.sha256.is_empty()
                    && self.base.hash_matches(&info.sha256, &target)
                {
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

        // 下载
        progress("下载模型...", 0.15);
        let dl_client = crate::api::Client::new_download_client(config)?;
        dl_client
            .download_file(&info.url, &target, |dl, total| {
                if let Some(t) = total {
                    let pct = 0.15 + (dl as f64 / t as f64) * 0.75;
                    progress(
                        &format!("下载中... {:.0}%", (dl as f64 / t as f64) * 100.0),
                        pct,
                    );
                }
            })
            .await?;

        if !info.sha256.is_empty() {
            progress("校验模型...", 0.92);
            if !crate::fileutil::hash::verify_sha256(&target, &info.sha256) {
                anyhow::bail!("SHA256 校验失败");
            }
        }

        // 保存记录
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
