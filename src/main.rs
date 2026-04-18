mod api;
mod config;
mod custom;
mod deployer;
mod feedback;
mod fileutil;
mod i18n;
mod skin;
mod types;
mod ui;
mod updater;

use clap::{ArgAction, Parser};
use i18n::{L10n, Lang};
use types::Schema;

#[derive(Parser, Debug)]
#[command(
    name = "snout",
    version,
    about = env!("CARGO_PKG_DESCRIPTION")
)]
struct Cli {
    /// 首次初始化模式 / First-time setup mode
    #[arg(long)]
    init: bool,

    /// 更新所有组件 / Update all components
    #[arg(long, short)]
    update: bool,

    /// 设置当前方案 / Set current schema
    #[arg(long)]
    schema: Option<Schema>,

    /// 仅更新方案 / Update scheme only
    #[arg(long)]
    scheme: bool,

    /// 仅更新词库 / Update dictionary only
    #[arg(long)]
    dict: bool,

    /// 仅更新模型 / Update model only
    #[arg(long)]
    model: bool,

    /// 启用模型 patch / Enable model patch
    #[arg(long)]
    patch_model: bool,

    /// 禁用模型 patch / Disable model patch
    #[arg(long, action = ArgAction::SetTrue)]
    no_patch_model: bool,

    /// 使用 CNB 镜像 / Use CNB mirror
    #[arg(long)]
    mirror: bool,

    /// 禁用镜像 / Disable mirror downloads
    #[arg(long, action = ArgAction::SetTrue)]
    no_mirror: bool,

    /// 下载线程数 / Download thread count
    #[arg(long)]
    download_threads: Option<usize>,

    /// 代理地址 / Proxy address (socks5://host:port or http://host:port)
    #[arg(long)]
    proxy: Option<String>,

    /// 显式启用代理 / Enable configured proxy
    #[arg(long, action = ArgAction::SetTrue)]
    proxy_enabled: bool,

    /// 显式禁用代理 / Disable configured proxy
    #[arg(long, action = ArgAction::SetTrue)]
    no_proxy: bool,

    /// 代理类型 / Proxy type (http|socks5)
    #[arg(long, value_parser = ["http", "socks5"])]
    proxy_type: Option<String>,

    /// GitHub token / GitHub token
    #[arg(long)]
    github_token: Option<String>,

    /// 语言 / Language (zh/en)
    #[arg(long)]
    lang: Option<String>,

    /// TUI 主题模式 / TUI theme mode (auto|light|dark)
    #[arg(long, value_parser = ["auto", "light", "dark"])]
    tui_theme: Option<String>,

    /// 用户数据保留策略 / User data policy (prompt|preserve|discard)
    #[arg(long, value_parser = ["prompt", "preserve", "discard"])]
    user_data_policy: Option<String>,

    /// 启用多引擎同步 / Enable multi-engine sync
    #[arg(long, action = ArgAction::SetTrue)]
    engine_sync: bool,

    /// 禁用多引擎同步 / Disable multi-engine sync
    #[arg(long, action = ArgAction::SetTrue)]
    no_engine_sync: bool,

    /// 同步方式使用软链接 / Use symlink for engine sync
    #[arg(long, action = ArgAction::SetTrue)]
    sync_link: bool,

    /// 同步方式使用复制 / Use copy for engine sync
    #[arg(long, action = ArgAction::SetTrue)]
    sync_copy: bool,

    /// 更新前 hook / Pre-update hook
    #[arg(long)]
    pre_update_hook: Option<String>,

    /// 更新后 hook / Post-update hook
    #[arg(long)]
    post_update_hook: Option<String>,

    /// 启用自动更新 / Enable auto update
    #[arg(long, action = ArgAction::SetTrue)]
    auto_update: bool,

    /// 禁用自动更新 / Disable auto update
    #[arg(long, action = ArgAction::SetTrue)]
    no_auto_update: bool,

    /// 自动更新倒计时 / Auto update countdown
    #[arg(long)]
    auto_update_countdown: Option<i32>,

    /// 追加排除文件模式 / Append exclude pattern
    #[arg(long = "exclude-file")]
    exclude_files: Vec<String>,

    /// 覆盖内置皮肤 key / Override skin patch key
    #[arg(long)]
    skin_patch_key: Option<String>,

    /// Linux Fcitx5 亮色主题 / Linux Fcitx5 light theme
    #[arg(long)]
    fcitx5_theme_light: Option<String>,

    /// Linux Fcitx5 暗色主题 / Linux Fcitx5 dark theme
    #[arg(long)]
    fcitx5_theme_dark: Option<String>,

    /// Linux Fcitx5 亮色主题启用圆角 / Enable rounded corners for Linux Fcitx5 light theme
    #[arg(long, action = ArgAction::SetTrue)]
    fcitx5_theme_light_round: bool,

