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

use clap::{ArgAction, ArgGroup, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, shells};
use i18n::{L10n, Lang};
use serde::Serialize;
use std::process::ExitCode;
use types::Schema;

#[derive(Subcommand, Debug)]
enum Commands {
    /// 生成 shell 自动补全 / Generate shell completions
    Completions {
        /// 目标 shell / Target shell
        #[arg(value_enum)]
        shell: CompletionShell,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CompletionShell {
    Zsh,
    Fish,
}

#[derive(Parser, Debug)]
#[command(
    name = "snout",
    version,
    about = env!("CARGO_PKG_DESCRIPTION"),
    args_conflicts_with_subcommands = true,
    group(
        ArgGroup::new("primary_action")
            .args(["init", "update", "scheme", "dict", "model"])
            .multiple(false)
    )
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// 输出单个 JSON 文档 / Emit one JSON document
    #[arg(long)]
    json: bool,

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
    #[arg(
        long,
        conflicts_with = "init",
        value_parser = clap::builder::NonEmptyStringValueParser::new()
    )]
    fcitx5_theme_light: Option<String>,

    /// Linux Fcitx5 暗色主题 / Linux Fcitx5 dark theme
    #[arg(
        long,
        conflicts_with = "init",
        value_parser = clap::builder::NonEmptyStringValueParser::new()
    )]
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

#[derive(Serialize)]
struct CliReport {
    schema_version: u8,
    ok: bool,
    command: &'static str,
    schema: Option<Schema>,
    results: Vec<CliResult>,
    error: Option<String>,
}

#[derive(Serialize)]
struct CliResult {
    component: &'static str,
    display_name: String,
    old_version: String,
    new_version: String,
    success: bool,
    message: String,
}

impl From<updater::UpdateResult> for CliResult {
    fn from(result: updater::UpdateResult) -> Self {
        Self {
            component: stable_component_id(result.kind),
            display_name: result.component,
            old_version: result.old_version,
            new_version: result.new_version,
            success: result.success,
            message: result.message,
        }
    }
}

