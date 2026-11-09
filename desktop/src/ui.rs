use crate::controller::{self, Command, Controller, Paths, Snapshot, Status};
use crate::theme::{
    accent, canvas, danger, ink, line, muted, on_accent, selection, sidebar_surface, surface,
};
use base64::prelude::*;
use nsh_base::theme;
use nsh_base::{button, input, progress, select, switch, SelectItem, SelectValue};
use nsh_core::animation::{KeyframeTiming, MotionChannel, MotionSpec};
use nsh_core::element::{on_key, ImageSrc};
use nsh_core::file_dialog::SaveDialog;
use nsh_core::prelude::*;
use snout::application::{self, UpdateAction};
use snout::cli::CompletionShell;
use snout::i18n::Lang;
use snout::skin;
use snout::types::{Config, Schema};
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

#[derive(Clone, PartialEq, Props)]
pub struct AppProps {
    #[prop(default)]
    pub config_path: Prop<SharedString>,
    #[prop(default)]
    pub rime_path: Prop<SharedString>,
    #[prop(default)]
    pub cache_path: Prop<SharedString>,
}

trait ComponentTestId: ComponentBuilder {
    fn test_id(mut self, id: &str) -> Self {
        let element = self.__element();
        let initial = ComponentElement::__new(
            <<Self::Component as Component>::Props as nsh_core::element::Props>::initial(),
        );
        let original = std::mem::replace(element, initial);
        *element = original.test_id(id);
        self
    }
}

impl<Builder: ComponentBuilder> ComponentTestId for Builder {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Overview,
    Schemes,
    Appearance,
    Settings,
    Activity,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    Update(UpdateAction),
    Save,
    ResetExclusions,
    ResetCandidate,
    Skins,
    Fcitx,
}

#[derive(Clone, Copy)]
struct Ui {
    controller: Stored<Controller>,
    state: Signal<Snapshot>,
    draft: Signal<Config>,
    dirty: Signal<bool>,
    saving: Signal<bool>,
    saving_exclusion: Signal<bool>,
    page: Signal<Page>,
    error: Signal<Option<String>>,
    pending: Signal<Option<Pending>>,
    exclusion: Signal<String>,
    editing_exclusion: Signal<Option<usize>>,
    candidate: Signal<String>,
    selected_skins: Signal<HashSet<String>>,
    default_skin: Signal<String>,
    light_theme: Signal<String>,
    dark_theme: Signal<String>,
    light_round: Signal<bool>,
    dark_round: Signal<bool>,
    diagnostics_open: Signal<bool>,
}

impl Ui {
    fn tr(self, en: &'static str, zh: &'static str) -> &'static str {
        if self.draft.with(|config| config.language == "zh") {
            zh
        } else {
            en
        }
    }

    fn locked(self) -> bool {
        self.state
            .with(|state| state.busy || state.config.is_none())
            || self.pending.get().is_some()
    }

    fn edit(self, update: impl FnOnce(&mut Config)) {
        self.controller.with(Controller::stop_countdown);
        self.state.update(|state| state.countdown = None);
        self.draft.update(update);
        self.dirty.set(true);
        self.error.set(None);
    }

    fn send(self, command: Command) {
        if self.dirty.get() && !matches!(command, Command::Save(_) | Command::Export { .. }) {
            self.error.set(Some(
                self.tr(
                    "Save or revert your settings first.",
                    "请先保存或还原设置。",
                )
                .into(),
            ));
            return;
        }
        let result = self
            .controller
            .with(|controller| controller.submit(command));
        match result {
            Ok(()) => {
                self.error.set(None);
                self.state.set(
                    self.controller
                        .with(|controller| controller.snapshot.clone()),
                );
            }
            Err(error) => self.error.set(Some(error.to_string())),
        }
    }

    fn request(self, pending: Pending) {
        if self.locked() {
            return;
        }
        self.controller.with(Controller::stop_countdown);
        if pending == Pending::Save {
            if let Err(error) = application::validate_settings(&self.draft.get()) {
                self.error.set(Some(error.to_string()));
                return;
            }
        } else if self.dirty.get() {
            self.error.set(Some(
                self.tr(
                    "Save or revert your settings first.",
                    "请先保存或还原设置。",
                )
                .into(),
            ));
            return;
        }
        self.pending.set(Some(pending));
    }

    fn confirm(self, preserve: bool) {
        let Some(pending) = self.pending.get() else {
            return;
        };
        self.pending.set(None);
        let command = match pending {
            Pending::Update(action) => {
                self.page.set(Page::Activity);
                Command::Update { action, preserve }
            }
            Pending::Save => {
                self.saving.set(true);
                Command::Save(Box::new(self.draft.get()))
            }
            Pending::ResetExclusions => Command::ResetExclusions,
            Pending::ResetCandidate => Command::CandidateSize(None),
            Pending::Skins => Command::ApplySkins {
                keys: self.selected_skins.get().into_iter().collect(),
                default: self.default_skin.get(),
            },
            Pending::Fcitx => {
                let light = self.light_theme.get();
                let dark = self.dark_theme.get();
                let light_rounded = skin::fcitx5::theme_supports_optional_rounding(&light)
                    .then(|| self.light_round.get());
                let dark_rounded = skin::fcitx5::theme_supports_optional_rounding(&dark)
                    .then(|| self.dark_round.get());
                Command::FcitxThemes {
                    light,
                    dark,
                    light_rounded,
                    dark_rounded,
                }
            }
        };
        self.send(command);
    }

    fn export(self, content: String, name: &'static str) {
        spawn(async move {
            match pick_save_path(
                SaveDialog::new()
                    .file_name(name)
                    .title(self.tr("Export", "导出")),
            )
            .await
            {
                Ok(Some(path)) => self.send(Command::Export { path, content }),
                Ok(None) => {}
                Err(error) => self.error.set(Some(error)),
            }
        });
    }
}

fn caption(ui: Ui, en: &'static str, zh: &'static str) -> AnyElement {
    text(move || ui.tr(en, zh))
        .text_size(px(14.0))
        .text_color(muted)
        .into_element()
}

fn reveal() -> Motion {
    motion().entering(
        MotionSpec::new(KeyframeTiming::new(140.0)).values(MotionChannel::Opacity, [0.0, 1.0]),
    )
}

fn heading(ui: Ui, en: &'static str, zh: &'static str) -> AnyElement {
    text(move || ui.tr(en, zh))
        .text_size(px(20.0))
        .font_weight(FontWeight::SEMIBOLD)
        .into_element()
}

fn section(ui: Ui, en: &'static str, zh: &'static str) -> View {
    view()
        .gap_4()
        .p_5()
        .rounded(move || theme().radius)
        .bg(surface)
        .border_1()
        .border_color(line)
        .child(
            text(move || ui.tr(en, zh))
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(px(16.0)),
        )
}

