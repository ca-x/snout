//! Verified terminal program updates, isolated from input-method data and package-managed installations.

use crate::types::{CancelSignal, Config};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MANIFEST_URL: &str = "https://github.com/ca-x/snout/releases/latest/download/latest.json";
const APP_ID: &str = "io.github.snout.desktop";
const MAX_MANIFEST: usize = 1024 * 1024;
const MAX_PAYLOAD: u64 = 128 * 1024 * 1024;

/// A newer official terminal binary and its integrity information.
#[derive(Clone, Debug, Serialize)]
pub struct Release {
    /// Semantic version of the offered program.
    pub version: String,
    /// Release notes with terminal control characters removed.
    pub notes: String,
    url: String,
    sha256: String,
    size: u64,
}

#[derive(Deserialize)]
struct Manifest {
    id: String,
    version: String,
    notes: Option<String>,
    platforms: BTreeMap<String, Asset>,
}

#[derive(Deserialize)]
struct Asset {
    url: String,
    sha256: String,
    size: u64,
}

/// An executable eligible for in-place terminal program updates.
#[derive(Debug)]
pub struct TerminalUpdater {
    executable: PathBuf,
}

fn managed(executable: &Path) -> bool {
    [
        "/usr",
        "/bin",
        "/sbin",
        "/opt",
        "/nix",
        "/snap",
        "/app",
        "/var/lib",
        "/home/linuxbrew",
    ]
    .iter()
    .any(|prefix| executable.starts_with(prefix))
        || executable.components().any(|part| {
            part.as_os_str().to_str().is_some_and(|name| {
                ["Cellar", "Caskroom", "scoop", "WindowsApps", "chocolatey"]
                    .iter()
                    .any(|managed| name.eq_ignore_ascii_case(managed))
            })
        })
        || executable.parent().is_some_and(|parent| {
            parent.join("package-managed").exists()
                || parent
                    .parent()
                    .is_some_and(|prefix| prefix.join("share/snout/package-managed").exists())
        })
}

impl TerminalUpdater {
    /// Locates the current program, rejecting system packages and development builds before network access.
    ///
    /// # Errors
    /// Returns an error for managed, read-only, development or unknown executable locations.
    pub fn current() -> Result<Self> {
        let executable = std::fs::canonicalize(std::env::current_exe()?)?;
        let updater = Self { executable };
        updater.allowed()?;
        ensure!(
            !cfg!(debug_assertions),
            "Program self-update is disabled in development builds / 开发构建禁用程序自更新"
        );
        #[cfg(windows)]
        updater.remove_leftovers()?;
        Ok(updater)
    }

    fn allowed(&self) -> Result<()> {
        ensure!(!managed(&self.executable), "Package-managed installation: update snout-bin with your package manager / 包管理器安装，请通过 AUR 助手或包管理器更新 snout-bin");
        ensure!(
            !self
                .executable
                .components()
                .any(|part| part.as_os_str() == "target"),
            "Program self-update is disabled in development builds / 开发构建禁用程序自更新"
        );
        let expected = if cfg!(windows) { "snout.exe" } else { "snout" };
        ensure!(self.executable.file_name().is_some_and(|name| name == expected), "Self-update requires the official snout executable name / 自更新需要使用官方 snout 可执行文件名");
        let parent = self
            .executable
            .parent()
            .context("executable has no directory")?;
        ensure!(
            !parent.join(".fingerprint").is_dir() && !parent.join("deps").is_dir(),
            "Program self-update is disabled in development builds / 开发构建禁用程序自更新"
        );
        let _probe = tempfile::Builder::new()
            .prefix(".snout-update-probe-")
            .tempfile_in(parent)
            .context("Installation is not writable / 安装目录不可写，请使用包管理器更新")?;
        Ok(())
    }

    /// Checks the stable release manifest without modifying the program or Rime data.
    ///
    /// # Errors
    /// Returns an error on cancellation, network failure or an invalid manifest.
    pub async fn check(&self, config: &Config, cancel: &CancelSignal) -> Result<Option<Release>> {
        self.allowed()?;
        let client = client(config)?;
        let mut response = cancellable(
            client
                .get(MANIFEST_URL)
                .timeout(Duration::from_secs(20))
                .send(),
            cancel,
        )
        .await?
        .error_for_status()?;
        let mut bytes = Vec::new();
        while let Some(chunk) = cancellable(response.chunk(), cancel).await? {
            ensure!(
                bytes.len().saturating_add(chunk.len()) <= MAX_MANIFEST,
                "Update manifest exceeds size limit"
            );
            bytes.extend_from_slice(&chunk);
        }
        parse_manifest(
            &bytes,
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
        )
    }

