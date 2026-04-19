use crate::config::Manager;
use crate::i18n::{L10n, Lang};
use crate::types::Schema;
use crate::updater;
use ratatui::style::{Modifier, Style};

#[derive(Debug, Clone, Default)]
pub(crate) struct ConfigStatusSnapshot {
    pub(crate) scheme_status: String,
    pub(crate) dict_status: String,
    pub(crate) model_status: String,
    pub(crate) model_patch_status: String,
    pub(crate) candidate_page_size: String,
    pub(crate) installed_scheme_version: String,
    pub(crate) installed_dict_version: String,
    pub(crate) installed_model_version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConfigAction {
    TuiTheme,
    UserDataPolicy,
    ExcludeRules,
    WanxiangDiagnosis,
    Mirror,
    DownloadThreads,
    Language,
    ProxyEnabled,
    ProxyType,
    ProxyAddress,
    ModelPatch,
    CandidatePageSize,
    EngineSync,
    SyncStrategy,
    Refresh,
}

pub(crate) fn config_actions(config: &crate::types::Config) -> Vec<ConfigAction> {
    let mut actions = vec![
        ConfigAction::TuiTheme,
        ConfigAction::UserDataPolicy,
        ConfigAction::ExcludeRules,
    ];
    if config.schema.is_wanxiang() {
        actions.push(ConfigAction::WanxiangDiagnosis);
    }
    actions.extend([
        ConfigAction::Mirror,
        ConfigAction::DownloadThreads,
        ConfigAction::Language,
        ConfigAction::ProxyEnabled,
    ]);
    if config.proxy_enabled {
        actions.push(ConfigAction::ProxyType);
        actions.push(ConfigAction::ProxyAddress);
    }
    actions.extend([
        ConfigAction::ModelPatch,
        ConfigAction::CandidatePageSize,
        ConfigAction::EngineSync,
    ]);
    if config.engine_sync_enabled {
        actions.push(ConfigAction::SyncStrategy);
    }
    actions.push(ConfigAction::Refresh);
    actions
}

pub(crate) async fn build_config_status_snapshot(
    schema: Schema,
    lang: Lang,
    rime_dir: std::path::PathBuf,
) -> ConfigStatusSnapshot {
    let t = L10n::new(lang);
    let manager = match Manager::new() {
        Ok(manager) => manager,
        Err(_) => {
            return ConfigStatusSnapshot {
                scheme_status: t.t("update.failed").into(),
                dict_status: t.t("update.failed").into(),
                model_status: t.t("update.failed").into(),
                model_patch_status: t.t("update.failed").into(),
                candidate_page_size: t.t("update.failed").into(),
                installed_scheme_version: t.t("config.unknown").into(),
                installed_dict_version: t.t("config.unknown").into(),
                installed_model_version: t.t("config.unknown").into(),
            };
        }
    };

    let scheme_local = updater::BaseUpdater::load_record(&manager.scheme_record_path());
    let dict_local = updater::BaseUpdater::load_record(&manager.dict_record_path());
    let model_local = updater::BaseUpdater::load_record(&manager.model_record_path());
    let model_patch_applied = updater::model_patch::is_model_patched(&rime_dir, &schema, lang);

    let base = match updater::BaseUpdater::new(
        &manager.config,
        manager.cache_dir.clone(),
        manager.rime_dir.clone(),
    ) {
        Ok(base) => base,
        Err(_) => {
            return ConfigStatusSnapshot {
                scheme_status: local_status_text(&t, scheme_local.as_ref(), None),
                dict_status: local_status_text(&t, dict_local.as_ref(), None),
                model_status: local_status_text(&t, model_local.as_ref(), None),
                model_patch_status: format!(
                    "{} / {}",
                    if manager.config.model_patch_enabled {
                        t.t("config.enabled")
                    } else {
                        t.t("config.disabled")
                    },
                    if model_patch_applied {
                        t.t("patch.model.enabled")
                    } else {
                        t.t("patch.model.disabled")
                    }
                ),
                candidate_page_size: candidate_page_size_text(&rime_dir, schema, &t),
                installed_scheme_version: installed_version_text(&t, scheme_local.as_ref()),
                installed_dict_version: installed_dict_version_text(
                    schema,
                    &t,
                    dict_local.as_ref(),
                ),
                installed_model_version: installed_version_text(&t, model_local.as_ref()),
            };
        }
    };

    let scheme_remote = if schema.is_wanxiang() {
        updater::wanxiang::WanxiangUpdater { base }
            .check_scheme_update(&schema, None)
            .await
            .ok()
    } else if schema == Schema::Ice {
        updater::ice::IceUpdater { base }
            .check_scheme_update(None)
            .await
            .ok()
    } else if schema == Schema::Frost {
        updater::frost::FrostUpdater { base }
            .check_scheme_update(None)
            .await
            .ok()
    } else {
        updater::mint::MintUpdater { base }
            .check_scheme_update(None)
            .await
            .ok()
    };

    let base = updater::BaseUpdater::new(
        &manager.config,
        manager.cache_dir.clone(),
        manager.rime_dir.clone(),
    )
    .ok();
    let dict_remote = if let Some(base) = base {
        if schema.is_wanxiang() {
            updater::wanxiang::WanxiangUpdater { base }
                .check_dict_update(&schema, None)
                .await
                .ok()
        } else if schema == Schema::Ice {
            updater::ice::IceUpdater { base }
                .check_dict_update(None)
                .await
                .ok()
        } else {
            None
        }
    } else {
        None
    };

    let base = updater::BaseUpdater::new(
        &manager.config,
        manager.cache_dir.clone(),
        manager.rime_dir.clone(),
    )
    .ok();
    let model_remote = if let Some(base) = base {
        updater::wanxiang::WanxiangUpdater { base }
            .check_model_update(None)
            .await
            .ok()
    } else {
        None
    };

    ConfigStatusSnapshot {
        scheme_status: local_status_text(&t, scheme_local.as_ref(), scheme_remote.as_ref()),
        dict_status: if schema.dict_zip().is_none() {
            t.t("config.na").into()
        } else {
            local_status_text(&t, dict_local.as_ref(), dict_remote.as_ref())
        },
        model_status: local_status_text(&t, model_local.as_ref(), model_remote.as_ref()),
        model_patch_status: format!(
            "{} / {}",
            if manager.config.model_patch_enabled {
                t.t("config.enabled")
            } else {
                t.t("config.disabled")
            },
            if model_patch_applied {
                t.t("patch.model.enabled")
            } else {
                t.t("patch.model.disabled")
            }
        ),
        candidate_page_size: candidate_page_size_text(&rime_dir, schema, &t),
        installed_scheme_version: installed_version_text(&t, scheme_local.as_ref()),
        installed_dict_version: installed_dict_version_text(schema, &t, dict_local.as_ref()),
        installed_model_version: installed_version_text(&t, model_local.as_ref()),
    }
}

fn local_status_text(
    t: &L10n,
    local: Option<&crate::types::UpdateRecord>,
    remote: Option<&crate::types::UpdateInfo>,
) -> String {
    match (local, remote) {
        (Some(local), Some(remote)) if local.tag == remote.tag => {
            format!("{} ({})", t.t("config.latest"), local.tag)
        }
        (Some(local), Some(remote)) => format!(
            "{} {} → {}",
            t.t("config.update_available"),
            local.tag,
            remote.tag
        ),
        (Some(local), None) => format!("{} ({})", t.t("config.installed"), local.tag),
        (None, Some(remote)) => format!("{} ({})", t.t("config.not_installed"), remote.tag),
        (None, None) => t.t("config.unknown").into(),
    }
}

fn candidate_page_size_text(rime_dir: &std::path::Path, schema: Schema, t: &L10n) -> String {
    match crate::custom::candidate_page_size(rime_dir, schema) {
        Ok(Some(size)) => size.to_string(),
        Ok(None) => t.t("config.default").into(),
        Err(_) => t.t("config.unknown").into(),
    }
}

fn installed_version_text(t: &L10n, local: Option<&crate::types::UpdateRecord>) -> String {
    local
        .map(|record| record.tag.clone())
        .unwrap_or_else(|| t.t("config.not_installed").into())
}

fn installed_dict_version_text(
    schema: Schema,
    t: &L10n,
    local: Option<&crate::types::UpdateRecord>,
) -> String {
    if schema.dict_zip().is_none() {
        t.t("config.na").into()
    } else {
        installed_version_text(t, local)
    }
}

pub(crate) fn next_user_data_policy(config: &crate::types::Config) -> String {
    match config.user_data_policy.trim().to_ascii_lowercase().as_str() {
        "prompt" => "preserve".into(),
        "preserve" => "discard".into(),
        _ => "prompt".into(),
    }
}

pub(crate) fn tui_theme_mode_label<'a>(mode: &str, lang: Lang) -> &'a str {
    let is_zh = matches!(lang, Lang::Zh);
    match mode.trim().to_ascii_lowercase().as_str() {
        "light" => {
            if is_zh {
                "浅色"
            } else {
                "Light"
            }
        }
        "dark" => {
            if is_zh {
                "深色"
            } else {
                "Dark"
            }
        }
        _ => {
            if is_zh {
                "自动"
            } else {
                "Auto"
            }
        }
    }
}

pub(crate) fn user_data_policy_label<'a>(policy: &str, lang: Lang) -> &'a str {
    let is_zh = matches!(lang, Lang::Zh);
    match policy.trim().to_ascii_lowercase().as_str() {
        "preserve" => {
            if is_zh {
                "保留（直接保留 userdb/custom）"
            } else {
                "Preserve (keep userdb/custom)"
            }
        }
        "discard" => {
            if is_zh {
                "不保留（允许覆盖学习数据）"
            } else {
                "Discard (allow overwrite)"
            }
        }
        _ => {
            if is_zh {
                "提示用户（更新前询问）"
            } else {
                "Prompt (ask before update)"
            }
        }
    }
}

pub(crate) fn update_notice_text<'a>(config: &crate::types::Config, t: &'a L10n) -> &'a str {
    match config.user_data_policy.trim().to_ascii_lowercase().as_str() {
        "discard" => t.t("update.discard_user_data_notice"),
        _ => t.t("update.preserve_user_data_notice"),
    }
}