fn action(
    ui: Ui,
    id: &str,
    en: &'static str,
    zh: &'static str,
    primary: bool,
    run: impl Fn() + 'static,
) -> AnyElement {
    button()
        .test_id(id)
        .disabled(move || ui.locked())
        .px_4()
        .py_2()
        .min_h(px(38.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(if primary { accent } else { line })
        .bg(move || if primary { accent() } else { surface() })
        .text_color(move || if primary { on_accent() } else { ink() })
        .focus(|style| {
            style
                .outline(px(2.0))
                .outline_color(accent)
                .outline_offset(px(2.0))
        })
        .transition_colors()
        .duration(120.0)
        .active(|style| style.opacity(0.75))
        .disabled_style(|style| style.opacity(0.45))
        .on_press(run)
        .child(
            text(move || ui.tr(en, zh))
                .text_size(px(14.0))
                .font_weight(FontWeight::MEDIUM),
        )
        .into_element()
}

fn field(
    ui: Ui,
    id: &str,
    en: &'static str,
    zh: &'static str,
    secret: bool,
    get: impl Fn(&Config) -> String + 'static,
    set: impl Fn(&mut Config, String) + 'static,
) -> AnyElement {
    view()
        .gap_2()
        .child(caption(ui, en, zh))
        .child(
            input()
                .test_id(id)
                .label(move || SharedString::from(ui.tr(en, zh)))
                .value(move || ui.draft.with(&get))
                .masked(secret)
                .auto_correct(false)
                .disabled(move || ui.locked())
                .on_change(move |value: &str| ui.edit(|config| set(config, value.into())))
                .min_h(px(38.0))
                .px_3()
                .text_size(px(14.0))
                .bg(surface)
                .text_color(ink)
                .rounded(px(6.0))
                .border_1()
                .border_color(line)
                .focus(|style| {
                    style
                        .outline(px(2.0))
                        .outline_color(accent)
                        .outline_offset(px(2.0))
                }),
        )
        .into_element()
}

fn toggle(
    ui: Ui,
    id: &str,
    en: &'static str,
    zh: &'static str,
    get: impl Fn(&Config) -> bool + 'static,
    set: impl Fn(&mut Config, bool) + 'static,
) -> AnyElement {
    switch().test_id(id).checked(move || ui.draft.with(&get)).disabled(move || ui.locked())
        .accessibility_label(move || Some(ui.tr(en, zh).into()))
        .app_class(Some(SharedString::from("flex-row items-center gap-3 py-2 focus:outline-2 focus:outline-ring focus:outline-offset-2 disabled:opacity-50")))
        .track_class("w-9 h-5 rounded-full bg-switch-track checked:bg-primary transition-colors duration-150")
        .thumb_class("w-4 h-4 mt-0.5 ml-0.5 rounded-full bg-switch-thumb checked:bg-primary-foreground checked:translate-x-4 transition-transform duration-150")
        .on_change(move |checked: &bool| ui.edit(|config| set(config, *checked)))
        .child(text(move || ui.tr(en, zh)).text_size(px(14.0))).into_element()
}

fn picker(
    ui: Ui,
    id: &str,
    en: &'static str,
    zh: &'static str,
    items: Vec<(String, String)>,
    get: impl Fn() -> String + 'static,
    set: impl Fn(String) + 'static,
) -> AnyElement {
    let get = Rc::new(get);
    let current = get.clone();
    let titles = items.clone();
    view()
        .gap_2()
        .child(caption(ui, en, zh))
        .child(
            select()
                .test_id(id)
                .items(
                    items
                        .into_iter()
                        .map(|(value, title)| SelectItem::new(value, title))
                        .collect::<Vec<_>>(),
                )
                .value(move || SelectValue::from(current()))
                .disabled(move || ui.locked())
                .searchable(true)
                .accessibility_label(move || Some(ui.tr(en, zh).into()))
                .popup_class("rounded-lg border border-border bg-popover p-1")
                .item_class(
                    "px-3 py-2 text-sm text-popover-foreground focus:bg-accent selected:bg-accent",
                )
                .search_class("px-3 py-2 text-sm bg-popover text-popover-foreground")
                .trigger(
                    view()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .px_3()
                        .min_h(px(38.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(line)
                        .bg(surface)
                        .focus(|style| {
                            style
                                .outline(px(2.0))
                                .outline_color(accent)
                                .outline_offset(px(2.0))
                        })
                        .child(
                            text({
                                let get = get.clone();
                                let titles = titles.clone();
                                move || {
                                    let value = get();
                                    titles
                                        .iter()
                                        .find(|(key, _)| key == &value)
                                        .map(|(_, title)| title.clone())
                                        .unwrap_or_else(|| "—".into())
                                }
                            })
                            .text_size(px(14.0)),
                        )
                        .child(text("⌄").text_color(muted)),
                )
                .on_change(move |value: &Option<SharedString>| {
                    if let Some(value) = value {
                        set(value.to_string());
                    }
                }),
        )
        .into_element()
}

fn config_picker(
    ui: Ui,
    id: &str,
    en: &'static str,
    zh: &'static str,
    choices: &[(&str, &str)],
    get: impl Fn(&Config) -> String + 'static,
    set: impl Fn(&mut Config, String) + 'static,
) -> AnyElement {
    picker(
        ui,
        id,
        en,
        zh,
        choices
            .iter()
            .map(|(key, label)| (key.to_string(), label.to_string()))
            .collect(),
        move || ui.draft.with(&get),
        move |value| ui.edit(|config| set(config, value)),
    )
}

#[component]
pub fn App(props: AppProps) -> impl IntoElement {
    crate::theme::install();
    let paths = (!props.config_path.get().is_empty()).then(|| Paths {
        config: props.config_path.get().to_string().into(),
        rime: props.rime_path.get().to_string().into(),
        cache: props.cache_path.get().to_string().into(),
    });
    let controller = match Controller::start(paths) {
        Ok(controller) => controller,
        Err(error) => return view().p_6().child(text(error.to_string())).into_element(),
    };
    let ui = Ui {
        controller: stored(controller),
        state: signal_with(Snapshot::default(), SignalEquals::Never),
        draft: signal_with(Config::default(), SignalEquals::Never),
        dirty: signal(false),
        saving: signal(false),
        saving_exclusion: signal(false),
        page: signal(Page::Overview),
        error: signal(None),
        pending: signal(None),
        exclusion: signal(String::new()),
        editing_exclusion: signal(None),
        candidate: signal("5".to_string()),
        selected_skins: signal(HashSet::new()),
        default_skin: signal(String::new()),
        light_theme: signal(String::new()),
        dark_theme: signal(String::new()),
        light_round: signal(true),
        dark_round: signal(true),
        diagnostics_open: signal(false),
    };
    set_interval(Duration::from_millis(60), move || {
        if !ui.controller.with(Controller::poll) {
            return;
        }
        let next = ui.controller.with(|controller| controller.snapshot.clone());
        if next.revision != ui.state.with(|state| state.revision) {
            if ui.saving_exclusion.get() && !next.busy {
                if next.status == Status::Success {
                    ui.editing_exclusion.set(None);
                    ui.exclusion.set(String::new());
                }
                ui.saving_exclusion.set(false);
            }
            if ui.saving.get() && !next.busy {
                if next.status == Status::Success {
                    ui.dirty.set(false);
                }
                ui.saving.set(false);
            }
            if !ui.dirty.get() {
                if let Some(config) = &next.config {
                    ui.draft.set(config.clone());
                }
            }
            ui.candidate
                .set(next.candidate_size.unwrap_or(5).to_string());
            ui.selected_skins.set(next.skins.clone());
            ui.default_skin
                .set(next.default_skin.clone().unwrap_or_default());
            ui.light_theme.set(
                next.fcitx_selection
                    .light
                    .clone()
                    .unwrap_or_else(|| "catppuccin-latte-sky".into()),
            );
            ui.dark_theme.set(
                next.fcitx_selection
                    .dark
                    .clone()
                    .unwrap_or_else(|| "catppuccin-mocha-sky".into()),
            );
            ui.light_round.set(next.light_rounded.unwrap_or(true));
            ui.dark_round.set(next.dark_rounded.unwrap_or(true));
        }
        let auto_due = next.auto_update_due;
        ui.state.set(next);
        if auto_due && ui.pending.get().is_none() && !ui.dirty.get() {
            ui.controller.with(Controller::stop_countdown);
            let config = ui.draft.get();
            if config.user_data_policy == "preserve"
                && config.pre_update_hook.is_empty()
                && config.post_update_hook.is_empty()
            {
                ui.send(Command::Update {
                    action: UpdateAction::All,
                    preserve: true,
                });
                ui.page.set(Page::Activity);
            } else {
                ui.pending.set(Some(Pending::Update(UpdateAction::All)));
            }
        }
    });
    cleanup(move || ui.controller.with(Controller::request_shutdown));
    for (shortcut, page) in [
        ("mod+1", Page::Overview),
        ("mod+2", Page::Schemes),
        ("mod+3", Page::Appearance),
        ("mod+4", Page::Settings),
        ("mod+5", Page::Activity),
    ] {
        on_key(shortcut, move |_| {
            if ui.pending.get().is_none() {
                ui.page.set(page);
            }
        });
    }
    on_key("mod+s", move |_| {
        if ui.dirty.get() {
            ui.request(Pending::Save);
        }
    });
    view().test_id("snout-workbench").flex_1().flex_row().bg(canvas).text_color(ink)
        .child(sidebar(ui))
        .child(view().flex_1().min_w(px(0.0)).child(header(ui)).child(confirmation(ui))
            .child(when(move || ui.state.with(|state| !state.warnings.is_empty()), move || {
                view().mx_6().my_2().p_3().rounded(px(6.0)).border_1().border_color(line)
                    .child(caption(ui, "Some installation details could not be read. Settings remain available.", "部分安装信息无法读取，设置仍然可用。"))
                    .child(text(move || ui.state.with(|state| state.warnings.join("\n"))).test_id("diagnostic-warnings").text_size(px(12.0)).text_color(muted))
            }))
            .child(when(move || ui.error.get().is_some() || ui.state.with(|state| state.error.is_some()), move || {
                view().mx_6().my_2().p_3().rounded(px(6.0)).border_1().border_color(danger).role(AccessibilityRole::Alert)
                    .child(text(move || ui.error.get().or_else(|| ui.state.with(|state| state.error.clone())).unwrap_or_default()).test_id("operation-error").text_size(px(14.0)))
            }))
            .child(scroll().test_id("content-scroll").flex_1().child(view().p_6().gap_6().w_full().max_w(px(1120.0))
                .child(dynamic(move || match ui.page.get() {
                    Page::Overview => overview(ui), Page::Schemes => schemes(ui), Page::Appearance => appearance(ui),
                    Page::Settings => settings(ui), Page::Activity => activity(ui),
                }))))
            .child(draft_bar(ui)).child(status_bar(ui)))
        .into_element()
}

fn icon(page: Page) -> AnyElement {
    let paths = match page {
        Page::Overview => "M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z",
        Page::Schemes => "M4 5h16v14H4z M7 9h10 M7 12h2 M11 12h2 M15 12h2 M8 15h8",
        Page::Appearance => "M12 3a9 9 0 1 0 0 18h1a2 2 0 0 0 0-4h-1a2 2 0 0 1 0-4h5a4 4 0 0 0 4-4c0-3-4-6-9-6z M7 9h.01 M10 6h.01 M15 6h.01",
        Page::Settings => "M4 7h16 M4 17h16 M8 4v6 M16 14v6",
        Page::Activity => "M4 4v16h16 M7 13l4-4 4 5 5-8",
    };
    svg(ImageSrc::svg_markup(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="{paths}"/></svg>"#
    )))
    .size(px(18.0)).accessible(false).flex_shrink_0().into_element()
}

fn sidebar(ui: Ui) -> AnyElement {
    let mut sidebar = view()
        .test_id("workspace-sidebar")
        .w(px(208.0))
        .flex_shrink_0()
        .p_4()
        .gap_2()
        .bg(sidebar_surface)
        .border_r_1()
        .border_color(line)
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .py_4()
                .mb_4()
                .child(
                    image(format!(
                        "data:image/png;base64,{}",
                        BASE64_STANDARD.encode(include_bytes!("../assets/logo.png"))
                    ))
                    .test_id("brand-logo")
                    .size(px(48.0))
                    .flex_shrink_0()
                    .object_fit(ResizeMode::Contain)
                    .label("snout"),
                )
                .child(
                    view()
                        .gap_1()
                        .child(
                            text("snout")
                                .text_size(px(22.0))
                                .font_weight(FontWeight::SEMIBOLD),
                        )
                        .child(caption(ui, "Rime workbench", "Rime 工作台")),
                ),
        )
        .child(
            text(move || ui.tr("WORKSPACE", "工作空间"))
                .px_3()
                .mb_2()
                .text_size(px(11.0))
                .text_color(muted)
                .font_weight(FontWeight::SEMIBOLD),
        );
    for (page, id, en, zh) in [
        (Page::Overview, "nav-overview", "Overview", "概览"),
        (Page::Schemes, "nav-schemes", "Schemes", "输入方案"),
        (Page::Appearance, "nav-appearance", "Appearance", "外观"),
        (Page::Settings, "nav-settings", "Settings", "设置"),
        (Page::Activity, "nav-activity", "Activity", "活动"),
    ] {
        sidebar = sidebar.child(
            button()
                .test_id(id)
                .selected(move || ui.page.get() == page)
                .disabled(move || ui.pending.get().is_some())
                .flex_row()
                .items_center()
                .gap_3()
                .px_3()
                .min_h(px(42.0))
                .rounded(px(8.0))
                .bg(move || {
                    if ui.page.get() == page {
                        selection()
                    } else {
                        sidebar_surface()
                    }
                })
                .text_color(move || {
                    if ui.page.get() == page {
                        accent()
                    } else {
                        muted()
                    }
                })
                .hover(|style| style.bg(selection))
                .focus(|style| {
                    style
                        .outline(px(2.0))
                        .outline_color(accent)
                        .outline_offset(px(2.0))
                })
                .transition_colors()
                .duration(140.0)
                .active(|style| style.opacity(0.7))
                .on_press(move || ui.page.set(page))
                .child(icon(page))
                .child(
                    text(move || ui.tr(en, zh))
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(px(14.0)),
                ),
        );
    }
    sidebar
        .child(view().flex_1())
        .child(
            view()
                .p_3()
                .gap_2()
                .rounded(px(8.0))
                .bg(surface)
                .child(
                    text(move || ui.tr("One place for your input.", "打理输入，从容一点。"))
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM),
                )
                .child(
                    text(move || ui.tr("Schemes, dictionaries & models", "方案、词库与语言模型"))
                        .text_size(px(12.0))
                        .text_color(muted),
                ),
        )
        .child(
            text(format!("snout desktop  {}", env!("CARGO_PKG_VERSION")))
                .px_3()
                .pt_3()
                .text_color(muted)
                .text_size(px(11.0)),
        )
        .into_element()
}

fn header(ui: Ui) -> AnyElement {
    view()
        .px_6()
        .py_5()
        .flex_row()
        .justify_between()
        .items_center()
        .gap_4()
        .bg(surface)
        .border_b_1()
        .border_color(line)
        .child(
            view()
                .gap_1()
                .flex_1()
                .min_w(px(0.0))
                .child(
                    text(move || match ui.page.get() {
                        Page::Overview => ui.tr("Overview", "概览"),
                        Page::Schemes => ui.tr("Schemes", "输入方案"),
                        Page::Appearance => ui.tr("Appearance", "外观"),
                        Page::Settings => ui.tr("Settings", "设置"),
                        Page::Activity => ui.tr("Activity", "活动"),
                    })
                    .text_size(px(22.0))
                    .font_weight(FontWeight::SEMIBOLD),
                )
                .child(
                    text(move || match ui.page.get() {
                        Page::Overview => ui.tr(
                            "Your input method, in good order.",
                            "方案与组件，一目了然。",
                        ),
                        Page::Schemes => {
                            ui.tr("Find the way you like to type.", "选择适合你的输入方式。")
                        }
                        Page::Appearance => ui.tr(
                            "A familiar space for every word.",
                            "让每一次输入，都合你心意。",
                        ),
                        Page::Settings => ui.tr(
                            "Tune once. Keep your workflow.",
                            "按你的习惯，安排每一处细节。",
                        ),
                        Page::Activity => ui.tr(
                            "Follow updates and inspect results.",
                            "查看更新进度，追溯每一步结果。",
                        ),
                    })
                    .text_color(muted)
                    .text_size(px(13.0)),
                ),
        )
        .child(action(
            ui,
            "update-all",
            "Update all",
            "更新全部",
            true,
            move || ui.request(Pending::Update(UpdateAction::All)),
        ))
        .into_element()
}

fn draft_bar(ui: Ui) -> AnyElement {
    when(
        move || ui.dirty.get(),
        move || {
            view()
                .test_id("unsaved-changes")
                .bind_motion(&reveal())
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_3()
                .px_6()
                .py_3()
                .bg(selection)
                .border_t_1()
                .border_color(line)
                .child(
                    text(move || ui.tr("Unsaved changes", "有尚未保存的修改"))
                        .flex_1()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM),
                )
                .child(action(
                    ui,
                    "discard-draft",
                    "Revert",
                    "还原",
                    false,
                    move || {
                        if let Some(config) = ui.state.with(|state| state.config.clone()) {
                            ui.draft.set(config);
                            ui.dirty.set(false);
                            ui.error.set(None);
                        }
                    },
                ))
                .child(action(
                    ui,
                    "save-draft",
                    "Save changes",
                    "保存修改",
                    true,
                    move || ui.request(Pending::Save),
                ))
        },
    )
    .into_element()
}