fn stable_component_id(component: updater::UpdateComponent) -> &'static str {
    match component {
        updater::UpdateComponent::Update => "update",
        updater::UpdateComponent::Scheme => "scheme",
        updater::UpdateComponent::Dict => "dict",
        updater::UpdateComponent::Model => "model",
        updater::UpdateComponent::ModelPatch => "model_patch",
        updater::UpdateComponent::Deploy => "deploy",
        updater::UpdateComponent::Fcitx5Theme => "fcitx5_theme",
        updater::UpdateComponent::Fcitx5Setup => "fcitx5_setup",
        updater::UpdateComponent::Sync => "sync",
        updater::UpdateComponent::Hook => "hook",
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let json_requested = std::env::args_os().any(|arg| arg == std::ffi::OsStr::new("--json"));
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let exit_code = error.exit_code();
            if exit_code == 0 {
                error.print().ok();
                return ExitCode::SUCCESS;
            }
            if json_requested {
                print_json_report(&CliReport {
                    schema_version: 1,
                    ok: false,
                    command: "parse",
                    schema: None,
                    results: Vec::new(),
                    error: Some(error.to_string()),
                });
            } else {
                error.print().ok();
            }
            return ExitCode::from(u8::try_from(exit_code).unwrap_or(1));
        }
    };
    let json = cli.json;
    let command = cli.action_name();

    match run(cli).await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(error) => {
            if json {
                print_json_report(&CliReport {
                    schema_version: 1,
                    ok: false,
                    command,
                    schema: None,
                    results: Vec::new(),
                    error: Some(format!("{error:#}")),
                });
            } else {
                eprintln!("Error: {error:#}");
            }
            ExitCode::from(1)
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<bool> {
    if let Some(command) = &cli.command {
        if cli.json {
            anyhow::bail!("--json cannot be used with the completions command");
        }
        match command {
            Commands::Completions { shell } => print_completions(*shell),
        }
        return Ok(true);
    }

    if cli.json && cli.init {
        anyhow::bail!("--json cannot be used with the interactive --init wizard");
    }
    if cli.json && !cli.has_direct_action() {
        anyhow::bail!("--json requires a non-interactive update or theme action");
    }
    #[cfg(not(target_os = "linux"))]
    if cli.fcitx5_theme_light.is_some() || cli.fcitx5_theme_dark.is_some() {
        anyhow::bail!("Fcitx5 theme commands are only supported on Linux");
    }

    feedback::set_json_active(cli.json);

    if cli.init {
        ui::wizard::run_init_wizard(cli.lang.as_deref()).await?;
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
        let command = cli.action_name();
        let mut results = Vec::new();

        if cli.clear_candidate_page_size {
            custom::set_candidate_page_size(&rime_dir, schema, None)?;
        } else if let Some(page_size) = cli.candidate_page_size {
            custom::set_candidate_page_size(&rime_dir, schema, Some(page_size))?;
        }

        if !cli.json && (cli.update || cli.scheme) {
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
                let apply_result = crate::skin::fcitx5::apply_theme_pair(
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
                .await;
                match apply_result {
                    Ok(()) => results.push(updater::BaseUpdater::success_result(
                        updater::UpdateComponent::Fcitx5Theme,
                        t.t("menu.fcitx5_theme"),
                        "-",
                        "-",
                        t.t("skin.applied"),
                    )),
                    Err(error) => {
                        results.push(updater::BaseUpdater::error_result(
                            updater::UpdateComponent::Fcitx5Theme,
                            t.t("menu.fcitx5_theme"),
                            &error.to_string(),
                        ));
                        return finish_cli_action(cli.json, command, schema, results);
                    }
                }
                if !(cli.update || cli.scheme || cli.dict || cli.model) {
                    return finish_cli_action(cli.json, command, schema, results);
                }
            }
        }

        if cli.update {
            match updater::update_all(
                &schema,
                &manager.config,
                cache_dir,
                rime_dir,
                types::CancelSignal::new(),
                |event| {
                    print_progress(cli.json, event);
                },
            )
            .await
            {
                Ok(update_results) => results.extend(update_results),
                Err(error) => results.push(updater::BaseUpdater::error_result(
                    updater::UpdateComponent::Update,
                    t.t("menu.update_all"),
                    &error.to_string(),
                )),
            }
            print_progress_end(cli.json);
        } else if cli.scheme {
            let result = match updater::BaseUpdater::new(&manager.config, cache_dir, rime_dir) {
                Ok(base) if schema.is_wanxiang() => {
                    updater::wanxiang::WanxiangUpdater { base }
                        .update_scheme(&schema, &manager.config, None, |event| {
                            print_progress(cli.json, event);
                        })
                        .await
                }
                Ok(base) if schema == Schema::Ice => {
                    updater::ice::IceUpdater { base }
                        .update_scheme(&manager.config, None, |event| {
                            print_progress(cli.json, event);
                        })
                        .await
                }
                Ok(base) if schema == Schema::Frost => {
                    updater::frost::FrostUpdater { base }
                        .update_scheme(&manager.config, None, |event| {
                            print_progress(cli.json, event);
                        })
                        .await
                }
                Ok(base) => {
                    updater::mint::MintUpdater { base }
                        .update_scheme(&manager.config, None, |event| {
                            print_progress(cli.json, event);
                        })
                        .await
                }
                Err(error) => Err(error),
            };
            match result {
                Ok(result) => results.push(result),
                Err(error) => results.push(updater::BaseUpdater::fail_result(
                    updater::UpdateComponent::Scheme,
                    t.t("update.scheme"),
                    &error,
                )),
            }
            print_progress_end(cli.json);
        } else if cli.dict {
            if schema.dict_zip().is_some() {
                let result = match updater::BaseUpdater::new(&manager.config, cache_dir, rime_dir) {
                    Ok(base) if schema.is_wanxiang() => {
                        updater::wanxiang::WanxiangUpdater { base }
                            .update_dict(&schema, &manager.config, None, |event| {
                                print_progress(cli.json, event);
                            })
                            .await
                    }
                    Ok(base) => {
                        updater::ice::IceUpdater { base }
                            .update_dict(&manager.config, None, |event| {
                                print_progress(cli.json, event);
                            })
                            .await
                    }
                    Err(error) => Err(error),
                };
                match result {
                    Ok(result) => results.push(result),
                    Err(error) => results.push(updater::BaseUpdater::fail_result(
                        updater::UpdateComponent::Dict,
                        t.t("update.dict"),
                        &error,
                    )),
                }
                print_progress_end(cli.json);
            } else {
                let message = t.t("update.no_dict");
                if !cli.json {
                    eprintln!("{message}");
                }
                results.push(updater::BaseUpdater::error_result(
                    updater::UpdateComponent::Dict,
                    t.t("update.dict"),
                    message,
                ));
            }
        } else if cli.model {
            if !schema.supports_model_patch() {
                let message = t.t("update.model_not_supported");
                if !cli.json {
                    eprintln!("{message}");
                }
                results.push(updater::BaseUpdater::error_result(
                    updater::UpdateComponent::Model,
                    t.t("update.model"),
                    message,
                ));
            } else {
                let result =
                    match updater::BaseUpdater::new(&manager.config, cache_dir, rime_dir.clone()) {
                        Ok(base) => {
                            updater::wanxiang::WanxiangUpdater { base }
                                .update_model(&manager.config, None, |event| {
                                    print_progress(cli.json, event);
                                })
                                .await
                        }
                        Err(error) => Err(error),
                    };
                let model_updated = match result {
                    Ok(result) => {
                        results.push(result);
                        true
                    }
                    Err(error) => {
                        results.push(updater::BaseUpdater::fail_result(
                            updater::UpdateComponent::Model,
                            t.t("update.model"),
                            &error,
                        ));
                        false
                    }
                };

                if model_updated && cli.patch_model {
                    match updater::model_patch::patch_model(
                        &rime_dir,
                        &schema,
                        Lang::from_str(&manager.config.language),
                    ) {
                        Ok(()) => results.push(updater::BaseUpdater::success_result(
                            updater::UpdateComponent::ModelPatch,
                            t.t("update.component.model_patch"),
                            "-",
                            t.t("patch.model.enabled"),
                            t.t("patch.model.enabled"),
                        )),
                        Err(error) => {
                            if !cli.json {
                                eprintln!("{error:#}");
                            }
                            results.push(updater::BaseUpdater::error_result(
                                updater::UpdateComponent::ModelPatch,
                                t.t("update.component.model_patch"),
                                &error.to_string(),
                            ));
                        }
                    }
                }
                print_progress_end(cli.json);
            }
        }
        return finish_cli_action(cli.json, command, schema, results);
    } else {
        // 默认启动 TUI
        ui::app::run_tui().await?;
    }

    Ok(true)
}

impl Cli {
    fn action_name(&self) -> &'static str {
        match self.command {
            Some(Commands::Completions { .. }) => "completions",
            None if self.init => "init",
            None if self.update => "update",
            None if self.scheme => "scheme",
            None if self.dict => "dict",
            None if self.model => "model",
            None if self.fcitx5_theme_light.is_some() || self.fcitx5_theme_dark.is_some() => {
                "fcitx5-theme"
            }
            None => "tui",
        }
    }

    fn has_direct_action(&self) -> bool {
        self.update
            || self.scheme
            || self.dict
            || self.model
            || self.fcitx5_theme_light.is_some()
            || self.fcitx5_theme_dark.is_some()
    }
}