pub(crate) fn update_detail_text<'a>(config: &crate::types::Config, t: &'a L10n) -> &'a str {
    match config.user_data_policy.trim().to_ascii_lowercase().as_str() {
        "discard" => t.t("update.discard_user_data_detail"),
        _ => t.t("update.preserve_user_data_detail"),
    }
}

pub(crate) fn user_data_policy_row_style(policy: &str) -> Style {
    match policy.trim().to_ascii_lowercase().as_str() {
        "discard" => Style::default()
            .fg(crate::ui::style::color_warning())
            .add_modifier(Modifier::BOLD),
        _ => crate::ui::style::primary_text(),
    }
}

pub(crate) fn effective_user_data_policy_label<'a>(
    config: &crate::types::Config,
    lang: Lang,
) -> &'a str {
    let is_zh = matches!(lang, Lang::Zh);
    match config.user_data_policy.trim().to_ascii_lowercase().as_str() {
        "discard" => {
            if is_zh {
                "不保留（允许覆盖学习数据）"
            } else {
                "Discard (allow overwrite)"
            }
        }
        "prompt" => {
            if is_zh {
                "保留（由提示确认）"
            } else {
                "Preserve (confirmed by prompt)"
            }
        }
        _ => {
            if is_zh {
                "保留（直接保留 userdb/custom）"
            } else {
                "Preserve (keep userdb/custom)"
            }
        }
    }
}

pub(crate) fn next_tui_theme_mode(config: &crate::types::Config) -> String {
    match config.tui_theme_mode.trim().to_ascii_lowercase().as_str() {
        "auto" => "light".into(),
        "light" => "dark".into(),
        _ => "auto".into(),
    }
}

pub(crate) fn proxy_source_label(
    effective_proxy: Option<&crate::api::EffectiveProxy>,
    t: &L10n,
) -> String {
    match effective_proxy.map(|proxy| proxy.source) {
        Some(crate::api::ProxySource::Config) => t.t("config.proxy_source_config").to_string(),
        Some(crate::api::ProxySource::Environment) => t.t("config.proxy_source_env").to_string(),
        None => t.t("config.none").to_string(),
    }
}

pub(crate) fn config_enabled_label(enabled: bool, t: &L10n) -> String {
    if enabled {
        t.t("config.enabled").into()
    } else {
        t.t("config.disabled").into()
    }
}

pub(crate) fn next_language_value(current: &str) -> String {
    if current.starts_with("zh") {
        "en".into()
    } else {
        "zh".into()
    }
}

pub(crate) fn next_proxy_type_value(current: &str) -> String {
    if current == "http" {
        "socks5".into()
    } else {
        "http".into()
    }
}