fn status_bar(ui: Ui) -> AnyElement {
    view()
        .px_6()
        .py_3()
        .gap_2()
        .border_t_1()
        .border_color(line)
        .child(
            progress()
                .value(move || (ui.state.with(|state| state.progress) * 100.0) as f32)
                .indeterminate(move || ui.state.with(|state| state.busy && state.progress == 0.0))
                .track_class("h-1 rounded-full bg-muted")
                .indicator_class("h-1 rounded-full bg-primary transition-transform duration-150"),
        )
        .child(
            view()
                .flex_row()
                .justify_between()
                .items_center()
                .gap_3()
                .child(
                    text(move || {
                        let state = ui.state.get();
                        let label = match state.status {
                            Status::Loading => ui.tr("Opening your workbench…", "正在打开工作台…"),
                            Status::Ready => ui.tr("Ready", "就绪"),
                            Status::Working => ui.tr("Working", "处理中"),
                            Status::Cancelling => ui.tr("Cancelling safely…", "正在安全取消…"),
                            Status::Success => ui.tr("Completed", "已完成"),
                            Status::PartialFailure => ui.tr(
                                "Completed with errors — see Activity",
                                "部分失败，请查看活动",
                            ),
                            Status::Cancelled => ui.tr("Cancelled", "已取消"),
                            Status::Error => ui.tr("Needs attention", "需要处理"),
                        };
                        if state.detail.is_empty() {
                            label.to_string()
                        } else {
                            format!(
                                "{label} · {}% · {}",
                                (state.progress * 100.0) as u32,
                                state.detail
                            )
                        }
                    })
                    .test_id("operation-status")
                    .text_size(px(12.0))
                    .text_color(muted),
                )
                .child(
                    button()
                        .test_id("cancel-operation")
                        .disabled_style(|style| style.opacity(0.4))
                        .disabled(move || {
                            !ui.state
                                .with(|state| state.busy || state.countdown.is_some())
                        })
                        .px_3()
                        .py_1()
                        .rounded(px(4.0))
                        .focus(|style| {
                            style
                                .outline(px(2.0))
                                .outline_color(accent)
                                .outline_offset(px(2.0))
                        })
                        .on_press(move || {
                            ui.controller.with(Controller::cancel);
                            ui.state
                                .set(ui.controller.with(|controller| controller.snapshot.clone()));
                        })
                        .child(text(move || ui.tr("Cancel", "取消")).text_size(px(12.0))),
                ),
        )
        .into_element()
}

