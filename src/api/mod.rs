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
            headers.insert(
                AUTHORIZATION,
                format!("Bearer {}", self.github_token).parse().unwrap(),
            );
        }
        headers
    }

    fn cnb_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, "application/vnd.cnb.web+json".parse().unwrap());
        if !self.github_token.is_empty() {
            headers.insert(
                AUTHORIZATION,
                format!("Bearer {}", self.github_token).parse().unwrap(),
            );
        }
        headers
    }

    // ── GitHub Releases ──

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

        let resp = self
            .http
            .get(&url)
            .headers(self.github_headers())
            .send()
            .await
            .context("GitHub API 请求失败")?;

        if !resp.status().is_success() {
            anyhow::bail!("GitHub API 返回 {}", resp.status());
        }

        if tag.is_empty() {
            let releases: Vec<GitHubRelease> = resp.json().await?;
            Ok(releases)
        } else {
            let release: GitHubRelease = resp.json().await?;
            Ok(vec![release])
        }
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
        let resp = self
            .http
            .get(&url)
            .headers(self.cnb_headers())
            .send()
            .await
            .context("CNB API 请求失败")?;

        if !resp.status().is_success() {
            anyhow::bail!("CNB API 返回 {}", resp.status());
        }

        let release: GitHubRelease = resp.json().await?;
        Ok(release)
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

    /// 流式下载到文件，支持进度回调
    pub async fn download_file(
        &self,
        url: &str,
        dest: &std::path::Path,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<()> {
        let resp = self.http.get(url).send().await?;
        let total = resp.content_length();

        let mut file = tokio::fs::File::create(dest).await?;
        let mut stream = resp.bytes_stream();
        let mut downloaded: u64 = 0;

        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("下载中断")?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            progress(downloaded, total);
        }

        file.flush().await?;
        Ok(())
    }

    pub fn use_mirror(&self) -> bool {
        self.use_mirror
    }
}
