use super::*;
use snout::application::component_status::{ComponentState, ComponentStatus};
use snout::application::progress::{component_label, Stage, StepState};

fn lang(ui: Ui) -> Lang {
    Lang::from_str(&ui.draft.get().language)
}

fn status(ui: Ui, index: usize) -> ComponentStatus {
    ui.state.with(|state| {
        if let Some(remote) = &state.remote_status {
            return [&remote.scheme, &remote.dict, &remote.model][index].clone();
        }
        let schema = state
            .config
            .as_ref()
            .map(|config| config.schema)
            .unwrap_or(Schema::WanxiangBase);
        if (index == 1 && schema.dict_zip().is_none())
            || (index == 2 && !schema.supports_model_patch())
        {
            return ComponentStatus::unsupported();
        }
        ComponentStatus {
            installed_version: state
                .versions
                .get(index)
                .map(|(_, version)| version.clone()),
            ..ComponentStatus::default()
        }
    })
}

fn status_color(state: ComponentState) -> AdaptiveColor {
    match state {
        ComponentState::Latest => crate::theme::success(),
        ComponentState::UpdateAvailable | ComponentState::NotInstalled => accent(),
        ComponentState::CheckFailed => crate::theme::warning(),
        _ => muted(),
    }
}

fn loading_dot() -> AnyElement {
    let pulse = motion().entering(
        MotionSpec::new(KeyframeTiming::new(900.0).infinite())
            .values(MotionChannel::Opacity, [1.0, 0.35, 1.0]),
    );
    view()
        .test_id("active-step-loading")
        .size(px(8.0))
        .rounded_full()
        .bg(accent)
        .bind_motion(&pulse)
        .accessible(false)
        .flex_shrink_0()
        .into_element()
}

pub(super) fn all_current(ui: Ui) -> bool {
    (0..3).all(|index| {
        matches!(
            status(ui, index).state,
            ComponentState::Latest | ComponentState::Unsupported
        )
    })
}

pub(super) fn component_cards(ui: Ui) -> AnyElement {
    view()
        .gap_3()
        .test_id("component-cards")
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .child(when(
                    move || ui.state.with(|state| state.checking_updates),
                    loading_dot,
                ))
                .child(
                    text(move || {
                        if ui.state.with(|state| state.checking_updates) {
                            return ui
                                .tr("Checking component versions…", "正在检查组件版本…")
                                .to_string();
                        }
                        let statuses: Vec<_> = (0..3).map(|index| status(ui, index)).collect();
                        let needed = statuses.iter().filter(|status| status.actionable()).count();
                        let current = statuses
                            .iter()
                            .filter(|status| status.state == ComponentState::Latest)
                            .count();
                        if all_current(ui) {
                            ui.tr("Everything is up to date", "所有组件均已是最新")
                                .into()
                        } else if needed > 0 {
                            format!(
                                "{} {needed} · {} {current}",
                                ui.tr("Need attention:", "待处理："),
                                ui.tr("Up to date:", "已是最新：")
                            )
                        } else {
                            ui.tr(
                                "Check versions to see what needs updating",
                                "检查版本后即可了解哪些组件需要更新",
                            )
                            .into()
                        }
                    })
                    .test_id("component-status-summary")
                    .text_size(px(14.0))
                    .text_color(muted),
                ),
        )
        .children(
            [
                (0, "scheme", "Scheme", "方案", UpdateAction::Scheme),
                (1, "dict", "Dictionary", "词库", UpdateAction::Dict),
                (
                    2,
                    "model",
                    "Language model",
                    "语言模型",
                    UpdateAction::Model,
                ),
            ]
            .into_iter()
            .map(move |(index, id, en, zh, action)| {
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    .p_4()
                    .rounded(px(8.0))
                    .bg(surface)
                    .child(
                        view()
                            .flex_1()
                            .min_w(px(180.0))
                            .gap_2()
                            .child(
                                view()
                                    .flex_row()
                                    .flex_wrap()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        text(move || ui.tr(en, zh))
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::MEDIUM),
                                    )
                                    .child(
                                        text(move || {
                                            if ui.state.with(|state| state.checking_updates)
                                                && status(ui, index).state
                                                    != ComponentState::Unsupported
                                            {
                                                ui.tr("Checking…", "正在检查…").to_string()
                                            } else {
                                                status(ui, index).label(lang(ui)).to_string()
                                            }
                                        })
                                        .test_id(&format!("status-{id}"))
                                        .text_size(px(12.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(move || status_color(status(ui, index).state)),
                                    ),
                            )
                            .child(
                                text(move || {
                                    let status = status(ui, index);
                                    if status.state == ComponentState::Unsupported {
                                        return ui
                                            .tr(
                                                "Included in the scheme, or not applicable",
                                                "已随方案提供，或不适用",
                                            )
                                            .to_string();
                                    }
                                    format!(
                                        "{} {}  ·  {} {}",
                                        ui.tr("Installed", "已安装"),
                                        status
                                            .installed_version
                                            .as_deref()
                                            .filter(|version| !version.is_empty())
                                            .unwrap_or("—"),
                                        ui.tr("Latest", "最新"),
                                        status
                                            .latest_version
                                            .as_deref()
                                            .filter(|version| !version.is_empty())
                                            .unwrap_or("—")
                                    )
                                })
                                .text_size(px(12.0))
                                .text_color(muted),
                            ),
                    )
                    .child(when(
                        move || status(ui, index).state != ComponentState::Unsupported,
                        move || {
                            button()
                                .test_id(&format!("update-{id}"))
                                .px_3()
                                .py_2()
                                .rounded(px(6.0))
                                .disabled(move || {
                                    ui.locked()
                                        || ui.dirty.get()
                                        || status(ui, index).state == ComponentState::Latest
                                })
                                .disabled_style(|style| style.opacity(0.5))
                                .hover(|style| style.bg(selection))
                                .active(|style| style.opacity(0.8))
                                .focus(|style| {
                                    style
                                        .outline(px(2.0))
                                        .outline_color(accent)
                                        .outline_offset(px(2.0))
                                })
                                .on_press(move || {
                                    if status(ui, index).state == ComponentState::CheckFailed {
                                        ui.send(Command::CheckUpdates);
                                    } else {
                                        ui.request(Pending::Update(action));
                                    }
                                })
                                .child(
                                    text(move || match status(ui, index).state {
                                        ComponentState::Latest => ui.tr("Up to date", "已是最新"),
                                        ComponentState::CheckFailed => {
                                            ui.tr("Retry check", "重新检查")
                                        }
                                        ComponentState::NotInstalled => ui.tr("Install…", "安装…"),
                                        _ => ui.tr("Update…", "更新…"),
                                    })
                                    .text_size(px(14.0))
                                    .text_color(accent),
                                )
                        },
                    ))
                    .into_element()
            })
            .collect::<Vec<_>>(),
        )
        .into_element()
}