fn confirmation(ui: Ui) -> AnyElement {
    when(
        move || ui.pending.get().is_some(),
        move || {
            let pending = ui.pending.get();
            nsh_base::dialog()
                .close_on_escape(true)
                .on_cancel(move || ui.pending.set(None))
                .flex_1()
                .child(
                    view()
                        .flex_1()
                        .p_6()
                        .items_center()
                        .justify_center()
                        .bg(crate::theme::overlay)
                        .text_color(ink)
                        .child(
                            scroll()
                                .w_full()
                                .max_w(px(640.0))
                                .max_h(move || window().height - px(48.0))
                                .child(
                                    view()
                                        .test_id("confirmation")
                                        .p_5()
                                        .gap_3()
                                        .rounded(px(8.0))
                                        .bg(surface)
                                        .border_1()
                                        .border_color(line)
                                        .bind_motion(&reveal())
                                        .role(AccessibilityRole::Alert)
                                        .child(heading(
                                            ui,
                                            "Review before continuing",
                                            "继续前，请确认",
                                        ))
                                        .child(
                                            text(move || {
                                                let config = ui.draft.get();
                                                let mut summary = match pending {
                            Some(Pending::Update(action)) => format!(
                                "{} · {}\n{}",
                                action.command(),
                                config
                                    .schema
                                    .display_name_lang(Lang::from_str(&config.language)),
                                ui.state.with(|state| state
                                    .paths
                                    .as_ref()
                                    .map(|paths| paths.rime.display().to_string())
                                    .unwrap_or_default())
                            ),
                            Some(Pending::Save) => ui
                                .tr(
                                    "Save settings and synchronize the model patch on disk.",
                                    "保存设置，并同步磁盘上的模型补丁。",
                                )
                                .into(),
                            Some(Pending::ResetExclusions) => ui
                                .tr(
                                    "Replace exclusion rules with built-in defaults.",
                                    "使用内置默认值替换所有排除规则。",
                                )
                                .into(),
                            Some(Pending::ResetCandidate) => ui
                                .tr(
                                    "Remove the candidate page-size override and redeploy.",
                                    "移除候选数量覆盖值，并重新部署。",
                                )
                                .into(),
                            Some(Pending::Skins) => ui
                                .tr(
                                    "Write selected presets and the default skin, then redeploy.",
                                    "写入所选预设和默认皮肤，然后重新部署。",
                                )
                                .into(),
                            Some(Pending::Fcitx) => format!(
                                "{} → {}\n{}",
                                ui.light_theme.get(),
                                ui.dark_theme.get(),
                                ui.tr(
                                    "Install themes and reload Fcitx5.",
                                    "安装主题，并重新载入 Fcitx5。"
                                )
                            ),
                            None => String::new(),
                        };
                                                if matches!(pending, Some(Pending::Update(_))) {
                                                    summary.push_str(&format!(
                                                        "\n{}: {}",
                                                        ui.tr("User data policy", "用户数据策略"),
                                                        config.user_data_policy
                                                    ));
                                                    if !config.pre_update_hook.is_empty()
                                                        || !config.post_update_hook.is_empty()
                                                    {
                                                        summary.push_str(&format!(
                                    "\n{}\n{}\n{}",
                                    ui.tr(
                                        "Warning: configured hooks execute local commands.",
                                        "警告：已配置的钩子将执行本地命令。"
                                    ),
                                    config.pre_update_hook,
                                    config.post_update_hook
                                ));
                                                    }
                                                }
                                                summary
                                            })
                                            .text_size(px(14.0)),
                                        )
                                        .child(
                                            view()
                                                .flex_row()
                                                .flex_wrap()
                                                .gap_3()
                                                .child(
                                                    button()
                                                        .test_id("confirm-cancel")
                                                        .px_4()
                                                        .py_2()
                                                        .rounded(px(6.0))
                                                        .border_1()
                                                        .border_color(line)
                                                        .focus(|style| {
                                                            style
                                                                .outline(px(2.0))
                                                                .outline_color(accent)
                                                                .outline_offset(px(2.0))
                                                        })
                                                        .on_press(move || ui.pending.set(None))
                                                        .child(text(move || {
                                                            ui.tr("Go back", "返回")
                                                        })),
                                                )
                                                .child(
                                                    button()
                                                        .test_id("confirm-continue")
                                                        .px_4()
                                                        .py_2()
                                                        .rounded(px(6.0))
                                                        .bg(accent)
                                                        .text_color(on_accent)
                                                        .focus(|style| {
                                                            style
                                                                .outline(px(2.0))
                                                                .outline_color(ink)
                                                                .outline_offset(px(2.0))
                                                        })
                                                        .on_press(move || ui.confirm(true))
                                                        .child(text(move || {
                                                            if matches!(
                                                                pending,
                                                                Some(Pending::Update(_))
                                                            ) {
                                                                ui.tr(
                                                                    "Continue · preserve user data",
                                                                    "继续 · 保留用户数据",
                                                                )
                                                            } else {
                                                                ui.tr("Confirm", "确认")
                                                            }
                                                        })),
                                                )
                                                .child(when(
                                                    move || {
                                                        matches!(pending, Some(Pending::Update(_)))
                                                            && ui.draft.with(|config| {
                                                                config.user_data_policy
                                                                    != "preserve"
                                                            })
                                                    },
                                                    move || {
                                                        button()
                                                            .test_id("confirm-discard")
                                                            .px_4()
                                                            .py_2()
                                                            .rounded(px(6.0))
                                                            .border_1()
                                                            .border_color(danger)
                                                            .text_color(danger)
                                                            .focus(|style| {
                                                                style
                                                                    .outline(px(2.0))
                                                                    .outline_color(accent)
                                                                    .outline_offset(px(2.0))
                                                            })
                                                            .on_press(move || ui.confirm(false))
                                                            .child(text(move || {
                                                                ui.tr(
                                                        "Discard user data and continue",
                                                        "丢弃用户数据并继续",
                                                    )
                                                            }))
                                                    },
                                                )),
                                        ),
                                ),
                        ),
                )
        },
    )
    .into_element()
}