    /// Downloads and verifies a release, then atomically replaces only the terminal executable.
    ///
    /// # Errors
    /// Returns an error on cancellation, invalid size/hash, managed installation or replacement failure.
    pub async fn install(
        &self,
        release: &Release,
        config: &Config,
        cancel: &CancelSignal,
        progress: impl Fn(u64, u64),
    ) -> Result<()> {
        self.allowed()?;
        let client = client(config)?;
        let mut response = cancellable(client.get(&release.url).send(), cancel)
            .await?
            .error_for_status()?;
        let parent = self
            .executable
            .parent()
            .context("executable has no directory")?;
        let mut staged = tempfile::Builder::new()
            .prefix(".snout-update-")
            .tempfile_in(parent)?;
        let mut hasher = Sha256::new();
        let mut received = 0_u64;
        while let Some(chunk) = cancellable(response.chunk(), cancel).await? {
            received = received
                .checked_add(chunk.len() as u64)
                .context("download size overflow")?;
            ensure!(
                received <= release.size && received <= MAX_PAYLOAD,
                "Update exceeds declared size"
            );
            hasher.update(&chunk);
            staged.write_all(&chunk)?;
            progress(received, release.size);
        }
        verify_payload(release, received, &hex::encode(hasher.finalize()))?;
        staged.flush()?;
        staged.as_file().sync_all()?;
        let mut permissions = std::fs::metadata(&self.executable)?.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(permissions.mode() & 0o777);
        }
        #[cfg(not(unix))]
        permissions.set_readonly(false);
        staged.as_file().set_permissions(permissions)?;
        cancel.checkpoint()?;
        self.allowed()?;
        replace(staged, &self.executable)
    }

    #[cfg(windows)]
    fn remove_leftovers(&self) -> Result<()> {
        let parent = self
            .executable
            .parent()
            .context("missing executable directory")?;
        for entry in std::fs::read_dir(parent)? {
            let entry = entry?;
            let name = entry.file_name();
            if name
                .to_str()
                .and_then(|name| name.strip_prefix(".snout.exe.snout-old-"))
                .is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
                })
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Ok(())
    }
}

fn replace(staged: tempfile::NamedTempFile, target: &Path) -> Result<()> {
    #[cfg(windows)]
    let backup = {
        let backup = target.with_file_name(format!(".snout.exe.snout-old-{}", std::process::id()));
        ensure!(
            !backup.exists(),
            "An earlier update is still using its backup"
        );
        std::fs::rename(target, &backup).context("move running executable aside")?;
        backup
    };
    match staged.persist(target) {
        Ok(_) => {
            #[cfg(windows)]
            let _ = std::fs::remove_file(&backup);
            Ok(())
        }
        Err(error) => {
            #[cfg(windows)]
            std::fs::rename(&backup, target).context("restore executable after failed update")?;
            Err(error.error).context("replace terminal executable")
        }
    }
}

fn verify_payload(release: &Release, received: u64, digest: &str) -> Result<()> {
    ensure!(received == release.size, "Update size mismatch");
    ensure!(
        digest.eq_ignore_ascii_case(&release.sha256),
        "Update SHA-256 mismatch"
    );
    Ok(())
}

fn parse_manifest(bytes: &[u8], current: &str, os: &str, arch: &str) -> Result<Option<Release>> {
    let manifest: Manifest =
        serde_json::from_slice(bytes).context("invalid program update manifest")?;
    ensure!(manifest.id == APP_ID, "Manifest is for another application");
    let version = semver::Version::parse(&manifest.version)?;
    ensure!(version.pre.is_empty(), "Prerelease is not a stable update");
    if version <= semver::Version::parse(current)? {
        return Ok(None);
    }
    let key = format!("cli-{os}-{arch}");
    let asset = manifest
        .platforms
        .get(&key)
        .context("No terminal update for this platform")?;
    let suffix = if os == "windows" { ".exe" } else { "" };
    let expected = format!("https://github.com/ca-x/snout/releases/download/v{version}/snout-v{version}-{os}-{arch}{suffix}");
    ensure!(
        asset.url == expected,
        "Not the official terminal release asset for this platform"
    );
    ensure!(
        asset.size > 0 && asset.size <= MAX_PAYLOAD,
        "Invalid update size"
    );
    ensure!(
        asset.sha256.len() == 64 && asset.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Invalid SHA-256"
    );
    Ok(Some(Release {
        version: manifest.version,
        notes: manifest
            .notes
            .unwrap_or_default()
            .chars()
            .filter(|character| !character.is_control() || *character == '\n' || *character == '\t')
            .collect(),
        url: asset.url.clone(),
        sha256: asset.sha256.clone(),
        size: asset.size,
    }))
}

