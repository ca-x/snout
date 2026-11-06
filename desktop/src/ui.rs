use crate::controller::{self, Command, Controller, Paths, Snapshot, Status};
use nsh_base::{button, input, progress, select, switch, SelectItem, SelectValue};
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

fn canvas() -> Rgba {
    if dark() {
        rgb(0x191c20)
    } else {
        rgb(0xf5f6f7)
    }
}
fn surface() -> Rgba {
    if dark() {
        rgb(0x22262b)
    } else {
        rgb(0xffffff)
    }
}
fn ink() -> Rgba {
    if dark() {
        rgb(0xf1f2f4)
    } else {
        rgb(0x24282f)
    }
}
fn muted() -> Rgba {
    if dark() {
        rgb(0xafb6c0)
    } else {
        rgb(0x59636f)
    }
}
fn line() -> Rgba {
    if dark() {
        rgb(0x3d444d)
    } else {
        rgb(0xd6dbe1)
    }
}
fn amber() -> Rgba {
    if dark() {
        rgb(0xf0bd5a)
    } else {
        rgb(0x966215)
    }
}

fn caption(ui: Ui, en: &'static str, zh: &'static str) -> AnyElement {
    text(move || ui.tr(en, zh))
        .text_size(px(14.0))
        .text_color(muted)
        .into_element()
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
        .rounded(px(8.0))
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
        .min_h(px(36.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(if primary { amber } else { line })
        .bg(move || if primary { amber() } else { surface() })
        .text_color(move || {
            if primary {
                if dark() {
                    rgb(0x22262b)
                } else {
                    rgb(0xffffff)
                }
            } else {
                ink()
            }
        })
        .focus(|style| {
            style
                .outline(px(2.0))
                .outline_color(amber)
                .outline_offset(px(2.0))
        })
        .active(|style| style.opacity(0.7))
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
                .h(px(36.0))
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
                        .outline_color(amber)
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
        .app_class(Some(SharedString::from("flex-row items-center gap-3 py-2 focus:border-2 focus:border-amber-600 disabled:opacity-50")))
        .track_class("w-9 h-5 rounded-full bg-slate-400 checked:bg-amber-600")
        .thumb_class("w-4 h-4 mt-0.5 ml-0.5 rounded-full bg-white checked:translate-x-4")
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
    view().gap_2().child(caption(ui, en, zh)).child(
        select().test_id(id).items(items.into_iter().map(|(value, title)| SelectItem::new(value, title)).collect::<Vec<_>>())
            .value(move || SelectValue::from(current()))
            .disabled(move || ui.locked()).searchable(true)
            .accessibility_label(move || Some(ui.tr(en, zh).into()))
            .popup_class("rounded-lg border border-slate-400 bg-white dark:bg-slate-800 p-1")
            .item_class("px-3 py-2 text-sm text-slate-900 dark:text-slate-100 focus:bg-amber-100 dark:focus:bg-slate-700 selected:bg-amber-100 dark:selected:bg-slate-700")
            .search_class("px-3 py-2 text-sm bg-white dark:bg-slate-800 text-slate-900 dark:text-slate-100")
            .trigger(view().flex_row().items_center().justify_between().gap_3().px_3().h(px(36.0))
                .rounded(px(6.0)).border_1().border_color(line).bg(surface).focus(|style| style.outline(px(2.0)).outline_color(amber).outline_offset(px(2.0)))
                .child(text({ let get = get.clone(); let titles = titles.clone(); move || {
                    let value = get();
                    titles.iter().find(|(key, _)| key == &value).map(|(_, title)| title.clone()).unwrap_or_else(|| "—".into())
                }}).text_size(px(14.0)))
                .child(text("⌄").text_color(muted)))
            .on_change(move |value: &Option<SharedString>| { if let Some(value) = value { set(value.to_string()); } })
    ).into_element()
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
    view().test_id("snout-workbench").flex_1().flex_row().bg(canvas).text_color(ink)
        .child(sidebar(ui))
        .child(view().flex_1().min_w(px(0.0)).child(header(ui)).child(confirmation(ui))
            .child(when(move || ui.state.with(|state| !state.warnings.is_empty()), move || {
                view().mx_6().my_2().p_3().rounded(px(6.0)).border_1().border_color(line)
                    .child(caption(ui, "Some installation details could not be read. Settings remain available.", "部分安装信息无法读取，设置仍然可用。"))
                    .child(text(move || ui.state.with(|state| state.warnings.join("\n"))).test_id("diagnostic-warnings").text_size(px(12.0)).text_color(muted))
            }))
            .child(when(move || ui.error.get().is_some() || ui.state.with(|state| state.error.is_some()), move || {
                view().mx_6().my_2().p_3().rounded(px(6.0)).border_1().border_color(amber).role(AccessibilityRole::Alert)
                    .child(text(move || ui.error.get().or_else(|| ui.state.with(|state| state.error.clone())).unwrap_or_default()).test_id("operation-error").text_size(px(14.0)))
            }))
            .child(scroll().test_id("content-scroll").flex_1().child(view().p_6().gap_6()
                .child(dynamic(move || match ui.page.get() {
                    Page::Overview => overview(ui), Page::Schemes => schemes(ui), Page::Appearance => appearance(ui),
                    Page::Settings => settings(ui), Page::Activity => activity(ui),
                }))))
            .child(status_bar(ui)))
        .into_element()
}

fn sidebar(ui: Ui) -> AnyElement {
    let mut sidebar = view()
        .w(px(176.0))
        .flex_shrink_0()
        .p_4()
        .gap_2()
        .border_r_1()
        .border_color(line)
        .child(
            text("snout")
                .text_size(px(28.0))
                .font_weight(FontWeight::SEMIBOLD)
                .mb_1(),
        )
        .child(caption(ui, "Input method workbench", "输入法工作台"));
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
                .px_3()
                .py_3()
                .rounded(px(6.0))
                .bg(move || {
                    if ui.page.get() == page {
                        surface()
                    } else {
                        canvas()
                    }
                })
                .text_color(move || {
                    if ui.page.get() == page {
                        ink()
                    } else {
                        muted()
                    }
                })
                .focus(|style| {
                    style
                        .outline(px(2.0))
                        .outline_color(amber)
                        .outline_offset(px(2.0))
                })
                .active(|style| style.opacity(0.7))
                .on_press(move || ui.page.set(page))
                .child(
                    text(move || ui.tr(en, zh))
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(px(14.0)),
                ),
        );
    }
    sidebar
        .child(view().flex_1())
        .child(caption(ui, "Rime component manager", "Rime 组件管理"))
        .child(
            text(env!("CARGO_PKG_VERSION"))
                .text_color(muted)
                .text_size(px(12.0)),
        )
        .into_element()
}

