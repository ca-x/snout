use super::Client;
use crate::i18n::L10n;
use crate::types::*;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DownloadRange {
    start: u64,
    end: u64,
    total: u64,
}

struct ParallelDownloadContext<'a> {
    url: &'a str,
    tmp_path: &'a Path,
    total_size: u64,
    threads: usize,
    cancel: Option<&'a CancelSignal>,
    prefer_direct_mirror: bool,
}

impl Client {
    pub async fn download_file(
        &self,
        url: &str,
        dest: &std::path::Path,
        config: &Config,
        cancel: Option<&CancelSignal>,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<()> {
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        let t = L10n::new(self.lang);
        let mut last_err = None;
        let tmp_path = temp_download_path(dest);
        let prefer_direct_mirror = self.has_proxy() && self.is_mirror_url(url);
        let desired_threads = config.download_threads.clamp(1, 8);
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
                Err(error) if error.is::<UpdateCancelled>() => return Err(error),
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

            let total = if resp.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                match response_range(&resp) {
                    Ok(range) if range.start == resume_from && range.end == range.total - 1 => {
                        Some(range.total)
                    }
                    Ok(range) => {
                        last_err = Some(anyhow::anyhow!(
                            "unexpected resume range {range:?} for offset {resume_from}"
                        ));
                        continue;
                    }
                    Err(error) => {
                        last_err = Some(error);
                        continue;
                    }
                }
            } else {
                resp.content_length()
            };
            if resume_from == 0 && desired_threads > 1 {
                if let Some(total_size) = total {
                    if self.supports_parallel_download(&resp)
                        && self
                            .download_file_parallel(
                                ParallelDownloadContext {
                                    url,
                                    tmp_path: &tmp_path,
                                    total_size,
                                    threads: desired_threads,
                                    cancel,
                                    prefer_direct_mirror,
                                },
                                &mut progress,
                            )
                            .await?
                    {
                        tokio::fs::rename(&tmp_path, dest).await?;
                        return Ok(());
                    }
                }
            }

            if let Some(signal) = cancel {
                signal.checkpoint()?;
            }
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
                        if total
                            .is_some_and(|total| c.len() as u64 > total.saturating_sub(downloaded))
                        {
                            stream_err =
                                Some(std::io::Error::other("download exceeded response length"));
                            break;
                        }
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

            if let Some(signal) = cancel {
                signal.checkpoint()?;
            }
            if let Some(e) = stream_err {
                last_err = Some(anyhow::anyhow!("{}: {e}", t.t("api.download_interrupted")));
                continue;
            }

            file.flush().await?;
            if total.is_some_and(|total| downloaded != total) {
                last_err = Some(anyhow::anyhow!("download ended before response length"));
                continue;
            }
            if let Some(signal) = cancel {
                signal.checkpoint()?;
            }
            tokio::fs::rename(&tmp_path, dest).await?;
            return Ok(());
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("{}", t.t("err.download_failed"))))
    }
}

