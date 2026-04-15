pub mod base;
pub mod frost;
pub mod ice;
pub mod mint;
pub mod model_patch;
pub mod wanxiang;

pub use self::base::{BaseUpdater, UpdateResult};
use self::frost::FrostUpdater;
use self::ice::IceUpdater;
use self::mint::MintUpdater;
use self::wanxiang::WanxiangUpdater;
use crate::types::*;
use anyhow::Result;
use std::path::PathBuf;

/// 组合更新 - 根据当前方案自动选择正确的更新器
pub async fn update_all(
    schema: &Schema,
    config: &Config,
    cache_dir: PathBuf,
    rime_dir: PathBuf,
    mut progress: impl FnMut(&str, f64),
) -> Result<Vec<UpdateResult>> {
    let mut results = Vec::new();

    // Pre-update hook
    if !config.pre_update_hook.is_empty() {
        progress("执行 pre-update hook...", 0.01);
        if let Err(e) = crate::deployer::run_hook(&config.pre_update_hook, "pre-update") {
            results.push(UpdateResult {
                component: "hook".into(),
                old_version: "-".into(),
                new_version: "-".into(),
                success: false,
                message: format!("pre-update hook 失败: {e}"),
            });
            return Ok(results);
        }
    }

    // 1. 方案 + 词库更新 (按方案类型分发)
    progress("更新方案...", 0.05);
    let base = match BaseUpdater::new(config, cache_dir.clone(), rime_dir.clone()) {
        Ok(b) => b,
        Err(e) => {
            results.push(BaseUpdater::fail_result("方案", &e));
            return Ok(results);
        }
    };

    if schema.is_wanxiang() {
        let wx = WanxiangUpdater { base };
        // 方案
        match wx
            .update_scheme(schema, config, |msg, pct| {
                progress(msg, 0.05 + pct * 0.35);
            })
            .await
        {
            Ok(r) => results.push(r),
            Err(e) => results.push(BaseUpdater::fail_result("方案", &e)),
        }

        // 词库
        if schema.dict_zip().is_some() {
            progress("更新词库...", 0.40);
            // 重新创建 updater (borrow checker)
            let base2 = match BaseUpdater::new(config, cache_dir.clone(), rime_dir.clone()) {
                Ok(b) => b,
                Err(e) => {
                    results.push(BaseUpdater::fail_result("词库", &e));
                    return Ok(results);
                }
            };
            let wx2 = WanxiangUpdater { base: base2 };
            match wx2
                .update_dict(schema, config, |msg, pct| {
                    progress(msg, 0.40 + pct * 0.30);
                })
                .await
            {
                Ok(r) => results.push(r),
                Err(e) => results.push(BaseUpdater::fail_result("词库", &e)),
            }
        }
    } else if *schema == Schema::Ice {
        let ice = IceUpdater { base };
        // 方案
        match ice
            .update_scheme(config, |msg, pct| {
                progress(msg, 0.05 + pct * 0.35);
            })
            .await
        {
            Ok(r) => results.push(r),
            Err(e) => results.push(BaseUpdater::fail_result("方案", &e)),
        }

        // 词库
        progress("更新词库...", 0.40);
        let base2 = match BaseUpdater::new(config, cache_dir.clone(), rime_dir.clone()) {
            Ok(b) => b,
            Err(e) => {
                results.push(BaseUpdater::fail_result("词库", &e));
                return Ok(results);
            }
        };
        let ice2 = IceUpdater { base: base2 };
        match ice2
            .update_dict(config, |msg, pct| {
                progress(msg, 0.40 + pct * 0.30);
            })
            .await
        {
            Ok(r) => results.push(r),
            Err(e) => results.push(BaseUpdater::fail_result("词库", &e)),
        }
    } else if *schema == Schema::Frost {
        // 白霜
        let frost = FrostUpdater { base };
        match frost
            .update_scheme(config, |msg, pct| {
                progress(msg, 0.05 + pct * 0.65);
            })
            .await
        {
            Ok(r) => results.push(r),
            Err(e) => results.push(BaseUpdater::fail_result("方案", &e)),
        }
    } else {
        let mint = MintUpdater { base };
        match mint
            .update_scheme(config, |msg, pct| {
                progress(msg, 0.05 + pct * 0.65);
            })
            .await
        {
            Ok(r) => results.push(r),
            Err(e) => results.push(BaseUpdater::fail_result("方案", &e)),
        }
    }

    // 2. 模型 (仅万象，且启用)
    if schema.supports_model_patch() && config.model_patch_enabled {
        progress("更新模型...", 0.70);
        let base3 = match BaseUpdater::new(config, cache_dir, rime_dir.clone()) {
            Ok(b) => b,
            Err(e) => {
                results.push(BaseUpdater::fail_result("模型", &e));
                return Ok(results);
            }
        };
        let wx3 = WanxiangUpdater { base: base3 };
        match wx3
            .update_model(config, |msg, pct| {
                progress(msg, 0.70 + pct * 0.20);
            })
            .await
        {
            Ok(r) => results.push(r),
            Err(e) => results.push(BaseUpdater::fail_result("模型", &e)),
        }

        // 自动 patch 模型
        if model_patch::is_model_patched(&rime_dir, schema) {
            // 已 patch, 无需重复
        } else if let Err(e) = model_patch::patch_model(&rime_dir, schema) {
            results.push(BaseUpdater::error_result("模型patch", &e.to_string()));
        }
    }

    // 3. 部署
    progress("部署...", 0.92);
    if let Err(e) = crate::deployer::deploy() {
        results.push(BaseUpdater::error_result("部署", &e.to_string()));
    } else {
        results.push(BaseUpdater::success_result("部署", "-", "-", "Rime 已重载"));
    }

    // 4. Fcitx 兼容同步 (Linux)
    if config.fcitx_compat {
        progress("同步 Fcitx 目录...", 0.96);
        if let Err(e) = crate::deployer::sync_to_fcitx(&rime_dir, config.fcitx_use_link) {
            results.push(BaseUpdater::error_result("fcitx同步", &e.to_string()));
        }
    }

    // Post-update hook
    if !config.post_update_hook.is_empty() {
        progress("执行 post-update hook...", 0.98);
        if let Err(e) = crate::deployer::run_hook(&config.post_update_hook, "post-update") {
            results.push(BaseUpdater::error_result(
                "hook",
                &format!("post-update hook 失败: {e}"),
            ));
        }
    }

    progress("更新完成", 1.0);
    Ok(results)
}