fn header(ui: Ui) -> AnyElement {
    view()
        .px_6()
        .py_4()
        .flex_row()
        .justify_between()
        .items_center()
        .gap_4()
        .border_b_1()
        .border_color(line)
        .child(
            view()
                .gap_1()
                .child(
                    text(move || match ui.page.get() {
                        Page::Overview => ui.tr("Rime workbench", "Rime 工作台"),
                        Page::Schemes => ui.tr("Your input, your scheme.", "选好方案，顺手输入。"),
                        Page::Appearance => {
                            ui.tr("Make room for your preferences.", "让外观贴合你的偏好。")
                        }
                        Page::Settings => ui.tr(
                            "Small choices. Predictable updates.",
                            "细致设置，安心更新。",
                        ),
                        Page::Activity => ui.tr("Every step, accounted for.", "每一步，都有记录。"),
                    })
                    .text_size(px(20.0))
                    .font_weight(FontWeight::SEMIBOLD),
                )
                .child(
                    text(move || {
                        ui.draft.with(|config| {
                            config
                                .schema
                                .display_name_lang(Lang::from_str(&config.language))
                        })
                    })
                    .text_color(muted)
                    .text_size(px(12.0)),
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
                .track_class("h-1 rounded-full bg-slate-300 dark:bg-slate-700")
                .indicator_class("h-1 rounded-full bg-amber-600 dark:bg-amber-400"),
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
                                .outline_color(amber)
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
            view()
                .test_id("confirmation")
                .mx_6()
                .mt_4()
                .p_4()
                .gap_3()
                .rounded(px(8.0))
                .bg(surface)
                .border_2()
                .border_color(amber)
                .role(AccessibilityRole::Alert)
                .child(heading(ui, "Review before continuing", "继续前，请确认"))
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
                                        .outline_color(amber)
                                        .outline_offset(px(2.0))
                                })
                                .on_press(move || ui.pending.set(None))
                                .child(text(move || ui.tr("Go back", "返回"))),
                        )
                        .child(
                            button()
                                .test_id("confirm-continue")
                                .px_4()
                                .py_2()
                                .rounded(px(6.0))
                                .bg(amber)
                                .text_color(
                                    move || if dark() { rgb(0x22262b) } else { rgb(0xffffff) },
                                )
                                .focus(|style| {
                                    style
                                        .outline(px(2.0))
                                        .outline_color(ink)
                                        .outline_offset(px(2.0))
                                })
                                .on_press(move || ui.confirm(true))
                                .child(text(move || {
                                    if matches!(pending, Some(Pending::Update(_))) {
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
                                    && ui
                                        .draft
                                        .with(|config| config.user_data_policy != "preserve")
                            },
                            move || {
                                button()
                                    .test_id("confirm-discard")
                                    .px_4()
                                    .py_2()
                                    .rounded(px(6.0))
                                    .border_1()
                                    .border_color(amber)
                                    .focus(|style| {
                                        style
                                            .outline(px(2.0))
                                            .outline_color(amber)
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
                )
        },
    )
    .into_element()
}

fn overview(ui: Ui) -> AnyElement {
    view().gap_6()
        .child(section(ui, "Selected scheme", "当前方案")
            .child(text(move || ui.draft.with(|config| config.schema.display_name_lang(Lang::from_str(&config.language)))).text_size(px(28.0)).font_weight(FontWeight::SEMIBOLD))
            .child(caption(ui, "Update the scheme, dictionary and language model together, or choose a component below.", "一起更新方案、词库和语言模型，或单独选择一个组件。"))
            .child(view().flex_row().flex_wrap().gap_3()
                .child(action(ui, "update-scheme", "Scheme only", "仅更新方案", false, move || ui.request(Pending::Update(UpdateAction::Scheme))))
                .child(action(ui, "update-dict", "Dictionary only", "仅更新词库", false, move || ui.request(Pending::Update(UpdateAction::Dict))))
                .child(action(ui, "update-model", "Model only", "仅更新模型", false, move || ui.request(Pending::Update(UpdateAction::Model))))))
        .child(when(move || ui.state.with(|state| state.countdown.is_some()), move || {
            section(ui, "Automatic update", "自动更新")
                .child(text(move || format!("{} {} s", ui.tr("Starting in", "将在以下时间后开始"), ui.state.with(|state| state.countdown.unwrap_or(0)))))
                .child(action(ui, "cancel-countdown", "Not now", "暂不更新", false, move || { ui.controller.with(Controller::stop_countdown); ui.state.set(ui.controller.with(|controller| controller.snapshot.clone())); }))
        }))
        .child(section(ui, "Components", "组件状态").child(dynamic(move || {
            let mut rows = view().gap_3();
            for (name, version) in ui.state.with(|state| state.versions.clone()) {
                rows = rows.child(view().flex_row().justify_between().gap_4().child(text(name).text_color(muted)).child(text(version)));
            }
            rows
        })).child(action(ui, "check-updates", "Check remote versions", "检查远端版本", false, move || ui.send(Command::CheckUpdates)))
            .child(when(move || ui.state.with(|state| state.remote_status.is_some()), move || {
                text(move || ui.state.with(|state| state.remote_status.as_ref().map(|status| format!("{}\n{}\n{}\n{}", status.scheme_status, status.dict_status, status.model_status, status.model_patch_status)).unwrap_or_default())).text_size(px(14.0))
            })))
        .child(section(ui, "Engine & Rime setup", "输入引擎与 Rime 设置")
            .child(text(move || { let engines = ui.state.with(|state| state.engines.clone()); if engines.is_empty() { ui.tr("No engine detected", "尚未检测到输入引擎").into() } else { engines.join(" · ") } }).text_size(px(14.0)))
            .child(caption(ui, "Check or add Rime to Fcitx5 once. This does not install packages or change the automatic setup preference in Settings.", "仅此次检查或添加 Fcitx5 中的 Rime；不会安装软件包，也不会更改设置中的自动集成偏好。"))
            .child(action(ui, "setup-rime", "Check / set up Rime", "检查 / 设置 Rime", false, move || ui.request(Pending::Update(UpdateAction::RimeSetup))))
            .child(action(ui, "refresh-status", "Refresh diagnostics", "刷新诊断", false, move || ui.send(Command::Refresh))))
        .child(section(ui, "Wanxiang diagnosis", "万象诊断").child(text(move || ui.state.with(|state| state.diagnosis.clone())).text_size(px(14.0))))
        .child(section(ui, "Locations", "文件位置").child(text(move || ui.state.with(|state| state.paths.as_ref().map(|paths| format!("Rime\n{}\n\n{}\n{}\n\n{}\n{}", paths.rime.display(), ui.tr("Configuration", "配置"), paths.config.display(), ui.tr("Cache", "缓存"), paths.cache.display())).unwrap_or_default())).text_size(px(12.0)).text_color(muted)))
        .into_element()
}

fn schemes(ui: Ui) -> AnyElement {
    let mut choices = view().gap_2();
    for schema in Schema::all() {
        let schema = *schema;
        choices = choices.child(
            button()
                .test_id(&format!("schema-{schema:?}"))
                .selected(move || ui.draft.with(|config| config.schema == schema))
                .disabled(move || ui.locked())
                .p_3()
                .rounded(px(6.0))
                .border_1()
                .border_color(move || {
                    if ui.draft.with(|config| config.schema == schema) {
                        amber()
                    } else {
                        line()
                    }
                })
                .bg(surface)
                .focus(|style| {
                    style
                        .outline(px(2.0))
                        .outline_color(amber)
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
                ),
        );
    }
    view().gap_6().child(heading(ui, "Choose a scheme", "选择输入方案"))
        .child(caption(ui, "All 13 schemes share the same updater as the CLI. Save the selection, then explicitly install or update.", "全部 13 个方案使用与命令行相同的更新器。保存选择后，再明确执行安装或更新。"))
        .child(choices)
        .child(toggle(ui, "model-patch", "Enable model patch", "启用模型补丁", |config| config.model_patch_enabled, |config, value| config.model_patch_enabled = value))
        .child(action(ui, "save-scheme", "Save selection", "保存选择", true, move || ui.request(Pending::Save)))
        .child(action(ui, "initialize-scheme", "Install selected scheme", "安装所选方案", false, move || ui.request(Pending::Update(UpdateAction::All))))
        .into_element()
}

fn appearance(ui: Ui) -> AnyElement {
    let candidates = (1..=9)
        .map(|size| (size.to_string(), size.to_string()))
        .collect();
    view()
        .gap_6()
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
            .px_4().py_2().rounded(px(6.0)).border_1().border_color(line).focus(|style| style.outline(px(2.0)).outline_color(amber).outline_offset(px(2.0)))
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
                    "flex-row items-center gap-3 py-2 focus:border-2 focus:border-amber-600",
                )))
                .track_class("w-9 h-5 rounded-full bg-slate-400 checked:bg-amber-600")
                .thumb_class("w-4 h-4 mt-0.5 ml-0.5 rounded-full bg-white checked:translate-x-4")
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
                    "flex-row items-center gap-3 py-2 focus:border-2 focus:border-amber-600",
                )))
                .track_class("w-9 h-5 rounded-full bg-slate-400 checked:bg-amber-600")
                .thumb_class("w-4 h-4 mt-0.5 ml-0.5 rounded-full bg-white checked:translate-x-4")
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
            .px_4().py_2().rounded(px(6.0)).border_1().border_color(line).focus(|style| style.outline(px(2.0)).outline_color(amber).outline_offset(px(2.0)))
            .on_press(move || ui.request(Pending::Skins)).child(text(move || ui.tr("Write presets & redeploy", "写入预设并部署"))))
        .into_element()
}

