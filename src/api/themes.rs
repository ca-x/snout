//! Discover Fcitx5 theme repositories through GitHub topics.
use super::{releases::github_status_error, Client};
use crate::types::CancelSignal;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A repository discovered through the supported Fcitx5 topics.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ThemeRepository {
    pub id: u64,
    pub full_name: String,
    pub default_branch: String,
    pub description: Option<String>,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub stargazers_count: u64,
}

#[derive(Deserialize)]
struct SearchPage {
    items: Vec<ThemeRepository>,
    total_count: usize,
    incomplete_results: bool,
}

fn merge_repositories(
    repositories: impl IntoIterator<Item = ThemeRepository>,
) -> Vec<ThemeRepository> {
    let mut unique: HashMap<u64, ThemeRepository> = HashMap::new();
    for repository in repositories {
        let entry = unique
            .entry(repository.id)
            .or_insert_with(|| repository.clone());
        for topic in repository.topics {
            if !entry.topics.contains(&topic) {
                entry.topics.push(topic);
            }
        }
    }
    let mut repositories: Vec<_> = unique.into_values().collect();
    repositories.sort_by(|a, b| {
        b.stargazers_count
            .cmp(&a.stargazers_count)
            .then_with(|| a.full_name.to_lowercase().cmp(&b.full_name.to_lowercase()))
    });
    repositories
}

impl Client {
    /// Search both Fcitx5 topics, paginate results and deduplicate by repository ID.
    pub async fn fetch_fcitx5_repositories(
        &self,
        cancel: &CancelSignal,
    ) -> Result<Vec<ThemeRepository>> {
        let mut repositories = Vec::new();
        for topic in ["fcitx5-theme", "fcitx5-skin"] {
            for page in 1..=10 {
                cancel.checkpoint()?;
                let url = format!("https://api.github.com/search/repositories?q=topic%3A{topic}&sort=stars&per_page=100&page={page}");
                let response = self
                    .send_get(&url, self.github_headers(), Some(cancel))
                    .await?;
                if !response.status().is_success() {
                    return Err(github_status_error(
                        self,
                        response.status(),
                        response.headers(),
                    ));
                }
                let result: SearchPage =
                    Self::await_with_cancel(response.json(), Some(cancel)).await?;
                ensure!(!result.incomplete_results && result.total_count <= 1000, "GitHub search results are incomplete; retry later / GitHub 搜索结果不完整，请稍后重试");
                let finished = page * 100 >= result.total_count;
                repositories.extend(result.items);
                if finished {
                    break;
                }
            }
        }
        Ok(merge_repositories(repositories))
    }

    /// Download a repository snapshot from GitHub's archive service with a size limit.
    pub async fn download_theme_repository(
        &self,
        repository: &ThemeRepository,
        cancel: &CancelSignal,
    ) -> Result<tempfile::NamedTempFile> {
        use futures_util::StreamExt;
        use std::io::Write;
        let mut url = reqwest::Url::parse("https://codeload.github.com/")?;
        let parts: Vec<_> = repository.full_name.split('/').collect();
        ensure!(
            parts.len() == 2
                && parts.iter().all(|part| !part.is_empty()
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte)))
                && !repository.default_branch.is_empty(),
            "Invalid theme repository"
        );
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("Invalid archive URL"))?
            .extend([parts[0], parts[1], "zip", &repository.default_branch]);
        let response = Self::await_with_cancel(
            self.http
                .get(url)
                .timeout(std::time::Duration::from_secs(300))
                .send(),
            Some(cancel),
        )
        .await?;
        ensure!(
            response.status().is_success(),
            "Theme archive HTTP {} / 主题下载失败",
            response.status()
        );
        const LIMIT: u64 = 80 * 1024 * 1024;
        ensure!(
            response.content_length().unwrap_or(0) <= LIMIT,
            "Theme archive exceeds 80 MiB / 主题包过大"
        );
        let mut archive = tempfile::NamedTempFile::new()?;
        let mut stream = response.bytes_stream();
        let mut size = 0;
        loop {
            let next = tokio::select! {
                chunk = stream.next() => chunk,
                result = Self::wait_for_cancel(cancel) => { result?; unreachable!() }
            };
            let Some(chunk) = next else { break };
            let chunk = chunk?;
            size += chunk.len() as u64;
            ensure!(size <= LIMIT, "Theme archive exceeds 80 MiB / 主题包过大");
            archive.write_all(&chunk)?;
        }
        Ok(archive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn topics_merge_by_identity_and_keep_stable_star_order() {
        let repository = |id, name: &str, stars, topic: &str| ThemeRepository {
            id,
            full_name: name.into(),
            stargazers_count: stars,
            topics: vec![topic.into()],
            default_branch: "main".into(),
            description: None,
        };
        let result = merge_repositories([
            repository(1, "owner/theme", 3, "fcitx5-theme"),
            repository(1, "Owner/Theme", 3, "fcitx5-skin"),
            repository(2, "other/theme", 5, "fcitx5-skin"),
        ]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, 2);
        assert_eq!(result[1].topics.len(), 2);
    }
}
