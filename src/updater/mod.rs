pub mod base;
pub mod frost;
pub mod ice;
pub mod mint;
pub mod model_patch;
pub mod progress;
pub mod repository;
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

#[derive(Debug)]
struct CompletedResults {
    results: Vec<UpdateResult>,
    message: String,
}

impl std::fmt::Display for CompletedResults {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

fn with_completed_results(error: anyhow::Error, results: Vec<UpdateResult>) -> anyhow::Error {
    let message = format!("{error:#}");
    error.context(CompletedResults { results, message })
}

/// Results committed before a multi-component operation stopped.
pub fn completed_results(error: &anyhow::Error) -> Vec<UpdateResult> {
    error
        .downcast_ref::<CompletedResults>()
        .map(|context| context.results.clone())
        .unwrap_or_default()
}

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
    progress: impl FnMut(UpdateEvent),
) -> Result<Vec<UpdateResult>> {
    let stage_cache_dir = cache_dir.clone();
    let stage_rime_dir = rime_dir.clone();
    let stage_cancel = cancel.clone();
    update_all_with_scheme_updates(
        schema,
        config,
        cache_dir,
        rime_dir,
        cancel,
        progress,
        async |results, emit| {
            let t = L10n::new(Lang::from_str(&config.language));
            let cache_dir = stage_cache_dir;
            let rime_dir = stage_rime_dir;
            let cancel = stage_cancel;
            // 1. 方案 + 词库更新 (按方案类型分发)
            emit(UpdateEvent {
                component: UpdateComponent::Scheme,
                phase: UpdatePhase::Starting,
                progress: 0.05,
                detail: t.t("menu.update_scheme").into(),
            });
            let base = match BaseUpdater::new(config, cache_dir.clone(), rime_dir.clone()) {
                Ok(b) => b,
                Err(e) => {
                    results.push(BaseUpdater::fail_result(
                        UpdateComponent::Scheme,
                        t.t("update.scheme"),
                        &e,
                    ));
                    return Ok(false);
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
                    Err(e) => results.push(BaseUpdater::fail_result(
                        UpdateComponent::Scheme,
                        t.t("update.scheme"),
                        &e,
                    )),
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
                    match wx
                        .update_dict(schema, config, Some(&cancel), |event| {
                            emit(rescale_event(event, 0.40, 0.30));
                        })
                        .await
                    {
                        Ok(r) => results.push(r),
                        Err(e) => results.push(BaseUpdater::fail_result(
                            UpdateComponent::Dict,
                            t.t("update.dict"),
                            &e,
                        )),
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
                    Err(e) => results.push(BaseUpdater::fail_result(
                        UpdateComponent::Scheme,
                        t.t("update.scheme"),
                        &e,
                    )),
                }

                // 词库
                cancel.checkpoint()?;
                emit(UpdateEvent {
                    component: UpdateComponent::Dict,
                    phase: UpdatePhase::Starting,
                    progress: 0.40,
                    detail: t.t("menu.update_dict").into(),
                });
                match ice
                    .update_dict(config, Some(&cancel), |event| {
                        emit(rescale_event(event, 0.40, 0.30));
                    })
                    .await
                {
                    Ok(r) => results.push(r),
                    Err(e) => results.push(BaseUpdater::fail_result(
                        UpdateComponent::Dict,
                        t.t("update.dict"),
                        &e,
                    )),
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
                    Err(e) => results.push(BaseUpdater::fail_result(
                        UpdateComponent::Scheme,
                        t.t("update.scheme"),
                        &e,
                    )),
                }
            } else if matches!(schema, Schema::Wubi86Jidian | Schema::Abcm) {
                match (repository::RepositoryUpdater { base })
                    .update_scheme(schema, config, Some(&cancel), |event| {
                        emit(rescale_event(event, 0.05, 0.65))
                    })
                    .await
                {
                    Ok(result) => results.push(result),
                    Err(error) => results.push(BaseUpdater::fail_result(
                        UpdateComponent::Scheme,
                        t.t("update.scheme"),
                        &error,
                    )),
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
                    Err(e) => results.push(BaseUpdater::fail_result(
                        UpdateComponent::Scheme,
                        t.t("update.scheme"),
                        &e,
                    )),
                }
            }

            Ok(true)
        },
    )
    .await
}

// Keep collection and all later stage boundaries shared with deterministic stage tests.
async fn update_all_with_scheme_updates<P: FnMut(UpdateEvent)>(
    schema: &Schema,
    config: &Config,
    cache_dir: PathBuf,
    rime_dir: PathBuf,
    cancel: CancelSignal,
    mut progress: P,
    scheme_updates: impl AsyncFnOnce(&mut Vec<UpdateResult>, &mut P) -> Result<bool>,
) -> Result<Vec<UpdateResult>> {
    let t = L10n::new(Lang::from_str(&config.language));
    let mut results = Vec::new();
    let outcome: Result<()> = async {
        let mut emit = &mut progress;

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
                    kind: UpdateComponent::Hook,
                    component: t.t("update.component.hook").into(),
                    old_version: "-".into(),
                    new_version: "-".into(),
                    success: false,
                    message: format!("{}: {e}", t.t("deploy.hook_failed")),
                });
                return Ok(());
            }
        }

        cancel.checkpoint()?;
        if !scheme_updates(&mut results, emit).await? {
            return Ok(());
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
                    results.push(BaseUpdater::fail_result(
                        UpdateComponent::Model,
                        t.t("update.model"),
                        &e,
                    ));
                    return Ok(());
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
                Err(e) => results.push(BaseUpdater::fail_result(
                    UpdateComponent::Model,
                    t.t("update.model"),
                    &e,
                )),
            }

            cancel.checkpoint()?;
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
                    UpdateComponent::ModelPatch,
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
                UpdateComponent::Deploy,
                t.t("update.component.deploy"),
                &e.to_string(),
            ));
        } else {
            results.push(BaseUpdater::success_result(
                UpdateComponent::Deploy,
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
                    UpdateComponent::Sync,
                    t.t("update.component.sync"),
                    &e.to_string(),
                ));
            } else {
                results.push(BaseUpdater::success_result(
                    UpdateComponent::Sync,
                    t.t("update.component.sync"),
                    "-",
                    "-",
                    t.t("update.complete"),
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
                    UpdateComponent::Hook,
                    t.t("update.component.hook"),
                    &format!("{}: {e}", t.t("deploy.hook_failed")),
                ));
            }
        }

        cancel.checkpoint()?;
        emit(UpdateEvent {
            component: UpdateComponent::Deploy,
            phase: UpdatePhase::Finished,
            progress: 1.0,
            detail: t.t("update.complete").into(),
        });
        Ok(())
    }
    .await;
    match outcome {
        Ok(()) => Ok(results),
        Err(error) => Err(with_completed_results(error, results)),
    }
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
            component: UpdateComponent::Fcitx5Setup,
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
                BaseUpdater::success_result(
                    UpdateComponent::Fcitx5Setup,
                    component,
                    "-",
                    "-",
                    message,
                )
            }
            Err(error) => {
                let message = format!("{}: {error:#}", t.t("fcitx5.setup.failed"));
                crate::feedback::warn(&message);
                BaseUpdater::error_result(UpdateComponent::Fcitx5Setup, component, &message)
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

// A fake first stage, with the real result collector and downstream dispatch.
#[cfg(test)]
pub(crate) async fn cancel_after_completed_scheme() -> (anyhow::Error, Vec<UpdateEvent>) {
    let root = tempfile::tempdir().expect("temporary update root");
    let config = Config {
        model_patch_enabled: true,
        engine_sync_enabled: true,
        rime_auto_setup: Some(true),
        post_update_hook: "exit 91".into(),
        ..Config::default()
    };
    let cancel = CancelSignal::new();
    let stage_cancel = cancel.clone();
    let mut events = Vec::new();
    let error = update_all_with_scheme_updates(
        &Schema::Ice,
        &config,
        root.path().join("cache"),
        root.path().join("rime"),
        cancel,
        |event| events.push(event),
        async |results, emit| {
            results.push(BaseUpdater::success_result(
                UpdateComponent::Scheme,
                "scheme",
                "v1",
                "v2",
                "installed",
            ));
            emit(UpdateEvent {
                component: UpdateComponent::Scheme,
                phase: UpdatePhase::Finished,
                progress: 0.7,
                detail: "installed".into(),
            });
            stage_cancel.cancel();
            Ok(true)
        },
    )
    .await
    .expect_err("cancellation after a completed stage must stop the update");
    (error, events)
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_after_completed_stage_retains_results_and_stops_dispatch() {
        let (error, events) = cancel_after_completed_scheme().await;
        assert!(error.is::<UpdateCancelled>());
        assert!(error.to_string().contains("update cancelled"));
        assert!(error.root_cause().is::<UpdateCancelled>());
        let results = completed_results(&error.context("install interrupted"));
        assert_eq!(results.len(), 1);
        let result = &results[0];
        assert_eq!(result.kind, UpdateComponent::Scheme);
        assert!(result.success);
        assert_eq!(result.old_version, "v1");
        assert_eq!(result.new_version, "v2");
        assert_eq!(result.message, "installed");
        assert_eq!(events.len(), 1, "later stages must not emit progress");
        assert_eq!(events[0].component, UpdateComponent::Scheme);
        assert_eq!(events[0].phase, UpdatePhase::Finished);
    }

    #[tokio::test]
    async fn precancelled_update_never_dispatches_scheme_stage() {
        let root = tempfile::tempdir().expect("temporary update root");
        let cancel = CancelSignal::new();
        cancel.cancel();
        let error = update_all_with_scheme_updates(
            &Schema::Ice,
            &Config::default(),
            root.path().join("cache"),
            root.path().join("rime"),
            cancel,
            |_| panic!("cancelled update emitted progress"),
            async |_, _| panic!("cancelled update started scheme stage"),
        )
        .await
        .expect_err("pre-cancelled update must stop");
        assert!(error.is::<UpdateCancelled>());
        assert!(completed_results(&error).is_empty());
    }
}