fn localized_component(ui: Ui, value: &str) -> String {
    match value {
        "Scheme / 方案" => ui.tr("Scheme", "方案"),
        "Dictionary / 词库" => ui.tr("Dictionary", "词库"),
        "Model / 模型" => ui.tr("Language model", "语言模型"),
        "Not installed / 未安装" => ui.tr("Not installed", "未安装"),
        "Unavailable / 无法读取" => ui.tr("Unavailable", "无法读取"),
        _ => value,
    }
    .to_string()
}

fn overview(ui: Ui) -> AnyElement {
    view()
        .gap_5()
        .child(
            view()
                .test_id("scheme-summary")
                .p_6()
                .gap_4()
                .rounded(px(12.0))
                .bg(surface)
                .border_1()
                .border_color(line)
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(caption(ui, "Selected scheme", "当前方案"))
                        .child(
                            text(move || ui.tr("RIME", "RIME"))
                                .text_size(px(11.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(accent)
                                .px_3()
                                .py_1()
                                .rounded_full()
                                .bg(selection),
                        ),
                )
                .child(
                    text(move || {
                        ui.draft.with(|config| {
                            config
                                .schema
                                .display_name_lang(Lang::from_str(&config.language))
                        })
                    })
                    .text_size(px(28.0))
                    .font_weight(FontWeight::SEMIBOLD),
                )
                .child(caption(
                    ui,
                    "Keep your scheme, dictionary and language model up to date.",
                    "一起维护方案、词库与语言模型，让输入保持顺手。",
                ))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .gap_3()
                        .child(action(
                            ui,
                            "browse-schemes",
                            "Change scheme",
                            "切换方案",
                            false,
                            move || ui.page.set(Page::Schemes),
                        ))
                        .child(action(
                            ui,
                            "open-appearance",
                            "Customize appearance",
                            "调整外观",
                            false,
                            move || ui.page.set(Page::Appearance),
                        )),
                ),
        )
        .child(when(
            move || ui.state.with(|state| state.countdown.is_some()),
            move || {
                section(ui, "Automatic update", "自动更新")
                    .child(text(move || {
                        format!(
                            "{} {} s",
                            ui.tr("Starting in", "将在以下时间后开始"),
                            ui.state.with(|state| state.countdown.unwrap_or(0))
                        )
                    }))
                    .child(action(
                        ui,
                        "cancel-countdown",
                        "Not now",
                        "暂不更新",
                        false,
                        move || {
                            ui.controller.with(Controller::stop_countdown);
                            ui.state
                                .set(ui.controller.with(|controller| controller.snapshot.clone()));
                        },
                    ))
            },
        ))
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_3()
                .child(heading(ui, "Components", "组件状态"))
                .child(action(
                    ui,
                    "check-updates",
                    "Check remote versions",
                    "检查远端版本",
                    false,
                    move || ui.send(Command::CheckUpdates),
                )),
        )
        .child(dynamic(move || {
            let mut cards = view()
                .test_id("component-cards")
                .flex_row()
                .flex_wrap()
                .gap_3();
            for (name, version) in ui.state.with(|state| state.versions.clone()) {
                cards = cards.child(
                    view()
                        .flex_1()
                        .min_w(px(160.0))
                        .p_4()
                        .gap_3()
                        .rounded(px(10.0))
                        .bg(surface)
                        .border_1()
                        .border_color(line)
                        .child(
                            text(localized_component(ui, &name))
                                .text_size(px(13.0))
                                .text_color(muted),
                        )
                        .child(
                            text(localized_component(ui, &version))
                                .text_size(px(14.0))
                                .font_weight(FontWeight::SEMIBOLD),
                        ),
                );
            }
            cards
        }))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child(action(
                    ui,
                    "update-scheme",
                    "Update scheme",
                    "更新方案",
                    false,
                    move || ui.request(Pending::Update(UpdateAction::Scheme)),
                ))
                .child(action(
                    ui,
                    "update-dict",
                    "Update dictionary",
                    "更新词库",
                    false,
                    move || ui.request(Pending::Update(UpdateAction::Dict)),
                ))
                .child(action(
                    ui,
                    "update-model",
                    "Update model",
                    "更新模型",
                    false,
                    move || ui.request(Pending::Update(UpdateAction::Model)),
                )),
        )
        .child(when(
            move || ui.state.with(|state| state.remote_status.is_some()),
            move || {
                section(ui, "Remote versions", "远端版本").child(
                    text(move || {
                        ui.state.with(|state| {
                            state
                                .remote_status
                                .as_ref()
                                .map(|status| {
                                    format!(
                                        "{}\n{}\n{}\n{}",
                                        status.scheme_status,
                                        status.dict_status,
                                        status.model_status,
                                        status.model_patch_status
                                    )
                                })
                                .unwrap_or_default()
                        })
                    })
                    .text_size(px(14.0)),
                )
            },
        ))
        .child(
            section(ui, "Engine & Rime setup", "输入引擎与 Rime 设置")
                .child(
                    text(move || {
                        let engines = ui.state.with(|state| state.engines.clone());
                        if engines.is_empty() {
                            ui.tr("No engine detected", "尚未检测到输入引擎").into()
                        } else {
                            engines.join(" · ")
                        }
                    })
                    .text_size(px(14.0)),
                )
                .child(caption(
                    ui,
                    "Check Rime integration without changing your automatic setup preference.",
                    "检查 Rime 集成，不改变自动集成偏好，也不会安装软件包。",
                ))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .gap_3()
                        .child(action(
                            ui,
                            "setup-rime",
                            "Check / set up Rime",
                            "检查 / 设置 Rime",
                            false,
                            move || ui.request(Pending::Update(UpdateAction::RimeSetup)),
                        ))
                        .child(action(
                            ui,
                            "refresh-status",
                            "Refresh diagnostics",
                            "刷新诊断",
                            false,
                            move || ui.send(Command::Refresh),
                        )),
                ),
        )
        .child(
            button()
                .test_id("toggle-diagnostics")
                .flex_row()
                .items_center()
                .gap_2()
                .py_2()
                .accessibility_label(move || {
                    Some(
                        ui.tr(
                            "Show or hide diagnostics and file locations",
                            "展开或收起诊断与文件位置",
                        )
                        .into(),
                    )
                })
                .focus(|style| style.outline(px(2.0)).outline_color(accent))
                .on_press(move || ui.diagnostics_open.update(|open| *open = !*open))
                .child(
                    text(move || {
                        if ui.diagnostics_open.get() {
                            "−"
                        } else {
                            "+"
                        }
                    })
                    .text_color(muted),
                )
                .child(
                    text(move || ui.tr("Diagnostics & file locations", "诊断与文件位置"))
                        .text_color(muted)
                        .text_size(px(13.0)),
                ),
        )
        .child(when(
            move || ui.diagnostics_open.get(),
            move || {
                view()
                    .gap_4()
                    .bind_motion(&reveal())
                    .child(
                        section(ui, "Wanxiang diagnosis", "万象诊断").child(
                            text(move || ui.state.with(|state| state.diagnosis.clone()))
                                .text_size(px(14.0)),
                        ),
                    )
                    .child(
                        section(ui, "Locations", "文件位置").child(
                            text(move || {
                                ui.state.with(|state| {
                                    state
                                        .paths
                                        .as_ref()
                                        .map(|paths| {
                                            format!(
                                                "Rime\n{}\n\n{}\n{}\n\n{}\n{}",
                                                paths.rime.display(),
                                                ui.tr("Configuration", "配置"),
                                                paths.config.display(),
                                                ui.tr("Cache", "缓存"),
                                                paths.cache.display()
                                            )
                                        })
                                        .unwrap_or_default()
                                })
                            })
                            .text_size(px(12.0))
                            .text_color(muted),
                        ),
                    )
            },
        ))
        .into_element()
}

