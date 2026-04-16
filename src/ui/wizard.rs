use crate::config::{self, Manager};
use crate::i18n::{L10n, Lang};
use crate::types::Schema;
use crate::updater;
use anyhow::Result;

/// 首次初始化向导
pub async fn run_init_wizard() -> Result<()> {
    let manager = Manager::new()?;
    let lang = Lang::from_str(&manager.config.language);
    let t = L10n::new(lang);

    println!("\n🚀 snout {}\n", t.t("wizard.title"));

    // 1. 检测引擎
    let engines = config::detect_installed_engines();
    if engines.is_empty() {
        println!("⚠️  {}", t.t("wizard.no_engine"));
        println!("{}", t.t("wizard.install_one_of"));
        println!("  • Weasel - Windows");
        println!("  • Squirrel - macOS");
        println!("  • Fcitx5 + Rime - Linux");
        return Ok(());
    }
    println!(
        "✅ {}: {}\n",
        t.t("wizard.engine_found"),
        engines.join(", ")
    );

    // 2. 选择方案
    println!("{}:", t.t("wizard.select_scheme"));
    let schemas = Schema::all();
    for (i, s) in schemas.iter().enumerate() {
        println!("  {:2}. {}", i + 1, s.display_name_lang(lang));
    }
    print!("\n[1]: ");
    std::io::Write::flush(&mut std::io::stdout())?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let idx: usize = input.trim().parse::<usize>().unwrap_or(1).saturating_sub(1);
    let schema = *schemas.get(idx).unwrap_or(&schemas[0]);

    println!("✅ {}\n", schema.display_name_lang(lang));

    // 3. 模型 patch
    let mut model_patch = false;
    if schema.supports_model_patch() {
        print!("{} (y/N): ", t.t("wizard.enable_model_patch"));
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
    println!("\n📦 {}...\n", t.t("wizard.downloading"));
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
    )
    .await?;

    println!("\n✅ {}!\n", t.t("wizard.complete"));
    println!("{}", t.t("wizard.open_tui"));

    Ok(())
}