fn print_completions(shell: CompletionShell) {
    let mut stdout = std::io::stdout();
    write_completions(shell, &mut stdout);
}

fn write_completions(shell: CompletionShell, output: &mut impl std::io::Write) {
    let mut command = Cli::command();
    match shell {
        CompletionShell::Zsh => generate(shells::Zsh, &mut command, "snout", output),
        CompletionShell::Fish => generate(shells::Fish, &mut command, "snout", output),
    }
}

fn print_progress(json: bool, event: updater::UpdateEvent) {
    if !json {
        print!("\r  [{:3.0}%] {}", event.progress * 100.0, event.detail);
        std::io::Write::flush(&mut std::io::stdout()).ok();
    }
}

fn print_progress_end(json: bool) {
    if !json {
        println!();
    }
}

fn finish_cli_action(
    json: bool,
    command: &'static str,
    schema: Schema,
    results: Vec<updater::UpdateResult>,
) -> anyhow::Result<bool> {
    let ok = results.iter().all(|result| result.success);
    if json {
        print_json_report(&CliReport {
            schema_version: 1,
            ok,
            command,
            schema: Some(schema),
            results: results.into_iter().map(CliResult::from).collect(),
            error: None,
        });
    } else {
        for result in results.iter().filter(|result| !result.success) {
            eprintln!("{}: {}", result.component, result.message);
        }
    }
    Ok(ok)
}

