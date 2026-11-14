use crate::i18n::Lang;
use crate::types::{UpdateInfo, UpdateRecord};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentState {
    #[default]
    Unchecked,
    Latest,
    UpdateAvailable,
    NotInstalled,
    CheckFailed,
    Unsupported,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ComponentStatus {
    pub state: ComponentState,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
}

impl ComponentStatus {
    pub fn checked(
        local: Option<&UpdateRecord>,
        remote: Option<&UpdateInfo>,
        installed: bool,
        current: bool,
    ) -> Self {
        let state = match remote {
            None => ComponentState::CheckFailed,
            Some(_) if current => ComponentState::Latest,
            Some(_) if !installed => ComponentState::NotInstalled,
            Some(_) => ComponentState::UpdateAvailable,
        };
        Self {
            state,
            installed_version: installed
                .then(|| local.map(|record| record.tag.clone()).unwrap_or_default()),
            latest_version: remote.map(|info| info.tag.clone()),
        }
    }

    pub fn unsupported() -> Self {
        Self {
            state: ComponentState::Unsupported,
            ..Self::default()
        }
    }

    pub fn actionable(&self) -> bool {
        matches!(
            self.state,
            ComponentState::UpdateAvailable | ComponentState::NotInstalled
        ) && self.latest_version.is_some()
    }

    pub fn label(&self, lang: Lang) -> &'static str {
        match (self.state, lang) {
            (ComponentState::Unchecked, Lang::Zh) => "尚未检查",
            (ComponentState::Latest, Lang::Zh) => "已是最新",
            (ComponentState::UpdateAvailable, Lang::Zh) => "需要更新",
            (ComponentState::NotInstalled, Lang::Zh) => "未安装",
            (ComponentState::CheckFailed, Lang::Zh) => "检查失败，请重试",
            (ComponentState::Unsupported, Lang::Zh) => "此方案无需此组件",
            (ComponentState::Unchecked, _) => "Not checked",
            (ComponentState::Latest, _) => "Up to date",
            (ComponentState::UpdateAvailable, _) => "Update available",
            (ComponentState::NotInstalled, _) => "Not installed",
            (ComponentState::CheckFailed, _) => "Check failed — retry",
            (ComponentState::Unsupported, _) => "Not needed by this scheme",
        }
    }

    pub fn summary(&self, lang: Lang) -> String {
        let mut summary = self.label(lang).to_string();
        if let Some(version) = &self.installed_version {
            if !version.is_empty() {
                summary.push_str(&format!(" · {version}"));
            }
        }
        if let Some(latest) = &self.latest_version {
            if self.state != ComponentState::Latest {
                summary.push_str(&format!(" → {latest}"));
            }
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::updater::BaseUpdater;

    #[test]
    fn equal_tags_do_not_hide_changed_hash_time_or_identity() {
        let local = UpdateRecord {
            name: "scheme.zip".into(),
            tag: "v1".into(),
            sha256: "old".into(),
            update_time: "2026-11-01T00:00:00Z".into(),
            apply_time: String::new(),
        };
        let mut remote = UpdateInfo {
            name: local.name.clone(),
            tag: local.tag.clone(),
            sha256: local.sha256.clone(),
            update_time: local.update_time.clone(),
            url: String::new(),
            description: String::new(),
            size: 0,
        };
        let status = |remote: &UpdateInfo| {
            ComponentStatus::checked(
                Some(&local),
                Some(remote),
                true,
                !BaseUpdater::needs_update(Some(&local), remote),
            )
        };
        assert_eq!(status(&remote).state, ComponentState::Latest);
        remote.sha256 = "new".into();
        assert!(status(&remote).actionable());
        remote.sha256 = local.sha256.clone();
        remote.update_time = "2026-11-02T00:00:00Z".into();
        assert!(status(&remote).actionable());
        remote.update_time = local.update_time.clone();
        remote.name = "other.zip".into();
        assert!(status(&remote).actionable());
        assert_eq!(
            ComponentStatus::checked(Some(&local), Some(&remote), false, false).state,
            ComponentState::NotInstalled
        );
        assert_eq!(
            ComponentStatus::checked(Some(&local), None, true, false).state,
            ComponentState::CheckFailed
        );
        assert!(!ComponentStatus::default().actionable());
        assert!(!ComponentStatus::unsupported().actionable());
    }
}
