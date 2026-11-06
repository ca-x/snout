//! Frontend-independent operations; rendering, dialogs, and event loops live in each adapter.

use crate::config::Manager;
use crate::i18n::{L10n, Lang};
use crate::types::{CancelSignal, Config, Schema};
use crate::updater::{self, BaseUpdater, UpdateComponent, UpdateEvent, UpdatePhase, UpdateResult};
use anyhow::Result;

pub mod status;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppearanceTarget {
    ThemePatch(std::path::PathBuf),
    Fcitx5Theme,
}

pub fn theme_patch_target(
    rime_dir: &std::path::Path,
    engines: &[String],
) -> Option<std::path::PathBuf> {
    if cfg!(target_os = "windows") && engines.iter().any(|engine| engine == "weasel") {
        Some(rime_dir.join("weasel.custom.yaml"))
    } else if cfg!(target_os = "macos") && engines.iter().any(|engine| engine == "squirrel") {
        Some(rime_dir.join("squirrel.custom.yaml"))
    } else {
        None
    }
}

pub fn appearance_target(
    rime_dir: &std::path::Path,
    engines: &[String],
) -> Option<AppearanceTarget> {
    if cfg!(target_os = "linux")
        && crate::skin::fcitx5::builtin_themes_available()
        && crate::skin::fcitx5::theme_supported(engines)
    {
        Some(AppearanceTarget::Fcitx5Theme)
    } else {
        theme_patch_target(rime_dir, engines).map(AppearanceTarget::ThemePatch)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateAction {
    All,
    Scheme,
    Dict,
    Model,
    RimeSetup,
}

impl UpdateAction {
    pub fn command(self) -> &'static str {
        match self {
            Self::All => "update",
            Self::Scheme => "scheme",
            Self::Dict => "dict",
            Self::Model => "model",
            Self::RimeSetup => "rime-setup",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchMode {
    Enabled,
    Disabled,
}

pub fn validate_settings(config: &Config) -> Result<()> {
    if !(1..=8).contains(&config.download_threads) {
        anyhow::bail!("download threads must be between 1 and 8");
    }
    if !(1..=60).contains(&config.auto_update_countdown) {
        anyhow::bail!("auto-update countdown must be between 1 and 60 seconds");
    }
    for (name, value, choices) in [
        ("language", config.language.as_str(), &["zh", "en"][..]),
        (
            "TUI theme",
            config.tui_theme_mode.as_str(),
            &["auto", "light", "dark"][..],
        ),
        (
            "user data policy",
            config.user_data_policy.as_str(),
            &["prompt", "preserve", "discard"][..],
        ),
        (
            "proxy type",
            config.proxy_type.as_str(),
            &["socks5", "http"][..],
        ),
    ] {
        if !choices.contains(&value) {
            anyhow::bail!("invalid {name}: {value}");
        }
    }
    if config.proxy_enabled {
        if config.proxy_address.trim().is_empty() {
            anyhow::bail!("proxy address cannot be empty");
        }
        reqwest::Proxy::all(format!("{}://{}", config.proxy_type, config.proxy_address))
            .map_err(|_| anyhow::anyhow!("invalid proxy address"))?;
    }
    for pattern in &config.exclude_files {
        if crate::config::parse_exclude_pattern(pattern)?.is_none() {
            anyhow::bail!("exclude pattern cannot be empty");
        }
    }
    Ok(())
}

pub fn save_settings(manager: &mut Manager, config: Config) -> Result<()> {
    validate_settings(&config)?;
    let synchronize_patch = config.model_patch_enabled != manager.config.model_patch_enabled
        || (config.schema != manager.config.schema && config.model_patch_enabled);
    let mode = if config.model_patch_enabled {
        PatchMode::Enabled
    } else {
        PatchMode::Disabled
    };
    let previous = std::mem::replace(&mut manager.config, config);
    let result = if synchronize_patch {
        set_model_patch(manager, mode)
    } else {
        manager.save()
    };
    if result.is_err() {
        manager.config = previous;
    }
    result
}

pub async fn apply_fcitx5_themes(
    light: &str,
    dark: &str,
    light_rounded: Option<bool>,
    dark_rounded: Option<bool>,
    lang: Lang,
) -> UpdateResult {
    let translator = L10n::new(lang);
    match crate::skin::fcitx5::apply_theme_pair(light, dark, light_rounded, dark_rounded, lang)
        .await
    {
        Ok(()) => BaseUpdater::success_result(
            UpdateComponent::Fcitx5Theme,
            translator.t("menu.fcitx5_theme"),
            "-",
            "-",
            translator.t("skin.applied"),
        ),
        Err(error) => BaseUpdater::error_result(
            UpdateComponent::Fcitx5Theme,
            translator.t("menu.fcitx5_theme"),
            &error.to_string(),
        ),
    }
}

/// Runs an explicit update, including platform deployment and configured hooks where applicable.
/// Custom manager paths are data roots, not an isolation boundary for platform side effects.
pub async fn run_update(
    action: UpdateAction,
    manager: &Manager,
    cancel: CancelSignal,
    mut progress: impl FnMut(UpdateEvent),
) -> Result<Vec<UpdateResult>> {
    cancel.checkpoint()?;
    let config = &manager.config;
    let schema = config.schema;
    let lang = Lang::from_str(&config.language);
    let translator = L10n::new(lang);

    match action {
        UpdateAction::All => {
            return updater::update_all(
                &schema,
                config,
                manager.cache_dir.clone(),
                manager.rime_dir.clone(),
                cancel,
                progress,
            )
            .await;
        }
        UpdateAction::RimeSetup => {
            let mut explicit_setup = config.clone();
            explicit_setup.rime_auto_setup = Some(true);
            return Ok(updater::setup_rime(&explicit_setup, &cancel, progress)
                .await?
                .into_iter()
                .collect());
        }
        UpdateAction::Dict if schema.dict_zip().is_none() => {
            return Ok(vec![BaseUpdater::error_result(
                UpdateComponent::Dict,
                translator.t("update.dict"),
                translator.t("update.no_dict"),
            )]);
        }
        UpdateAction::Model if !schema.supports_model_patch() => {
            return Ok(vec![BaseUpdater::error_result(
                UpdateComponent::Model,
                translator.t("update.model"),
                translator.t("update.model_not_supported"),
            )]);
        }
        _ => {}
    }

    let base = BaseUpdater::new(config, manager.cache_dir.clone(), manager.rime_dir.clone())?;
    let result = match action {
        UpdateAction::Scheme => {
            if schema.is_wanxiang() {
                updater::wanxiang::WanxiangUpdater { base }
                    .update_scheme(&schema, config, Some(&cancel), progress)
                    .await?
            } else if schema == Schema::Ice {
                updater::ice::IceUpdater { base }
                    .update_scheme(config, Some(&cancel), progress)
                    .await?
            } else if schema == Schema::Frost {
                updater::frost::FrostUpdater { base }
                    .update_scheme(config, Some(&cancel), progress)
                    .await?
            } else {
                updater::mint::MintUpdater { base }
                    .update_scheme(config, Some(&cancel), progress)
                    .await?
            }
        }
        UpdateAction::Dict => {
            if schema.is_wanxiang() {
                updater::wanxiang::WanxiangUpdater { base }
                    .update_dict(&schema, config, Some(&cancel), progress)
                    .await?
            } else {
                updater::ice::IceUpdater { base }
                    .update_dict(config, Some(&cancel), progress)
                    .await?
            }
        }
        UpdateAction::Model => {
            let result = updater::wanxiang::WanxiangUpdater { base }
                .update_model(config, Some(&cancel), &mut progress)
                .await?;
            let mut results = vec![result];
            if config.model_patch_enabled && schema.supports_model_patch() {
                progress(UpdateEvent {
                    component: UpdateComponent::ModelPatch,
                    phase: UpdatePhase::Applying,
                    progress: 0.96,
                    detail: translator.t("menu.model_patch").into(),
                });
                cancel.checkpoint()?;
                if let Err(error) =
                    updater::model_patch::patch_model(&manager.rime_dir, &schema, lang)
                {
                    results.push(BaseUpdater::error_result(
                        UpdateComponent::ModelPatch,
                        translator.t("update.component.model_patch"),
                        &error.to_string(),
                    ));
                } else {
                    results.push(BaseUpdater::success_result(
                        UpdateComponent::ModelPatch,
                        translator.t("update.component.model_patch"),
                        "-",
                        translator.t("patch.model.enabled"),
                        translator.t("patch.model.enabled"),
                    ));
                    progress(UpdateEvent {
                        component: UpdateComponent::ModelPatch,
                        phase: UpdatePhase::Finished,
                        progress: 1.0,
                        detail: translator.t("patch.model.enabled").into(),
                    });
                }
            }
            return Ok(results);
        }
        UpdateAction::All | UpdateAction::RimeSetup => {
            unreachable!("handled before updater creation")
        }
    };
    Ok(vec![result])
}

pub fn set_model_patch(manager: &mut Manager, mode: PatchMode) -> Result<()> {
    let schema = manager.config.schema;
    let lang = Lang::from_str(&manager.config.language);
    let enabled = mode == PatchMode::Enabled;
    let was_enabled = manager.config.model_patch_enabled;
    let was_applied = updater::model_patch::is_model_patched(&manager.rime_dir, &schema, lang);
    let patch_path = updater::model_patch::patch_file_path(&manager.rime_dir, &schema);
    let original_patch = match std::fs::read(&patch_path) {
        Ok(contents) => Some(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let patch_changed = enabled != was_applied;

    if patch_changed {
        if enabled {
            updater::model_patch::patch_model(&manager.rime_dir, &schema, lang)?;
        } else {
            updater::model_patch::unpatch_model(&manager.rime_dir, &schema, lang)?;
        }
    }

    manager.config.model_patch_enabled = enabled;
    if let Err(save_error) = manager.save() {
        manager.config.model_patch_enabled = was_enabled;
        let rollback = if !patch_changed {
            Ok(())
        } else if let Some(contents) = original_patch {
            crate::fileutil::atomic_write(&patch_path, &contents)
        } else {
            std::fs::remove_file(&patch_path).map_err(anyhow::Error::from)
        };
        return match rollback {
            Ok(()) => Err(save_error),
            Err(rollback_error) => Err(anyhow::anyhow!(
                "{save_error}; model patch rollback failed: {rollback_error}"
            )),
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager(root: &std::path::Path) -> Manager {
        let manager = Manager::with_paths(
            root.join("config.json"),
            root.join("rime"),
            root.join("cache"),
        )
        .expect("isolated manager");
        manager.save().expect("initial configuration");
        manager
    }

    #[tokio::test]
    async fn cancelled_operation_does_not_start_or_modify_files() {
        let root = tempfile::tempdir().expect("temporary root");
        let manager = manager(root.path());
        let original = std::fs::read(&manager.config_path).expect("configuration");
        let cancel = CancelSignal::new();
        cancel.cancel();
        let error = run_update(UpdateAction::All, &manager, cancel, |_| {
            panic!("unexpected progress")
        })
        .await
        .expect_err("cancelled");
        assert!(error.is::<crate::types::UpdateCancelled>());
        assert_eq!(
            std::fs::read(&manager.config_path).expect("configuration"),
            original
        );
        assert!(!manager.rime_dir.exists());
    }

    #[tokio::test]
    async fn unsupported_dictionary_returns_a_structured_failure_without_network() {
        let root = tempfile::tempdir().expect("temporary root");
        let mut manager = manager(root.path());
        manager.config.schema = Schema::Mint;
        let results = run_update(UpdateAction::Dict, &manager, CancelSignal::new(), |_| {
            panic!("unexpected progress")
        })
        .await
        .expect("structured result");
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert_eq!(results[0].kind, UpdateComponent::Dict);
        assert!(!manager.rime_dir.exists());
    }

    #[test]
    fn invalid_settings_preserve_memory_and_disk() {
        let root = tempfile::tempdir().expect("temporary root");
        let mut manager = manager(root.path());
        let original = std::fs::read(&manager.config_path).expect("configuration");
        let mut proposed = manager.config.clone();
        proposed.download_threads = 0;
        assert!(save_settings(&mut manager, proposed).is_err());
        assert_eq!(manager.config.download_threads, 4);
        assert_eq!(
            std::fs::read(&manager.config_path).expect("configuration"),
            original
        );
    }

    #[test]
    fn invalid_exclusions_are_rejected_by_the_shared_settings_boundary() {
        let mut config = Config::default();
        config.exclude_files.push("[".into());
        assert!(validate_settings(&config).is_err());
    }

    #[test]
    fn settings_save_failure_restores_config_and_model_patch() {
        let root = tempfile::tempdir().expect("temporary root");
        let mut manager = manager(root.path());
        let mut proposed = manager.config.clone();
        proposed.model_patch_enabled = true;
        proposed.language = "en".into();
        manager.config_path = root.path().to_path_buf();
        assert!(save_settings(&mut manager, proposed).is_err());
        assert!(!manager.config.model_patch_enabled);
        assert_eq!(manager.config.language, "zh");
        assert!(!updater::model_patch::is_model_patched(
            &manager.rime_dir,
            &manager.config.schema,
            Lang::Zh
        ));
        assert!(!manager.rime_dir.join("wanxiang.custom.yaml").exists());
    }

    #[test]
    fn model_patch_rollback_restores_exact_custom_yaml() {
        let root = tempfile::tempdir().expect("temporary root");
        let mut manager = manager(root.path());
        manager.config.schema = Schema::Ice;
        std::fs::create_dir_all(&manager.rime_dir).expect("Rime directory");
        let patch = manager.rime_dir.join("rime_ice.custom.yaml");
        let original = b"# Preserve my model\npatch:\n  grammar/language: my-custom-model\n  translator/max_homophones: 11\n  menu/page_size: 7\n";
        std::fs::write(&patch, original).expect("custom patch");
        manager.config_path = root.path().to_path_buf();
        let mut proposed = manager.config.clone();
        proposed.model_patch_enabled = true;
        assert!(save_settings(&mut manager, proposed).is_err());
        assert!(!manager.config.model_patch_enabled);
        assert_eq!(std::fs::read(patch).expect("restored patch"), original);
    }

    #[test]
    fn model_patch_rollback_restores_exact_applied_yaml_when_disabling() {
        let root = tempfile::tempdir().expect("temporary root");
        let mut manager = manager(root.path());
        updater::model_patch::patch_model(&manager.rime_dir, &manager.config.schema, Lang::Zh)
            .expect("enabled patch");
        let patch = manager.rime_dir.join("wanxiang.custom.yaml");
        let original = format!(
            "# Preserve this comment\n{}",
            std::fs::read_to_string(&patch).expect("enabled YAML")
        );
        std::fs::write(&patch, &original).expect("annotated patch");
        manager.config.model_patch_enabled = true;
        manager.config_path = root.path().to_path_buf();
        assert!(set_model_patch(&mut manager, PatchMode::Disabled).is_err());
        assert!(manager.config.model_patch_enabled);
        assert_eq!(
            std::fs::read_to_string(patch).expect("restored patch"),
            original
        );
    }

    #[test]
    fn exclusion_save_failure_restores_the_in_memory_rules() {
        let root = tempfile::tempdir().expect("temporary root");
        let mut manager = manager(root.path());
        let original = manager.config.exclude_files.clone();
        manager.config_path = root.path().to_path_buf();
        assert!(manager
            .add_exclude_pattern("local-only.txt".into())
            .is_err());
        assert_eq!(manager.config.exclude_files, original);
    }

    #[test]
    fn saving_unrelated_settings_does_not_remove_a_manual_model_patch() {
        let root = tempfile::tempdir().expect("temporary root");
        let mut manager = manager(root.path());
        updater::model_patch::patch_model(&manager.rime_dir, &manager.config.schema, Lang::Zh)
            .expect("manual patch");
        let mut proposed = manager.config.clone();
        proposed.language = "en".into();
        save_settings(&mut manager, proposed).expect("save settings");
        assert!(!manager.config.model_patch_enabled);
        assert!(updater::model_patch::is_model_patched(
            &manager.rime_dir,
            &manager.config.schema,
            Lang::En
        ));
    }

    #[cfg(unix)]
    #[test]
    fn atomic_save_preserves_a_configuration_symlink() {
        let root = tempfile::tempdir().expect("temporary root");
        let mut manager = manager(root.path());
        let target = manager.config_path.clone();
        let link = root.path().join("config-link.json");
        std::os::unix::fs::symlink(&target, &link).expect("configuration symlink");
        manager.config_path = link.clone();
        manager.config.language = "en".into();
        manager.save().expect("atomic save through symlink");
        assert!(link.is_symlink());
        let saved: Config = serde_json::from_slice(&std::fs::read(target).expect("target file"))
            .expect("saved configuration");
        assert_eq!(saved.language, "en");
    }
}
