use super::Client;
use crate::i18n::L10n;
use crate::types::*;
use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CnbReleaseEnvelope {
    release: CnbRelease,
}

#[derive(Debug, Deserialize)]
struct CnbRelease {
    tag_ref: String,
    body: String,
    assets: Vec<CnbAsset>,
}

#[derive(Debug, Deserialize)]
struct CnbAsset {
    name: String,
    path: String,
    updated_at: Option<String>,
    hash_algo: Option<String>,
    hash_value: Option<String>,
    size_in_byte: i64,
}

#[derive(Debug, Deserialize)]
struct CnbTagsPage {
    tags: Vec<CnbTagEntry>,
    tag_count: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct CnbTagEntry {
    tag: String,
    has_release: bool,
}

impl Client {
    /// 获取 GitHub 分支头信息并构造归档下载信息
    pub async fn fetch_github_branch_archive(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        archive_name: &str,
        cancel: Option<&CancelSignal>,
    ) -> Result<UpdateInfo> {
        let t = L10n::new(self.lang);
        if let Some(signal) = cancel {
            signal.checkpoint()?;
        }
        let url = format!("{GITHUB_API}/repos/{owner}/{repo}/branches/{branch}");
        let resp = self.send_get(&url, self.github_headers(), cancel).await?;

        if !resp.status().is_success() {
            anyhow::bail!("{} {}", t.t("api.github_branch_status"), resp.status());
        }

        let branch_info: serde_json::Value = resp.json().await?;
        let sha = branch_info
            .get("commit")
            .and_then(|v| v.get("sha"))
            .and_then(|v| v.as_str())
            .with_context(|| t.t("api.github_branch_missing_sha").to_string())?;

        Ok(UpdateInfo {
            name: archive_name.into(),
            url: format!("https://github.com/{owner}/{repo}/archive/refs/heads/{branch}.zip"),
            update_time: String::new(),
            tag: sha.into(),
            description: format!("{owner}/{repo}@{branch}"),
            sha256: String::new(),
            size: 0,
        })
    }

    /// 获取 GitHub Releases (可选 tag 过滤)
    pub async fn fetch_github_releases(
        &self,
        owner: &str,
        repo: &str,
        tag: &str,
        cancel: Option<&CancelSignal>,
    ) -> Result<Vec<GitHubRelease>> {
        let t = L10n::new(self.lang);
        let url = if tag.is_empty() {
            format!("{GITHUB_API}/repos/{owner}/{repo}/releases?per_page=30")
        } else {
            format!("{GITHUB_API}/repos/{owner}/{repo}/releases/tags/{tag}")
        };

        let mut last_err = None;
        for attempt in 0..3 {
            if let Some(signal) = cancel {
                signal.checkpoint()?;
            }
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(1 << attempt);
                Client::cancellable_sleep(delay, cancel).await?;
            }

            let resp = self.send_get(&url, self.github_headers(), cancel).await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    if tag.is_empty() {
                        let releases: Vec<GitHubRelease> = r.json().await?;
                        return Ok(releases);
                    } else {
                        let release: GitHubRelease = r.json().await?;
                        return Ok(vec![release]);
                    }
                }
                Ok(r) => {
                    last_err = Some(anyhow::anyhow!(
                        "{} {}",
                        t.t("api.github_status"),
                        r.status()
                    ));
                }
                Err(e) => {
                    last_err = Some(anyhow::anyhow!("{}: {e}", t.t("api.github_request_failed")));
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("{}", t.t("api.github_request_failed"))))
    }

    /// 获取 CNB Release
    pub async fn fetch_cnb_release(
        &self,
        owner: &str,
        repo: &str,
        tag: &str,
        cancel: Option<&CancelSignal>,
    ) -> Result<GitHubRelease> {
        let t = L10n::new(self.lang);
        let url = format!("{CNB_BASE}/{owner}/{repo}/-/releases/tags/{tag}");

        let mut last_err = None;
        for attempt in 0..3 {
            if let Some(signal) = cancel {
                signal.checkpoint()?;
            }
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(1 << attempt);
                Client::cancellable_sleep(delay, cancel).await?;
            }

            let resp = self.send_get(&url, self.cnb_headers(), cancel).await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    let release: CnbReleaseEnvelope = r.json().await?;
                    return Ok(convert_cnb_release(release.release));
                }
                Ok(r) => {
                    last_err = Some(anyhow::anyhow!("{} {}", t.t("api.cnb_status"), r.status()));
                }
                Err(e) => {
                    last_err = Some(anyhow::anyhow!("{}: {e}", t.t("api.cnb_request_failed")));
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("{}", t.t("api.cnb_request_failed"))))
    }

    /// 获取 CNB 最新 tag
    #[allow(dead_code)]
    pub async fn fetch_cnb_latest_tag(
        &self,
        owner: &str,
        repo: &str,
        cancel: Option<&CancelSignal>,
    ) -> Result<String> {
        let t = L10n::new(self.lang);
        let (tags, _) = self
            .fetch_cnb_release_tags_page(owner, repo, 1, cancel)
            .await?;
        tags.into_iter()
            .next()
            .with_context(|| t.t("api.cnb_no_release").to_string())
    }

    pub async fn fetch_cnb_release_tags_page(
        &self,
        owner: &str,
        repo: &str,
        page: usize,
        cancel: Option<&CancelSignal>,
    ) -> Result<(Vec<String>, usize)> {
        let t = L10n::new(self.lang);
        let url = format!("{CNB_BASE}/{owner}/{repo}/-/git/tags?page={page}");

        let mut last_err = None;
        for attempt in 0..3 {
            if let Some(signal) = cancel {
                signal.checkpoint()?;
            }
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(1 << attempt);
                Client::cancellable_sleep(delay, cancel).await?;
            }

            let resp = self.send_get(&url, self.cnb_headers(), cancel).await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    let tags_page: CnbTagsPage = r.json().await?;
                    let mut tags = Vec::new();
                    for tag in tags_page.tags {
                        if tag.has_release {
                            tags.push(normalize_cnb_tag(&tag.tag));
                        }
                    }
                    let total_pages = match tags_page.tag_count {
                        Some(total) if !tags.is_empty() => total.div_ceil(tags.len()),
                        _ => 1,
                    };
                    return Ok((tags, total_pages.max(1)));
                }
                Ok(r) => {
                    last_err = Some(anyhow::anyhow!("{} {}", t.t("api.cnb_status"), r.status()));
                }
                Err(e) => {
                    last_err = Some(anyhow::anyhow!("{}: {e}", t.t("api.cnb_request_failed")));
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("{}", t.t("api.cnb_request_failed"))))
    }

    pub async fn find_latest_cnb_asset_info(
        &self,
        owner: &str,
        repo: &str,
        matches: impl Fn(&str) -> bool,
        fallback_tag: Option<&str>,
        cancel: Option<&CancelSignal>,
    ) -> Result<UpdateInfo> {
        let mut page = 1usize;
        let mut total_pages = 1usize;
        while page <= total_pages {
            let (tags, pages) = self
                .fetch_cnb_release_tags_page(owner, repo, page, cancel)
                .await?;
            if page == 1 {
                total_pages = pages;
            }

            for tag in tags {
                if fallback_tag.is_some_and(|fallback| fallback == tag) {
                    continue;
                }

                let release = self.fetch_cnb_release(owner, repo, &tag, cancel).await?;
                if let Some(info) = find_matching_asset_info(&release, &matches) {
                    return Ok(info);
                }
            }

            page += 1;
        }

        if let Some(tag) = fallback_tag {
            let release = self.fetch_cnb_release(owner, repo, tag, cancel).await?;
            if let Some(info) = find_matching_asset_info(&release, &matches) {
                return Ok(info);
            }
        }

        anyhow::bail!("no matching CNB asset found")
    }
}

