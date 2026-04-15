use crate::types::*;
use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, ACCEPT, AUTHORIZATION};
use reqwest::Proxy;

pub struct Client {
    http: reqwest::Client,
    github_token: String,
    use_mirror: bool,
}

impl Client {
    pub fn new(config: &Config) -> Result<Self> {
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("snout/0.1");

        // 代理
        if config.proxy_enabled {
            let proxy = match config.proxy_type.as_str() {
                "http" | "https" => Proxy::all(format!("http://{}", config.proxy_address))?,
                "socks5" => Proxy::all(format!("socks5://{}", config.proxy_address))?,
                _ => {
                    eprintln!("⚠️ 未知代理类型: {}", config.proxy_type);
                    return Err(anyhow::anyhow!("未知代理类型"));
                }
            };
            builder = builder.proxy(proxy);
        }

        Ok(Self {
            http: builder.build()?,
            github_token: config.github_token.clone(),
            use_mirror: config.use_mirror,
        })
    }

    /// 无超时的 client (用于大文件下载)
    pub fn new_download_client(config: &Config) -> Result<Self> {
        let mut builder = reqwest::Client::builder().user_agent("snout/0.1");

        if config.proxy_enabled {
            let proxy = match config.proxy_type.as_str() {
                "http" | "https" => Proxy::all(format!("http://{}", config.proxy_address))?,
                "socks5" => Proxy::all(format!("socks5://{}", config.proxy_address))?,
                _ => return Err(anyhow::anyhow!("未知代理类型")),
            };
            builder = builder.proxy(proxy);
        }

        Ok(Self {
            http: builder.build()?,
            github_token: config.github_token.clone(),
            use_mirror: config.use_mirror,
        })
    }

    fn github_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if !self.github_token.is_empty() {
            if let Ok(val) = format!("Bearer {}", self.github_token).parse() {
                headers.insert(AUTHORIZATION, val);
            }
        }
        headers
    }

    fn cnb_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(val) = "application/vnd.cnb.web+json".parse() {
            headers.insert(ACCEPT, val);
        }
        if !self.github_token.is_empty() {
            if let Ok(val) = format!("Bearer {}", self.github_token).parse() {
                headers.insert(AUTHORIZATION, val);
            }
        }
        headers
    }

    // ── GitHub Releases ──

    /// 获取 GitHub 分支头信息并构造归档下载信息
    pub async fn fetch_github_branch_archive(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        archive_name: &str,
    ) -> Result<UpdateInfo> {
        let url = format!("{GITHUB_API}/repos/{owner}/{repo}/branches/{branch}");

        let resp = self
            .http
            .get(&url)
            .headers(self.github_headers())
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("GitHub Branch API 返回 {}", resp.status());
        }

        let branch_info: serde_json::Value = resp.json().await?;
        let sha = branch_info
            .get("commit")
            .and_then(|v| v.get("sha"))
            .and_then(|v| v.as_str())
            .context("GitHub Branch API 缺少 commit.sha")?;

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
    ) -> Result<Vec<GitHubRelease>> {
        let url = if tag.is_empty() {
            format!("{GITHUB_API}/repos/{owner}/{repo}/releases?per_page=30")
        } else {
            format!("{GITHUB_API}/repos/{owner}/{repo}/releases/tags/{tag}")
        };

        let mut last_err = None;
        for attempt in 0..3 {
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(1 << attempt);
                tokio::time::sleep(delay).await;
            }

            let resp = self
                .http
                .get(&url)
                .headers(self.github_headers())
                .send()
                .await;

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
                    last_err = Some(anyhow::anyhow!("GitHub API 返回 {}", r.status()));
                }
                Err(e) => {
                    last_err = Some(anyhow::anyhow!("GitHub API 请求失败: {e}"));
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("GitHub API 请求失败")))
    }

    // ── CNB 镜像 ──

    /// 获取 CNB Release
    pub async fn fetch_cnb_release(
        &self,
        owner: &str,
        repo: &str,
        tag: &str,
    ) -> Result<GitHubRelease> {
        let url = format!("{CNB_BASE}/{owner}/{repo}/-/releases/tags/{tag}");

        let mut last_err = None;
        for attempt in 0..3 {
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(1 << attempt);
                tokio::time::sleep(delay).await;
            }

            let resp = self.http.get(&url).headers(self.cnb_headers()).send().await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    let release: GitHubRelease = r.json().await?;
                    return Ok(release);
                }
                Ok(r) => {
                    last_err = Some(anyhow::anyhow!("CNB API 返回 {}", r.status()));
                }
                Err(e) => {
                    last_err = Some(anyhow::anyhow!("CNB API 请求失败: {e}"));
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("CNB API 请求失败")))
    }

    /// 获取 CNB 最新 tag
    #[allow(dead_code)]
    pub async fn fetch_cnb_latest_tag(&self, owner: &str, repo: &str) -> Result<String> {
        let url = format!("{CNB_BASE}/{owner}/{repo}/-/releases?page=1&per_page=1");
        let resp = self
            .http
            .get(&url)
            .headers(self.cnb_headers())
            .send()
            .await?;

        let releases: Vec<GitHubRelease> = resp.json().await?;
        releases
            .into_iter()
            .next()
            .map(|r| r.tag_name)
            .context("CNB 无 release")
    }

    // ── 通用下载 ──

    /// 流式下载到文件，支持进度回调和重试
    pub async fn download_file(
        &self,
        url: &str,
        dest: &std::path::Path,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<()> {
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        let mut last_err = None;
        for attempt in 0..3 {
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(1 << attempt);
                eprintln!(
                    "⚠️ 下载失败，{}s 后重试 ({}/3)...",
                    delay.as_secs(),
                    attempt + 1
                );
                tokio::time::sleep(delay).await;
            }

            // 每次重试重新创建文件 (截断)
            let resp = match self.http.get(url).send().await {
                Ok(r) => r,
                Err(e) => {
                    last_err = Some(anyhow::anyhow!("下载请求失败: {e}"));
                    continue;
                }
            };

            if !resp.status().is_success() {
                last_err = Some(anyhow::anyhow!("下载失败: HTTP {}", resp.status()));
                continue;
            }

            let total = resp.content_length();
            let mut file = tokio::fs::File::create(dest).await?;
            let mut stream = resp.bytes_stream();
            let mut downloaded: u64 = 0;
            let mut stream_err = None;

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(c) => {
                        if let Err(e) = file.write_all(&c).await {
                            stream_err = Some(e);
                            break;
                        }
                        downloaded += c.len() as u64;
                        progress(downloaded, total);
                    }
                    Err(e) => {
                        stream_err = Some(std::io::Error::other(e));
                        break;
                    }
                }
            }

            if let Some(e) = stream_err {
                last_err = Some(anyhow::anyhow!("下载中断: {e}"));
                continue;
            }

            file.flush().await?;
            return Ok(());
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("下载失败")))
    }

    pub fn use_mirror(&self) -> bool {
        self.use_mirror
    }
}