impl Client {
    fn supports_parallel_download(&self, resp: &reqwest::Response) -> bool {
        if resp.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            return true;
        }
        resp.headers()
            .get(reqwest::header::ACCEPT_RANGES)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.to_ascii_lowercase().contains("bytes"))
    }

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
                Err(error) if error.is::<UpdateCancelled>() => return Err(error),
                Err(error) => direct_error = Some(error),
            }
        }

        let mut proxied = self.http.get(url);
        if resume_from > 0 {
            proxied = proxied.header("Range", format!("bytes={resume_from}-"));
        }
        match (
            Client::await_with_cancel(proxied.send(), cancel).await,
            direct_error,
        ) {
            (Ok(response), _) => Ok(response),
            (Err(error), _) if error.is::<UpdateCancelled>() => Err(error),
            (Err(error), Some(direct_error)) => Err(anyhow::anyhow!(
                "mirror download failed without proxy ({direct_error}) and with proxy ({error})"
            )),
            (Err(error), None) => Err(error),
        }
    }

    async fn download_file_parallel(
        &self,
        context: ParallelDownloadContext<'_>,
        progress: &mut impl FnMut(u64, Option<u64>),
    ) -> Result<bool> {
        if context.threads <= 1 || context.total_size < 512 * 1024 {
            return Ok(false);
        }

        let threads = context
            .threads
            .min(context.total_size.div_ceil(512_000) as usize)
            .max(1);
        if threads <= 1 {
            return Ok(false);
        }

        let chunk_size = context.total_size.div_ceil(threads as u64);
        let mut workers = tokio::task::JoinSet::new();
        let mut part_paths = Vec::with_capacity(threads);
        let mut merging = false;

        let result: Result<()> = async {
            for index in 0..threads {
                if let Some(signal) = context.cancel {
                    signal.checkpoint()?;
                }
                let start = index as u64 * chunk_size;
                if start >= context.total_size {
                    break;
                }
                let range = DownloadRange {
                    start,
                    end: start.saturating_add(chunk_size).min(context.total_size) - 1,
                    total: context.total_size,
                };
                let part_path = parallel_part_path(context.tmp_path, index);
                let file = tokio::fs::File::create(&part_path).await?;
                part_paths.push(part_path);
                let url = context.url.to_string();
                let cancel = context.cancel.cloned();
                let this = self.clone();
                let prefer_direct_mirror = context.prefer_direct_mirror;

                workers.spawn(async move {
                    this.download_range_part(
                        &url,
                        file,
                        range,
                        cancel.as_ref(),
                        prefer_direct_mirror,
                    )
                    .await
                });
            }

            let mut downloaded = 0;
            loop {
                let result = if let Some(signal) = context.cancel {
                    tokio::select! {
                        result = workers.join_next() => result,
                        result = Client::wait_for_cancel(signal) => {
                            result?;
                            unreachable!()
                        }
                    }
                } else {
                    workers.join_next().await
                };
                let Some(result) = result else {
                    break;
                };
                downloaded += result??;
                progress(downloaded, Some(context.total_size));
            }
            if let Some(signal) = context.cancel {
                signal.checkpoint()?;
            }
            merging = true;
            merge_parallel_parts(context.tmp_path, &part_paths, context.cancel).await
        }
        .await;

        workers.abort_all();
        while workers.join_next().await.is_some() {}
        if result.is_err() && merging {
            part_paths.push(context.tmp_path.to_path_buf());
        }
        let mut cleanup_error = None;
        for part_path in part_paths {
            if let Err(error) = remove_download_file(&part_path).await {
                cleanup_error.get_or_insert(error);
            }
        }
        if let Some(signal) = context.cancel {
            signal.checkpoint()?;
        }
        if let Some(error) = cleanup_error {
            return Err(error);
        }
        match result {
            Ok(()) => Ok(true),
            Err(error) if error.is::<UpdateCancelled>() => Err(error),
            Err(_) => Ok(false),
        }
    }

    async fn download_range_part(
        &self,
        url: &str,
        mut file: tokio::fs::File,
        range: DownloadRange,
        cancel: Option<&CancelSignal>,
        prefer_direct_mirror: bool,
    ) -> Result<u64> {
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        let resp = self
            .send_download_request_with_range(
                url,
                range.start,
                range.end,
                cancel,
                prefer_direct_mirror,
            )
            .await?;
        if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            anyhow::bail!(
                "range download requires 206 response, got {}",
                resp.status()
            );
        }

        anyhow::ensure!(
            response_range(&resp)? == range,
            "unexpected parallel response range"
        );
        let expected_length = range.end - range.start + 1;
        let mut stream = resp.bytes_stream();
        let mut downloaded = 0u64;
        while let Some(chunk) = if let Some(signal) = cancel {
            tokio::select! {
                chunk = stream.next() => chunk,
                result = Client::wait_for_cancel(signal) => {
                    result?;
                    unreachable!()
                }
            }
        } else {
            stream.next().await
        } {
            let chunk = chunk?;
            anyhow::ensure!(
                chunk.len() as u64 <= expected_length - downloaded,
                "range download exceeded requested length"
            );
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
        }
        file.flush().await?;
        anyhow::ensure!(
            downloaded == expected_length,
            "range download ended before requested length"
        );
        Ok(downloaded)
    }

    async fn send_download_request_with_range(
        &self,
        url: &str,
        start: u64,
        end: u64,
        cancel: Option<&CancelSignal>,
        prefer_direct_mirror: bool,
    ) -> Result<reqwest::Response> {
        let range_header = format!("bytes={start}-{end}");
        let mut direct_error = None;

        if prefer_direct_mirror {
            let direct = self
                .http_direct
                .get(url)
                .header(reqwest::header::RANGE, range_header.clone())
                .send();
            match Client::await_with_cancel(direct, cancel).await {
                Ok(response)
                    if response.status().is_success()
                        || response.status() == reqwest::StatusCode::PARTIAL_CONTENT =>
                {
                    return Ok(response);
                }
                Ok(response) => {
                    direct_error = Some(anyhow::anyhow!(
                        "direct mirror ranged download returned {}",
                        response.status()
                    ));
                }
                Err(error) if error.is::<UpdateCancelled>() => return Err(error),
                Err(error) => direct_error = Some(error),
            }
        }

        let proxied = self
            .http
            .get(url)
            .header(reqwest::header::RANGE, range_header)
            .send();
        match (Client::await_with_cancel(proxied, cancel).await, direct_error) {
            (Ok(response), _) => Ok(response),
            (Err(error), _) if error.is::<UpdateCancelled>() => Err(error),
            (Err(error), Some(direct_error)) => Err(anyhow::anyhow!(
                "mirror ranged download failed without proxy ({direct_error}) and with proxy ({error})"
            )),
            (Err(error), None) => Err(error),
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

fn parallel_part_path(dest: &Path, index: usize) -> std::path::PathBuf {
    let file_name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    dest.with_file_name(format!("{file_name}.part{index}"))
}

async fn remove_download_file(path: &Path) -> Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to clean download file {}", path.display()))
        }
    }
}

