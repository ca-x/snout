use crate::i18n::{L10n, Lang};
use crate::ui::config_logic::{
    config_actions, config_enabled_label, proxy_source_label, tui_theme_mode_label,
    user_data_policy_label, user_data_policy_row_style, ConfigAction, ConfigStatusSnapshot,
};
use crate::ui::style::{
    accent_text, panel_block, primary_text, secondary_text, section_title_text, selection_style,
    tertiary_text,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Paragraph, Row, Table, Wrap},
    Frame,
};

pub(crate) struct ConfigViewData<'a> {
    pub(crate) left_lines: Vec<Line<'a>>,
    pub(crate) runtime_rows: Vec<Row<'a>>,
    pub(crate) config_title: &'a str,
    pub(crate) detail_title: &'a str,
}

pub(crate) struct ConfigScreenState<'a> {
    pub(crate) selected_index: usize,
    pub(crate) schema_name: String,
    pub(crate) config: &'a crate::types::Config,
    pub(crate) detected_engines: String,
    pub(crate) effective_proxy: Option<&'a crate::api::EffectiveProxy>,
    pub(crate) env_proxy_active: bool,
    pub(crate) config_status: &'a ConfigStatusSnapshot,
    pub(crate) is_loading: bool,
    pub(crate) rime_dir: &'a str,
    pub(crate) config_path: &'a str,
    pub(crate) lang: Lang,
}

fn config_action_index(actions: &[ConfigAction], action: ConfigAction, fallback: usize) -> usize {
    actions
        .iter()
        .position(|candidate| *candidate == action)
        .unwrap_or(fallback)
}

pub(crate) fn action_line<'a>(selected: bool, label: String, value: String) -> Line<'a> {
    let label_style = if selected {
        accent_text().add_modifier(Modifier::BOLD)
    } else {
        secondary_text()
    };

    Line::from(vec![
        Span::styled(crate::ui::style::selector_prefix(selected), accent_text()),
        Span::styled(label, label_style),
        Span::styled(value, primary_text()),
    ])
}

pub(crate) fn loading_or(is_loading: bool, loading_label: &str, value: &str) -> String {
    if is_loading {
        loading_label.to_string()
    } else {
        value.to_string()
    }
}

pub(crate) fn runtime_row<'a>(label: String, value: String) -> Row<'a> {
    Row::new(vec![label, value])
}

pub(crate) fn render_config(f: &mut Frame, area: Rect, data: ConfigViewData<'_>) {
    let chunks = if area.width < 110 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
            .split(area)
    };

    let left = Paragraph::new(data.left_lines)
        .wrap(Wrap { trim: false })
        .block(panel_block(data.config_title));
    let right = Table::new(
        data.runtime_rows,
        [Constraint::Percentage(36), Constraint::Percentage(64)],
    )
    .column_spacing(1)
    .block(panel_block(data.detail_title))
    .style(primary_text())
    .row_highlight_style(selection_style());

    f.render_widget(left, chunks[0]);
    f.render_widget(right, chunks[1]);
}

pub(crate) fn render_config_screen(
    f: &mut Frame,
    area: Rect,
    t: &L10n,
    state: ConfigScreenState<'_>,
) {
    let data = build_config_view_data(t, state);
    render_config(f, area, data);
}

