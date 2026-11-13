//! Verified application updates for portable desktop distributions, never system packages.

use anyhow::{bail, ensure, Context, Result};
use nsh_updater::{install::Installation, Available, Download};
use snout::types::{CancelSignal, Config};
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const APP_ID: &str = "io.github.snout.desktop";
const REPOSITORY: &str = "ca-x/snout";
const MANAGED: &str = "package-managed";
const MAX_MANIFEST: u64 = 1024 * 1024;
const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;
static RESTART: AtomicBool = AtomicBool::new(false);

/// Observable update state, separate from input-method data updates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    /// Why checks and installation are disabled for this distribution.
    pub disabled: Option<String>,
    /// Whether a check completed successfully.
    pub checked: bool,
    /// A newer release offered by the official manifest.
    pub available: Option<Available>,
    /// The installed release needs a normal window close and process restart.
    pub installed: bool,
}

pub(crate) struct Updater {
    pub state: State,
    executable: Option<PathBuf>,
}

fn update_config() -> nsh_updater::Config {
    let mut config = nsh_updater::Config::default();
    config.id = APP_ID.into();
    config.version = env!("CARGO_PKG_VERSION").into();
    config.github = Some(REPOSITORY.into());
    config
}

fn protected_path(path: &Path) -> bool {
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
    .any(|prefix| path.starts_with(prefix))
        || path.components().any(|part| {
            matches!(
                part.as_os_str().to_str(),
                Some("Cellar" | "Caskroom" | "scoop" | "WindowsApps" | "chocolatey")
            )
        })
}

fn portable_root(executable: &Path) -> Result<PathBuf> {
    ensure!(
        !protected_path(executable),
        "Managed installation: update with your package manager / 包管理器安装，请使用包管理器更新"
    );
    let parent = executable.parent().context("executable has no parent")?;
    let root = if cfg!(target_os = "macos") {
        parent
            .parent()
            .and_then(Path::parent)
            .context("invalid app bundle")?
    } else if cfg!(target_os = "linux") {
        parent.parent().context("invalid portable directory")?
    } else {
        parent
    };
    ensure!(
        !root.join(MANAGED).exists() && !parent.join(MANAGED).exists(),
        "Package-managed installation / 包管理器安装，请使用包管理器更新"
    );
    ensure!(
        !root.join("share/snout-desktop").join(MANAGED).exists(),
        "Package-managed installation / 包管理器安装，请使用包管理器更新"
    );
    let resources = if cfg!(target_os = "macos") {
        root.join("Contents/Resources/nsh")
    } else {
        root.join("nsh")
    };
    ensure!(
        !resources.join(MANAGED).exists(),
        "Package-managed installation / 包管理器安装，请使用包管理器更新"
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(resources.join("app.json")).context(
            "Self-update requires an official portable package / 自更新仅适用于官方便携包",
        )?)?;
    ensure!(metadata["id"] == APP_ID, "Not a snout portable package");
    let expected = if cfg!(target_os = "macos") {
        "Contents/MacOS/snout-desktop"
    } else if cfg!(windows) {
        "snout-desktop.exe"
    } else {
        "bin/snout-desktop"
    };
    ensure!(
        root.join(expected) == executable,
        "Unexpected portable executable layout"
    );
    Ok(root.to_owned())
}

fn installation(executable: &Path) -> Result<Installation> {
    let root = portable_root(executable)?;
    let installation =
        Installation::directory(&root, executable).context("invalid installation")?;
    ensure!(
        installation.can_install(),
        "Installation is not writable / 安装目录不可写，请使用包管理器更新"
    );
    Ok(installation)
}

impl Updater {
    pub fn new(live: bool) -> Self {
        let executable = std::env::current_exe().and_then(std::fs::canonicalize).ok();
        let result = if live {
            executable
                .as_deref()
                .context("Cannot locate running executable")
                .and_then(installation)
        } else {
            Err(anyhow::anyhow!(
                "Self-update disabled in preview / 预览模式禁用程序自更新"
            ))
        };
        if let Ok(installation) = &result {
            installation.remove_leftovers();
        }
        Self {
            state: State {
                disabled: result.err().map(|error| error.to_string()),
                ..State::default()
            },
            executable,
        }
    }

    fn allowed(&self) -> Result<Installation> {
        if let Some(reason) = &self.state.disabled {
            bail!("{reason}");
        }
        installation(self.executable.as_deref().context("missing executable")?)
    }

