pub mod base;
pub mod frost;
pub mod ice;
pub mod mint;
pub mod model_patch;
pub mod progress;
pub mod wanxiang;

pub use self::base::{BaseUpdater, UpdateResult};
use self::frost::FrostUpdater;
use self::ice::IceUpdater;
use self::mint::MintUpdater;
pub use self::progress::{UpdateComponent, UpdateEvent, UpdatePhase};
use self::wanxiang::WanxiangUpdater;
use crate::i18n::{L10n, Lang};
use crate::types::*;
use anyhow::Result;
use std::path::PathBuf;

fn rescale_event(event: UpdateEvent, offset: f64, span: f64) -> UpdateEvent {
    UpdateEvent {
        progress: offset + event.progress * span,
        ..event
    }
}

/// 组合更新 - 根据当前方案自动选择正确的更新器
pub async fn update_all(
    schema: &Schema,
    config: &Config,
    cache_dir: PathBuf,
    rime_dir: PathBuf,
    cancel: CancelSignal,
    mut progress: impl FnMut(UpdateEvent),
) -> Result<Vec<UpdateResult>> {
    let t = L10n::new(Lang::from_str(&config.language));
    let mut results = Vec::new();
    let mut emit = |event: UpdateEvent| progress(event);

    // Pre-update hook
    cancel.checkpoint()?;
    if !config.pre_update_hook.is_empty() {
        emit(UpdateEvent {
            component: UpdateComponent::Hook,
            phase: UpdatePhase::Running,
            progress: 0.01,
            detail: t.t("hook.pre_update").into(),
        });
        if let Err(e) = crate::deployer::run_hook(
            &config.pre_update_hook,
            "pre-update",
            Lang::from_str(&config.language),
        ) {
            results.push(UpdateResult {
                component: t.t("update.component.hook").into(),
                old_version: "-".into(),
                new_version: "-".into(),
                success: false,
                message: format!("{}: {e}", t.t("deploy.hook_failed")),
            });
            return Ok(results);
        }
    }

    // 1. 方案 + 词库更新 (按方案类型分发)
    cancel.checkpoint()?;
    emit(UpdateEvent {
        component: UpdateComponent::Scheme,
        phase: UpdatePhase::Starting,
        progress: 0.05,
        detail: t.t("menu.update_scheme").into(),
    });
    let base = match BaseUpdater::new(config, cache_dir.clone(), rime_dir.clone()) {
        Ok(b) => b,
        Err(e) => {
            results.push(BaseUpdater::fail_result(t.t("update.scheme"), &e));
            return Ok(results);
        }
    };

    if schema.is_wanxiang() {
        let wx = WanxiangUpdater { base };
        // 方案
        match wx
            .update_scheme(schema, config, Some(&cancel), |event| {
                emit(rescale_event(event, 0.05, 0.35));
            })
            .await
        {
            Ok(r) => results.push(r),
            Err(e) => results.push(BaseUpdater::fail_result(t.t("update.scheme"), &e)),
        }

        // 词库
        if schema.dict_zip().is_some() {
            cancel.checkpoint()?;
            emit(UpdateEvent {
                component: UpdateComponent::Dict,
                phase: UpdatePhase::Starting,
                progress: 0.40,
                detail: t.t("menu.update_dict").into(),
            });
            // 重新创建 updater (borrow checker)
            let base2 = match BaseUpdater::new(config, cache_dir.clone(), rime_dir.clone()) {
                Ok(b) => b,
                Err(e) => {
                    results.push(BaseUpdater::fail_result(t.t("update.dict"), &e));
                    return Ok(results);
                }
            };
            let wx2 = WanxiangUpdater { base: base2 };
            match wx2
                .update_dict(schema, config, Some(&cancel), |event| {
                    emit(rescale_event(event, 0.40, 0.30));
                })
                .await
            {
                Ok(r) => results.push(r),
                Err(e) => results.push(BaseUpdater::fail_result(t.t("update.dict"), &e)),
            }
        }
    } else if *schema == Schema::Ice {
        let ice = IceUpdater { base };
        // 方案
        match ice
            .update_scheme(config, Some(&cancel), |event| {
                emit(rescale_event(event, 0.05, 0.35));
            })
            .await
        {
            Ok(r) => results.push(r),
            Err(e) => results.push(BaseUpdater::fail_result(t.t("update.scheme"), &e)),
        }

        // 词库
        cancel.checkpoint()?;
        emit(UpdateEvent {
            component: UpdateComponent::Dict,
            phase: UpdatePhase::Starting,
            progress: 0.40,
            detail: t.t("menu.update_dict").into(),
        });
        let base2 = match BaseUpdater::new(config, cache_dir.clone(), rime_dir.clone()) {
            Ok(b) => b,
            Err(e) => {
                results.push(BaseUpdater::fail_result(t.t("update.dict"), &e));
                return Ok(results);
            }
        };
        let ice2 = IceUpdater { base: base2 };
        match ice2
            .update_dict(config, Some(&cancel), |event| {
                emit(rescale_event(event, 0.40, 0.30));
            })
            .await
        {
            Ok(r) => results.push(r),
            Err(e) => results.push(BaseUpdater::fail_result(t.t("update.dict"), &e)),
        }
    } else if *schema == Schema::Frost {
        // 白霜
        let frost = FrostUpdater { base };
        match frost
            .update_scheme(config, Some(&cancel), |event| {
                emit(rescale_event(event, 0.05, 0.65));
            })
            .await
        {
            Ok(r) => results.push(r),
            Err(e) => results.push(BaseUpdater::fail_result(t.t("update.scheme"), &e)),
        }
    } else {
        let mint = MintUpdater { base };
        match mint
            .update_scheme(config, Some(&cancel), |event| {
                emit(rescale_event(event, 0.05, 0.65));
            })
            .await
        {
            Ok(r) => results.push(r),
            Err(e) => results.push(BaseUpdater::fail_result(t.t("update.scheme"), &e)),
        }
    }

    // 2. 模型 (下载万象模型，并按当前方案决定 patch 目标)
    if schema.supports_model_patch() && config.model_patch_enabled {
        cancel.checkpoint()?;
        emit(UpdateEvent {
            component: UpdateComponent::Model,
            phase: UpdatePhase::Starting,
            progress: 0.70,
            detail: t.t("menu.update_model").into(),
        });
        let base3 = match BaseUpdater::new(config, cache_dir, rime_dir.clone()) {
            Ok(b) => b,
            Err(e) => {
                results.push(BaseUpdater::fail_result(t.t("update.model"), &e));
                return Ok(results);
            }
        };
        let wx3 = WanxiangUpdater { base: base3 };
        match wx3
            .update_model(config, Some(&cancel), |event| {
                emit(rescale_event(event, 0.70, 0.20));
            })
            .await
        {
            Ok(r) => results.push(r),
            Err(e) => results.push(BaseUpdater::fail_result(t.t("update.model"), &e)),
        }

        // 自动 patch 模型
        if model_patch::is_model_patched(&rime_dir, schema, Lang::from_str(&config.language)) {
            // 已 patch, 无需重复
        } else if let Err(e) =
            model_patch::patch_model(&rime_dir, schema, Lang::from_str(&config.language))
        {
            emit(UpdateEvent {
                component: UpdateComponent::ModelPatch,
                phase: UpdatePhase::Applying,
                progress: 0.89,
                detail: t.t("menu.model_patch").into(),
            });
            results.push(BaseUpdater::error_result(
                t.t("update.component.model_patch"),
                &e.to_string(),
            ));
        } else {
            emit(UpdateEvent {
                component: UpdateComponent::ModelPatch,
                phase: UpdatePhase::Finished,
                progress: 0.90,
                detail: t.t("patch.model.enabled").into(),
            });
        }
    }

    // 3. 部署
    cancel.checkpoint()?;
    emit(UpdateEvent {
        component: UpdateComponent::Deploy,
        phase: UpdatePhase::Deploying,
        progress: 0.92,
        detail: t.t("update.deploying").into(),
    });
    if let Err(e) = crate::deployer::deploy(Lang::from_str(&config.language)) {
        results.push(BaseUpdater::error_result(
            t.t("update.component.deploy"),
            &e.to_string(),
        ));
    } else {
        results.push(BaseUpdater::success_result(
            t.t("update.component.deploy"),
            "-",
            "-",
            t.t("deploy.complete"),
        ));
    }

    if let Some(result) = setup_rime(config, &cancel, &mut emit).await? {
        results.push(result);
    }

    // 4. 多引擎同步 (Linux/macOS/windows: 仅在检测到多个引擎时执行)
    if config.engine_sync_enabled {
        cancel.checkpoint()?;
        emit(UpdateEvent {
            component: UpdateComponent::Sync,
            phase: UpdatePhase::Syncing,
            progress: 0.96,
            detail: t.t("update.syncing").into(),
        });
        let sync_result = crate::deployer::sync_to_engines(
            &rime_dir,
            &config.exclude_files,
            config.engine_sync_use_link,
            Lang::from_str(&config.language),
        );
        if let Err(e) = sync_result {
            results.push(BaseUpdater::error_result(
                t.t("update.component.sync"),
                &e.to_string(),
            ));
        }
    }

    // Post-update hook
    if !config.post_update_hook.is_empty() {
        cancel.checkpoint()?;
        emit(UpdateEvent {
            component: UpdateComponent::Hook,
            phase: UpdatePhase::Running,
            progress: 0.98,
            detail: t.t("hook.post_update").into(),
        });
        if let Err(e) = crate::deployer::run_hook(
            &config.post_update_hook,
            "post-update",
            Lang::from_str(&config.language),
        ) {
            results.push(BaseUpdater::error_result(
                t.t("update.component.hook"),
                &format!("{}: {e}", t.t("deploy.hook_failed")),
            ));
        }
    }

    emit(UpdateEvent {
        component: UpdateComponent::Deploy,
        phase: UpdatePhase::Finished,
        progress: 1.0,
        detail: t.t("update.complete").into(),
    });
    Ok(results)
}