fn schemes(ui: Ui) -> AnyElement {
    let mut choices = view().flex_row().flex_wrap().gap_3();
    for schema in Schema::all() {
        let schema = *schema;
        choices = choices.child(
            button()
                .test_id(&format!("schema-{schema:?}"))
                .min_w(px(230.0))
                .max_w(px(280.0))
                .flex_1()
                .min_h(px(64.0))
                .flex_row()
                .items_center()
                .justify_between()
                .gap_2()
                .selected(move || ui.draft.with(|config| config.schema == schema))
                .disabled(move || ui.locked())
                .p_3()
                .rounded(px(6.0))
                .border_1()
                .border_color(move || {
                    if ui.draft.with(|config| config.schema == schema) {
                        accent()
                    } else {
                        line()
                    }
                })
                .bg(move || {
                    if ui.draft.with(|config| config.schema == schema) {
                        selection()
                    } else {
                        surface()
                    }
                })
                .transition_colors()
                .duration(140.0)
                .active(|style| style.opacity(0.75))
                .focus(|style| {
                    style
                        .outline(px(2.0))
                        .outline_color(accent)
                        .outline_offset(px(2.0))
                })
                .on_press(move || ui.edit(|config| config.schema = schema))
                .child(
                    text(move || {
                        schema.display_name_lang(Lang::from_str(
                            &ui.draft.with(|config| config.language.clone()),
                        ))
                    })
                    .text_size(px(14.0)),
                )
                .child(
                    text(move || {
                        if ui.draft.with(|config| config.schema == schema) {
                            "✓"
                        } else {
                            ""
                        }
                    })
                    .accessible(false)
                    .text_color(accent)
                    .text_size(px(16.0)),
                ),
        );
    }
    view().gap_6().child(heading(ui, "Choose a scheme", "选择输入方案"))
        .child(caption(ui, "All 13 schemes share the same updater as the CLI. Save the selection, then explicitly install or update.", "全部 13 个方案使用与命令行相同的更新器。保存选择后，再明确执行安装或更新。"))
        .child(choices)
        .child(toggle(ui, "model-patch", "Enable model patch", "启用模型补丁", |config| config.model_patch_enabled, |config, value| config.model_patch_enabled = value))
        .child(view().flex_row().flex_wrap().gap_3()
            .child(action(ui, "save-scheme", "Save selection", "保存选择", true, move || ui.request(Pending::Save)))
            .child(action(ui, "initialize-scheme", "Install selected scheme", "安装所选方案", false, move || ui.request(Pending::Update(UpdateAction::All)))))
        .into_element()
}

fn candidate_preview(ui: Ui) -> AnyElement {
    section(ui, "Layout preview", "布局预览")
        .child(caption(
            ui,
            "Preview candidate count. Your input engine controls the actual colors and typography.",
            "预览候选数量；实际颜色和字体由输入引擎的主题决定。",
        ))
        .child(
            view()
                .p_5()
                .gap_3()
                .rounded(px(8.0))
                .bg(canvas)
                .child(
                    text("ni hao")
                        .text_size(px(18.0))
                        .font_weight(FontWeight::MEDIUM),
                )
                .child(dynamic(move || {
                    let samples = [
                        "你好", "您好", "你号", "拟好", "霓好", "呢好", "你", "呢", "拟",
                    ];
                    let count = ui.candidate.get().parse::<usize>().unwrap_or(5).clamp(1, 9);
                    let mut candidates = view()
                        .test_id("candidate-preview")
                        .flex_row()
                        .flex_wrap()
                        .gap_2();
                    for (index, sample) in samples.iter().take(count).enumerate() {
                        candidates = candidates.child(
                            view()
                                .flex_row()
                                .gap_2()
                                .px_3()
                                .py_2()
                                .rounded(px(6.0))
                                .bg(move || if index == 0 { accent() } else { surface() })
                                .text_color(move || if index == 0 { on_accent() } else { ink() })
                                .child(text(format!("{}", index + 1)).text_size(px(12.0)))
                                .child(text(*sample).text_size(px(16.0))),
                        );
                    }
                    candidates
                })),
        )
        .into_element()
}

fn appearance(ui: Ui) -> AnyElement {
    let candidates = (1..=9)
        .map(|size| (size.to_string(), size.to_string()))
        .collect();
    view()
        .gap_6()
        .child(candidate_preview(ui))
        .child(
            section(ui, "Candidate window", "候选窗口")
                .child(picker(
                    ui,
                    "candidate-size",
                    "Candidates per page",
                    "每页候选数",
                    candidates,
                    move || ui.candidate.get(),
                    move |size| ui.candidate.set(size),
                ))
                .child(
                    view()
                        .flex_row()
                        .gap_3()
                        .child(action(
                            ui,
                            "apply-candidate",
                            "Apply & redeploy",
                            "应用并部署",
                            false,
                            move || match ui.candidate.get().parse::<u8>() {
                                Ok(size) => ui.send(Command::CandidateSize(Some(size))),
                                Err(error) => ui.error.set(Some(error.to_string())),
                            },
                        ))
                        .child(action(
                            ui,
                            "reset-candidate",
                            "Use scheme default",
                            "使用方案默认值",
                            false,
                            move || ui.request(Pending::ResetCandidate),
                        )),
                ),
        )
        .child(fcitx_appearance(ui))
        .child(builtin_appearance(ui))
        .into_element()
}

