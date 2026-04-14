mod config;
mod types;
mod api;
mod updater;
mod fileutil;
mod deployer;
mod detector;
mod skin;
mod ui;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "rime-init", version, about = "Rime 输入法初始化与更新工具")]
struct Cli {
    /// 首次初始化模式
    #[arg(long)]
    init: bool,

    /// 更新所有组件
    #[arg(long, short)]
    update: bool,

    /// 仅更新方案
    #[arg(long)]
    scheme: bool,

    /// 仅更新词库
    #[arg(long)]
    dict: bool,

    /// 仅更新模型
    #[arg(long)]
    model: bool,

    /// 启用模型 patch
    #[arg(long)]
    patch_model: bool,

    /// 使用 CNB 镜像
    #[arg(long)]
    mirror: bool,

    /// 代理地址 (socks5://host:port 或 http://host:port)
    #[arg(long)]
    proxy: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.init {
        ui::wizard::run_init_wizard().await?;
    } else if cli.update || cli.scheme || cli.dict || cli.model {
        let manager = config::Manager::new()?;
        let schema = manager.config.schema;
        let cache_dir = manager.cache_dir.clone();
        let rime_dir = manager.rime_dir.clone();

        if cli.update {
            updater::update_all(&schema, &manager.config, cache_dir, rime_dir, |msg, pct| {
                print!("\r  [{:3.0}%] {}", pct * 100.0, msg);
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }).await?;
            println!();
        } else if cli.scheme {
            let base = updater::BaseUpdater::new(&manager.config, cache_dir, rime_dir)?;
            let scheme_updater = updater::SchemeUpdater { base };
            scheme_updater.run(&schema, &manager.config, |msg, pct| {
                print!("\r  [{:3.0}%] {}", pct * 100.0, msg);
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }).await?;
            println!();
        } else if cli.dict {
            if schema.dict_zip().is_some() {
                let base = updater::BaseUpdater::new(&manager.config, cache_dir, rime_dir)?;
                let dict = updater::DictUpdater { base };
                dict.run(&schema, &manager.config, |msg, pct| {
                    print!("\r  [{:3.0}%] {}", pct * 100.0, msg);
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                }).await?;
                println!();
            } else {
                eprintln!("此方案无独立词库");
            }
        } else if cli.model {
            let base = updater::BaseUpdater::new(&manager.config, cache_dir, rime_dir.clone())?;
            let model = updater::ModelUpdater { base };
            model.run(&manager.config, |msg, pct| {
                print!("\r  [{:3.0}%] {}", pct * 100.0, msg);
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }).await?;

            if cli.patch_model && schema.supports_model_patch() {
                updater::model_patch::patch_model(&rime_dir, &schema)?;
            }
            println!();
        }
    } else {
        // 默认启动 TUI
        ui::app::run_tui().await?;
    }

    Ok(())
}