pub(crate) fn build_config_view_data<'a>(
    t: &'a L10n,
    state: ConfigScreenState<'a>,
) -> ConfigViewData<'a> {
    let actions = config_actions(state.config);
    let left_lines = vec![
        Line::from(vec![Span::styled(
            format!("  {}:", t.t("config.features_section")),
            section_title_text(),
        )]),
        action_line(
            state.selected_index == config_action_index(&actions, ConfigAction::TuiTheme, 0),
            format!("{}: ", t.t("config.tui_theme_label")),
            tui_theme_mode_label(&state.config.tui_theme_mode, state.lang).to_string(),
        ),
        action_line(
            state.selected_index == config_action_index(&actions, ConfigAction::UserDataPolicy, 1),
            format!("{}: ", t.t("config.user_data_policy_label")),
            user_data_policy_label(&state.config.user_data_policy, state.lang).to_string(),
        ),
        action_line(
            state.selected_index == config_action_index(&actions, ConfigAction::ExcludeRules, 2),
            format!("{}: ", t.t("config.exclude_rules_label")),
            t.t("hint.edit").into(),
        ),
        if state.config.schema.is_wanxiang() {
            action_line(
                state.selected_index
                    == config_action_index(&actions, ConfigAction::WanxiangDiagnosis, 3),
                format!("{}: ", t.t("config.wanxiang_diagnosis_label")),
                t.t("hint.view").into(),
            )
        } else {
            Line::from("")
        },
        action_line(
            state.selected_index == config_action_index(&actions, ConfigAction::Mirror, 4),
            format!("{}: ", t.t("config.mirror_label")),
            config_enabled_label(state.config.use_mirror, t),
        ),
        action_line(
            state.selected_index == config_action_index(&actions, ConfigAction::DownloadThreads, 3),
            format!("{}: ", t.t("config.download_threads_label")),
            state.config.download_threads.to_string(),
        ),
        action_line(
            state.selected_index == config_action_index(&actions, ConfigAction::Language, 4),
            format!("{}: ", t.t("config.language_label")),
            if state.lang == Lang::Zh {
                t.t("config.lang.zh").to_string()
            } else {
                t.t("config.lang.en").to_string()
            },
        ),
        action_line(
            state.selected_index == config_action_index(&actions, ConfigAction::ProxyEnabled, 5),
            format!("{}: ", t.t("config.proxy_label")),
            config_enabled_label(
                state.config.proxy_enabled || state.effective_proxy.is_some(),
                t,
            ),
        ),
        if state.config.proxy_enabled {
            action_line(
                state.selected_index == config_action_index(&actions, ConfigAction::ProxyType, 5),
                format!("{}: ", t.t("config.proxy_type_label")),
                if state.config.proxy_type == "http" {
                    t.t("config.proxy_type_http").into()
                } else {
                    t.t("config.proxy_type_socks5").into()
                },
            )
        } else {
            Line::from("")
        },
        if state.config.proxy_enabled {
            action_line(
                state.selected_index
                    == config_action_index(&actions, ConfigAction::ProxyAddress, 6),
                format!("{}: ", t.t("config.proxy_address_label")),
                if state.config.proxy_address.trim().is_empty() {
                    t.t("config.none").into()
                } else {
                    state.config.proxy_address.clone()
                },
            )
        } else {
            Line::from("")
        },
        if state.env_proxy_active {
            Line::from(vec![
                Span::styled("   ", ratatui::style::Style::default()),
                Span::styled(t.t("config.proxy_env_readonly"), tertiary_text()),
            ])
        } else {
            Line::from("")
        },
        action_line(
            state.selected_index == config_action_index(&actions, ConfigAction::ModelPatch, 7),
            format!("{}: ", t.t("config.model_patch_label")),
            config_enabled_label(state.config.model_patch_enabled, t),
        ),
        action_line(
            state.selected_index
                == config_action_index(&actions, ConfigAction::CandidatePageSize, 8),
            format!("{}: ", t.t("config.candidate_page_size_label")),
            loading_or(
                state.is_loading,
                t.t("config.loading"),
                &state.config_status.candidate_page_size,
            ),
        ),
        action_line(
            state.selected_index == config_action_index(&actions, ConfigAction::EngineSync, 9),
            format!("{}: ", t.t("config.engine_sync_label")),
            config_enabled_label(state.config.engine_sync_enabled, t),
        ),
        action_line(
            state.selected_index == config_action_index(&actions, ConfigAction::SyncStrategy, 10),
            format!("{}: ", t.t("config.sync_strategy_label")),
            if state.config.engine_sync_use_link {
                t.t("config.sync_link").into()
            } else {
                t.t("config.sync_copy").into()
            },
        ),
        Line::from(""),
        action_line(
            state.selected_index == config_action_index(&actions, ConfigAction::Refresh, 11),
            format!("{}: ", t.t("hint.refresh")),
            t.t("hint.confirm").into(),
        ),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("  {}", t.t("config.back")),
            tertiary_text(),
        )]),
    ];

    let runtime_rows = vec![
        runtime_row(t.t("config.current_scheme").to_string(), state.schema_name),
        runtime_row(
            t.t("config.user_data_policy_label").to_string(),
            user_data_policy_label(&state.config.user_data_policy, state.lang).to_string(),
        )
        .style(user_data_policy_row_style(&state.config.user_data_policy)),
        runtime_row(
            t.t("config.detected_engines").to_string(),
            state.detected_engines,
        ),
        runtime_row(
            t.t("config.proxy_source_label").to_string(),
            proxy_source_label(state.effective_proxy, t),
        ),
        runtime_row(
            t.t("config.proxy_value_label").to_string(),
            state
                .effective_proxy
                .map(|proxy| proxy.url.clone())
                .unwrap_or_else(|| t.t("config.none").into()),
        ),
        runtime_row(
            t.t("config.scheme_status_label").to_string(),
            loading_or(
                state.is_loading,
                t.t("config.loading"),
                &state.config_status.scheme_status,
            ),
        ),
        runtime_row(
            t.t("config.dict_status_label").to_string(),
            loading_or(
                state.is_loading,
                t.t("config.loading"),
                &state.config_status.dict_status,
            ),
        ),
        runtime_row(
            t.t("config.model_status_label").to_string(),
            loading_or(
                state.is_loading,
                t.t("config.loading"),
                &state.config_status.model_status,
            ),
        ),
        runtime_row(
            t.t("config.model_patch_status_label").to_string(),
            loading_or(
                state.is_loading,
                t.t("config.loading"),
                &state.config_status.model_patch_status,
            ),
        ),
        runtime_row(
            t.t("config.candidate_page_size_label").to_string(),
            loading_or(
                state.is_loading,
                t.t("config.loading"),
                &state.config_status.candidate_page_size,
            ),
        ),
        runtime_row(
            t.t("config.rime_dir").to_string(),
            state.rime_dir.to_string(),
        ),
        runtime_row(
            t.t("config.config_file").to_string(),
            state.config_path.to_string(),
        ),
    ];

    ConfigViewData {
        left_lines,
        runtime_rows,
        config_title: t.t("config.title"),
        detail_title: t.t("menu.result"),
    }
}