fn fcitx_appearance(ui: Ui) -> AnyElement {
    let themes = skin::fcitx5::builtin_theme_choices();
    section(ui, "Fcitx5 themes", "Fcitx5 主题")
        .child(caption(ui, "Linux only. Choose a light/dark pair; optional rounding is available for Catppuccin themes.", "仅适用于 Linux。选择亮色和暗色主题；Catppuccin 主题支持可选圆角。"))
        .child(when(move || !ui.state.with(|state| state.fcitx_supported), move || caption(ui, "Fcitx5 is unavailable in this session. Install/start it, then refresh diagnostics.", "当前会话无法使用 Fcitx5。请安装或启动后刷新诊断。")))
        .child(picker(ui, "fcitx-light", "Light theme", "亮色主题", themes.clone(), move || ui.light_theme.get(), move |value| ui.light_theme.set(value)))
        .child(rounding(ui, "fcitx-light-rounded", true))
        .child(picker(ui, "fcitx-dark", "Dark theme", "暗色主题", themes, move || ui.dark_theme.get(), move |value| ui.dark_theme.set(value)))
        .child(rounding(ui, "fcitx-dark-rounded", false))
        .child(button().test_id("apply-fcitx").disabled(move || ui.locked() || !ui.state.with(|state| state.fcitx_supported))
            .px_4().py_2().rounded(px(6.0)).border_1().border_color(line).focus(|style| style.outline(px(2.0)).outline_color(accent).outline_offset(px(2.0)))
            .on_press(move || ui.request(Pending::Fcitx)).child(text(move || ui.tr("Apply theme pair", "应用主题组合"))))
        .into_element()
}

fn rounding(ui: Ui, id: &'static str, light: bool) -> AnyElement {
    when(
        move || {
            skin::fcitx5::theme_supports_optional_rounding(&if light {
                ui.light_theme.get()
            } else {
                ui.dark_theme.get()
            })
        },
        move || {
            switch()
                .test_id(id)
                .disabled(move || ui.locked())
                .checked(move || {
                    if light {
                        ui.light_round.get()
                    } else {
                        ui.dark_round.get()
                    }
                })
                .app_class(Some(SharedString::from(
                    "flex-row items-center gap-3 py-2 focus:outline-2 focus:outline-ring focus:outline-offset-2",
                )))
                .track_class("w-9 h-5 rounded-full bg-switch-track checked:bg-primary transition-colors duration-150")
                .thumb_class("w-4 h-4 mt-0.5 ml-0.5 rounded-full bg-switch-thumb checked:bg-primary-foreground checked:translate-x-4 transition-transform duration-150")
                .on_change(move |checked: &bool| {
                    if light {
                        ui.light_round.set(*checked)
                    } else {
                        ui.dark_round.set(*checked)
                    }
                })
                .child(text(move || ui.tr("Rounded corners", "圆角")))
        },
    )
    .into_element()
}

fn builtin_appearance(ui: Ui) -> AnyElement {
    let presets = skin::list_available_skins(Lang::En);
    let mut list = view().gap_2();
    for (key, name) in &presets {
        let checked_key = key.clone();
        let changed_key = key.clone();
        list = list.child(
            switch()
                .test_id(&format!("skin-{key}"))
                .disabled(move || ui.locked())
                .checked(move || ui.selected_skins.with(|keys| keys.contains(&checked_key)))
                .app_class(Some(SharedString::from(
                    "flex-row items-center gap-3 py-2 focus:outline-2 focus:outline-ring focus:outline-offset-2",
                )))
                .track_class("w-9 h-5 rounded-full bg-switch-track checked:bg-primary transition-colors duration-150")
                .thumb_class("w-4 h-4 mt-0.5 ml-0.5 rounded-full bg-switch-thumb checked:bg-primary-foreground checked:translate-x-4 transition-transform duration-150")
                .on_change(move |checked: &bool| {
                    ui.selected_skins.update(|keys| {
                        if *checked {
                            keys.insert(changed_key.clone());
                        } else {
                            keys.remove(&changed_key);
                        }
                    })
                })
                .child(text(name.clone()).text_size(px(14.0))),
        );
    }
    section(ui, "Built-in skin patches", "内置皮肤补丁")
        .child(caption(ui, "Squirrel / Weasel presets. Linux uses the Fcitx5 themes above; the patch key remains editable in Settings.", "鼠须管 / 小狼毫预设。Linux 使用上方的 Fcitx5 主题；补丁 key 仍可在设置中编辑。"))
        .child(scroll().h(px(200.0)).child(list))
        .child(picker(ui, "default-skin", "Default skin", "默认皮肤", presets, move || ui.default_skin.get(), move |value| ui.default_skin.set(value)))
        .child(button().test_id("apply-skins").disabled(move || ui.locked() || !ui.state.with(|state| matches!(state.appearance, Some(application::AppearanceTarget::ThemePatch(_)))))
            .px_4().py_2().rounded(px(6.0)).border_1().border_color(line).focus(|style| style.outline(px(2.0)).outline_color(accent).outline_offset(px(2.0)))
            .on_press(move || ui.request(Pending::Skins)).child(text(move || ui.tr("Write presets & redeploy", "写入预设并部署"))))
        .into_element()
}

