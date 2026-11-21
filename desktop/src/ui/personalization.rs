use super::*;
use nsh_base::{radio, radio_group, RadioElement};
use snout::i18n::L10n;

fn choice(
    ui: Ui,
    id: &str,
    checked: impl Fn() -> bool + 'static,
    choose: impl Fn() + 'static,
) -> RadioElement {
    radio()
        .test_id(id)
        .checked(checked)
        .disabled(move || ui.locked())
        .on_change(move |_: &bool| choose())
        .app_class(Some("rounded-md border border-border bg-card checked:border-primary checked:bg-accent checked:font-bold focus:outline-2 focus:outline-ring focus:outline-offset-2 disabled:opacity-50".into()))
        .min_h(px(38.0))
        .px_3()
        .py_2()
        .justify_center()
}

fn scheme_family(ui: Ui, schema: Schema) -> AnyElement {
    let wanxiang = schema == Schema::WanxiangBase;
    let checked = move || {
        ui.draft.with(|config| {
            if wanxiang {
                config.schema.is_wanxiang()
            } else {
                config.schema == schema
            }
        })
    };
    let choose = move || {
        let current = ui.draft.with(|config| config.schema);
        if current.is_wanxiang() {
            ui.last_wanxiang.set(current);
        }
        let next = if wanxiang {
            ui.last_wanxiang.get()
        } else {
            schema
        };
        ui.edit(|config| config.schema = next);
    };
    let name = move || {
        if wanxiang {
            ui.tr("Wanxiang Pinyin", "万象拼音").to_owned()
        } else {
            ui.scheme_name(schema)
        }
    };
    choice(ui, &format!("schema-{schema:?}"), checked, choose)
        .accessibility_label(move || Some(name().into()))
        .basis(relative(0.30))
        .min_w(px(172.0))
        .flex_grow(1.0)
        .flex_row()
        .items_center()
        .justify_between()
        .gap_2()
        .p_3()
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .gap_1()
                .child(
                    text(name)
                        .text_size(px(16.0))
                        .font_weight(FontWeight::MEDIUM),
                )
                .child(
                    text(format!("{}/{}", schema.owner(), schema.repo()))
                        .text_color(muted)
                        .text_size(px(12.0))
                        .number_of_lines(1),
                ),
        )
        .child(
            text(move || if checked() { "✓" } else { "" })
                .accessible(false)
                .text_color(accent)
                .text_size(px(16.0)),
        )
        .into_element()
}

fn wanxiang_editions(ui: Ui) -> AnyElement {
    let lang = computed(move || ui.lang());
    view().gap_2().pt_4().border_t_1().border_color(line)
        .child(dynamic(move || {
            let labels = L10n::new(lang.get());
            let editions = Schema::all().iter().filter(|s| s.is_wanxiang())
                .map(|s| (s.i18n_key().into(), labels.t(s.i18n_key()).into()))
                .collect();
            picker(ui, "wanxiang-edition", "Wanxiang edition", "万象版本", editions,
                move || ui.draft.with(|c| c.schema.i18n_key().into()),
                move |key| {
                    if let Some(&edition) = Schema::all().iter().find(|s| s.i18n_key() == key) {
                        ui.edit(|config| config.schema = edition);
                    }
                })
        }))
        .child(text(move || ui.tr(
            "Base supports full and double pinyin. Choose a Pro edition for the auxiliary code you use.",
            "标准版支持全拼和双拼。使用辅助码时，选择对应的 Pro 版本。"
        )).text_size(px(13.0)).text_color(muted))
        .into_element()
}

pub(super) fn schemes(ui: Ui) -> AnyElement {
    let families = Schema::all()
        .iter()
        .copied()
        .filter(|schema| !schema.is_wanxiang() || *schema == Schema::WanxiangBase);
    section(ui, "Choose a scheme", "选择输入方案")
        .child(
            radio_group()
                .flex_row()
                .flex_wrap()
                .gap_3()
                .children(families.map(move |schema| scheme_family(ui, schema))),
        )
        .child(when(
            move || ui.draft.with(|c| c.schema.is_wanxiang()),
            move || wanxiang_editions(ui),
        ))
        .child(when(
            move || ui.draft.with(|c| c.schema.supports_model_patch()),
            move || {
                toggle(
                    ui,
                    "model-patch",
                    "Enable language model patch",
                    "启用语言模型补丁",
                    |config| config.model_patch_enabled,
                    |config, value| config.model_patch_enabled = value,
                )
            },
        ))
        .into_element()
}

