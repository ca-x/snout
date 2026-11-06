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

        if !scheme_switched && scheme_is_current(&self.base.rime_dir, local.as_ref(), &info) {
            progress(UpdateEvent {
                component: UpdateComponent::Scheme,
                phase: UpdatePhase::Finished,
                progress: 1.0,
                detail: t.t("update.up_to_date").into(),
            });
            return Ok(BaseUpdater::success_result(
                UpdateComponent::Scheme,
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

        std::fs::create_dir_all(&self.base.cache_dir)?;
        let staging = tempfile::tempdir_in(&self.base.cache_dir)?;
        self.base
            .download_and_extract(
                &info,
                config,
                staging.path(),
                UpdateComponent::Scheme,
                cancel,
                &mut progress,
            )
            .await?;

        if let Some(signal) = cancel {
            signal.checkpoint()?;
        }
        install_mint_distribution(
            staging.path(),
            &self.base.rime_dir,
            user_data_behavior_for_config(config),
            &crate::config::effective_exclude_patterns(config),
        )?;

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
            kind: UpdateComponent::Scheme,
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

fn user_data_behavior_for_config(
    config: &crate::types::Config,
) -> crate::fileutil::extract::UserDataBehavior {
    match config.user_data_policy.trim().to_ascii_lowercase().as_str() {
        "discard" => crate::fileutil::extract::UserDataBehavior::Discard,
        _ => crate::fileutil::extract::UserDataBehavior::Preserve,
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

fn install_mint_distribution(
    incoming: &Path,
    rime_dir: &Path,
    user_data_behavior: crate::fileutil::extract::UserDataBehavior,
    exclude_patterns: &[String],
) -> Result<()> {
    crate::fileutil::extract::handle_nested_dir(
        incoming,
        MINT_ARCHIVE,
        user_data_behavior,
        exclude_patterns,
    )?;
    filter_mint_distribution(incoming)?;
    crate::fileutil::extract::merge_directory(
        incoming,
        rime_dir,
        user_data_behavior,
        exclude_patterns,
    )
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

fn scheme_is_current(rime_dir: &Path, local: Option<&UpdateRecord>, info: &UpdateInfo) -> bool {
    !BaseUpdater::needs_update(local, info)
        && [
            "rime_mint.schema.yaml",
            "rime_mint.dict.yaml",
            "dicts/rime_mint.chars.dict.yaml",
            "dicts/rime_mint.base.dict.yaml",
        ]
        .iter()
        .all(|file| rime_dir.join(file).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const SCHEME_FILES: &[&str] = &[
        "rime_mint.schema.yaml",
        "rime_mint.dict.yaml",
        "dicts/rime_mint.chars.dict.yaml",
        "dicts/rime_mint.base.dict.yaml",
    ];

    fn cached_update(root: &Path) -> (UpdateInfo, UpdateRecord) {
        let info = UpdateInfo {
            name: MINT_ARCHIVE.into(),
            url: String::new(),
            update_time: "2026-11-01T00:00:00Z".into(),
            tag: "v1".into(),
            description: String::new(),
            sha256: String::new(),
            size: 0,
        };
        let record = UpdateRecord {
            name: info.name.clone(),
            update_time: info.update_time.clone(),
            tag: info.tag.clone(),
            apply_time: String::new(),
            sha256: String::new(),
        };
        let path = root.join("cache/scheme_record.json");
        BaseUpdater::save_record(&path, &record).expect("save record");
        (info, BaseUpdater::load_record(&path).expect("read record"))
    }

    fn install_scheme_files(root: &Path) {
        for filename in SCHEME_FILES {
            let path = root.join(filename);
            std::fs::create_dir_all(path.parent().expect("file parent")).expect("parent");
            std::fs::write(path, "installed").expect("installed file");
        }
    }

    #[test]
    fn scheme_freshness_requires_installed_files_despite_matching_cache() {
        let root = tempfile::tempdir().expect("temporary root");
        let rime_dir = root.path().join("rime");
        let (info, record) = cached_update(root.path());
        install_scheme_files(&rime_dir);
        assert!(scheme_is_current(&rime_dir, Some(&record), &info));
        for filename in SCHEME_FILES {
            let path = rime_dir.join(filename);
            std::fs::remove_file(&path).expect("remove required file");
            assert!(
                !scheme_is_current(&rime_dir, Some(&record), &info),
                "missing {filename} must trigger repair"
            );
            std::fs::create_dir(&path).expect("directory placeholder");
            assert!(
                !scheme_is_current(&rime_dir, Some(&record), &info),
                "directory at {filename} is not an installed file"
            );
            std::fs::remove_dir(&path).expect("remove placeholder");
            std::fs::write(path, "restored").expect("restore installed file");
        }
    }

    #[test]
    fn scheme_freshness_rejects_the_same_cache_for_a_different_rime_root() {
        let root = tempfile::tempdir().expect("temporary root");
        let installed = root.path().join("installed");
        let other = root.path().join("other");
        let (info, record) = cached_update(root.path());
        install_scheme_files(&installed);
        assert!(scheme_is_current(&installed, Some(&record), &info));
        assert!(!scheme_is_current(&other, Some(&record), &info));
        std::fs::create_dir(&other).expect("empty Rime root");
        assert!(!scheme_is_current(&other, Some(&record), &info));
    }

    #[test]
    fn scheme_freshness_preserves_record_checks_when_files_exist() {
        let root = tempfile::tempdir().expect("temporary root");
        let (info, mut record) = cached_update(root.path());
        install_scheme_files(root.path());
        assert!(scheme_is_current(root.path(), Some(&record), &info));
        assert!(!scheme_is_current(root.path(), None, &info));
        record.tag = "older".into();
        assert!(!scheme_is_current(root.path(), Some(&record), &info));
    }

    #[cfg(unix)]
    fn assert_mint_rejects_destination_symlink(entry_name: &str, link_name: &str) {
        let root = tempfile::tempdir().unwrap();
        let incoming = root.path().join("incoming");
        let live = root.path().join("live");
        let outside = root.path().join("outside");
        std::fs::create_dir_all(incoming.join("lua")).unwrap();
        std::fs::create_dir_all(&live).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("victim.lua");
        std::fs::write(&victim, b"outside-original").unwrap();
        std::fs::write(incoming.join(entry_name), b"incoming-replacement").unwrap();
        let target = if link_name == "lua" {
            &outside
        } else {
            &victim
        };
        std::os::unix::fs::symlink(target, live.join(link_name)).unwrap();

        let result = install_mint_distribution(
            &incoming,
            &live,
            crate::fileutil::extract::UserDataBehavior::Discard,
            &[],
        );

        assert_eq!(std::fs::read(&victim).unwrap(), b"outside-original");
        assert!(result.is_err());
        assert!(live.join(link_name).is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn regression_mint_rejects_symlinked_destination_directory() {
        assert_mint_rejects_destination_symlink("lua/victim.lua", "lua");
    }

    #[cfg(unix)]
    #[test]
    fn regression_mint_rejects_symlinked_destination_file() {
        assert_mint_rejects_destination_symlink("weasel.yaml", "weasel.yaml");
    }

    fn assert_mint_archive_preserves_live_directory(archive_prefix: &str) {
        let root = tempfile::tempdir().expect("create fixture root");
        let incoming = root.path().join("incoming");
        let live = root.path().join("live");
        let preserved = [
            ("rime_mint.custom.yaml", "custom schema patch"),
            ("weasel.custom.yaml", "custom skin patch"),
            ("terra_pinyin.userdb/learned.txt", "learned words"),
            ("installation.yaml", "installation identity"),
            ("user.yaml", "user preferences"),
            ("custom_phrase.txt", "custom phrases"),
            (MODEL_FILE, "installed language model"),
            ("weasel.yaml", "excluded app configuration"),
            ("lua/protected.lua", "excluded lua customization"),
            ("lua/user.custom.yaml", "nested custom patch"),
            ("dicts/private.userdb/learned.txt", "nested learned words"),
            ("lua/retained.lua", "local lua module"),
            ("dicts/local.dict.yaml", "local dictionary"),
            ("unrelated.txt", "unrelated user file"),
        ];
        for (relative, content) in preserved {
            let path = live.join(relative);
            std::fs::create_dir_all(path.parent().expect("fixture file parent"))
                .expect("create live fixture directory");
            std::fs::write(path, content).expect("write live fixture file");
        }
        for relative in [
            "rime_mint.schema.yaml",
            "lua/runtime.lua",
            "dicts/runtime.dict.yaml",
        ] {
            std::fs::write(live.join(relative), "old distribution")
                .expect("write old distribution file");
        }

        let archive_path = root.path().join(MINT_ARCHIVE);
        let archive_file = std::fs::File::create(&archive_path).expect("create fixture archive");
        let mut archive = zip::ZipWriter::new(archive_file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (relative, content) in [
            ("rime_mint.schema.yaml", "updated schema"),
            ("lua/runtime.lua", "updated lua module"),
            ("dicts/runtime.dict.yaml", "updated dictionary"),
            ("opencc/config.json", "incoming opencc configuration"),
            ("rime_mint.custom.yaml", "incoming custom patch"),
            ("weasel.custom.yaml", "incoming skin patch"),
            ("terra_pinyin.userdb/learned.txt", "incoming learned words"),
            ("installation.yaml", "incoming installation identity"),
            ("user.yaml", "incoming preferences"),
            ("custom_phrase.txt", "incoming phrases"),
            (MODEL_FILE, "incoming model"),
            ("weasel.yaml", "incoming app configuration"),
            ("lua/protected.lua", "incoming lua customization"),
            ("lua/user.custom.yaml", "incoming nested custom patch"),
            ("dicts/private.userdb/learned.txt", "incoming nested words"),
            ("README.md", "repository readme"),
            (".github/workflows/ci.yaml", "repository ci"),
            ("plum/install.sh", "repository installer"),
        ] {
            archive
                .start_file(format!("{archive_prefix}{relative}"), options)
                .expect("start fixture archive file");
            archive
                .write_all(content.as_bytes())
                .expect("write fixture archive file");
        }
        archive.finish().expect("finish fixture archive");

        let config = Config {
            user_data_policy: "preserve".into(),
            exclude_files: vec!["weasel.yaml".into(), "protected.lua".into()],
            ..Config::default()
        };
        let exclude_patterns = crate::config::effective_exclude_patterns(&config);
        let user_data_behavior = user_data_behavior_for_config(&config);
        crate::fileutil::extract::extract_zip(
            &archive_path,
            &incoming,
            user_data_behavior,
            &exclude_patterns,
        )
        .expect("extract fixture archive");
        install_mint_distribution(&incoming, &live, user_data_behavior, &exclude_patterns)
            .expect("install fixture distribution");

        for (relative, expected) in preserved {
            let actual = std::fs::read_to_string(live.join(relative))
                .unwrap_or_else(|error| panic!("preserved file {relative}: {error}"));
            assert_eq!(actual, expected, "preserved file {relative}");
        }
        for (relative, expected) in [
            ("rime_mint.schema.yaml", "updated schema"),
            ("lua/runtime.lua", "updated lua module"),
            ("dicts/runtime.dict.yaml", "updated dictionary"),
            ("opencc/config.json", "incoming opencc configuration"),
        ] {
            assert_eq!(
                std::fs::read_to_string(live.join(relative)).expect("read installed asset"),
                expected,
                "installed asset {relative}"
            );
        }
        for relative in ["README.md", ".github", "plum", "oh-my-rime-main"] {
            assert!(!live.join(relative).exists(), "filtered entry {relative}");
        }
    }

    #[test]
    fn flat_mint_archive_preserves_populated_live_directory() {
        assert_mint_archive_preserves_live_directory("");
    }

    #[test]
    fn wrapped_mint_archive_preserves_populated_live_directory() {
        assert_mint_archive_preserves_live_directory("oh-my-rime-main/");
    }

    #[test]
    fn discard_policy_overwrites_existing_incoming_assets() {
        let root = tempfile::tempdir().expect("create fixture root");
        let incoming = root.path().join("incoming");
        let live = root.path().join("live");
        for directory in [&incoming, &live] {
            std::fs::create_dir_all(directory.join("lua")).expect("create fixture directory");
        }
        for relative in ["weasel.yaml", "lua/user.custom.yaml"] {
            std::fs::write(incoming.join(relative), "incoming").expect("write incoming asset");
            std::fs::write(live.join(relative), "existing").expect("write existing asset");
        }
        let config = Config {
            user_data_policy: "discard".into(),
            exclude_files: vec!["weasel.yaml".into()],
            ..Config::default()
        };

        install_mint_distribution(
            &incoming,
            &live,
            user_data_behavior_for_config(&config),
            &crate::config::effective_exclude_patterns(&config),
        )
        .expect("install distribution with discard policy");

        for relative in ["weasel.yaml", "lua/user.custom.yaml"] {
            assert_eq!(
                std::fs::read_to_string(live.join(relative)).expect("read installed asset"),
                "incoming",
                "discarded existing asset {relative}"
            );
        }
    }

    #[test]
    fn mint_staging_directories_are_unique_and_cleaned_up() {
        let root = tempfile::tempdir().expect("create fixture root");
        let first = tempfile::tempdir_in(root.path()).expect("create first staging directory");
        let second = tempfile::tempdir_in(root.path()).expect("create second staging directory");
        assert_ne!(first.path(), second.path());
        let first_path = first.path().to_path_buf();
        std::fs::write(first.path().join("asset.yaml"), "incoming").expect("write staged asset");

        drop(first);

        assert!(!first_path.exists(), "staging directory should be removed");
        assert!(
            second.path().exists(),
            "other staging directory should remain"
        );
    }

    #[test]
    fn filter_mint_distribution_keeps_rime_assets_only() {
        let root = tempfile::tempdir().expect("create fixture root");
        let dir = root.path();
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

        filter_mint_distribution(dir).expect("filter mint distribution");

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