    pub async fn check(&mut self, settings: &Config, cancel: &CancelSignal) -> Result<()> {
        self.allowed()?;
        let config = update_config();
        let client = http_client(settings)?;
        let bytes = fetch(&client, &config.manifest_url(), MAX_MANIFEST, cancel).await?;
        self.state.available = config.check(&bytes, None, &nsh_updater::platform())?;
        self.state.checked = true;
        Ok(())
    }

    pub async fn install(
        &mut self,
        settings: &Config,
        cancel: &CancelSignal,
        progress: impl Fn(u64, u64),
    ) -> Result<()> {
        self.allowed()?;
        self.check(settings, cancel).await?;
        let update = self
            .state
            .available
            .as_ref()
            .context("No newer desktop release / 没有可用的新版本")?;
        let digest = update.sha256.as_deref().context("Release has no SHA-256")?;
        let size = update.size.context("Release has no download size")?;
        ensure!(size > 0 && size <= MAX_ARCHIVE, "Invalid archive size");
        ensure!(
            update
                .url
                .starts_with("https://github.com/ca-x/snout/releases/download/"),
            "Not an official snout release asset"
        );
        let client = http_client(settings)?;
        let mut response = cancellable(client.get(&update.url).send(), cancel)
            .await?
            .error_for_status()?;
        let suffix = if cfg!(windows) { ".zip" } else { ".tar.gz" };
        let mut archive = tempfile::Builder::new()
            .prefix("snout-update-")
            .suffix(suffix)
            .tempfile()?;
        let mut download = Download::new(digest, Some(size));
        loop {
            cancel.checkpoint()?;
            let Some(chunk) = cancellable(response.chunk(), cancel).await? else {
                break;
            };
            let received = download.update(&chunk)?;
            archive.write_all(&chunk)?;
            progress(received, size);
        }
        download.finish()?;
        archive.flush()?;
        cancel.checkpoint()?;
        self.allowed()?.install(archive.path())?;
        self.state.installed = true;
        RESTART.store(true, Ordering::Release);
        Ok(())
    }
}

