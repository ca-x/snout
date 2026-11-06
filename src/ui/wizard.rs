use crate::application::{self, UpdateAction};
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
    anyhow::ensure!(
        !engines.is_empty(),
        "{}",
        config::rime_installation_message_with_engines(lang, &engines)
    );
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
    manager.config.schema = schema;
    manager.config.model_patch_enabled = model_patch;
    manager.save()?;

    // 5. 执行更新
    println!("\n📦 {}...\n", t.t("wizard.downloading"));
    let results = application::run_update(
        UpdateAction::All,
        &manager,
        crate::types::CancelSignal::new(),
        |event| {
            print!("\r  [{:3.0}%] {}", event.progress * 100.0, event.detail);
            std::io::Write::flush(&mut std::io::stdout()).ok();
        },
    )
    .await?;

    for result in results.iter().filter(|result| !result.success) {
        eprintln!("\n❌ {}: {}", result.component, result.message);
    }
    ensure_update_succeeded(&results)?;
    manager = manager.reload()?;
    if updater::should_prompt_rime_setup(&manager.config) {
        if let Some(enabled) =
            prompt_rime_auto_setup(&mut std::io::stdin().lock(), &mut std::io::stdout(), &t)?
        {
            manager.config.rime_auto_setup = Some(enabled);
            manager.save()?;
            let setup_result = updater::setup_rime(
                &manager.config,
                &crate::types::CancelSignal::new(),
                |event| {
                    println!("{}", event.detail);
                },
            )
            .await?;
            ensure_update_succeeded(setup_result.as_slice())?;
        }
    }
    println!("\n✅ {}!\n", t.t("wizard.complete"));
    println!("{}", t.t("wizard.open_tui"));

    Ok(())
}

fn ensure_update_succeeded(results: &[updater::UpdateResult]) -> Result<()> {
    let failures: Vec<_> = results
        .iter()
        .filter(|result| !result.success)
        .map(|result| format!("{}: {}", result.component, result.message))
        .collect();
    anyhow::ensure!(failures.is_empty(), "{}", failures.join("\n"));
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
    fn failed_update_result_is_an_error() {
        let results = [updater::BaseUpdater::error_result(
            updater::UpdateComponent::Scheme,
            "Scheme",
            "download failed",
        )];

        let error = ensure_update_succeeded(&results).expect_err("failed initialization");

        assert_eq!(error.to_string(), "Scheme: download failed");
    }

    #[test]
    fn successful_and_skipped_updates_are_successful() {
        let results = [updater::BaseUpdater::success_result(
            updater::UpdateComponent::Scheme,
            "Scheme",
            "old",
            "new",
            "updated",
        )];
        let skipped_setup: Option<updater::UpdateResult> = None;

        assert!(ensure_update_succeeded(&results).is_ok());
        assert!(ensure_update_succeeded(skipped_setup.as_slice()).is_ok());
    }

    #[test]
    fn mixed_update_results_report_every_failed_component() {
        let results = [
            updater::BaseUpdater::success_result(
                updater::UpdateComponent::Scheme,
                "Scheme",
                "old",
                "new",
                "updated",
            ),
            updater::BaseUpdater::error_result(
                updater::UpdateComponent::Dict,
                "Dictionary",
                "download failed",
            ),
            updater::BaseUpdater::error_result(
                updater::UpdateComponent::Model,
                "Model",
                "checksum mismatch",
            ),
        ];

        let error = ensure_update_succeeded(&results).expect_err("failed initialization");

        assert_eq!(
            error.to_string(),
            "Dictionary: download failed\nModel: checksum mismatch"
        );
    }

    #[test]
    fn failed_optional_setup_result_is_an_error() {
        let setup_result = Some(updater::BaseUpdater::error_result(
            updater::UpdateComponent::Fcitx5Setup,
            "Rime setup",
            "session bus unavailable",
        ));

        let error =
            ensure_update_succeeded(setup_result.as_slice()).expect_err("failed optional setup");

        assert_eq!(error.to_string(), "Rime setup: session bus unavailable");
    }

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
