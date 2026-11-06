pub(crate) use crate::application::status::{
    build_config_status_snapshot, config_enabled_label, ConfigStatusSnapshot,
};
use crate::i18n::{L10n, Lang};
use ratatui::style::{Modifier, Style};

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
    RimeAutoSetup,
    EngineSync,
    SyncStrategy,
    Refresh,
}

pub(crate) fn config_actions(config: &crate::types::Config) -> Vec<ConfigAction> {
    let mut actions = vec![ConfigAction::TuiTheme];
    if cfg!(target_os = "linux") {
        actions.push(ConfigAction::RimeAutoSetup);
    }
    actions.extend([ConfigAction::UserDataPolicy, ConfigAction::ExcludeRules]);
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
