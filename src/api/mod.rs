mod download;
mod releases;

use crate::i18n::{L10n, Lang};
use crate::types::*;
use anyhow::Result;
use reqwest::header::{HeaderMap, ACCEPT, AUTHORIZATION};
use reqwest::Proxy;

pub struct Client {
    pub(crate) http: reqwest::Client,
    pub(crate) http_direct: reqwest::Client,
    pub(crate) github_token: String,
    use_mirror: bool,
    pub(crate) lang: Lang,
    has_proxy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxySource {
    Config,
    Environment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveProxy {
    pub source: ProxySource,
    pub url: String,
}

impl Client {
    pub(crate) async fn wait_for_cancel(cancel: &CancelSignal) -> Result<()> {
        loop {
            cancel.checkpoint()?;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    pub(crate) async fn await_with_cancel<T, F>(
        future: F,
        cancel: Option<&CancelSignal>,
    ) -> Result<T>
    where
        F: std::future::Future<Output = reqwest::Result<T>>,
    {
        if let Some(signal) = cancel {
            tokio::select! {
                result = future => result.map_err(Into::into),
                result = Self::wait_for_cancel(signal) => result.and_then(|_| unreachable!()),
            }
        } else {
            future.await.map_err(Into::into)
        }
    }

    pub(crate) async fn cancellable_sleep(
        delay: std::time::Duration,
        cancel: Option<&CancelSignal>,
    ) -> Result<()> {
        let start = std::time::Instant::now();
        while start.elapsed() < delay {
            if let Some(signal) = cancel {
                signal.checkpoint()?;
            }
            let remaining = delay.saturating_sub(start.elapsed());
            tokio::time::sleep(std::cmp::min(
                remaining,
                std::time::Duration::from_millis(100),
            ))
            .await;
        }
        Ok(())
    }

    pub fn new(config: &Config) -> Result<Self> {
        Self::new_internal(config, true)
    }

    pub fn new_download_client(config: &Config) -> Result<Self> {
        Self::new_internal(config, false)
    }

    fn new_internal(config: &Config, with_timeout: bool) -> Result<Self> {
        let t = L10n::new(Lang::from_str(&config.language));
        let mut builder = reqwest::Client::builder().user_agent("snout/0.1");
        let mut direct_builder = reqwest::Client::builder().user_agent("snout/0.1");

        if with_timeout {
            let timeout = std::time::Duration::from_secs(30);
            builder = builder.timeout(timeout);
            direct_builder = direct_builder.timeout(timeout);
        }

        let proxy_url = resolve_proxy_url(config, &t)?;
        if let Some(proxy_url) = &proxy_url {
            let proxy = Proxy::all(proxy_url)?;
            builder = builder.proxy(proxy);
        }
        direct_builder = direct_builder.no_proxy();

        Ok(Self {
            http: builder.build()?,
            http_direct: direct_builder.build()?,
            github_token: config.github_token.clone(),
            use_mirror: config.use_mirror,
            lang: Lang::from_str(&config.language),
            has_proxy: proxy_url.is_some(),
        })
    }

    pub(crate) fn github_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if !self.github_token.is_empty() {
            if let Ok(val) = format!("Bearer {}", self.github_token).parse() {
                headers.insert(AUTHORIZATION, val);
            }
        }
        headers
    }

    pub(crate) fn cnb_headers(&self) -> HeaderMap {
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

    pub fn use_mirror(&self) -> bool {
        self.use_mirror
    }

    pub(crate) fn has_proxy(&self) -> bool {
        self.has_proxy
    }

    pub(crate) fn is_mirror_url(&self, url: &str) -> bool {
        url.starts_with(CNB_BASE)
    }

    pub(crate) async fn send_get(
        &self,
        url: &str,
        headers: HeaderMap,
        cancel: Option<&CancelSignal>,
    ) -> Result<reqwest::Response> {
        let try_direct_first = self.has_proxy() && self.is_mirror_url(url);
        let mut direct_error = None;

        if try_direct_first {
            let direct = self.http_direct.get(url).headers(headers.clone()).send();
            match Self::await_with_cancel(direct, cancel).await {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    direct_error = Some(anyhow::anyhow!(
                        "direct mirror request returned {}",
                        response.status()
                    ));
                }
                Err(error) => direct_error = Some(error),
            }
        }

        let proxied = self.http.get(url).headers(headers).send();
        match Self::await_with_cancel(proxied, cancel).await {
            Ok(response) => Ok(response),
            Err(error) if direct_error.is_some() => Err(anyhow::anyhow!(
                "mirror request failed without proxy ({}) and with proxy ({error})",
                direct_error.expect("direct error")
            )),
            Err(error) => Err(error),
        }
    }
}

fn resolve_proxy_url(config: &Config, t: &L10n) -> Result<Option<String>> {
    if config.proxy_enabled {
        let proxy_url = match config.proxy_type.as_str() {
            "http" | "https" => format!("http://{}", config.proxy_address.trim()),
            "socks5" => format!("socks5://{}", config.proxy_address.trim()),
            _ => {
                eprintln!("⚠️ {}: {}", t.t("api.proxy_unknown"), config.proxy_type);
                return Err(anyhow::anyhow!("{}", t.t("api.proxy_unknown")));
            }
        };
        return Ok(Some(proxy_url));
    }

    Ok(proxy_url_from_env())
}

pub fn effective_proxy(config: &Config) -> Result<Option<EffectiveProxy>> {
    let t = L10n::new(Lang::from_str(&config.language));
    if config.proxy_enabled {
        let url = resolve_proxy_url(config, &t)?.unwrap_or_default();
        return Ok(Some(EffectiveProxy {
            source: ProxySource::Config,
            url,
        }));
    }

    Ok(proxy_url_from_env().map(|url| EffectiveProxy {
        source: ProxySource::Environment,
        url,
    }))
}

fn proxy_url_from_env() -> Option<String> {
    proxy_url_from_env_with(|key| std::env::var(key).ok())
}

fn proxy_url_from_env_with(lookup: impl Fn(&str) -> Option<String>) -> Option<String> {
    for key in [
        "https_proxy",
        "HTTPS_PROXY",
        "http_proxy",
        "HTTP_PROXY",
        "all_proxy",
        "ALL_PROXY",
    ] {
        let Some(value) = lookup(key) else {
            continue;
        };
        let normalized = normalize_proxy_env_value(key, &value)?;
        return Some(normalized);
    }
    None
}

fn normalize_proxy_env_value(key: &str, value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains("://") {
        return Some(trimmed.to_string());
    }
    if key.eq_ignore_ascii_case("all_proxy") {
        return None;
    }
    Some(format!("http://{trimmed}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    fn base_config() -> Config {
        Config {
            github_token: String::new(),
            proxy_enabled: false,
            proxy_type: "socks5".into(),
            proxy_address: "127.0.0.1:1080".into(),
            language: "en".into(),
            ..Config::default()
        }
    }

    #[test]
    fn client_new_accepts_supported_proxy_types() {
        for proxy_type in ["http", "https", "socks5"] {
            let mut config = base_config();
            config.proxy_enabled = true;
            config.proxy_type = proxy_type.into();
            assert!(Client::new(&config).is_ok(), "proxy_type={proxy_type}");
        }
    }

    #[test]
    fn client_new_rejects_unknown_proxy_type() {
        let mut config = base_config();
        config.proxy_enabled = true;
        config.proxy_type = "nope".into();
        assert!(Client::new(&config).is_err());
    }

    #[test]
    fn download_client_accepts_supported_proxy_types() {
        for proxy_type in ["http", "https", "socks5"] {
            let mut config = base_config();
            config.proxy_enabled = true;
            config.proxy_type = proxy_type.into();
            assert!(
                Client::new_download_client(&config).is_ok(),
                "proxy_type={proxy_type}"
            );
        }
    }

    #[test]
    fn download_client_rejects_unknown_proxy_type() {
        let mut config = base_config();
        config.proxy_enabled = true;
        config.proxy_type = "bad".into();
        assert!(Client::new_download_client(&config).is_err());
    }

    #[test]
    fn github_headers_omit_auth_when_token_missing() {
        let client = Client::new(&base_config()).expect("client");
        let headers = client.github_headers();
        assert!(!headers.contains_key(AUTHORIZATION));
    }

    #[test]
    fn github_headers_include_bearer_token() {
        let mut config = base_config();
        config.github_token = "secret".into();
        let client = Client::new(&config).expect("client");
        let headers = client.github_headers();
        assert_eq!(
            headers.get(AUTHORIZATION).unwrap(),
            &"Bearer secret".parse::<HeaderValue>().unwrap()
        );
    }

    #[test]
    fn cnb_headers_always_include_accept_header() {
        let client = Client::new(&base_config()).expect("client");
        let headers = client.cnb_headers();
        assert_eq!(
            headers.get(ACCEPT).unwrap(),
            &"application/vnd.cnb.web+json"
                .parse::<HeaderValue>()
                .unwrap()
        );
    }

    #[test]
    fn cnb_headers_include_optional_bearer_token() {
        let mut config = base_config();
        config.github_token = "token".into();
        let client = Client::new(&config).expect("client");
        let headers = client.cnb_headers();
        assert_eq!(
            headers.get(AUTHORIZATION).unwrap(),
            &"Bearer token".parse::<HeaderValue>().unwrap()
        );
    }

    #[test]
    fn resolve_proxy_url_prefers_config_over_environment() {
        let mut config = base_config();
        config.proxy_enabled = true;
        config.proxy_type = "socks5".into();
        config.proxy_address = "127.0.0.1:1081".into();

        let proxy = resolve_proxy_url(&config, &L10n::new(Lang::En)).expect("proxy");
        assert_eq!(proxy.as_deref(), Some("socks5://127.0.0.1:1081"));
    }

    #[test]
    fn proxy_url_from_env_uses_https_then_http() {
        let proxy = proxy_url_from_env_with(|key| match key {
            "https_proxy" => Some("http://secure-proxy:8443".into()),
            "http_proxy" => Some("http://plain-proxy:8080".into()),
            _ => None,
        });

        assert_eq!(proxy.as_deref(), Some("http://secure-proxy:8443"));
    }

    #[test]
    fn proxy_url_from_env_adds_http_scheme_when_missing() {
        let proxy = proxy_url_from_env_with(|key| match key {
            "http_proxy" => Some("127.0.0.1:7890".into()),
            _ => None,
        });

        assert_eq!(proxy.as_deref(), Some("http://127.0.0.1:7890"));
    }

    #[test]
    fn effective_proxy_reports_environment_source_when_config_disabled() {
        let config = base_config();
        let proxy = proxy_url_from_env_with(|key| match key {
            "https_proxy" => Some("http://secure-proxy:8443".into()),
            _ => None,
        })
        .map(|url| EffectiveProxy {
            source: ProxySource::Environment,
            url,
        });

        assert_eq!(
            proxy,
            Some(EffectiveProxy {
                source: ProxySource::Environment,
                url: "http://secure-proxy:8443".into()
            })
        );
        assert!(!config.proxy_enabled);
    }
}