/// Only an explicit opt-in may access the session bus or change input method groups.
pub async fn setup_rime(
    config: &Config,
    cancel: &CancelSignal,
    mut progress: impl FnMut(UpdateEvent),
) -> Result<Option<UpdateResult>> {
    if config.rime_auto_setup != Some(true) {
        return Ok(None);
    }
    #[cfg(target_os = "linux")]
    {
        cancel.checkpoint()?;
        let t = L10n::new(Lang::from_str(&config.language));
        progress(UpdateEvent {
            component: UpdateComponent::Deploy,
            phase: UpdatePhase::Applying,
            progress: 0.94,
            detail: t.t("fcitx5.setup.configuring").into(),
        });
        let component = t.t("fcitx5.setup.component");
        let result = match crate::deployer::fcitx5::ensure_rime(t.lang()).await {
            Ok(added) => {
                let message = t.t(if added {
                    "fcitx5.setup.added"
                } else {
                    "fcitx5.setup.already_added"
                });
                crate::feedback::info(message);
                BaseUpdater::success_result(component, "-", "-", message)
            }
            Err(error) => {
                let message = format!("{}: {error:#}", t.t("fcitx5.setup.failed"));
                crate::feedback::warn(&message);
                BaseUpdater::error_result(component, &message)
            }
        };
        Ok(Some(result))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (cancel, &mut progress);
        Ok(None)
    }
}