fn step_color(state: StepState) -> AdaptiveColor {
    match state {
        StepState::Active => accent(),
        StepState::Complete => crate::theme::success(),
        StepState::Failed => danger(),
        StepState::Cancelled => crate::theme::warning(),
        _ => muted(),
    }
}

pub(super) fn operation_timeline(ui: Ui) -> AnyElement {
    when(move || ui.state.with(|state| !state.operation.components.is_empty()), move || {
        section(ui, "Installation progress", "安装与更新进度").test_id("operation-timeline")
            .child(caption(ui, "Completed steps stay visible. Unneeded steps are skipped.", "已完成的步骤会保留，无需执行的步骤会跳过。"))
            .child(progress().accessibility_label(move || Some(ui.tr("Overall progress", "总体进度").into()))
                .value(move || (ui.state.with(|state| state.progress) * 100.0) as f32)
                .indeterminate(move || ui.state.with(|state| state.busy && state.progress == 0.0))
                .track_class("h-1.5 rounded-full bg-muted")
                .indicator_class(move || Some(SharedString::from(if ui.state.with(|state| state.busy && state.progress == 0.0) { "h-1.5 w-1/3 rounded-full bg-primary" } else { "h-1.5 rounded-full bg-primary transition-transform duration-150" }))))
            .child(each(move || ui.state.with(|state| state.operation.components.clone()), |entry| entry.component, move |entry, index| {
                view().test_id(&format!("timeline-{:?}", entry.get().component)).flex_row().gap_3().py_2()
                    .child(view().size(px(28.0)).rounded_full().border_1().border_color(move || step_color(entry.get().state)).items_center().justify_center()
                        .child(text(move || format!("{}", index.get() + 1)).text_size(px(12.0)).text_color(move || step_color(entry.get().state))))
                    .child(view().flex_1().gap_2()
                        .child(view().flex_row().flex_wrap().items_center().gap_3()
                            .child(text(move || component_label(entry.get().component, lang(ui))).text_size(px(14.0)).font_weight(FontWeight::MEDIUM))
                            .child(when(move || entry.get().state == StepState::Active, loading_dot))
                            .child(text(move || entry.get().label(lang(ui))).text_size(px(13.0)).text_color(move || step_color(entry.get().state))))
                        .child(view().flex_row().flex_wrap().gap_3().children(Stage::ALL.into_iter().map(move |stage| {
                            text(move || {
                                let state = entry.get().step_state(stage);
                                format!("{} {}", match state { StepState::Complete => "✓", StepState::Skipped => "–", StepState::Failed => "×", StepState::Cancelled => "–", _ => "·" }, stage.label(lang(ui)))
                            }).text_size(px(12.0)).text_color(move || step_color(entry.get().step_state(stage))).into_element()
                        }).collect::<Vec<_>>())))
            }))
    }).into_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsh_core::{element::Root, Size};
    use nsh_engine::EngineConfig;
    use nsh_testing::Headless;
    use snout::application::{progress::OperationProgress, status::ConfigStatusSnapshot};
    use snout::updater::{UpdateComponent, UpdateEvent, UpdatePhase};

    #[component]
    fn ExperienceFixture(props: AppProps) -> impl IntoElement {
        crate::theme::install();
        let controller = Controller::start(Some(Paths {
            config: props.config_path.get().to_string().into(),
            rime: props.rime_path.get().to_string().into(),
            cache: props.cache_path.get().to_string().into(),
        }))
        .unwrap();
        let ui = new_ui(controller);
        ui.draft.update(|config| config.language = "en".into());
        let mut operation =
            OperationProgress::new([UpdateComponent::Scheme, UpdateComponent::Dict]);
        operation.observe(&UpdateEvent {
            component: UpdateComponent::Dict,
            phase: UpdatePhase::Downloading,
            progress: 0.4,
            detail: "diagnostic-only-detail".into(),
        });
        ui.state.set(Snapshot {
            config: Some(Config::default()),
            busy: false,
            progress: 0.4,
            remote_status: Some(ConfigStatusSnapshot {
                scheme: ComponentStatus {
                    state: ComponentState::Latest,
                    installed_version: Some("v1".into()),
                    latest_version: Some("v1".into()),
                },
                dict: ComponentStatus {
                    state: ComponentState::UpdateAvailable,
                    installed_version: Some("v1".into()),
                    latest_version: Some("v2".into()),
                },
                model: ComponentStatus::unsupported(),
                ..Default::default()
            }),
            operation,
            activity: vec!["diagnostic-only-detail".into()],
            ..Snapshot::default()
        });
        view()
            .flex_1()
            .p_6()
            .gap_4()
            .child(component_cards(ui))
            .child(activity(ui))
            .child(action(
                ui,
                "fixture-zh",
                "Chinese",
                "中文",
                ButtonStyle::Quiet,
                move || ui.draft.update(|config| config.language = "zh".into()),
            ))
            .child(action(
                ui,
                "fixture-failed",
                "Fail",
                "失败",
                ButtonStyle::Quiet,
                move || {
                    ui.state.update(|state| {
                        state.remote_status.as_mut().unwrap().scheme.state =
                            ComponentState::CheckFailed;
                        state
                            .operation
                            .finish([(UpdateComponent::Scheme, false)], true, false);
                    })
                },
            ))
            .child(action(
                ui,
                "fixture-checking",
                "Check",
                "检查",
                ButtonStyle::Quiet,
                move || ui.state.update(|state| state.checking_updates = true),
            ))
    }

    fn props(path: &std::path::Path) -> AppProps {
        AppProps {
            config_path: path
                .join("config.json")
                .to_string_lossy()
                .to_string()
                .into(),
            rime_path: path.join("rime").to_string_lossy().to_string().into(),
            cache_path: path.join("cache").to_string_lossy().to_string().into(),
        }
    }

    #[test]
    fn freshness_actions_timeline_and_diagnostics_are_explicit_in_both_languages() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_owned();
        let root = Root::new::<ExperienceFixture>(move || props(&path));
        let mut app = Headless::start(EngineConfig {
            viewport: Size::new(px(1040.0), px(1400.0)),
            ui_sets: vec![nsh_base::registry()],
            ..EngineConfig::new(root)
        })
        .unwrap();
        app.wait_for("Need attention: 1 · Up to date: 1").unwrap();
        assert!(!app.screen().enabled("update-scheme").unwrap());
        assert!(app.screen().enabled("update-dict").unwrap());
        assert!(!app.screen().matches("update-model").unwrap());
        assert!(app.shows("Installation progress"));
        assert!(!app.shows("diagnostic-only-detail"));
        app.press("toggle-operation-log").unwrap();
        app.wait_for("diagnostic-only-detail").unwrap();
        app.set_environment("dark and reduced motion", |environment| {
            environment.color_scheme = nsh_engine::ColorScheme::Dark;
            environment.reduce_motion = true;
        })
        .unwrap();
        app.press("fixture-failed").unwrap();
        app.wait_for("Check failed — retry").unwrap();
        assert!(app.screen().enabled("update-scheme").unwrap());
        app.wait_for("Cancelled").unwrap();
        app.press("fixture-zh").unwrap();
        app.wait_for("检查失败，请重试").unwrap();
        app.wait_for("已取消").unwrap();
        app.press("fixture-checking").unwrap();
        app.wait_for("正在检查组件版本…").unwrap();
        assert!(!directory.path().join("rime").exists());
    }

    #[cfg(all(target_os = "linux", feature = "native"))]
    #[test]
    #[ignore = "requires a native display; isolated update experience visual review"]
    fn native_update_experience_visual() {
        use nsh_platform_linux as platform;
        platform::gtk4::init().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_owned();
        let root = platform::Root::new::<ExperienceFixture>(move || props(&path));
        let _ = platform::run_app(
            platform::AppSource::Rust(root),
            platform::AppOptions {
                title: "snout update experience review".into(),
                app_id: "io.github.snout.update-review".into(),
                size: (1040, 1000),
                ..Default::default()
            },
        );
        assert!(!directory.path().join("rime").exists());
    }
}
