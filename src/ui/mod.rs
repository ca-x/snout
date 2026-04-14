use crate::config::Manager;
use crate::types::Schema;
use crate::updater;
use anyhow::Result;

pub async fn run_init_wizard() -> Result<()> {
    println!("\n🚀 rime-init 首次初始化向导\n");

    // 1. 检测 Rime 引擎
    let engines = crate::config::detect_installed_engines();
    if engines.is_empty() {
        println!("⚠️  未检测到已安装的 Rime 输入法引擎");
        println!("请先安装以下任一引擎:");
        println!("  • 小狼毫 (Weasel) - Windows");
        println!("  • 鼠须管 (Squirrel) - macOS: brew install --cask squirrel");
        println!("  • Fcitx5 + Rime - Linux: sudo pacman -S fcitx5-im fcitx5-rime");
        println!("\n安装后请重新运行 rime-init");
        return Ok(());
    }
    println!("✅ 检测到引擎: {}\n", engines.join(", "));

    // 2. 选择方案
    println!("请选择要安装的方案:");
    let schemas = Schema::all();
    for (i, s) in schemas.iter().enumerate() {
        println!("  {}. {}", i + 1, s.display_name());
    }
    print!("\n请输入编号 [1]: ");
    std::io::Write::flush(&mut std::io::stdout())?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let idx: usize = input.trim().parse::<usize>().unwrap_or(1).saturating_sub(1);
    let schema = schemas.get(idx).unwrap_or(&schemas[0]).clone();

    println!("\n已选择: {}\n", schema.display_name());

    // 3. 模型 patch 选项 (仅万象)
    let mut model_patch = false;
    if schema.supports_model_patch() {
        print!("是否启用语言模型 patch? (y/N) [N]: ");
        std::io::Write::flush(&mut std::io::stdout())?;
        input.clear();
        std::io::stdin().read_line(&mut input)?;
        model_patch = input.trim().to_lowercase() == "y";
    }

    // 4. 创建配置
    let mut manager = Manager::new()?;
    manager.config.schema = schema;
    manager.config.model_patch_enabled = model_patch;
    manager.save()?;

    println!("\n📦 开始下载和安装...\n");

    // 5. 执行更新
    let cache_dir = manager.cache_dir.clone();
    let rime_dir = manager.rime_dir.clone();

    updater::update_all(
        &manager.config.schema,
        &manager.config,
        cache_dir,
        rime_dir.clone(),
        |msg, pct| {
            print!("\r  [{:3.0}%] {}", pct * 100.0, msg);
            std::io::Write::flush(&mut std::io::stdout()).ok();
        },
    ).await?;

    // 6. 模型 patch
    if model_patch && schema.supports_model_patch() {
        println!("\n🔧 应用模型 patch...");
        crate::updater::model_patch::patch_model(&rime_dir, &schema)?;
    }

    // 7. 部署
    println!("\n🔄 部署中...");
    if let Err(e) = crate::deployer::deploy() {
        println!("⚠️  部署失败: {e}");
        println!("请手动重新部署 Rime");
    }

    println!("\n✅ 初始化完成！\n");
    println!("使用 TUI 请直接运行: rime-init");
    println!("更新请运行: rime-init --update");

    Ok(())
}