fn client(config: &Config) -> Result<reqwest::Client> {
    let mut client = reqwest::Client::builder()
        .https_only(true)
        .user_agent(concat!("snout/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180));
    if let Some(proxy) = crate::api::effective_proxy(config)? {
        client = client.proxy(reqwest::Proxy::all(proxy.url)?);
    }
    Ok(client.build()?)
}

async fn cancellable<Value>(
    future: impl Future<Output = reqwest::Result<Value>>,
    cancel: &CancelSignal,
) -> Result<Value> {
    tokio::pin!(future);
    loop {
        cancel.checkpoint()?;
        tokio::select! {
            result = &mut future => return Ok(result?),
            _ = tokio::time::sleep(Duration::from_millis(100)) => {},
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> serde_json::Value {
        serde_json::json!({"id":APP_ID, "version":"99.0.0", "notes":"Update\u{001b} notes", "platforms":{
            "cli-linux-x86_64":{"url":"https://github.com/ca-x/snout/releases/download/v99.0.0/snout-v99.0.0-linux-x86_64", "size":3, "sha256":hex::encode(Sha256::digest(b"abc"))}
        }})
    }

    #[test]
    fn only_newer_matching_official_assets_are_offered() {
        let mut manifest = manifest();
        let parse = |value: &serde_json::Value, current: &str| {
            parse_manifest(
                &serde_json::to_vec(value).unwrap(),
                current,
                "linux",
                "x86_64",
            )
        };
        let release = parse(&manifest, "1.0.0").unwrap().unwrap();
        assert_eq!(release.version, "99.0.0");
        assert!(!release.notes.contains('\u{001b}'));
        assert!(parse(&manifest, "99.0.0").unwrap().is_none());
        assert!(parse(&manifest, "100.0.0").unwrap().is_none());
        manifest["platforms"]["cli-linux-x86_64"]["url"] = "https://evil.example/snout".into();
        assert!(parse(&manifest, "1.0.0").is_err());
        manifest = self::manifest();
        manifest["id"] = "other.app".into();
        assert!(parse(&manifest, "1.0.0").is_err());
    }

    #[test]
    fn corrupt_truncated_and_oversized_payloads_are_rejected() {
        let release = parse_manifest(
            &serde_json::to_vec(&manifest()).unwrap(),
            "1.0.0",
            "linux",
            "x86_64",
        )
        .unwrap()
        .unwrap();
        assert!(verify_payload(&release, 3, &hex::encode(Sha256::digest(b"abc"))).is_ok());
        assert!(verify_payload(&release, 3, &hex::encode(Sha256::digest(b"bad"))).is_err());
        assert!(verify_payload(&release, 2, &release.sha256).is_err());
        assert!(verify_payload(&release, 4, &release.sha256).is_err());
    }

    #[test]
    fn managed_paths_and_markers_are_rejected_even_if_writable() {
        for path in [
            "/usr/bin/snout",
            "/usr/local/bin/snout",
            "/opt/snout",
            "/nix/store/snout",
            "/home/user/scoop/apps/snout/snout.exe",
        ] {
            assert!(managed(Path::new(path)), "{path}");
        }
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let executable = bin.join(if cfg!(windows) { "snout.exe" } else { "snout" });
        std::fs::write(&executable, "old").unwrap();
        let updater = TerminalUpdater { executable };
        assert!(updater.allowed().is_ok());
        let marker = root.path().join("share/snout/package-managed");
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(marker, "pacman\n").unwrap();
        assert!(updater.allowed().is_err());
    }

    #[test]
    fn replacement_keeps_sibling_files_and_changes_only_the_executable() {
        let root = tempfile::tempdir().unwrap();
        let target = root
            .path()
            .join(if cfg!(windows) { "snout.exe" } else { "snout" });
        let sibling = root.path().join("rime-user-data");
        std::fs::write(&target, "old").unwrap();
        std::fs::write(&sibling, "untouched").unwrap();
        let mut staged = tempfile::NamedTempFile::new_in(root.path()).unwrap();
        staged.write_all(b"new").unwrap();
        replace(staged, &target).unwrap();
        assert_eq!(std::fs::read_to_string(target).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(sibling).unwrap(), "untouched");
    }

    #[tokio::test]
    async fn cancellation_stops_a_stalled_request() {
        let cancel = CancelSignal::new();
        let request = cancellable(std::future::pending::<reqwest::Result<()>>(), &cancel);
        let stop = async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            cancel.cancel();
        };
        let (result, ()) = tokio::join!(request, stop);
        assert!(result.unwrap_err().is::<crate::types::UpdateCancelled>());
    }
}
