use crate::config::{self, Manager};
use crate::types::Schema;
use crate::updater;
use anyhow::Result;

/// 首次初始化向导 (纯文本，不使用 TUI)
pub async fn run_init_wizard() -> Result<()> {
    println!("\n🚀 rime-init 首次初始化向导\n");

    // 1. 检测引擎
    let engines = config::detect_installed_engines();
    if engines.is_empty() {
        println!("⚠️  未检测到已安装的 Rime 输入法引擎");
        println!("请先安装:");
        println!("  • 小狼毫 (Weasel) - Windows");
        println!("  • 鼠须管 (Squirrel) - macOS");
        println!("  • Fcitx5 + Rime - Linux: sudo pacman -S fcitx5-im fcitx5-rime");
        return Ok(());
    }
    println!("✅ 检测到引擎: {}\n", engines.join(", "));

    // 2. 选择方案
    println!("选择方案:");
    let schemas = Schema::all();
    for (i, s) in schemas.iter().enumerate() {
        let tag = if s.is_wanxiang() { "万象" } else { "通用" };
        println!("  {:2}. {} [{}]", i + 1, s.display_name(), tag);
    }
    print!("\n编号 [1]: ");
    std::io::Write::flush(&mut std::io::stdout())?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let idx: usize = input.trim().parse::<usize>().unwrap_or(1).saturating_sub(1);
    let schema = schemas.get(idx).unwrap_or(&schemas[0]).clone();

    println!("✅ {}\n", schema.display_name());

    // 3. 模型 patch
    let mut model_patch = false;
    if schema.supports_model_patch() {
        print!("启用语言模型 patch? (y/N): ");
        std::io::Write::flush(&mut std::io::stdout())?;
        input.clear();
        std::io::stdin().read_line(&mut input)?;
        model_patch = input.trim().to_lowercase() == "y";
    }

    // 4. 保存配置
    let mut manager = Manager::new()?;
    manager.config.schema = schema;
    manager.config.model_patch_enabled = model_patch;
    manager.save()?;

    // 5. 执行更新
    println!("\n📦 下载安装中...\n");
    let cache_dir = manager.cache_dir.clone();
    let rime_dir = manager.rime_dir.clone();

    updater::update_all(
        &schema,
        &manager.config,
        cache_dir,
        rime_dir.clone(),
        |msg, pct| {
            print!("\r  [{:3.0}%] {}", pct * 100.0, msg);
            std::io::Write::flush(&mut std::io::stdout()).ok();
        },
    ).await?;

    // 6. Patch
    if model_patch && schema.supports_model_patch() {
        println!("\n🔧 模型 patch...");
        updater::model_patch::patch_model(&rime_dir, &schema)?;
    }

    // 7. 部署
    println!("🔄 部署...");
    if let Err(e) = crate::deployer::deploy() {
        println!("⚠️  部署失败: {e} (可手动部署)");
    }

    println!("\n✅ 完成！运行 `rime-init` 打开 TUI\n");
    Ok(())
}
