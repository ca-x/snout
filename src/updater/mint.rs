use super::base::{BaseUpdater, UpdateResult};
use crate::i18n::{L10n, Lang};
use crate::types::*;
use crate::updater::{UpdateComponent, UpdateEvent, UpdatePhase};
use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

/// 薄荷方案更新器
pub struct MintUpdater {
    pub base: BaseUpdater,
}

impl MintUpdater {
    /// 检查方案更新（主分支归档）
    pub async fn check_scheme_update(&self, cancel: Option<&CancelSignal>) -> Result<UpdateInfo> {
        if self.base.client.use_mirror() {
            match self
                .base
                .client
                .fetch_cnb_release(MINT_OWNER, MINT_REPO, "latest", cancel)
                .await
            {
                Ok(release) => match find_mint_release_asset(&release) {
                    Some(info) => Ok(info),
                    None => self.fetch_github_branch_archive(cancel).await,
                },
                Err(_) => self.fetch_github_branch_archive(cancel).await,
            }
        } else {
            self.fetch_github_branch_archive(cancel).await
        }
    }

    async fn fetch_github_branch_archive(
        &self,
        cancel: Option<&CancelSignal>,
    ) -> Result<UpdateInfo> {
        self.base
            .client
            .fetch_github_branch_archive(MINT_OWNER, MINT_REPO, MINT_BRANCH, MINT_ARCHIVE, cancel)
            .await
    }

    /// 更新方案
    pub async fn update_scheme(
        &self,
        config: &crate::types::Config,
        cancel: Option<&CancelSignal>,
        mut progress: impl FnMut(UpdateEvent),
    ) -> Result<UpdateResult> {
        let t = L10n::new(Lang::from_str(&config.language));
        progress(UpdateEvent {
            component: UpdateComponent::Scheme,
            phase: UpdatePhase::Checking,
            progress: 0.05,
            detail: t.t("update.mint_scheme_checking").into(),
        });

        let info = self.check_scheme_update(cancel).await?;
        let record_path = self.base.cache_dir.join("scheme_record.json");
        let local = BaseUpdater::load_record(&record_path);

        let scheme_switched = local
            .as_ref()
            .map(|r| r.name != MINT_ARCHIVE)
            .unwrap_or(false);

        if !scheme_switched && !BaseUpdater::needs_update(local.as_ref(), &info) {
            progress(UpdateEvent {
                component: UpdateComponent::Scheme,
                phase: UpdatePhase::Finished,
                progress: 1.0,
                detail: t.t("update.up_to_date").into(),
            });
            return Ok(BaseUpdater::success_result(
                t.t("update.scheme"),
                &info.tag,
                &info.tag,
                t.t("update.up_to_date"),
            ));
        }

        if scheme_switched {
            progress(UpdateEvent {
                component: UpdateComponent::Scheme,
                phase: UpdatePhase::Checking,
                progress: 0.05,
                detail: t.t("update.scheme_switched").into(),
            });
        }

        self.base
            .download_and_extract(
                &info,
                config,
                &self.base.rime_dir,
                UpdateComponent::Scheme,
                cancel,
                &mut progress,
            )
            .await?;

        crate::fileutil::extract::handle_nested_dir(&self.base.rime_dir, &info.name)?;
        filter_mint_distribution(&self.base.rime_dir)?;

        if let Some(signal) = cancel {
            signal.checkpoint()?;
        }
        progress(UpdateEvent {
            component: UpdateComponent::Scheme,
            phase: UpdatePhase::Saving,
            progress: 0.95,
            detail: t.t("update.saving").into(),
        });
        let record = UpdateRecord {
            name: info.name.clone(),
            update_time: info.update_time.clone(),
            tag: info.tag.clone(),
            apply_time: chrono::Utc::now().to_rfc3339(),
            sha256: info.sha256.clone(),
        };
        BaseUpdater::save_record(&record_path, &record)?;
        crate::config::persist_installed_schema(Schema::Mint)?;

        let build_dir = self.base.rime_dir.join("build");
        if build_dir.exists() {
            let _ = std::fs::remove_dir_all(&build_dir);
        }

        progress(UpdateEvent {
            component: UpdateComponent::Scheme,
            phase: UpdatePhase::Finished,
            progress: 1.0,
            detail: t.t("update.mint_scheme_done").into(),
        });
        Ok(UpdateResult {
            component: t.t("update.scheme").into(),
            old_version: local
                .map(|r| r.tag)
                .unwrap_or_else(|| t.t("status.not_installed").into()),
            new_version: info.tag,
            success: true,
            message: t.t("update.complete").into(),
        })
    }
}