fn http_client(config: &Config) -> Result<reqwest::Client> {
    let mut client = reqwest::Client::builder()
        .https_only(true)
        .user_agent(concat!("snout-desktop/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180));
    if let Some(proxy) = snout::api::effective_proxy(config)? {
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

async fn fetch(
    client: &reqwest::Client,
    url: &str,
    limit: u64,
    cancel: &CancelSignal,
) -> Result<Vec<u8>> {
    let mut response = cancellable(
        client.get(url).timeout(Duration::from_secs(20)).send(),
        cancel,
    )
    .await?
    .error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = cancellable(response.chunk(), cancel).await? {
        cancel.checkpoint()?;
        ensure!(
            (bytes.len() as u64).saturating_add(chunk.len() as u64) <= limit,
            "Update response exceeds size limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Starts the replacement only after the native event loop and all workers have stopped.
///
/// # Errors
/// Returns an error if the operating system cannot start the updated executable.
pub fn restart_if_requested(executable: &Path) -> Result<()> {
    if RESTART.swap(false, Ordering::AcqRel) {
        std::process::Command::new(executable)
            .current_dir(
                executable
                    .parent()
                    .context("missing executable directory")?,
            )
            .spawn()
            .context("restart updated snout desktop")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_paths_are_blocked_even_when_writable() {
        for path in [
            "/usr/bin/snout-desktop",
            "/usr/local/bin/snout-desktop",
            "/opt/snout/bin/snout-desktop",
            "/nix/store/snout/bin/snout-desktop",
            "/home/me/scoop/apps/snout/snout-desktop.exe",
        ] {
            assert!(protected_path(Path::new(path)), "{path}");
            assert!(portable_root(Path::new(path)).is_err(), "{path}");
        }
    }

    #[test]
    fn preview_never_checks_or_installs() {
        let updater = Updater::new(false);
        assert!(updater.allowed().is_err());
        assert!(updater
            .state
            .disabled
            .as_deref()
            .unwrap()
            .contains("preview"));
    }

    fn portable_fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let root = if cfg!(target_os = "macos") {
            directory.path().join("snout.app")
        } else {
            directory.path().join("snout")
        };
        let executable = root.join(if cfg!(target_os = "macos") {
            "Contents/MacOS/snout-desktop"
        } else if cfg!(windows) {
            "snout-desktop.exe"
        } else {
            "bin/snout-desktop"
        });
        let resources = root.join(if cfg!(target_os = "macos") {
            "Contents/Resources/nsh"
        } else {
            "nsh"
        });
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::write(&executable, b"fixture").unwrap();
        (directory, executable, resources)
    }

    #[test]
    fn portable_identity_is_required_and_managed_marker_overrides_it() {
        let (_directory, executable, resources) = portable_fixture();
        assert!(
            portable_root(&executable).is_err(),
            "a writable executable alone is not portable"
        );
        std::fs::write(
            resources.join("app.json"),
            serde_json::to_vec(&serde_json::json!({"id": APP_ID})).unwrap(),
        )
        .unwrap();
        assert!(portable_root(&executable).is_ok());
        std::fs::write(resources.join(MANAGED), "pacman\n").unwrap();
        assert!(
            portable_root(&executable).is_err(),
            "package marker overrides portable identity"
        );
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_stalled_network_future() {
        let cancel = CancelSignal::new();
        let future = cancellable(std::future::pending::<reqwest::Result<()>>(), &cancel);
        let cancellation = async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            cancel.cancel();
        };
        let (result, ()) = tokio::join!(future, cancellation);
        assert!(result
            .unwrap_err()
            .downcast_ref::<snout::types::UpdateCancelled>()
            .is_some());
    }

    #[test]
    fn manifest_checks_identity_version_and_digest() {
        let config = update_config();
        let mut manifest = serde_json::json!({"id": APP_ID, "version": "99.0.0", "platforms": {
            nsh_updater::platform(): {"url": "snout.tar.gz", "size": 3, "sha256": nsh_updater::sha256(b"abc")}
        }});
        let available = config
            .check(
                &serde_json::to_vec(&manifest).unwrap(),
                None,
                &nsh_updater::platform(),
            )
            .unwrap()
            .unwrap();
        let mut download = Download::new(available.sha256.as_deref().unwrap(), available.size);
        download.update(b"bad").unwrap();
        assert!(download.finish().is_err());
        manifest["version"] = env!("CARGO_PKG_VERSION").into();
        assert!(config
            .check(
                &serde_json::to_vec(&manifest).unwrap(),
                None,
                &nsh_updater::platform()
            )
            .unwrap()
            .is_none());
        manifest["id"] = "other.app".into();
        assert!(config
            .check(
                &serde_json::to_vec(&manifest).unwrap(),
                None,
                &nsh_updater::platform()
            )
            .is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn portable_archive_replaces_only_its_own_directory_and_keeps_executable_mode() {
        use std::os::unix::fs::PermissionsExt;

        let (_directory, executable, resources) = portable_fixture();
        std::fs::write(
            resources.join("app.json"),
            serde_json::to_vec(&serde_json::json!({"id": APP_ID})).unwrap(),
        )
        .unwrap();
        let archive_file = tempfile::Builder::new()
            .suffix(".tar.gz")
            .tempfile()
            .unwrap();
        let encoder = flate2::write::GzEncoder::new(
            archive_file.reopen().unwrap(),
            flate2::Compression::default(),
        );
        let mut archive = tar::Builder::new(encoder);
        let payload = b"replacement executable";
        let mut header = tar::Header::new_gnu();
        header.set_size(payload.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        archive
            .append_data(&mut header, "snout-v99/bin/snout-desktop", &payload[..])
            .unwrap();
        let metadata =
            serde_json::to_vec(&serde_json::json!({"id": APP_ID, "version": "99.0.0"})).unwrap();
        header.set_size(metadata.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, "snout-v99/nsh/app.json", &metadata[..])
            .unwrap();
        archive.into_inner().unwrap().finish().unwrap();
        let root = portable_root(&executable).unwrap();
        let sibling = root.parent().unwrap().join("rime-user-data");
        std::fs::write(&sibling, "untouched").unwrap();
        installation(&executable)
            .unwrap()
            .install(archive_file.path())
            .unwrap();
        assert_eq!(std::fs::read(&executable).unwrap(), payload);
        assert_eq!(
            std::fs::metadata(&executable).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(std::fs::read_to_string(sibling).unwrap(), "untouched");
        assert_eq!(portable_root(&executable).unwrap(), root);
    }
}