fn convert_cnb_release(release: CnbRelease) -> GitHubRelease {
    GitHubRelease {
        tag_name: normalize_cnb_tag(&release.tag_ref),
        body: release.body,
        assets: release.assets.into_iter().map(convert_cnb_asset).collect(),
        published_at: None,
    }
}

fn convert_cnb_asset(asset: CnbAsset) -> GitHubAsset {
    let sha256 = match asset.hash_algo.as_deref() {
        Some(algo) if algo.eq_ignore_ascii_case("sha256") => asset.hash_value,
        _ => None,
    };
    GitHubAsset {
        name: asset.name,
        browser_download_url: format!("{CNB_BASE}{}", asset.path),
        updated_at: asset.updated_at,
        size: asset.size_in_byte,
        sha256,
        digest: None,
    }
}

fn normalize_cnb_tag(tag: &str) -> String {
    tag.rsplit('/').next().unwrap_or(tag).to_string()
}

fn find_matching_asset_info(
    release: &GitHubRelease,
    matches: &impl Fn(&str) -> bool,
) -> Option<UpdateInfo> {
    release
        .assets
        .iter()
        .find(|asset| matches(&asset.name))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_cnb_tag_strips_refs_prefix() {
        assert_eq!(normalize_cnb_tag("refs/tags/v1.0.0"), "v1.0.0");
        assert_eq!(normalize_cnb_tag("v2.0.0"), "v2.0.0");
    }

    #[test]
    fn convert_cnb_asset_preserves_sha256_hash() {
        let asset = CnbAsset {
            name: "asset.zip".into(),
            path: "/download/asset.zip".into(),
            updated_at: Some("2026-01-01T00:00:00Z".into()),
            hash_algo: Some("sha256".into()),
            hash_value: Some("abc".into()),
            size_in_byte: 123,
        };

        let converted = convert_cnb_asset(asset);
        assert_eq!(converted.sha256.as_deref(), Some("abc"));
        assert_eq!(
            converted.browser_download_url,
            "https://cnb.cool/download/asset.zip"
        );
    }
}