/// TUI 模式 - 简单的文本交互菜单
pub async fn run_tui() -> Result<()> {
    let mut manager = Manager::new()?;
    let schema = manager.config.schema;

    println!("\n╔══════════════════════════════╗");
    println!("║     rime-init v0.1.0        ║");
    println!("║  Rime 初始化与更新工具       ║");
    println!("╚══════════════════════════════╝\n");
    println!("当前方案: {}", schema.display_name());
    println!("Rime 目录: {}\n", manager.rime_dir.display());

    println!("  1. 一键更新");
    println!("  2. 更新方案");
    println!("  3. 更新词库");
    println!("  4. 更新模型");
    println!("  5. 模型 Patch");
    println!("  6. 皮肤 Patch");
    println!("  7. 切换方案");
    println!("  8. 配置");
    println!("  0. 退出\n");

    print!("请选择: ");
    std::io::Write::flush(&mut std::io::stdout())?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    match input.trim() {
        "1" => {
            let cache_dir = manager.cache_dir.clone();
            let rime_dir = manager.rime_dir.clone();
            updater::update_all(&schema, &manager.config, cache_dir, rime_dir, |msg, pct| {
                print!("\r  [{:3.0}%] {}", pct * 100.0, msg);
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }).await?;
        }
        "2" => {
            let base = updater::BaseUpdater::new(&manager.config, manager.cache_dir.clone(), manager.rime_dir.clone())?;
            let scheme = updater::SchemeUpdater { base };
            scheme.run(&schema, &manager.config, |msg, pct| {
                print!("\r  [{:3.0}%] {}", pct * 100.0, msg);
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }).await?;
        }
        "3" => {
            if schema.dict_zip().is_some() {
                let base = updater::BaseUpdater::new(&manager.config, manager.cache_dir.clone(), manager.rime_dir.clone())?;
                let dict = updater::DictUpdater { base };
                dict.run(&schema, &manager.config, |msg, pct| {
                    print!("\r  [{:3.0}%] {}", pct * 100.0, msg);
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                }).await?;
            } else {
                println!("此方案无独立词库");
            }
        }
        "4" => {
            if schema.supports_model_patch() {
                let base = updater::BaseUpdater::new(&manager.config, manager.cache_dir.clone(), manager.rime_dir.clone())?;
                let model = updater::ModelUpdater { base };
                model.run(&manager.config, |msg, pct| {
                    print!("\r  [{:3.0}%] {}", pct * 100.0, msg);
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                }).await?;
            } else {
                println!("此方案不支持模型");
            }
        }
        "5" => {
            if schema.supports_model_patch() {
                if crate::updater::model_patch::is_model_patched(&manager.rime_dir, &schema) {
                    print!("模型已 patch，是否移除? (y/N): ");
                    std::io::Write::flush(&mut std::io::stdout())?;
                    input.clear();
                    std::io::stdin().read_line(&mut input)?;
                    if input.trim().to_lowercase() == "y" {
                        crate::updater::model_patch::unpatch_model(&manager.rime_dir, &schema)?;
                    }
                } else {
                    crate::updater::model_patch::patch_model(&manager.rime_dir, &schema)?;
                }
            } else {
                println!("此方案不支持模型 patch");
            }
        }
        "6" => {
            println!("\n可用内置皮肤:");
            for (i, (key, name)) in crate::skin::list_available_skins().iter().enumerate() {
                println!("  {}. {} ({})", i + 1, name, key);
            }
            print!("\n选择主题编号: ");
            std::io::Write::flush(&mut std::io::stdout())?;
            input.clear();
            std::io::stdin().read_line(&mut input)?;
            if let Ok(idx) = input.trim().parse::<usize>() {
                let skins = crate::skin::list_available_skins();
                if let Some((key, _)) = skins.get(idx.saturating_sub(1)) {
                    let patch_target = manager.rime_dir.join("weasel.custom.yaml");
                    // 如果不存在则尝试 squirrel
                    let patch_target = if !patch_target.exists() {
                        manager.rime_dir.join("squirrel.custom.yaml")
                    } else {
                        patch_target
                    };
                    crate::skin::write_skin_presets(&patch_target, &[key.as_str()])?;
                    crate::skin::set_default_skin(&patch_target, key)?;
                    println!("✅ 皮肤已写入");
                }
            }
        }
        "7" => {
            println!("\n可用方案:");
            for (i, s) in Schema::all().iter().enumerate() {
                println!("  {}. {}", i + 1, s.display_name());
            }
            print!("\n选择方案编号: ");
            std::io::Write::flush(&mut std::io::stdout())?;
            input.clear();
            std::io::stdin().read_line(&mut input)?;
            if let Ok(idx) = input.trim().parse::<usize>() {
                if let Some(s) = Schema::all().get(idx.saturating_sub(1)) {
                    manager.config.schema = *s;
                    manager.save()?;
                    println!("✅ 方案已切换为: {}", s.display_name());
                }
            }
        }
        "0" | "q" => return Ok(()),
        _ => println!("未知选项"),
    }

    println!("\n按 Enter 继续...");
    std::io::stdin().read_line(&mut String::new())?;

    // 递归调用显示菜单
    Box::pin(run_tui()).await
}