fn find_mint_release_asset(release: &GitHubRelease) -> Option<UpdateInfo> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == MINT_ARCHIVE)
        .map(|asset| UpdateInfo {
            name: asset.name.clone(),
            url: asset.browser_download_url.clone(),
            update_time: asset.updated_at.clone().unwrap_or_default(),
            tag: release.tag_name.clone(),
            description: release.body.clone(),
            sha256: asset.sha256.clone().unwrap_or_default(),
            size: asset.size,
        })
}

fn filter_mint_distribution(base: &Path) -> Result<()> {
    let keep: HashSet<&'static str> = HashSet::from([
        "default.yaml",
        "dicts",
        "double_pinyin.schema.yaml",
        "double_pinyin_abc.schema.yaml",
        "double_pinyin_flypy.schema.yaml",
        "double_pinyin_mspy.schema.yaml",
        "double_pinyin_sogou.schema.yaml",
        "double_pinyin_ziguang.schema.yaml",
        "ibus_rime.yaml",
        "lua",
        "melt_eng.dict.yaml",
        "melt_eng.schema.yaml",
        "opencc",
        "radical_pinyin.dict.yaml",
        "radical_pinyin.schema.yaml",
        "radical_pinyin_flypy.schema.yaml",
        "rime_mint.dict.yaml",
        "rime_mint.schema.yaml",
        "rime_mint_flypy.schema.yaml",
        "squirrel.yaml",
        "stroke.dict.yaml",
        "stroke.schema.yaml",
        "symbols.yaml",
        "t9.schema.yaml",
        "terra_pinyin.dict.yaml",
        "terra_pinyin.schema.yaml",
        "terra_symbols.yaml",
        "weasel.yaml",
        "wubi86_jidian.dict.yaml",
        "wubi86_jidian.schema.yaml",
        "wubi98_mint.dict.yaml",
        "wubi98_mint.schema.yaml",
    ]);

    for entry in std::fs::read_dir(base)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if keep.contains(name.as_str()) {
            continue;
        }

        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("snout-{name}-{nanos}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn filter_mint_distribution_keeps_rime_assets_only() {
        let dir = temp_dir("mint-filter");
        std::fs::write(dir.join("default.yaml"), "").expect("write default");
        std::fs::write(dir.join("rime_mint.dict.yaml"), "").expect("write dict");
        std::fs::write(dir.join("rime_mint.schema.yaml"), "").expect("write schema");
        std::fs::write(dir.join("weasel.yaml"), "").expect("write weasel");
        std::fs::write(dir.join("squirrel.yaml"), "").expect("write squirrel");
        std::fs::create_dir_all(dir.join("dicts")).expect("create dicts");
        std::fs::create_dir_all(dir.join("lua")).expect("create lua");
        std::fs::create_dir_all(dir.join("opencc")).expect("create opencc");
        std::fs::create_dir_all(dir.join("plum")).expect("create plum");
        std::fs::create_dir_all(dir.join(".github")).expect("create github");
        std::fs::write(dir.join("README.md"), "").expect("write readme");

        filter_mint_distribution(&dir).expect("filter mint distribution");

        assert!(dir.join("default.yaml").exists());
        assert!(dir.join("rime_mint.dict.yaml").exists());
        assert!(dir.join("rime_mint.schema.yaml").exists());
        assert!(dir.join("dicts").exists());
        assert!(dir.join("lua").exists());
        assert!(dir.join("opencc").exists());
        assert!(dir.join("weasel.yaml").exists());
        assert!(dir.join("squirrel.yaml").exists());
        assert!(!dir.join("plum").exists());
        assert!(!dir.join(".github").exists());
        assert!(!dir.join("README.md").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn find_mint_release_asset_extracts_latest_archive() {
        let release = GitHubRelease {
            tag_name: "latest".into(),
            body: "body".into(),
            assets: vec![GitHubAsset {
                name: MINT_ARCHIVE.into(),
                browser_download_url:
                    "https://cnb.cool/Mintimate/oh-my-rime/-/releases/download/latest/oh-my-rime.zip"
                        .into(),
                updated_at: Some("2026-04-13T11:50:14Z".into()),
                size: 26553904,
                sha256: Some("deadbeef".into()),
                digest: None,
            }],
            published_at: None,
        };

        let info = find_mint_release_asset(&release).expect("mint asset");

        assert_eq!(info.name, MINT_ARCHIVE);
        assert_eq!(info.tag, "latest");
        assert_eq!(
            info.url,
            "https://cnb.cool/Mintimate/oh-my-rime/-/releases/download/latest/oh-my-rime.zip"
        );
        assert_eq!(info.sha256, "deadbeef");
    }
}