    /// Linux Fcitx5 暗色主题启用圆角 / Enable rounded corners for Linux Fcitx5 dark theme
    #[arg(long, action = ArgAction::SetTrue)]
    fcitx5_theme_dark_round: bool,

    /// 设置候选词数量 / Set candidate page size
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=9))]
    candidate_page_size: Option<u8>,

    /// 清除候选词数量覆写 / Clear candidate page size override
    #[arg(long, action = ArgAction::SetTrue)]
    clear_candidate_page_size: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.init {
        ui::wizard::run_init_wizard().await?;
    } else if cli.update
        || cli.scheme
        || cli.dict
        || cli.model
        || cli.fcitx5_theme_light.is_some()
        || cli.fcitx5_theme_dark.is_some()
    {
        let mut manager = config::Manager::new()?;
        apply_cli_overrides(&mut manager.config, &cli);
        let t = L10n::new(Lang::from_str(&manager.config.language));

        let schema = manager.config.schema;
        let cache_dir = manager.cache_dir.clone();
        let rime_dir = manager.rime_dir.clone();

        if cli.clear_candidate_page_size {
            custom::set_candidate_page_size(&rime_dir, schema, None)?;
        } else if let Some(page_size) = cli.candidate_page_size {
            custom::set_candidate_page_size(&rime_dir, schema, Some(page_size))?;
        }

        if cli.update || cli.scheme {
            println!("ℹ️  {}", user_data_policy_notice(&manager.config, &t));
            println!("   {}", user_data_policy_detail(&manager.config, &t));
            println!("   {}", t.t("update.preserve_user_data_scope"));
        }

        #[cfg(target_os = "linux")]
        if cli.fcitx5_theme_light.is_some() || cli.fcitx5_theme_dark.is_some() {
            let selection = crate::skin::fcitx5::current_theme_selection()?;
            let light_theme = cli
                .fcitx5_theme_light
                .clone()
                .or(selection.light.clone())
                .or(cli.fcitx5_theme_dark.clone())
                .unwrap_or_default();
            let dark_theme = cli
                .fcitx5_theme_dark
                .clone()
                .or(selection.dark.clone())
                .or(cli.fcitx5_theme_light.clone())
                .unwrap_or_default();
            if !light_theme.is_empty() && !dark_theme.is_empty() {
                crate::skin::fcitx5::apply_theme_pair(
                    &light_theme,
                    &dark_theme,
                    if cli.fcitx5_theme_light_round {
                        Some(true)
                    } else {
                        None
                    },
                    if cli.fcitx5_theme_dark_round {
                        Some(true)
                    } else {
                        None
                    },
                    Lang::from_str(&manager.config.language),
                )
                .await?;
                if !(cli.update || cli.scheme || cli.dict || cli.model) {
                    return Ok(());
                }
            }
        }

        if cli.update {
            updater::update_all(
                &schema,
                &manager.config,
                cache_dir,
                rime_dir,
                types::CancelSignal::new(),
                |event| {
                    print!("\r  [{:3.0}%] {}", event.progress * 100.0, event.detail);
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                },
            )
            .await?;
            println!();
        } else if cli.scheme {
            let base = updater::BaseUpdater::new(&manager.config, cache_dir, rime_dir)?;
            if schema.is_wanxiang() {
                updater::wanxiang::WanxiangUpdater { base }
                    .update_scheme(&schema, &manager.config, None, |event| {
                        print!("\r  [{:3.0}%] {}", event.progress * 100.0, event.detail);
                        std::io::Write::flush(&mut std::io::stdout()).ok();
                    })
                    .await?;
            } else if schema == Schema::Ice {
                updater::ice::IceUpdater { base }
                    .update_scheme(&manager.config, None, |event| {
                        print!("\r  [{:3.0}%] {}", event.progress * 100.0, event.detail);
                        std::io::Write::flush(&mut std::io::stdout()).ok();
                    })
                    .await?;
            } else if schema == Schema::Frost {
                updater::frost::FrostUpdater { base }
                    .update_scheme(&manager.config, None, |event| {
                        print!("\r  [{:3.0}%] {}", event.progress * 100.0, event.detail);
                        std::io::Write::flush(&mut std::io::stdout()).ok();
                    })
                    .await?;
            } else {
                updater::mint::MintUpdater { base }
                    .update_scheme(&manager.config, None, |event| {
                        print!("\r  [{:3.0}%] {}", event.progress * 100.0, event.detail);
                        std::io::Write::flush(&mut std::io::stdout()).ok();
                    })
                    .await?;
            }
            println!();
        } else if cli.dict {
            if schema.dict_zip().is_some() {
                let base = updater::BaseUpdater::new(&manager.config, cache_dir, rime_dir)?;
                if schema.is_wanxiang() {
                    updater::wanxiang::WanxiangUpdater { base }
                        .update_dict(&schema, &manager.config, None, |event| {
                            print!("\r  [{:3.0}%] {}", event.progress * 100.0, event.detail);
                            std::io::Write::flush(&mut std::io::stdout()).ok();
                        })
                        .await?;
                } else {
                    updater::ice::IceUpdater { base }
                        .update_dict(&manager.config, None, |event| {
                            print!("\r  [{:3.0}%] {}", event.progress * 100.0, event.detail);
                            std::io::Write::flush(&mut std::io::stdout()).ok();
                        })
                        .await?;
                }
                println!();
            } else {
                eprintln!("{}", t.t("update.no_dict"));
            }
        } else if cli.model {
            if !schema.supports_model_patch() {
                eprintln!("{}", t.t("update.model_not_supported"));
                std::process::exit(1);
            } else {
                let base = updater::BaseUpdater::new(&manager.config, cache_dir, rime_dir.clone())?;
                updater::wanxiang::WanxiangUpdater { base }
                    .update_model(&manager.config, None, |event| {
                        print!("\r  [{:3.0}%] {}", event.progress * 100.0, event.detail);
                        std::io::Write::flush(&mut std::io::stdout()).ok();
                    })
                    .await?;

                if cli.patch_model {
                    updater::model_patch::patch_model(
                        &rime_dir,
                        &schema,
                        Lang::from_str(&manager.config.language),
                    )?;
                }
                println!();
            }
        }
    } else {
        // 默认启动 TUI
        ui::app::run_tui().await?;
    }

    Ok(())
}

