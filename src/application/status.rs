use super::component_status::ComponentStatus;
use crate::config::Manager;
use crate::i18n::{L10n, Lang};
use crate::types::Schema;
use crate::updater;
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ConfigStatusSnapshot {
    pub scheme: ComponentStatus,
    pub dict: ComponentStatus,
    pub model: ComponentStatus,
    pub scheme_status: String,
    pub dict_status: String,
    pub model_status: String,
    pub model_patch_status: String,
    pub candidate_page_size: String,
    pub installed_scheme_version: String,
    pub installed_dict_version: String,
    pub installed_model_version: String,
}

pub async fn build_config_status_snapshot(
    manager: anyhow::Result<Manager>,
    schema: Schema,
    lang: Lang,
    rime_dir: std::path::PathBuf,
) -> ConfigStatusSnapshot {
    let t = L10n::new(lang);
    let manager = match manager {
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
                scheme: ComponentStatus::checked(None, None, false, false),
                dict: if schema.dict_zip().is_some() {
                    ComponentStatus::checked(None, None, false, false)
                } else {
                    ComponentStatus::unsupported()
                },
                model: if schema.supports_model_patch() {
                    ComponentStatus::checked(None, None, false, false)
                } else {
                    ComponentStatus::unsupported()
                },
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
                model_patch_status: model_patch_status_text(
                    &t,
                    manager.config.model_patch_enabled,
                    model_patch_applied,
                ),
                candidate_page_size: candidate_page_size_text(&rime_dir, schema, &t),
                installed_scheme_version: installed_version_text(&t, scheme_local.as_ref()),
                installed_dict_version: installed_dict_version_text(
                    schema,
                    &t,
                    dict_local.as_ref(),
                ),
                installed_model_version: installed_version_text(&t, model_local.as_ref()),
                scheme: ComponentStatus::checked(
                    scheme_local.as_ref(),
                    None,
                    scheme_local.is_some(),
                    false,
                ),
                dict: if schema.dict_zip().is_some() {
                    ComponentStatus::checked(dict_local.as_ref(), None, dict_local.is_some(), false)
                } else {
                    ComponentStatus::unsupported()
                },
                model: if schema.supports_model_patch() {
                    ComponentStatus::checked(
                        model_local.as_ref(),
                        None,
                        model_local.is_some(),
                        false,
                    )
                } else {
                    ComponentStatus::unsupported()
                },
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
    } else if matches!(schema, Schema::Wubi86Jidian | Schema::Abcm) {
        updater::repository::RepositoryUpdater { base }
            .check_scheme_update(&schema, None)
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
    let model_remote = if let Some(base) = base.filter(|_| schema.supports_model_patch()) {
        updater::wanxiang::WanxiangUpdater { base }
            .check_model_update(None)
            .await
            .ok()
    } else {
        None
    };

    let scheme_installed = crate::config::schema_record_matches_files(schema, &rime_dir);
    let scheme = ComponentStatus::checked(
        scheme_local.as_ref(),
        scheme_remote.as_ref(),
        scheme_installed,
        scheme_installed
            && scheme_remote.as_ref().is_some_and(|remote| {
                !updater::BaseUpdater::needs_update(scheme_local.as_ref(), remote)
            }),
    );
    let dict = if schema.dict_zip().is_some() {
        let current = dict_remote.as_ref().is_some_and(|remote| {
            if schema.is_wanxiang() {
                updater::wanxiang::dict_is_current(&rime_dir, &schema, dict_local.as_ref(), remote)
            } else {
                updater::ice::dict_is_current(&rime_dir, dict_local.as_ref(), remote)
            }
        });
        ComponentStatus::checked(
            dict_local.as_ref(),
            dict_remote.as_ref(),
            dict_local.is_some() && rime_dir.join("dicts").is_dir(),
            current,
        )
    } else {
        ComponentStatus::unsupported()
    };
    let model = if schema.supports_model_patch() {
        let target = rime_dir.join(crate::types::MODEL_FILE);
        let current = model_remote.as_ref().is_some_and(|remote| {
            updater::wanxiang::model_is_current(&target, model_local.as_ref(), remote)
        });
        ComponentStatus::checked(
            model_local.as_ref(),
            model_remote.as_ref(),
            target.is_file(),
            current,
        )
    } else {
        ComponentStatus::unsupported()
    };
    ConfigStatusSnapshot {
        scheme_status: scheme.summary(lang),
        dict_status: dict.summary(lang),
        model_status: model.summary(lang),
        scheme,
        dict,
        model,
        model_patch_status: model_patch_status_text(
            &t,
            manager.config.model_patch_enabled,
            model_patch_applied,
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
        (Some(local), None) => format!("{} ({})", t.t("config.unknown"), local.tag),
        (None, Some(remote)) => format!("{} ({})", t.t("config.not_installed"), remote.tag),
        (None, None) => t.t("config.unknown").into(),
    }
}

fn candidate_page_size_text(rime_dir: &std::path::Path, schema: Schema, t: &L10n) -> String {
    match crate::custom::candidate_page_size(rime_dir, schema) {
        Ok(Some(size)) => size.to_string(),
        Ok(None) => t.t("config.default_value").into(),
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

pub(crate) fn config_enabled_label(enabled: bool, t: &L10n) -> String {
    if enabled {
        t.t("config.enabled").into()
    } else {
        t.t("config.disabled").into()
    }
}

fn model_patch_status_text(t: &L10n, auto_enabled: bool, applied: bool) -> String {
    format!(
        "{}: {} / {}: {}",
        t.t("config.model_patch_auto_status_label"),
        config_enabled_label(auto_enabled, t),
        t.t("config.model_patch_current_status_label"),
        if applied {
            t.t("patch.model.status_enabled")
        } else {
            t.t("patch.model.status_disabled")
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_patch_status_names_preference_and_applied_state() {
        let t = L10n::new(Lang::Zh);

        assert_eq!(
            model_patch_status_text(&t, false, true),
            "自动: 关闭 / 当前: 已启用"
        );
        assert_eq!(
            model_patch_status_text(&t, true, false),
            "自动: 开启 / 当前: 未启用"
        );

        let t = L10n::new(Lang::En);
        assert_eq!(
            model_patch_status_text(&t, false, true),
            "Auto: Disabled / Current: Enabled"
        );
    }
}
