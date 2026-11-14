use crate::application::progress::{component_label, OperationProgress, Stage, StepState};
use crate::i18n::L10n;
use crate::ui::style::{
    color_accent, color_border, color_danger, color_secondary, color_selection_bg, color_success,
    color_warning, panel_block, primary_text, secondary_text, tertiary_text,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdateOutcome {
    Success,
    Partial,
    Failure,
    Cancelled,
}

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Gauge, Paragraph, Wrap},
    Frame,
};

pub(crate) struct UpdatingViewData<'a> {
    pub(crate) update_msg: &'a str,
    pub(crate) update_pct: f64,
    pub(crate) operation: &'a OperationProgress,
    pub(crate) animation_frame: usize,
}

pub(crate) struct ResultViewData<'a> {
    pub(crate) update_done: bool,
    pub(crate) update_outcome: Option<UpdateOutcome>,
    pub(crate) update_msg: &'a str,
    pub(crate) update_user_data_policy_summary: Option<&'a str>,
    pub(crate) update_results: &'a [String],
}

pub(crate) fn render_updating(f: &mut Frame, area: Rect, t: &L10n, data: UpdatingViewData<'_>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(3),
        ])
        .split(area);

    let msg = Paragraph::new(Line::from(vec![
        Span::styled("  >>> ", crate::ui::style::accent_text()),
        Span::styled(data.update_msg, primary_text()),
    ]))
    .block(panel_block(t.t("update.checking")));
    f.render_widget(msg, chunks[0]);

    let gauge = Gauge::default()
        .block(panel_block(t.t("update.progress")))
        .gauge_style(Style::default().fg(color_accent()).bg(color_selection_bg()))
        .ratio(data.update_pct)
        .label(format!("{:.0}%", data.update_pct * 100.0));
    f.render_widget(gauge, chunks[1]);

    let stage_panel = Paragraph::new(timeline_lines(t, data.operation, data.animation_frame))
        .wrap(Wrap { trim: true })
        .block(panel_block(t.t("update.status_section")));
    f.render_widget(stage_panel, chunks[2]);
}

pub(crate) fn timeline_lines(
    t: &L10n,
    operation: &OperationProgress,
    animation_frame: usize,
) -> Vec<Line<'static>> {
    if operation.components.is_empty() {
        vec![Line::from(vec![Span::styled(
            format!("  {}", t.t("hint.wait")),
            tertiary_text(),
        )])]
    } else {
        operation
            .components
            .iter()
            .enumerate()
            .flat_map(|(index, entry)| {
                let marker = match entry.state {
                    StepState::Active => {
                        ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"][animation_frame % 8]
                    }
                    StepState::Complete => "✓",
                    StepState::Failed => "×",
                    StepState::Cancelled | StepState::Skipped => "–",
                    StepState::Pending => "·",
                };
                let color = match entry.state {
                    StepState::Active => color_accent(),
                    StepState::Complete => color_success(),
                    StepState::Failed => color_danger(),
                    StepState::Cancelled => color_warning(),
                    _ => color_secondary(),
                };
                vec![
                    Line::from(Span::styled(
                        format!(
                            "  {}. {marker} {} — {}",
                            index + 1,
                            component_label(entry.component, t.lang()),
                            entry.label(t.lang())
                        ),
                        Style::default().fg(color),
                    )),
                    Line::from(
                        Stage::ALL
                            .into_iter()
                            .map(|stage| {
                                let marker = match entry.step_state(stage) {
                                    StepState::Complete => "✓",
                                    StepState::Active => "›",
                                    StepState::Failed => "×",
                                    StepState::Skipped | StepState::Cancelled => "–",
                                    StepState::Pending => "·",
                                };
                                Span::styled(
                                    format!("   {marker} {}", stage.label(t.lang())),
                                    secondary_text(),
                                )
                            })
                            .collect::<Vec<_>>(),
                    ),
                    Line::from(""),
                ]
            })
            .collect()
    }
}

pub(crate) fn render_result(f: &mut Frame, area: Rect, t: &L10n, data: ResultViewData<'_>) {
    let title = if data.update_done {
        format!(" {} ", t.t("menu.done"))
    } else {
        format!(" {} ", t.t("menu.result"))
    };
    let (accent, status_color) = match data.update_outcome {
        Some(UpdateOutcome::Success) => (color_border(), color_success()),
        Some(UpdateOutcome::Partial) => (color_border(), color_warning()),
        Some(UpdateOutcome::Failure) => (color_border(), color_danger()),
        Some(UpdateOutcome::Cancelled) => (color_border(), color_warning()),
        None => (color_border(), color_secondary()),
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                data.update_msg,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
    ];

    if let Some(policy) = data.update_user_data_policy_summary {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {}: ", t.t("config.user_data_policy_label")),
                secondary_text(),
            ),
            Span::styled(policy.to_string(), primary_text()),
        ]));
        lines.push(Line::from(""));
    }

    for result in data.update_results {
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(result, primary_text()),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        format!("  >>> {}", t.t("result.back_to_menu")),
        tertiary_text(),
    )]));

    let result_panel = Paragraph::new(lines)
        .block(panel_block(&title).border_style(Style::default().fg(accent)))
        .wrap(Wrap { trim: true });
    f.render_widget(result_panel, area);
}
