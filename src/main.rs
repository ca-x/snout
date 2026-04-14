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
    #[arg(long)]
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
        println!("🚀 rime-init 首次初始化");
        ui::run_init_wizard().await?;
    } else if cli.update || cli.scheme || cli.dict || cli.model {
        println!("🔄 更新模式");
        // TODO: 启动更新流程
    } else {
        // 默认启动 TUI
        ui::run_tui().await?;
    }

    Ok(())
}