fn candidate_preview(ui: Ui) -> AnyElement {
    const WORDS: [&str; 9] = ["是", "时", "事", "市", "十", "使", "试", "式", "诗"];
    view()
        .test_id("candidate-preview")
        .p_4()
        .gap_3()
        .rounded(px(8.0))
        .bg(canvas)
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_3()
                .child(
                    text("shi")
                        .text_size(px(18.0))
                        .font_weight(FontWeight::MEDIUM),
                )
                .child(
                    text(move || {
                        if ui.candidate.get().is_none() {
                            ui.tr(
                                "Layout example with 5 candidates",
                                "默认数量由方案决定，此处以 5 项示意",
                            )
                        } else {
                            ui.tr("Layout preview", "候选布局示例")
                        }
                    })
                    .text_color(muted)
                    .text_size(px(12.0)),
                ),
        )
        .child(dynamic(move || {
            let count = usize::from(ui.candidate.get().unwrap_or(5)).clamp(1, WORDS.len());
            view().flex_row().flex_wrap().gap_1().children(
                WORDS
                    .into_iter()
                    .take(count)
                    .enumerate()
                    .map(|(index, word)| {
                        let color = move || if index == 0 { on_accent() } else { ink() };
                        view()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_2()
                            .rounded(px(6.0))
                            .bg(move || if index == 0 { accent() } else { canvas() })
                            .child(
                                text((index + 1).to_string())
                                    .text_size(px(12.0))
                                    .text_color(
                                        move || if index == 0 { on_accent() } else { muted() },
                                    ),
                            )
                            .child(text(word).text_color(color).text_size(px(16.0)))
                    }),
            )
        }))
        .into_element()
}

fn candidate_choice(ui: Ui, size: Option<u8>) -> AnyElement {
    let id = size.map_or_else(|| "default".into(), |n| n.to_string());
    let label = move || {
        size.map_or_else(
            || ui.tr("Default", "跟随方案").to_owned(),
            |n| n.to_string(),
        )
    };
    choice(
        ui,
        &format!("candidate-count-{id}"),
        move || ui.candidate.get() == size,
        move || {
            ui.stop_countdown();
            ui.candidate.set(size);
        },
    )
    .accessibility_label(move || {
        Some(
            match (ui.lang(), size) {
                (_, None) => ui.tr("Scheme default", "跟随方案").into(),
                (Lang::Zh, Some(n)) => format!("每页 {n} 个候选词"),
                (Lang::En, Some(1)) => "1 candidate per page".into(),
                (Lang::En, Some(n)) => format!("{n} candidates per page"),
            }
            .into(),
        )
    })
    .min_w(px(38.0))
    .items_center()
    .child(text(label).text_size(px(14.0)))
    .into_element()
}

pub(super) fn candidate_window(ui: Ui) -> AnyElement {
    when(
        move || ui.state.with(|state| state.candidate_size.is_ok()),
        move || candidate_controls(ui),
    )
    .otherwise(move || {
        section(ui, "Candidate window", "候选窗口")
            .child(caption(
                ui,
                "Could not read the saved candidate count.",
                "无法读取已保存的候选数。",
            ))
            .child(
                text(move || {
                    ui.state
                        .with(|s| s.candidate_size.as_ref().err().cloned().unwrap_or_default())
                })
                .text_size(px(13.0))
                .text_color(muted),
            )
            .child(view().items_start().child(action_enabled(
                ui,
                "retry-candidate-read",
                "Read again",
                "重新读取",
                ButtonStyle::Secondary,
                move || !ui.dirty.get(),
                move || ui.send(Command::Refresh),
            )))
    })
    .into_element()
}

fn candidate_controls(ui: Ui) -> AnyElement {
    section(ui, "Candidate window", "候选窗口")
        .child(
            text(move || {
                ui.state.with(|state| {
                    let Some(config) = &state.config else {
                        return String::new();
                    };
                    let name = ui.scheme_name(config.schema);
                    match &state.candidate_size {
                        Ok(Some(size)) if ui.lang() == Lang::Zh => {
                            format!("{name} · 已保存每页 {size} 项")
                        }
                        Ok(Some(size)) => format!("{name} · Saved count: {size}"),
                        Err(_) => name,
                        Ok(None) => {
                            format!("{name} · {}", ui.tr("Scheme default", "跟随方案"))
                        }
                    }
                })
            })
            .test_id("candidate-saved-setting")
            .text_size(px(13.0))
            .text_color(muted),
        )
        .child(
            view()
                .gap_2()
                .child(caption(ui, "Candidates per page", "每页候选数"))
                .child(
                    radio_group().flex_row().flex_wrap().gap_1().children(
                        std::iter::once(None)
                            .chain((1..=9).map(Some))
                            .map(move |size| candidate_choice(ui, size)),
                    ),
                ),
        )
        .child(
            when(
                move || ui.candidate.get().is_none_or(|n| (1..=9).contains(&n)),
                move || candidate_preview(ui),
            )
            .otherwise(move || {
                caption(
                    ui,
                    "Choose a count from 1 to 9 to preview it.",
                    "选取 1 到 9 中的数量即可预览。",
                )
            }),
        )
        .child(appearance_guard(ui))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .gap_3()
                .items_center()
                .child(action_enabled(
                    ui,
                    "apply-candidate",
                    "Apply candidate count",
                    "应用候选数",
                    ButtonStyle::Primary,
                    move || {
                        !ui.dirty.get()
                            && (appearance_changed(ui, AppearanceTask::Candidate)
                                || appearance_failed(ui, AppearanceTask::Candidate))
                    },
                    move || ui.send(Command::CandidateSize(ui.candidate.get())),
                ))
                .child(appearance_feedback(ui, AppearanceTask::Candidate)),
        )
        .into_element()
}