pub(crate) struct ExcludeRulesViewData<'a> {
    pub(crate) help_text: &'a str,
    pub(crate) effective_count_label: &'a str,
    pub(crate) patterns_len: usize,
    pub(crate) descriptions: Vec<String>,
    pub(crate) selected_index: usize,
    pub(crate) add_label: &'a str,
    pub(crate) reset_label: &'a str,
    pub(crate) examples_label: &'a str,
    pub(crate) title: &'a str,
}

pub(crate) fn render_exclude_rules(f: &mut Frame, area: Rect, data: ExcludeRulesViewData<'_>) {
    let mut lines: Vec<Line> = vec![
        Line::from(vec![Span::styled(data.help_text, secondary_text())]),
        Line::from(vec![Span::styled(
            format!("{}: {}", data.effective_count_label, data.patterns_len),
            tertiary_text(),
        )]),
        Line::from(""),
    ];
    lines.extend(data.descriptions.iter().enumerate().map(|(i, desc)| {
        let prefix = crate::ui::style::selector_prefix(i == data.selected_index);
        let style = if i == data.selected_index {
            selection_style()
        } else {
            primary_text()
        };
        Line::from(vec![Span::styled(format!("{prefix}{desc}"), style)])
    }));
    lines.push(Line::from(""));

    let add_index = data.patterns_len;
    let reset_index = data.patterns_len + 1;

    lines.push(Line::from(vec![Span::styled(
        format!(
            "{}{}",
            crate::ui::style::selector_prefix(data.selected_index == add_index),
            data.add_label
        ),
        if data.selected_index == add_index {
            selection_style()
        } else {
            accent_text()
        },
    )]));
    lines.push(Line::from(vec![Span::styled(
        format!(
            "{}{}",
            crate::ui::style::selector_prefix(data.selected_index == reset_index),
            data.reset_label
        ),
        if data.selected_index == reset_index {
            selection_style()
        } else {
            tertiary_text()
        },
    )]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        data.examples_label,
        tertiary_text(),
    )]));

    let panel = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(panel_block(data.title));
    f.render_widget(panel, area);
}

pub(crate) struct WanxiangDiagnosisViewData<'a> {
    pub(crate) title: &'a str,
    pub(crate) failed_label: &'a str,
    pub(crate) current_scheme_label: &'a str,
    pub(crate) markers_label: &'a str,
    pub(crate) error_message: Option<String>,
    pub(crate) detected_schema: String,
    pub(crate) record_schema: String,
    pub(crate) config_schema: String,
    pub(crate) custom_patch_schema: String,
    pub(crate) marker_files: Vec<(String, bool)>,
}

pub(crate) fn render_wanxiang_diagnosis(
    f: &mut Frame,
    area: Rect,
    data: WanxiangDiagnosisViewData<'_>,
) {
    if let Some(error_message) = data.error_message {
        let panel = Paragraph::new(format!("{}: {error_message}", data.failed_label))
            .block(panel_block(data.title));
        f.render_widget(panel, area);
        return;
    }

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{}: ", data.current_scheme_label), secondary_text()),
            Span::styled(data.detected_schema, primary_text()),
        ]),
        Line::from(vec![
            Span::styled("Record: ", secondary_text()),
            Span::styled(data.record_schema, primary_text()),
        ]),
        Line::from(vec![
            Span::styled("Config: ", secondary_text()),
            Span::styled(data.config_schema, primary_text()),
        ]),
        Line::from(vec![
            Span::styled("Custom: ", secondary_text()),
            Span::styled(data.custom_patch_schema, primary_text()),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(data.markers_label, section_title_text())]),
    ];

    for (path, exists) in data.marker_files {
        lines.push(Line::from(vec![
            Span::styled(
                if exists { "  ✓ " } else { "  · " },
                if exists {
                    accent_text()
                } else {
                    tertiary_text()
                },
            ),
            Span::styled(
                path,
                if exists {
                    primary_text()
                } else {
                    tertiary_text()
                },
            ),
        ]));
    }

    let panel = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(panel_block(data.title));
    f.render_widget(panel, area);
}

pub(crate) struct ConfigInputViewData<'a> {
    pub(crate) title: &'a str,
    pub(crate) hint: &'a str,
    pub(crate) value: &'a str,
    pub(crate) placeholder: &'a str,
    pub(crate) edit_title: &'a str,
}

pub(crate) fn render_config_input(f: &mut Frame, area: Rect, data: ConfigInputViewData<'_>) {
    let text = vec![
        Line::from(vec![Span::styled(
            format!("{}:", data.title),
            section_title_text(),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(data.hint, tertiary_text())]),
        Line::from(""),
        Line::from(vec![Span::styled(
            if data.value.is_empty() {
                data.placeholder
            } else {
                data.value
            },
            primary_text(),
        )]),
    ];

    let panel = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .block(panel_block(data.edit_title));
    f.render_widget(panel, area);
}
