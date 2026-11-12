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
            return Err(github_status_error(self, resp.status(), resp.headers()));
        }

        let branch_info: serde_json::Value = resp.json().await?;
        let sha = branch_info
            .get("commit")
            .and_then(|v| v.get("sha"))
            .and_then(|v| v.as_str())
            .with_context(|| t.t("api.github_branch_missing_sha").to_string())?;

        Ok(UpdateInfo {
            name: archive_name.into(),
            url: format!("https://codeload.github.com/{owner}/{repo}/zip/{sha}"),
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
        let url = if tag.is_empty() {
            format!("{GITHUB_API}/repos/{owner}/{repo}/releases?per_page=30")
        } else {
            format!("{GITHUB_API}/repos/{owner}/{repo}/releases/tags/{tag}")
        };

        self.fetch_github_releases_url(&url, !tag.is_empty(), cancel)
            .await
    }

    async fn fetch_github_releases_url(
        &self,
        url: &str,
        single_release: bool,
        cancel: Option<&CancelSignal>,
    ) -> Result<Vec<GitHubRelease>> {
        let t = L10n::new(self.lang);
        let mut last_err = None;
        for attempt in 0..3 {
            if let Some(signal) = cancel {
                signal.checkpoint()?;
            }
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(1 << attempt);
                Client::cancellable_sleep(delay, cancel).await?;
            }

            let resp = self.send_get(url, self.github_headers(), cancel).await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    if !single_release {
                        let releases: Vec<GitHubRelease> = r.json().await?;
                        return Ok(releases);
                    } else {
                        let release: GitHubRelease = r.json().await?;
                        return Ok(vec![release]);
                    }
                }
                Ok(r) => {
                    let error = github_status_error(self, r.status(), r.headers());
                    if matches!(r.status().as_u16(), 401 | 403 | 429) {
                        return Err(error);
                    }
                    last_err = Some(error);
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

pub(super) fn github_status_error(
    client: &Client,
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
) -> anyhow::Error {
    let t = L10n::new(client.lang);
    let remaining = headers
        .get("x-ratelimit-remaining")
        .and_then(|value| value.to_str().ok());
    let hint = match status.as_u16() {
        401 => Some("api.github_unauthorized"),
        403 | 429 if remaining == Some("0") => Some(if client.github_token.is_empty() {
            "api.github_rate_limit_anonymous"
        } else {
            "api.github_rate_limit_authenticated"
        }),
        429 => Some("api.github_throttled"),
        403 => Some("api.github_forbidden"),
        _ => None,
    };
    let mut message = format!("{} {status}", t.t("api.github_status"));
    if let Some(hint) = hint {
        message.push_str(&format!("; {}", t.t(hint)));
    }
    if matches!(status.as_u16(), 403 | 429) {
        if client.github_token.is_empty() {
            message.push_str(&format!("; {}", t.t("api.github_auth_help")));
        }
        if remaining == Some("0") {
            if let Some(reset) = headers
                .get("x-ratelimit-reset")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<i64>().ok())
                .and_then(|value| chrono::DateTime::<chrono::Utc>::from_timestamp(value, 0))
            {
                message.push_str(&format!(
                    "; {}: {}",
                    t.t("api.github_rate_reset"),
                    reset.to_rfc3339()
                ));
            }
        }
        if let Some(seconds) = headers
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
        {
            message.push_str(&format!("; {}: {seconds}", t.t("api.github_retry_after")));
        }
    }
    anyhow::anyhow!(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client(token: &str) -> Client {
        let mut client = Client::new(&Config {
            github_token: "fixture-token".into(),
            language: "en".into(),
            ..Config::default()
        })
        .unwrap();
        client.github_token = token.into();
        client.http = reqwest::Client::builder().no_proxy().build().unwrap();
        client
    }

    #[test]
    fn github_limits_explain_authentication_and_reset_without_leaking_tokens() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ratelimit-remaining", "0".parse().unwrap());
        headers.insert("x-ratelimit-reset", "1794268800".parse().unwrap());
        let anonymous =
            github_status_error(&test_client(""), reqwest::StatusCode::FORBIDDEN, &headers)
                .to_string();
        assert!(anonymous.contains("anonymous API rate limit"));
        assert!(anonymous.contains("GH_TOKEN/GITHUB_TOKEN"));
        assert!(anonymous.contains("2026-11-10T00:00:00+00:00"));
        let authenticated = github_status_error(
            &test_client("fixture-secret"),
            reqwest::StatusCode::FORBIDDEN,
            &headers,
        )
        .to_string();
        assert!(authenticated.contains("current token"));
        assert!(!authenticated.contains("fixture-secret"));
    }

    #[test]
    fn forbidden_without_limit_evidence_is_not_reported_as_quota_exhaustion() {
        let error = github_status_error(
            &test_client(""),
            reqwest::StatusCode::FORBIDDEN,
            &reqwest::header::HeaderMap::new(),
        )
        .to_string();
        assert!(error.contains("permissions"));
        assert!(!error.contains("exhausted"));
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("retry-after", "60".parse().unwrap());
        let error = github_status_error(
            &test_client(""),
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            &headers,
        )
        .to_string();
        assert!(error.contains("Wait at least (seconds): 60"));
    }

    #[tokio::test]
    async fn github_auth_failures_are_not_retried() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::time::Duration;
        for status in [401, 403, 429] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0 && request.len() < 8192);
                    request.extend_from_slice(&buffer[..count]);
                }
                assert!(String::from_utf8_lossy(&request)
                    .contains("authorization: Bearer fixture-token"));
                write!(
                    stream,
                    "HTTP/1.1 {status} Denied\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
            });
            let client = test_client("fixture-token");
            let response = tokio::time::timeout(
                Duration::from_secs(1),
                client.fetch_github_releases_url(&format!("http://{address}"), false, None),
            )
            .await
            .expect("authentication and rate-limit errors return immediately");
            let error = response.unwrap_err().to_string();
            assert!(error.contains(&status.to_string()));
            assert!(!error.contains("fixture-token"));
            server.join().unwrap();
        }
    }

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
