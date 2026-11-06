#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
use nsh_platform_linux as platform;
#[cfg(target_os = "macos")]
use nsh_platform_macos as platform;
#[cfg(target_os = "windows")]
use nsh_platform_windows as platform;

fn main() -> anyhow::Result<impl std::process::Termination> {
    let shutdown = snout_desktop::controller::ProcessShutdown::install()?;
    let root = if let Some(directory) = std::env::var_os("SNOUT_DESKTOP_SANDBOX") {
        let directory = std::path::PathBuf::from(directory);
        anyhow::ensure!(
            directory.is_absolute(),
            "SNOUT_DESKTOP_SANDBOX must be an absolute path"
        );
        platform::Root::new::<snout_desktop::App>(move || snout_desktop::AppProps {
            config_path: directory
                .join("config.json")
                .to_string_lossy()
                .to_string()
                .into(),
            rime_path: directory.join("rime").to_string_lossy().to_string().into(),
            cache_path: directory.join("cache").to_string_lossy().to_string().into(),
        })
    } else {
        platform::Root::of::<snout_desktop::App>()
    };
    let exit = platform::run_app(
        platform::AppSource::Rust(root),
        platform::AppOptions {
            title: "snout".to_owned(),
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            app_id: "io.github.snout.desktop".to_owned(),
            size: (1040, 760),
            ..Default::default()
        },
    );
    shutdown.wait()?;
    Ok(exit)
}