async fn merge_parallel_parts(
    dest: &Path,
    part_paths: &[PathBuf],
    cancel: Option<&CancelSignal>,
) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut output = tokio::fs::File::create(dest).await?;
    for part_path in part_paths {
        let mut input = tokio::fs::File::open(part_path).await?;
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            if let Some(signal) = cancel {
                signal.checkpoint()?;
            }
            let read = input.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            output.write_all(&buffer[..read]).await?;
        }
    }
    output.flush().await?;
    if let Some(signal) = cancel {
        signal.checkpoint()?;
    }
    Ok(())
}

fn response_range(resp: &reqwest::Response) -> Result<DownloadRange> {
    let header = resp
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .context("partial response is missing Content-Range")?
        .to_str()?;
    let (interval, total) = header
        .strip_prefix("bytes ")
        .and_then(|range| range.split_once('/'))
        .context("invalid Content-Range")?;
    let (start, end) = interval
        .split_once('-')
        .context("invalid Content-Range interval")?;
    let range = DownloadRange {
        start: start.parse()?,
        end: end.parse()?,
        total: total.parse()?,
    };
    anyhow::ensure!(
        range.start <= range.end && range.end < range.total,
        "invalid Content-Range bounds"
    );
    anyhow::ensure!(
        resp.content_length()
            .is_none_or(|length| length == range.end - range.start + 1),
        "Content-Length disagrees with Content-Range"
    );
    Ok(range)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct HttpFixture {
        url: String,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for HttpFixture {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn http_fixture<Handler, Response>(handler: Handler) -> HttpFixture
    where
        Handler: Fn(tokio::net::TcpStream, Option<String>) -> Response + Send + Sync + 'static,
        Response: std::future::Future<Output = ()> + Send + 'static,
    {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/asset", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            let handler = Arc::new(handler);
            loop {
                tokio::select! {
                    connection = listener.accept() => {
                        let (mut socket, _) = connection.unwrap();
                        let handler = handler.clone();
                        connections.spawn(async move {
                            let mut request = Vec::new();
                            let mut buffer = [0; 1024];
                            while !request.ends_with(b"\r\n\r\n") {
                                let read = socket.read(&mut buffer).await.unwrap();
                                if read == 0 {
                                    return;
                                }
                                request.extend_from_slice(&buffer[..read]);
                            }
                            let range = String::from_utf8(request).unwrap().lines().find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("range").then(|| value.trim().to_owned())
                            });
                            handler(socket, range).await;
                        });
                    }
                    Some(result) = connections.join_next() => result.unwrap(),
                }
            }
        });
        HttpFixture { url, task }
    }

    async fn http_response(
        socket: &mut tokio::net::TcpStream,
        status: &str,
        headers: &str,
        body: &[u8],
    ) {
        let headers = format!("HTTP/1.1 {status}\r\nConnection: close\r\n{headers}\r\n");
        if socket.write_all(headers.as_bytes()).await.is_ok() {
            let _ = socket.write_all(body).await;
        }
    }

    async fn rejects_resume_response(headers: &'static str, body: &'static [u8]) {
        let content = b"0123456789abcdefghijklmnopqrstuvwxyz";
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = requests.clone();
        let server = http_fixture(move |mut socket, range| {
            let mut requests = observed.lock().unwrap();
            requests.push(range);
            let first = requests.len() == 1;
            async move {
                if first {
                    http_response(&mut socket, "206 Partial Content", headers, body).await;
                } else {
                    http_response(&mut socket, "200 OK", "Content-Length: 36\r\n", content).await;
                }
            }
        })
        .await;
        let directory = tempfile::tempdir().unwrap();
        let dest = directory.path().join("asset.bin");
        std::fs::write(temp_download_path(&dest), &content[..10]).unwrap();
        Client::new_download_client(&base_config())
            .unwrap()
            .download_file(&server.url, &dest, &base_config(), None, |_, _| {})
            .await
            .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), content);
        assert_eq!(
            requests.lock().unwrap().as_slice(),
            &[Some("bytes=10-".into()), Some("bytes=10-".into())]
        );
    }

    #[tokio::test]
    async fn regression_resume_rejects_wrong_offset() {
        rejects_resume_response(
            "Content-Length: 36\r\nContent-Range: bytes 0-35/36\r\n",
            b"0123456789abcdefghijklmnopqrstuvwxyz",
        )
        .await;
    }

    #[tokio::test]
    async fn regression_resume_rejects_short_interval() {
        rejects_resume_response(
            "Content-Length: 25\r\nContent-Range: bytes 10-34/36\r\n",
            b"abcdefghijklmnopqrstuvwxy",
        )
        .await;
    }

    #[tokio::test]
    async fn regression_resume_rejects_invalid_total() {
        rejects_resume_response(
            "Content-Length: 26\r\nContent-Range: bytes 10-35/35\r\n",
            b"abcdefghijklmnopqrstuvwxyz",
        )
        .await;
    }

    #[tokio::test]
    async fn regression_resume_rejects_unknown_total() {
        rejects_resume_response(
            "Content-Length: 26\r\nContent-Range: bytes 10-35/*\r\n",
            b"abcdefghijklmnopqrstuvwxyz",
        )
        .await;
    }

    #[tokio::test]
    async fn regression_resume_rejects_missing_range() {
        rejects_resume_response("Content-Length: 26\r\n", b"abcdefghijklmnopqrstuvwxyz").await;
    }

    #[tokio::test]
    async fn regression_resume_rejects_inconsistent_length() {
        rejects_resume_response(
            "Content-Length: 25\r\nContent-Range: bytes 10-35/36\r\n",
            b"abcdefghijklmnopqrstuvwxy",
        )
        .await;
    }

    #[tokio::test]
    async fn download_file_resumes_after_short_response_body() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = requests.clone();
        let server = http_fixture(move |mut socket, range| {
            let mut requests = observed.lock().unwrap();
            requests.push(range);
            let first = requests.len() == 1;
            async move {
                if first {
                    http_response(
                        &mut socket,
                        "206 Partial Content",
                        "Content-Range: bytes 10-35/36\r\n",
                        b"abcdefghijklmnopqrstuvwxy",
                    )
                    .await;
                } else {
                    http_response(
                        &mut socket,
                        "206 Partial Content",
                        "Content-Length: 1\r\nContent-Range: bytes 35-35/36\r\n",
                        b"z",
                    )
                    .await;
                }
            }
        })
        .await;
        let directory = tempfile::tempdir().unwrap();
        let dest = directory.path().join("asset.bin");
        std::fs::write(temp_download_path(&dest), b"0123456789").unwrap();
        Client::new_download_client(&base_config())
            .unwrap()
            .download_file(&server.url, &dest, &base_config(), None, |_, _| {})
            .await
            .unwrap();

        assert_eq!(
            std::fs::read(&dest).unwrap(),
            b"0123456789abcdefghijklmnopqrstuvwxyz"
        );
        assert_eq!(
            requests.lock().unwrap().as_slice(),
            &[Some("bytes=10-".into()), Some("bytes=35-".into())]
        );
    }

    async fn rejects_parallel_response(fault: &'static str) {
        let content = Arc::new(
            (0..1_048_576)
                .map(|index| (index % 251) as u8)
                .collect::<Vec<_>>(),
        );
        let expected = content.clone();
        let server = http_fixture(move |mut socket, range| {
            let content = content.clone();
            async move {
                if let Some(range) = range {
                    let (start, end) = range
                        .strip_prefix("bytes=")
                        .unwrap()
                        .split_once('-')
                        .unwrap();
                    let mut start: usize = start.parse().unwrap();
                    let mut end: usize = end.parse().unwrap();
                    let mut total = content.len();
                    if start == 0 {
                        match fault {
                            "wrong interval" => {
                                start += 1;
                                end += 1;
                            }
                            "short interval" => end -= 1,
                            "wrong total" => total += 1,
                            _ => {}
                        }
                    }
                    let mut headers = format!("Content-Range: bytes {start}-{end}/{total}\r\n");
                    let mut body = &content[start..=end];
                    if start == 0 && matches!(fault, "wrong total" | "missing range") {
                        body = &content[1..=end + 1];
                    }
                    match fault {
                        "short body" => body = &body[..body.len() - 1],
                        "long body" => body = &content[start..=(end + 1).min(content.len() - 1)],
                        "missing range" => headers.clear(),
                        _ => headers.push_str(&format!("Content-Length: {}\r\n", body.len())),
                    }
                    http_response(&mut socket, "206 Partial Content", &headers, body).await;
                } else {
                    http_response(
                        &mut socket,
                        "200 OK",
                        &format!(
                            "Content-Length: {}\r\nAccept-Ranges: bytes\r\n",
                            content.len()
                        ),
                        &content,
                    )
                    .await;
                }
            }
        })
        .await;
        let directory = tempfile::tempdir().unwrap();
        let dest = directory.path().join("asset.bin");
        let config = Config {
            download_threads: 2,
            ..base_config()
        };

        Client::new_download_client(&config)
            .unwrap()
            .download_file(&server.url, &dest, &config, None, |_, _| {})
            .await
            .unwrap();

        assert!(
            std::fs::read(&dest).unwrap() == *expected,
            "committed invalid parallel response: {fault}"
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn regression_parallel_rejects_wrong_interval() {
        rejects_parallel_response("wrong interval").await;
    }

    #[tokio::test]
    async fn regression_parallel_rejects_short_interval() {
        rejects_parallel_response("short interval").await;
    }

    #[tokio::test]
    async fn regression_parallel_rejects_wrong_total() {
        rejects_parallel_response("wrong total").await;
    }

    #[tokio::test]
    async fn regression_parallel_rejects_missing_range() {
        rejects_parallel_response("missing range").await;
    }

    #[tokio::test]
    async fn regression_parallel_rejects_short_body() {
        rejects_parallel_response("short body").await;
    }

    #[tokio::test]
    async fn regression_parallel_rejects_long_body() {
        rejects_parallel_response("long body").await;
    }

    async fn parallel_worker_lifetime(failing_index: usize, cancellation: bool) {
        let directory = tempfile::tempdir().unwrap();
        let dest = directory.path().join("asset.bin");
        let tmp = temp_download_path(&dest);
        let delayed_part = parallel_part_path(&tmp, 1 - failing_index);
        let started = Arc::new(tokio::sync::Notify::new());
        let worker_started = started.clone();
        let disconnected = Arc::new(tokio::sync::Notify::new());
        let worker_disconnected = disconnected.clone();
        let content = Arc::new(vec![42; 1_048_576]);
        let expected = content.clone();
        let server = http_fixture(move |mut socket, range| {
            let content = content.clone();
            let delayed_part = delayed_part.clone();
            let started = worker_started.clone();
            let disconnected = worker_disconnected.clone();
            async move {
                if let Some(range) = range {
                    let (start, end) = range
                        .strip_prefix("bytes=")
                        .unwrap()
                        .split_once('-')
                        .unwrap();
                    let start: usize = start.parse().unwrap();
                    let end: usize = end.parse().unwrap();
                    if start / 524_288 == failing_index && !cancellation {
                        started.notified().await;
                        http_response(
                            &mut socket,
                            "500 Internal Server Error",
                            "Content-Length: 0\r\n",
                            b"",
                        )
                        .await;
                    } else {
                        http_response(
                            &mut socket,
                            "206 Partial Content",
                            &format!(
                                "Content-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\n",
                                end - start + 1,
                                content.len()
                            ),
                            &content[start..start + 16],
                        )
                        .await;
                        tokio::time::timeout(Duration::from_secs(2), async {
                            while !delayed_part.exists() {
                                tokio::time::sleep(Duration::from_millis(5)).await;
                            }
                        })
                        .await
                        .unwrap();
                        started.notify_one();
                        let mut buffer = [0; 1];
                        let _ = socket.read(&mut buffer).await;
                        disconnected.notify_one();
                    }
                } else {
                    http_response(
                        &mut socket,
                        "200 OK",
                        &format!(
                            "Content-Length: {}\r\nAccept-Ranges: bytes\r\n",
                            content.len()
                        ),
                        &content,
                    )
                    .await;
                }
            }
        })
        .await;
        let config = Config {
            download_threads: 2,
            ..base_config()
        };
        let client = Client::new_download_client(&config).unwrap();
        let cancel = CancelSignal::new();
        let download = client.download_file(&server.url, &dest, &config, Some(&cancel), |_, _| {});
        let trigger = async {
            if cancellation {
                started.notified().await;
                cancel.cancel();
            }
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(download, trigger)
        })
        .await
        .expect("failed or cancelled worker must not leave a sibling running");

        if cancellation {
            assert!(result.unwrap_err().is::<UpdateCancelled>());
            assert!(!dest.exists());
            assert!(
                !tmp.exists(),
                "cancellation must not start single-stream fallback"
            );
        } else {
            result.unwrap();
            assert!(std::fs::read(&dest).unwrap() == *expected);
        }
        for index in 0..2 {
            assert!(
                !parallel_part_path(&tmp, index).exists(),
                "worker part survived completion"
            );
        }
        tokio::time::timeout(Duration::from_secs(1), disconnected.notified())
            .await
            .expect("delayed worker still owns its response after completion");
    }

    #[tokio::test]
    async fn regression_parallel_drains_delayed_worker_before_fallback() {
        parallel_worker_lifetime(0, false).await;
    }

    #[tokio::test]
    async fn regression_parallel_observes_failure_before_delayed_worker() {
        parallel_worker_lifetime(1, false).await;
    }

    #[tokio::test]
    async fn regression_parallel_cancellation_cleans_parts_without_fallback() {
        parallel_worker_lifetime(0, true).await;
    }

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
                let range = request.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("range")
                        .then(|| value.trim().to_owned())
                });
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
                &base_config(),
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
                &base_config(),
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
            let (_socket, _) = server.accept().await.unwrap();
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
                    &base_config(),
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

        assert!(result.unwrap_err().is::<UpdateCancelled>());

        server_task.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn download_file_uses_parallel_ranges_when_enabled() {
        let content = (0..2_000_000u32)
            .map(|n| (n % 251) as u8)
            .collect::<Vec<_>>();
        let server_content = content.clone();
        let observed_ranges = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed_ranges_server = observed_ranges.clone();

        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = match server.accept().await {
                    Ok(value) => value,
                    Err(_) => break,
                };
                let mut buf = [0u8; 4096];
                let n = match tokio::io::AsyncReadExt::read(&mut socket, &mut buf).await {
                    Ok(n) => n,
                    Err(_) => continue,
                };
                let request = String::from_utf8_lossy(&buf[..n]);
                let range_header = request
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if name.trim().eq_ignore_ascii_case("range") {
                            Some(value.trim())
                        } else {
                            None
                        }
                    })
                    .map(str::trim)
                    .map(str::to_string);

                if let Some(range) = range_header {
                    observed_ranges_server.lock().unwrap().push(range.clone());
                    let range = range.trim_start_matches("bytes=");
                    let (start, end) = range.split_once('-').unwrap();
                    let start: usize = start.parse().unwrap();
                    let end: usize = end.parse().unwrap();
                    let body = &server_content[start..=end];
                    let response = format!(
                        "HTTP/1.1 206 Partial Content\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\n\r\n",
                        body.len(),
                        start,
                        end,
                        server_content.len()
                    );
                    let _ =
                        tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes()).await;
                    let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, body).await;
                } else {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\n\r\n",
                        server_content.len()
                    );
                    let _ =
                        tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes()).await;
                    let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, &server_content).await;
                }
            }
        });

        let mut config = base_config();
        config.download_threads = 4;
        let client = Client::new_download_client(&config).expect("client");
        let dir = std::env::temp_dir().join("snout-api-parallel-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("asset.bin");

        client
            .download_file(
                &format!("http://{addr}/asset"),
                &dest,
                &config,
                None,
                |_downloaded, _total| {},
            )
            .await
            .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), content);
        assert!(observed_ranges.lock().unwrap().len() >= 2);

        server_task.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
