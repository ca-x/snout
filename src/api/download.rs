use super::Client;
use crate::i18n::L10n;
use crate::types::*;
use anyhow::Result;
use std::path::Path;

impl Client {
    pub async fn download_file(
        &self,
        url: &str,
        dest: &std::path::Path,
        cancel: Option<&CancelSignal>,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<()> {
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        let t = L10n::new(self.lang);
        let mut last_err = None;
        let tmp_path = temp_download_path(dest);
        let prefer_direct_mirror = self.has_proxy() && self.is_mirror_url(url);
        for attempt in 0..3 {
            if let Some(signal) = cancel {
                signal.checkpoint()?;
            }
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(1 << attempt);
                crate::feedback::warn(format!(
                    "⚠️ {}: {}s ({}/3)...",
                    t.t("api.download_retry"),
                    delay.as_secs(),
                    attempt + 1
                ));
                Client::cancellable_sleep(delay, cancel).await?;
            }

            let resume_from = tokio::fs::metadata(&tmp_path)
                .await
                .map(|meta| meta.len())
                .unwrap_or(0);

            let resp = match self
                .send_download_request(url, resume_from, cancel, prefer_direct_mirror)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    last_err = Some(anyhow::anyhow!(
                        "{}: {e}",
                        t.t("api.download_request_failed")
                    ));
                    continue;
                }
            };

            if !resp.status().is_success() {
                last_err = Some(anyhow::anyhow!(
                    "{} {}",
                    t.t("api.download_http_failed"),
                    resp.status()
                ));
                continue;
            }

            let total = response_total_length(&resp, resume_from);
            let mut downloaded = resume_from;
            let mut file =
                if resp.status() == reqwest::StatusCode::PARTIAL_CONTENT && resume_from > 0 {
                    tokio::fs::OpenOptions::new()
                        .append(true)
                        .open(&tmp_path)
                        .await?
                } else {
                    downloaded = 0;
                    tokio::fs::File::create(&tmp_path).await?
                };
            let mut stream = resp.bytes_stream();
            let mut stream_err = None;

            loop {
                let next_chunk = if let Some(signal) = cancel {
                    tokio::select! {
                        chunk = stream.next() => chunk,
                        result = Client::wait_for_cancel(signal) => {
                            result?;
                            unreachable!()
                        }
                    }
                } else {
                    stream.next().await
                };

                let Some(chunk) = next_chunk else {
                    break;
                };
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
                last_err = Some(anyhow::anyhow!("{}: {e}", t.t("api.download_interrupted")));
                continue;
            }

            file.flush().await?;
            tokio::fs::rename(&tmp_path, dest).await?;
            return Ok(());
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("{}", t.t("err.download_failed"))))
    }
}

impl Client {
    async fn send_download_request(
        &self,
        url: &str,
        resume_from: u64,
        cancel: Option<&CancelSignal>,
        prefer_direct_mirror: bool,
    ) -> Result<reqwest::Response> {
        let mut direct_error = None;

        if prefer_direct_mirror {
            let mut direct = self.http_direct.get(url);
            if resume_from > 0 {
                direct = direct.header("Range", format!("bytes={resume_from}-"));
            }
            match Client::await_with_cancel(direct.send(), cancel).await {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    direct_error = Some(anyhow::anyhow!(
                        "direct mirror download returned {}",
                        response.status()
                    ));
                }
                Err(error) => direct_error = Some(error),
            }
        }

        let mut proxied = self.http.get(url);
        if resume_from > 0 {
            proxied = proxied.header("Range", format!("bytes={resume_from}-"));
        }
        match Client::await_with_cancel(proxied.send(), cancel).await {
            Ok(response) => Ok(response),
            Err(error) if direct_error.is_some() => Err(anyhow::anyhow!(
                "mirror download failed without proxy ({}) and with proxy ({error})",
                direct_error.expect("direct error")
            )),
            Err(error) => Err(error),
        }
    }
}

fn temp_download_path(dest: &Path) -> std::path::PathBuf {
    let file_name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    dest.with_file_name(format!("{file_name}.tmp"))
}

fn response_total_length(resp: &reqwest::Response, resume_from: u64) -> Option<u64> {
    if let Some(range) = resp.headers().get(reqwest::header::CONTENT_RANGE) {
        if let Ok(range) = range.to_str() {
            if let Some(total) = range.split('/').nth(1).and_then(|v| v.parse::<u64>().ok()) {
                return Some(total);
            }
        }
    }
    resp.content_length().map(|len| {
        if resp.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            len + resume_from
        } else {
            len
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test]
    async fn download_file_resumes_from_partial_tmp_file() {
        let full_content = b"0123456789abcdefghijklmnopqrstuvwxyz";
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = match server.accept().await {
                    Ok(value) => value,
                    Err(_) => break,
                };
                let mut buf = [0u8; 1024];
                let n = match tokio::io::AsyncReadExt::read(&mut socket, &mut buf).await {
                    Ok(n) => n,
                    Err(_) => continue,
                };
                let request = String::from_utf8_lossy(&buf[..n]);
                let range = request
                    .lines()
                    .find(|line| line.starts_with("Range:"))
                    .map(|line| line.trim().to_string());
                if let Some(range) = range {
                    assert!(range.contains("bytes=10-"));
                    let body = &full_content[10..];
                    let response = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes 10-35/36\r\n\r\n",
                        body.len()
                    );
                    let _ =
                        tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes()).await;
                    let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, body).await;
                } else {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                        full_content.len()
                    );
                    let _ =
                        tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes()).await;
                    let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, full_content).await;
                }
            }
        });

        let client = Client::new_download_client(&base_config()).expect("client");
        let dir = std::env::temp_dir().join("snout-api-resume-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("asset.bin");
        let tmp = temp_download_path(&dest);
        std::fs::write(&tmp, &full_content[..10]).unwrap();

        client
            .download_file(
                &format!("http://{addr}/asset"),
                &dest,
                None,
                |_downloaded, _total| {},
            )
            .await
            .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), full_content);
        assert!(!tmp.exists());

        server_task.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn download_file_restarts_when_server_ignores_range() {
        let content = b"complete file content";
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = match server.accept().await {
                    Ok(value) => value,
                    Err(_) => break,
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                    content.len()
                );
                let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes()).await;
                let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, content).await;
            }
        });

        let client = Client::new_download_client(&base_config()).expect("client");
        let dir = std::env::temp_dir().join("snout-api-no-resume-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("asset.bin");
        let tmp = temp_download_path(&dest);
        std::fs::write(&tmp, b"partial").unwrap();

        client
            .download_file(
                &format!("http://{addr}/asset"),
                &dest,
                None,
                |_downloaded, _total| {},
            )
            .await
            .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), content);
        assert!(!tmp.exists());

        server_task.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn download_file_cancels_while_waiting_for_response() {
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let _ = server.accept().await;
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        });

        let client = Client::new_download_client(&base_config()).expect("client");
        let dir = std::env::temp_dir().join("snout-api-cancel-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("asset.bin");
        let cancel = CancelSignal::new();
        let cancel_clone = cancel.clone();

        let task = tokio::spawn(async move {
            client
                .download_file(
                    &format!("http://{addr}/asset"),
                    &dest,
                    Some(&cancel_clone),
                    |_downloaded, _total| {},
                )
                .await
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        cancel.cancel();

        let result = tokio::time::timeout(std::time::Duration::from_secs(2), task)
            .await
            .expect("download task should stop quickly")
            .expect("join");

        assert!(result.is_err());

        server_task.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