fn apply_cli_overrides(config: &mut types::Config, cli: &Cli) {
    if let Some(schema) = cli.schema {
        config.schema = schema;
    }
    if cli.mirror {
        config.use_mirror = true;
    }
    if cli.no_mirror {
        config.use_mirror = false;
    }
    if let Some(download_threads) = cli.download_threads {
        config.download_threads = download_threads.clamp(1, 8);
    }
    if let Some(ref token) = cli.github_token {
        config.github_token = token.clone();
    }
    if cli.proxy_enabled {
        config.proxy_enabled = true;
    }
    if cli.no_proxy {
        config.proxy_enabled = false;
    }
    if let Some(ref proxy_type) = cli.proxy_type {
        config.proxy_type = proxy_type.clone();
    }
    if let Some(ref proxy) = cli.proxy {
        config.proxy_enabled = true;
        if proxy.starts_with("http://") {
            config.proxy_type = "http".into();
            config.proxy_address = proxy.trim_start_matches("http://").into();
        } else if proxy.starts_with("socks5://") {
            config.proxy_type = "socks5".into();
            config.proxy_address = proxy.trim_start_matches("socks5://").into();
        } else {
            config.proxy_address = proxy.clone();
        }
    }
    if let Some(ref lang) = cli.lang {
        config.language = lang.clone();
    }
    if let Some(ref tui_theme) = cli.tui_theme {
        config.tui_theme_mode = tui_theme.clone();
    }
    if let Some(ref user_data_policy) = cli.user_data_policy {
        config.user_data_policy = user_data_policy.clone();
    }
    if cli.patch_model {
        config.model_patch_enabled = true;
    }
    if cli.no_patch_model {
        config.model_patch_enabled = false;
    }
    if cli.engine_sync {
        config.engine_sync_enabled = true;
    }
    if cli.no_engine_sync {
        config.engine_sync_enabled = false;
    }
    if cli.sync_link {
        config.engine_sync_use_link = true;
    }
    if cli.sync_copy {
        config.engine_sync_use_link = false;
    }
    if let Some(ref hook) = cli.pre_update_hook {
        config.pre_update_hook = hook.clone();
    }
    if let Some(ref hook) = cli.post_update_hook {
        config.post_update_hook = hook.clone();
    }
    if cli.auto_update {
        config.auto_update = true;
    }
    if cli.no_auto_update {
        config.auto_update = false;
    }
    if let Some(countdown) = cli.auto_update_countdown {
        config.auto_update_countdown = countdown.clamp(1, 60);
    }
    if !cli.exclude_files.is_empty() {
        config.exclude_files.extend(cli.exclude_files.clone());
    }
    if let Some(ref skin_key) = cli.skin_patch_key {
        config.skin_patch_key = skin_key.clone();
    }
}

fn user_data_policy_notice<'a>(config: &types::Config, t: &'a L10n) -> &'a str {
    match config.user_data_policy.trim().to_ascii_lowercase().as_str() {
        "discard" => t.t("update.discard_user_data_notice"),
        _ => t.t("update.preserve_user_data_notice"),
    }
}

fn user_data_policy_detail<'a>(config: &types::Config, t: &'a L10n) -> &'a str {
    match config.user_data_policy.trim().to_ascii_lowercase().as_str() {
        "discard" => t.t("update.discard_user_data_detail"),
        _ => t.t("update.preserve_user_data_detail"),
    }
}