fn settings(ui: Ui) -> AnyElement {
    view().gap_6()
        .child(section(ui, "General", "常规")
            .child(config_picker(ui, "language", "Language", "语言", &[("zh", "简体中文"), ("en", "English")], |config| config.language.clone(), |config, value| config.language = value))
            .child(config_picker(ui, "tui-theme", "Terminal appearance", "终端外观", &[("auto", "System / 跟随系统"), ("light", "Light / 亮色"), ("dark", "Dark / 暗色")], |config| config.tui_theme_mode.clone(), |config, value| config.tui_theme_mode = value))
            .child(caption(ui, "Desktop appearance follows your operating system.", "桌面外观跟随操作系统。"))
            .child(config_picker(ui, "data-policy", "User data policy", "用户数据策略", &[("prompt", "Ask each time / 每次询问"), ("preserve", "Preserve / 保留"), ("discard", "Discard / 丢弃")], |config| config.user_data_policy.clone(), |config, value| config.user_data_policy = value))
            .child(config_picker(ui, "rime-auto-setup", "Rime integration", "Rime 集成", &[("ask", "Ask / 询问"), ("on", "Automatic / 自动"), ("off", "Manual / 手动")], |config| match config.rime_auto_setup { None => "ask", Some(true) => "on", Some(false) => "off" }.into(), |config, value| config.rime_auto_setup = match value.as_str() { "on" => Some(true), "off" => Some(false), _ => None }))
            .child(field(ui, "skin-patch-key", "Default skin patch key", "默认皮肤补丁 key", false, |config| config.skin_patch_key.clone(), |config, value| config.skin_patch_key = value)))
        .child(section(ui, "Downloads & network", "下载与网络")
            .child(toggle(ui, "mirror", "Use CNB mirror", "使用 CNB 镜像", |config| config.use_mirror, |config, value| config.use_mirror = value))
            .child(config_picker(ui, "download-threads", "Download threads", "下载线程数", &[("1", "1"), ("2", "2"), ("3", "3"), ("4", "4"), ("5", "5"), ("6", "6"), ("7", "7"), ("8", "8")], |config| config.download_threads.to_string(), |config, value| { if let Ok(value) = value.parse() { config.download_threads = value; } }))
            .child(toggle(ui, "proxy-enabled", "Use configured proxy", "使用已配置代理", |config| config.proxy_enabled, |config, value| config.proxy_enabled = value))
            .child(config_picker(ui, "proxy-type", "Proxy type", "代理类型", &[("http", "HTTP"), ("socks5", "SOCKS5")], |config| config.proxy_type.clone(), |config, value| config.proxy_type = value))
            .child(field(ui, "proxy-address", "Proxy address (host:port)", "代理地址（主机:端口）", false, |config| config.proxy_address.clone(), |config, value| config.proxy_address = value))
            .child(caption(ui, "Configured proxy disabled does not disable environment proxies.", "关闭已配置代理不会禁用环境变量中的代理。"))
            .child(text(move || ui.state.with(|state| state.proxy.clone())).text_size(px(12.0)).text_color(muted))
            .child(field(ui, "github-token", "GitHub token", "GitHub 令牌", true, |config| config.github_token.clone(), |config, value| config.github_token = value)))
        .child(section(ui, "Automation", "自动化")
            .child(toggle(ui, "auto-update", "Update automatically on launch", "启动时自动更新", |config| config.auto_update, |config, value| config.auto_update = value))
            .child(field(ui, "countdown", "Countdown in seconds (1–60)", "倒计时秒数（1–60）", false, |config| config.auto_update_countdown.to_string(), |config, value| config.auto_update_countdown = value.parse().unwrap_or(0)))
            .child(toggle(ui, "engine-sync", "Synchronize installed engines", "同步已安装的输入引擎", |config| config.engine_sync_enabled, |config, value| config.engine_sync_enabled = value))
            .child(toggle(ui, "sync-link", "Use symlinks instead of copies", "使用软链接而不是复制", |config| config.engine_sync_use_link, |config, value| config.engine_sync_use_link = value))
            .child(field(ui, "pre-hook", "Pre-update hook", "更新前钩子", false, |config| config.pre_update_hook.clone(), |config, value| config.pre_update_hook = value))
            .child(field(ui, "post-hook", "Post-update hook", "更新后钩子", false, |config| config.post_update_hook.clone(), |config, value| config.post_update_hook = value))
            .child(caption(ui, "Hooks run local commands with your permissions. Review them before every update.", "钩子以你的权限执行本地命令。每次更新前请仔细核对。")))
        .child(view().flex_row().gap_3()
            .child(action(ui, "save-settings", "Save settings", "保存设置", true, move || ui.request(Pending::Save)))
            .child(action(ui, "revert-settings", "Revert changes", "还原修改", false, move || { if let Some(config) = ui.state.with(|state| state.config.clone()) { ui.draft.set(config); ui.dirty.set(false); ui.error.set(None); } })))
        .child(exclusions(ui))
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
                .h(px(36.0))
                .px_3()
                .rounded(px(6.0))
                .border_1()
                .border_color(line)
                .focus(|style| {
                    style
                        .outline(px(2.0))
                        .outline_color(amber)
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
                .px_4().py_2().rounded(px(6.0)).border_1().border_color(line).focus(|style| style.outline(px(2.0)).outline_color(amber).outline_offset(px(2.0)))
                .on_press(move || { if let Some(content) = ui.state.with(|state| state.report.clone()) { ui.export(content, "snout-report.json"); } })
                .child(text(move || ui.tr("Export JSON…", "导出 JSON…")))))
        .child(section(ui, "Shell completions", "Shell 自动补全")
            .child(caption(ui, "Generated from the real CLI command definition.", "从真实命令行定义生成。"))
            .child(view().flex_row().gap_3()
                .child(action(ui, "export-zsh", "Export zsh…", "导出 zsh…", false, move || ui.export(controller::completion(CompletionShell::Zsh), "_snout")))
                .child(action(ui, "export-fish", "Export fish…", "导出 fish…", false, move || ui.export(controller::completion(CompletionShell::Fish), "snout.fish")))))
        .into_element()
}