pub fn should_prompt_rime_setup(config: &Config) -> bool {
    config.rime_auto_setup.is_none()
        && cfg!(target_os = "linux")
        && crate::deployer::detect_engines()
            .iter()
            .any(|engine| engine == "fcitx5")
}

#[cfg(test)]
mod rime_setup_tests {
    use super::*;

    #[tokio::test]
    async fn disabled_or_undecided_setup_skips_all_work() {
        for choice in [None, Some(false)] {
            let config = Config {
                rime_auto_setup: choice,
                ..Config::default()
            };
            let cancel = CancelSignal::new();
            cancel.cancel();
            let started = std::time::Instant::now();
            let result = setup_rime(&config, &cancel, |_| {
                panic!("disabled setup emitted progress")
            })
            .await
            .unwrap();
            eprintln!("Disabled Rime setup ({choice:?}): {:?}", started.elapsed());
            assert!(result.is_none());
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn enabled_setup_respects_cancellation_before_accessing_bus() {
        let config = Config {
            rime_auto_setup: Some(true),
            ..Config::default()
        };
        let cancel = CancelSignal::new();
        cancel.cancel();
        assert!(setup_rime(&config, &cancel, |_| panic!(
            "cancelled setup emitted progress"
        ))
        .await
        .unwrap_err()
        .is::<UpdateCancelled>());
    }

    #[test]
    fn saved_choice_never_prompts_again() {
        for choice in [Some(false), Some(true)] {
            let config = Config {
                rime_auto_setup: choice,
                ..Config::default()
            };
            assert!(!should_prompt_rime_setup(&config));
        }
    }
}
