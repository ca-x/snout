use super::base::{BaseUpdater, UpdateResult};
use crate::i18n::{L10n, Lang};
use crate::types::*;
use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

/// 薄荷方案更新器
pub struct MintUpdater {
    pub base: BaseUpdater,
}

impl MintUpdater {
    /// 检查方案更新（主分支归档）
    pub async fn check_scheme_update(&self) -> Result<UpdateInfo> {
        self.base
            .client
            .fetch_github_branch_archive(MINT_OWNER, MINT_REPO, MINT_BRANCH, MINT_ARCHIVE)
            .await
    }

    /// 更新方案
    pub async fn update_scheme(
        &self,
        config: &crate::types::Config,
        mut progress: impl FnMut(&str, f64),
    ) -> Result<UpdateResult> {
        let t = L10n::new(Lang::from_str(&config.language));
        progress(t.t("update.mint_scheme_checking"), 0.05);

        let info = self.check_scheme_update().await?;
        let record_path = self.base.cache_dir.join("scheme_record.json");
        let local = BaseUpdater::load_record(&record_path);

        let scheme_switched = local
            .as_ref()
            .map(|r| r.name != MINT_ARCHIVE)
            .unwrap_or(false);

        if !scheme_switched && !BaseUpdater::needs_update(local.as_ref(), &info) {
            progress(t.t("update.up_to_date"), 1.0);
            return Ok(BaseUpdater::success_result(
                t.t("update.scheme"),
                &info.tag,
                &info.tag,
                t.t("update.up_to_date"),
            ));
        }

        if scheme_switched {
            progress(t.t("update.scheme_switched"), 0.05);
        }

        self.base
            .download_and_extract(&info, config, &self.base.rime_dir, &mut progress)
            .await?;

        crate::fileutil::extract::handle_nested_dir(&self.base.rime_dir, &info.name)?;
        filter_mint_distribution(&self.base.rime_dir)?;

        progress(t.t("update.saving"), 0.95);
        let record = UpdateRecord {
            name: info.name.clone(),
            update_time: info.update_time.clone(),
            tag: info.tag.clone(),
            apply_time: chrono::Utc::now().to_rfc3339(),
            sha256: info.sha256.clone(),
        };
        BaseUpdater::save_record(&record_path, &record)?;

        let build_dir = self.base.rime_dir.join("build");
        if build_dir.exists() {
            let _ = std::fs::remove_dir_all(&build_dir);
        }

        progress(t.t("update.mint_scheme_done"), 1.0);
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
        "plum",
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
        std::fs::write(dir.join("rime_mint.schema.yaml"), "").expect("write schema");
        std::fs::create_dir_all(dir.join("dicts")).expect("create dicts");
        std::fs::create_dir_all(dir.join(".github")).expect("create github");
        std::fs::write(dir.join("README.md"), "").expect("write readme");

        filter_mint_distribution(&dir).expect("filter mint distribution");

        assert!(dir.join("default.yaml").exists());
        assert!(dir.join("rime_mint.schema.yaml").exists());
        assert!(dir.join("dicts").exists());
        assert!(!dir.join(".github").exists());
        assert!(!dir.join("README.md").exists());

        std::fs::remove_dir_all(&dir).ok();
    }
}