fn settings(ui: Ui) -> AnyElement {
    view().flex_row().flex_wrap().items_start().gap_6()
        .child(section(ui, "General", "常规").min_w(px(300.0)).flex_1()
            .child(config_picker(ui, "language", "Language", "语言", &[("zh", "简体中文"), ("en", "English")], |config| config.language.clone(), |config, value| config.language = value))
            .child(config_picker(ui, "tui-theme", "Terminal appearance", "终端外观", &[("auto", "System / 跟随系统"), ("light", "Light / 亮色"), ("dark", "Dark / 暗色")], |config| config.tui_theme_mode.clone(), |config, value| config.tui_theme_mode = value))
            .child(caption(ui, "Desktop appearance follows your operating system.", "桌面外观跟随操作系统。"))
            .child(config_picker(ui, "data-policy", "User data policy", "用户数据策略", &[("prompt", "Ask each time / 每次询问"), ("preserve", "Preserve / 保留"), ("discard", "Discard / 丢弃")], |config| config.user_data_policy.clone(), |config, value| config.user_data_policy = value))
            .child(config_picker(ui, "rime-auto-setup", "Rime integration", "Rime 集成", &[("ask", "Ask / 询问"), ("on", "Automatic / 自动"), ("off", "Manual / 手动")], |config| match config.rime_auto_setup { None => "ask", Some(true) => "on", Some(false) => "off" }.into(), |config, value| config.rime_auto_setup = match value.as_str() { "on" => Some(true), "off" => Some(false), _ => None }))
            .child(field(ui, "skin-patch-key", "Default skin patch key", "默认皮肤补丁 key", false, |config| config.skin_patch_key.clone(), |config, value| config.skin_patch_key = value)))
        .child(section(ui, "Downloads & network", "下载与网络").min_w(px(300.0)).flex_1()
            .child(toggle(ui, "mirror", "Use CNB mirror", "使用 CNB 镜像", |config| config.use_mirror, |config, value| config.use_mirror = value))
            .child(config_picker(ui, "download-threads", "Download threads", "下载线程数", &[("1", "1"), ("2", "2"), ("3", "3"), ("4", "4"), ("5", "5"), ("6", "6"), ("7", "7"), ("8", "8")], |config| config.download_threads.to_string(), |config, value| { if let Ok(value) = value.parse() { config.download_threads = value; } }))
            .child(toggle(ui, "proxy-enabled", "Use configured proxy", "使用已配置代理", |config| config.proxy_enabled, |config, value| config.proxy_enabled = value))
            .child(config_picker(ui, "proxy-type", "Proxy type", "代理类型", &[("http", "HTTP"), ("socks5", "SOCKS5")], |config| config.proxy_type.clone(), |config, value| config.proxy_type = value))
            .child(field(ui, "proxy-address", "Proxy address (host:port)", "代理地址（主机:端口）", false, |config| config.proxy_address.clone(), |config, value| config.proxy_address = value))
            .child(caption(ui, "Configured proxy disabled does not disable environment proxies.", "关闭已配置代理不会禁用环境变量中的代理。"))
            .child(text(move || ui.state.with(|state| state.proxy.clone())).text_size(px(12.0)).text_color(muted))
            .child(field(ui, "github-token", "GitHub token", "GitHub 令牌", true, |config| config.github_token.clone(), |config, value| config.github_token = value)))
        .child(section(ui, "Automation", "自动化").min_w(px(300.0)).flex_1()
            .child(toggle(ui, "auto-update", "Update automatically on launch", "启动时自动更新", |config| config.auto_update, |config, value| config.auto_update = value))
            .child(field(ui, "countdown", "Countdown in seconds (1–60)", "倒计时秒数（1–60）", false, |config| config.auto_update_countdown.to_string(), |config, value| config.auto_update_countdown = value.parse().unwrap_or(0)))
            .child(toggle(ui, "engine-sync", "Synchronize installed engines", "同步已安装的输入引擎", |config| config.engine_sync_enabled, |config, value| config.engine_sync_enabled = value))
            .child(toggle(ui, "sync-link", "Use symlinks instead of copies", "使用软链接而不是复制", |config| config.engine_sync_use_link, |config, value| config.engine_sync_use_link = value))
            .child(field(ui, "pre-hook", "Pre-update hook", "更新前钩子", false, |config| config.pre_update_hook.clone(), |config, value| config.pre_update_hook = value))
            .child(field(ui, "post-hook", "Post-update hook", "更新后钩子", false, |config| config.post_update_hook.clone(), |config, value| config.post_update_hook = value))
            .child(caption(ui, "Hooks run local commands with your permissions. Review them before every update.", "钩子以你的权限执行本地命令。每次更新前请仔细核对。")))
        .child(view().w_full().flex_row().flex_wrap().gap_3()
            .child(action(ui, "save-settings", "Save settings", "保存设置", true, move || ui.request(Pending::Save)))
            .child(action(ui, "revert-settings", "Revert changes", "还原修改", false, move || { if let Some(config) = ui.state.with(|state| state.config.clone()) { ui.draft.set(config); ui.dirty.set(false); ui.error.set(None); } })))
        .child(view().min_w(px(300.0)).flex_1().child(exclusions(ui)))
        .into_element()
}

fn exclusions(ui: Ui) -> AnyElement {
    section(ui, "Preserve / exclude files", "保留 / 排除文件")
        .child(caption(
            ui,
            "Exact paths, wildcards and regex rules use the shared core parser.",
            "精确路径、通配符和正则表达式规则使用共享内核解析。",
        ))
        .child(dynamic(move || {
            let mut rows = view().gap_2();
            for (index, pattern) in ui
                .state
                .with(|state| {
                    state
                        .config
                        .as_ref()
                        .map(|config| config.exclude_files.clone())
                        .unwrap_or_default()
                })
                .into_iter()
                .enumerate()
            {
                let selected = pattern.clone();
                rows = rows.child(
                    view()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(text(pattern).text_size(px(14.0)))
                        .child(
                            view()
                                .flex_row()
                                .gap_2()
                                .child(action(
                                    ui,
                                    &format!("edit-exclusion-{index}"),
                                    "Edit",
                                    "编辑",
                                    false,
                                    move || {
                                        ui.exclusion.set(selected.clone());
                                        ui.editing_exclusion.set(Some(index));
                                    },
                                ))
                                .child(action(
                                    ui,
                                    &format!("remove-exclusion-{index}"),
                                    "Remove",
                                    "移除",
                                    false,
                                    move || ui.send(Command::RemoveExclusion(index)),
                                )),
                        ),
                );
            }
            rows
        }))
        .child(
            input()
                .test_id("exclusion-input")
                .label(move || SharedString::from(ui.tr("Exclusion pattern", "排除规则")))
                .value(move || ui.exclusion.get())
                .on_change(move |value: &str| ui.exclusion.set(value.into()))
                .disabled(move || ui.locked())
                .min_h(px(38.0))
                .px_3()
                .rounded(px(6.0))
                .border_1()
                .border_color(line)
                .focus(|style| {
                    style
                        .outline(px(2.0))
                        .outline_color(accent)
                        .outline_offset(px(2.0))
                })
                .text_size(px(14.0))
                .bg(surface)
                .text_color(ink),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .gap_3()
                .child(action(
                    ui,
                    "save-exclusion",
                    "Add / save rule",
                    "添加 / 保存规则",
                    false,
                    move || {
                        if ui.dirty.get() {
                            ui.error
                                .set(Some(ui.tr("Save settings first.", "请先保存设置。").into()));
                            return;
                        }
                        let command = match ui.editing_exclusion.get() {
                            Some(index) => Command::EditExclusion(index, ui.exclusion.get()),
                            None => Command::AddExclusion(ui.exclusion.get()),
                        };
                        ui.send(command);
                        ui.saving_exclusion.set(ui.state.with(|state| state.busy));
                    },
                ))
                .child(action(
                    ui,
                    "reset-exclusions",
                    "Reset rules",
                    "重置规则",
                    false,
                    move || ui.request(Pending::ResetExclusions),
                )),
        )
        .into_element()
}

fn activity(ui: Ui) -> AnyElement {
    view().gap_6()
        .child(section(ui, "Update activity", "更新活动")
            .child(when(move || ui.state.with(|state| state.activity.is_empty()), move || caption(ui, "Nothing has run in this session. Updates and changes appear here.", "本次会话尚无操作。更新和修改记录将显示在这里。")))
            .child(text(move || ui.state.with(|state| state.activity.join("\n"))).text_size(px(14.0))))
        .child(section(ui, "Structured results", "结构化结果")
            .child(caption(ui, "The same JSON report as the CLI. Credentials are redacted before display or export.", "与命令行相同的 JSON 报告。凭据在显示和导出前均会脱敏。"))
            .child(input().test_id("report-json").readonly(true).multiline(true).h(px(240.0)).text_size(px(12.0)).bg(surface).text_color(ink)
                .value(move || ui.state.with(|state| state.report.clone().unwrap_or_else(|| ui.tr("No update report yet.", "尚无更新报告。").into()))))
            .child(button().test_id("export-json").disabled(move || ui.locked() || ui.state.with(|state| state.report.is_none()))
                .px_4().py_2().rounded(px(6.0)).border_1().border_color(line).focus(|style| style.outline(px(2.0)).outline_color(accent).outline_offset(px(2.0)))
                .on_press(move || { if let Some(content) = ui.state.with(|state| state.report.clone()) { ui.export(content, "snout-report.json"); } })
                .child(text(move || ui.tr("Export JSON…", "导出 JSON…")))))
        .child(section(ui, "Shell completions", "Shell 自动补全")
            .child(caption(ui, "Generated from the real CLI command definition.", "从真实命令行定义生成。"))
            .child(view().flex_row().gap_3()
                .child(action(ui, "export-zsh", "Export zsh…", "导出 zsh…", false, move || ui.export(controller::completion(CompletionShell::Zsh), "_snout")))
                .child(action(ui, "export-fish", "Export fish…", "导出 fish…", false, move || ui.export(controller::completion(CompletionShell::Fish), "snout.fish")))))
        .into_element()
}