fn print_json_report(report: &CliReport) {
    println!(
        "{}",
        serde_json::to_string_pretty(report).expect("serializing a CLI report cannot fail")
    );
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

#[cfg(test)]
mod cli_tests {
    use super::*;

    #[test]
    fn json_flag_selects_a_non_interactive_action() {
        let cli = Cli::try_parse_from(["snout", "--update", "--json"]).expect("valid CLI");

        assert!(cli.json);
        assert!(cli.has_direct_action());
        assert_eq!(cli.action_name(), "update");
    }

    #[test]
    fn completion_subcommand_accepts_zsh_and_fish() {
        for shell in ["zsh", "fish"] {
            let cli = Cli::try_parse_from(["snout", "completions", shell]).expect("valid CLI");
            assert_eq!(cli.action_name(), "completions");
        }
    }

    #[test]
    fn completion_subcommand_rejects_update_flags() {
        let error = Cli::try_parse_from(["snout", "--update", "completions", "fish"])
            .expect_err("actions and completion generation must not be combined");

        assert_eq!(error.exit_code(), 2);
    }

    #[test]
    fn primary_actions_are_mutually_exclusive() {
        let actions = ["--init", "--update", "--scheme", "--dict", "--model"];
        for (index, first) in actions.iter().enumerate() {
            for second in &actions[index + 1..] {
                let error = Cli::try_parse_from(["snout", *first, *second])
                    .expect_err("primary actions must not be combined");
                assert_eq!(error.exit_code(), 2, "{first} {second}");
            }
        }
    }

    #[test]
    fn theme_names_cannot_be_empty() {
        for flag in ["--fcitx5-theme-light", "--fcitx5-theme-dark"] {
            let error = Cli::try_parse_from(["snout", flag, ""])
                .expect_err("empty theme names must be rejected");
            assert_eq!(error.exit_code(), 2, "{flag}");
        }
    }

    #[test]
    fn theme_actions_cannot_be_combined_with_init() {
        let error = Cli::try_parse_from([
            "snout",
            "--init",
            "--fcitx5-theme-light",
            "catppuccin-latte-sky",
        ])
        .expect_err("interactive init must not ignore a theme action");

        assert_eq!(error.exit_code(), 2);
    }

    #[test]
    fn generated_completions_include_json_and_update_flags() {
        for shell in [CompletionShell::Zsh, CompletionShell::Fish] {
            let mut output = Vec::new();
            write_completions(shell, &mut output);
            let output = String::from_utf8(output).expect("completion output is UTF-8");

            let (json_flag, update_flag) = match shell {
                CompletionShell::Zsh => ("--json", "--update"),
                CompletionShell::Fish => ("-l json", "-l update"),
            };
            assert!(output.contains(json_flag));
            assert!(output.contains(update_flag));
            assert!(output.contains("completions"));
        }
    }

    #[test]
    fn json_report_exposes_status_command_schema_and_results() {
        let report = CliReport {
            schema_version: 1,
            ok: true,
            command: "scheme",
            schema: Some(Schema::Ice),
            results: vec![CliResult::from(updater::BaseUpdater::success_result(
                updater::UpdateComponent::Scheme,
                "scheme",
                "old",
                "new",
                "updated",
            ))],
            error: None,
        };
        let value = serde_json::to_value(report).expect("serializable report");

        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["ok"], true);
        assert_eq!(value["command"], "scheme");
        assert_eq!(value["schema"], "Ice");
        assert_eq!(value["results"][0]["component"], "scheme");
        assert_eq!(value["results"][0]["display_name"], "scheme");
        assert_eq!(value["results"][0]["success"], true);
        assert_eq!(value["results"][0]["new_version"], "new");
    }

    #[test]
    fn json_error_uses_the_same_envelope() {
        let report = CliReport {
            schema_version: 1,
            ok: false,
            command: "parse",
            schema: None,
            results: Vec::new(),
            error: Some("invalid argument".into()),
        };
        let value = serde_json::to_value(report).expect("serializable report");

        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["ok"], false);
        assert!(value["schema"].is_null());
        assert_eq!(value["results"], serde_json::json!([]));
        assert_eq!(value["error"], "invalid argument");
    }

    #[test]
    fn stable_component_ids_do_not_depend_on_display_names() {
        assert_eq!(stable_component_id(updater::UpdateComponent::Dict), "dict");
        assert_eq!(
            stable_component_id(updater::UpdateComponent::Fcitx5Theme),
            "fcitx5_theme"
        );
        assert_eq!(
            stable_component_id(updater::UpdateComponent::Fcitx5Setup),
            "fcitx5_setup"
        );
    }
}
