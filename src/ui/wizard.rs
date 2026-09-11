use crate::config::{self, Manager};
use crate::i18n::{L10n, Lang};
use crate::types::Schema;
use crate::updater;
use anyhow::Result;

/// 首次初始化向导
pub async fn run_init_wizard(language: Option<&str>) -> Result<()> {
    let mut manager = Manager::new()?;
    if let Some(language) = language {
        manager.config.language = language.into();
    }
    let lang = Lang::from_str(&manager.config.language);
    let t = L10n::new(lang);

    println!("\n🚀 snout {}\n", t.t("wizard.title"));

    // 1. 检测引擎
    let engines = config::detect_installed_engines();
    if engines.is_empty() {
        println!("{}", config::rime_installation_message(lang));
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
    manager.config.model_patch_enabled = model_patch;
    manager.save()?;

    // 5. 执行更新
    println!("\n📦 {}...\n", t.t("wizard.downloading"));
    let cache_dir = manager.cache_dir.clone();
    let rime_dir = manager.rime_dir.clone();

    let results = updater::update_all(
        &schema,
        &manager.config,
        cache_dir,
        rime_dir.clone(),
        crate::types::CancelSignal::new(),
        |event| {
            print!("\r  [{:3.0}%] {}", event.progress * 100.0, event.detail);
            std::io::Write::flush(&mut std::io::stdout()).ok();
        },
    )
    .await?;

    if !results.iter().all(|result| result.success) {
        for result in results.iter().filter(|result| !result.success) {
            eprintln!("\n❌ {}: {}", result.component, result.message);
        }
        return Ok(());
    }
    println!("\n✅ {}!\n", t.t("wizard.complete"));
    // The updater may have persisted a newly selected schema.
    manager = Manager::new()?;
    if updater::should_prompt_rime_setup(&manager.config) {
        if let Some(enabled) =
            prompt_rime_auto_setup(&mut std::io::stdin().lock(), &mut std::io::stdout(), &t)?
        {
            manager.config.rime_auto_setup = Some(enabled);
            manager.save()?;
            updater::setup_rime(
                &manager.config,
                &crate::types::CancelSignal::new(),
                |event| {
                    println!("{}", event.detail);
                },
            )
            .await?;
        }
    }
    println!("{}", t.t("wizard.open_tui"));

    Ok(())
}

fn prompt_rime_auto_setup(
    input: &mut impl std::io::BufRead,
    output: &mut impl std::io::Write,
    t: &L10n,
) -> Result<Option<bool>> {
    writeln!(output, "{}", t.t("rime.setup.detail"))?;
    loop {
        write!(output, "{} (y/N): ", t.t("rime.setup.prompt"))?;
        output.flush()?;
        let mut answer = String::new();
        if input.read_line(&mut answer)? == 0 {
            return Ok(None);
        }
        match answer.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(Some(true)),
            "" | "n" | "no" => return Ok(Some(false)),
            _ => continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_prompt_requires_yes_and_keeps_eof_undecided() {
        for lang in [Lang::Zh, Lang::En] {
            for (answer, expected) in [
                ("y\n", Some(true)),
                (" YES \n", Some(true)),
                ("n\n", Some(false)),
                ("No\n", Some(false)),
                ("\n", Some(false)),
                ("", None),
                ("typo\ny\n", Some(true)),
            ] {
                let t = L10n::new(lang);
                let mut output = Vec::new();
                assert_eq!(
                    prompt_rime_auto_setup(&mut answer.as_bytes(), &mut output, &t).unwrap(),
                    expected
                );
                let text = String::from_utf8(output).unwrap();
                assert!(text.contains(t.t("rime.setup.prompt")));
                assert!(text.contains(t.t("rime.setup.detail")));
                assert!(!text.contains("rime.setup."));
            }
        }
    }
}
