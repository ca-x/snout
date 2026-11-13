use anyhow::{bail, Context, Result};
use snout::application::{self, AppearanceTarget, UpdateAction};
use snout::cli::{completion_script, CompletionShell};
use snout::config::{self, Manager};
use snout::i18n::Lang;
use snout::report::Report;
use snout::skin::{fcitx5, patch};
use snout::types::{CancelSignal, Config, UpdateCancelled, UpdateRecord};
use snout::updater::UpdateEvent;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub config: PathBuf,
    pub rime: PathBuf,
    pub cache: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Loading,
    Ready,
    Working,
    Cancelling,
    Success,
    PartialFailure,
    Cancelled,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeTask {
    Search,
    Download { id: u64, name: String },
    Apply,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppearanceTask {
    Candidate,
    Skins,
}

#[derive(Clone, Debug)]
pub struct ThemeDownload {
    pub repository_id: u64,
    pub keys: Vec<String>,
}

#[derive(Clone)]
pub struct Snapshot {
    pub app_update: crate::app_update::State,
    pub config: Option<Config>,
    pub settings_saved: Option<bool>,
    pub appearance_task: Option<AppearanceTask>,
    pub revision: u64,
    pub paths: Option<Paths>,
    pub engines: Vec<String>,
    pub versions: Vec<(String, String)>,
    pub diagnosis: String,
    pub proxy: String,
    pub candidate_size: Option<u8>,
    pub skins: HashSet<String>,
    pub default_skin: Option<String>,
    pub appearance: Option<AppearanceTarget>,
    pub warnings: Vec<String>,
    pub remote_status: Option<application::status::ConfigStatusSnapshot>,
    pub fcitx_repositories: Vec<snout::api::ThemeRepository>,
    pub fcitx_catalog_loaded: bool,
    pub theme_task: Option<ThemeTask>,
    pub theme_download: Option<ThemeDownload>,
    pub fcitx_choices: Vec<(String, String)>,
    pub fcitx_supported: bool,
    pub fcitx_selection: fcitx5::ThemeSelection,
    pub light_rounded: Option<bool>,
    pub dark_rounded: Option<bool>,
    pub status: Status,
    pub busy: bool,
    pub progress: f64,
    pub detail: String,
    pub error: Option<String>,
    pub report: Option<String>,
    pub activity: Vec<String>,
    pub countdown: Option<u64>,
    pub auto_update_due: bool,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            app_update: Default::default(),
            config: None,
            settings_saved: None,
            appearance_task: None,
            revision: 0,
            paths: None,
            engines: Vec::new(),
            versions: Vec::new(),
            diagnosis: String::new(),
            proxy: String::new(),
            candidate_size: None,
            skins: HashSet::new(),
            default_skin: None,
            appearance: None,
            warnings: Vec::new(),
            remote_status: None,
            fcitx_repositories: Vec::new(),
            fcitx_catalog_loaded: false,
            theme_task: None,
            theme_download: None,
            fcitx_choices: Vec::new(),
            fcitx_supported: false,
            fcitx_selection: fcitx5::ThemeSelection::default(),
            light_rounded: None,
            dark_rounded: None,
            status: Status::Loading,
            busy: true,
            progress: 0.0,
            detail: String::new(),
            error: None,
            report: None,
            activity: Vec::new(),
            countdown: None,
            auto_update_due: false,
        }
    }
}

pub enum Command {
    CheckAppUpdate,
    InstallAppUpdate,
    Refresh,
    CheckUpdates,
    Save(Box<Config>),
    InstallScheme {
        config: Box<Config>,
        preserve: bool,
    },
    Update {
        action: UpdateAction,
        preserve: bool,
    },
    AddExclusion(String),
    EditExclusion(usize, String),
    RemoveExclusion(usize),
    ResetExclusions,
    CandidateSize(Option<u8>),
    ApplySkins {
        keys: Vec<String>,
        default: String,
    },
    SearchFcitxThemes,
    ImportFcitxTheme(snout::api::ThemeRepository),
    FcitxThemes {
        light: String,
        dark: String,
        light_rounded: Option<bool>,
        dark_rounded: Option<bool>,
    },
    Export {
        path: PathBuf,
        content: String,
    },
    Shutdown,
}

impl Command {
    fn theme_task(&self) -> Option<ThemeTask> {
        match self {
            Self::SearchFcitxThemes => Some(ThemeTask::Search),
            Self::ImportFcitxTheme(repository) => Some(ThemeTask::Download {
                id: repository.id,
                name: repository.full_name.clone(),
            }),
            Self::FcitxThemes { .. } => Some(ThemeTask::Apply),
            _ => None,
        }
    }
    fn report_command(&self) -> Option<&'static str> {
        match self {
            Self::Update { action, .. } => Some(action.command()),
            Self::InstallScheme { .. } => Some("install"),
            Self::FcitxThemes { .. } => Some("fcitx5-theme"),
            _ => None,
        }
    }
}

enum Event {
    Loaded(Box<Snapshot>),
    Progress(UpdateEvent),
    Finished {
        snapshot: Box<Snapshot>,
        report: Option<String>,
        status: Status,
        error: Option<String>,
    },
    Failed(String),
}

struct Request {
    command: Command,
    cancel: CancelSignal,
}

struct WorkerState {
    cancel: CancelSignal,
    closed: bool,
}

struct Worker {
    sender: Sender<Request>,
    state: Mutex<WorkerState>,
    handle: Mutex<Option<JoinHandle<()>>>,
    process_owned: bool,
}

impl Worker {
    fn submit(&self, command: Command) -> Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.closed {
            bail!("The workbench is closing / 工作台正在关闭");
        }
        state.cancel = CancelSignal::new();
        self.sender.send(Request {
            command,
            cancel: state.cancel.clone(),
        })?;
        Ok(())
    }

    fn cancel(&self) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .cancel
            .cancel();
    }

    fn request_shutdown(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if !state.closed {
            state.closed = true;
            state.cancel.cancel();
            let _ = self.sender.send(Request {
                command: Command::Shutdown,
                cancel: state.cancel.clone(),
            });
        }
    }

    fn join(&self) -> Result<()> {
        let mut handle = self
            .handle
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(worker) = handle.take() {
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("desktop worker panicked"))?;
        }
        Ok(())
    }
}

#[derive(Default)]
struct ProcessWorkers {
    closing: bool,
    workers: Vec<Arc<Worker>>,
}

static PROCESS_WORKERS: OnceLock<Mutex<ProcessWorkers>> = OnceLock::new();

#[must_use]
pub struct ProcessShutdown;

impl ProcessShutdown {
    pub fn install() -> Result<Self> {
        if PROCESS_WORKERS
            .set(Mutex::new(ProcessWorkers::default()))
            .is_err()
        {
            bail!("desktop process shutdown owner is already installed");
        }
        Ok(Self)
    }

    pub fn wait(self) -> Result<()> {
        Self::finish_workers()
    }

    fn finish_workers() -> Result<()> {
        let Some(registry) = PROCESS_WORKERS.get() else {
            return Ok(());
        };
        let workers = {
            let mut registry = registry.lock().unwrap_or_else(|error| error.into_inner());
            registry.closing = true;
            std::mem::take(&mut registry.workers)
        };
        for worker in &workers {
            worker.request_shutdown();
        }
        let mut result = Ok(());
        for worker in workers {
            if let Err(error) = worker.join() {
                result = Err(error);
            }
        }
        result
    }
}

impl Drop for ProcessShutdown {
    fn drop(&mut self) {
        let _ = Self::finish_workers();
    }
}

pub struct Controller {
    receiver: Receiver<Event>,
    worker: Arc<Worker>,
    closed: bool,
    deadline: Option<Instant>,
    pub snapshot: Snapshot,
}

impl Controller {
    pub fn start(paths: Option<Paths>) -> Result<Self> {
        let mut registry = PROCESS_WORKERS
            .get()
            .map(|registry| registry.lock().unwrap_or_else(|error| error.into_inner()));
        if registry.as_ref().is_some_and(|registry| registry.closing) {
            bail!("The workbench is closing / 工作台正在关闭");
        }
        let (sender, requests) = mpsc::channel();
        let (events, receiver) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("snout-desktop-worker".into())
            .spawn(move || {
                if let Err(error) = worker_loop(paths, requests, &events) {
                    let _ = events.send(Event::Failed(error.to_string()));
                }
            })
            .context("start desktop worker")?;
        let worker = Arc::new(Worker {
            sender,
            state: Mutex::new(WorkerState {
                cancel: CancelSignal::new(),
                closed: false,
            }),
            handle: Mutex::new(Some(handle)),
            process_owned: registry.is_some(),
        });
        if let Some(registry) = registry.as_mut() {
            registry.workers.push(Arc::clone(&worker));
        }
        Ok(Self {
            receiver,
            worker,
            closed: false,
            deadline: None,
            snapshot: Snapshot::default(),
        })
    }

    pub fn submit(&mut self, command: Command) -> Result<()> {
        if self.closed || self.snapshot.app_update.installed {
            bail!("The workbench is closing / 工作台正在关闭");
        }
        if self.snapshot.busy {
            bail!("Wait for the current operation / 请等待当前操作完成");
        }
        self.stop_countdown();
        let reporting = command.report_command().is_some();
        let theme_task = command.theme_task();
        let appearance_task = match command {
            Command::CandidateSize(_) => Some(AppearanceTask::Candidate),
            Command::ApplySkins { .. } => Some(AppearanceTask::Skins),
            _ => None,
        };
        self.worker.submit(command)?;
        self.snapshot.theme_task = theme_task;
        self.snapshot.appearance_task = appearance_task;
        if reporting {
            self.snapshot.report = None;
        }
        self.snapshot.busy = true;
        self.snapshot.status = Status::Working;
        self.snapshot.error = None;
        self.snapshot.progress = 0.0;
        self.snapshot.detail.clear();
        Ok(())
    }

    pub fn cancel(&mut self) {
        self.stop_countdown();
        if self.snapshot.busy {
            self.worker.cancel();
            self.snapshot.status = Status::Cancelling;
        }
    }

    pub fn stop_countdown(&mut self) {
        self.deadline = None;
        self.snapshot.countdown = None;
        self.snapshot.auto_update_due = false;
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.receiver.try_recv() {
                Ok(Event::Loaded(snapshot)) => {
                    self.snapshot = *snapshot;
                    if let Some(config) = self
                        .snapshot
                        .config
                        .as_ref()
                        .filter(|config| config.auto_update)
                    {
                        self.deadline = Some(
                            Instant::now()
                                + Duration::from_secs(
                                    config.auto_update_countdown.clamp(1, 60) as u64
                                ),
                        );
                    }
                    changed = true;
                }
                Ok(Event::Progress(event)) => {
                    self.snapshot.progress = event.progress.clamp(0.0, 1.0);
                    self.snapshot.detail = event.detail;
                    self.snapshot.activity.push(format!(
                        "{:?} · {:?} · {}",
                        event.component, event.phase, self.snapshot.detail
                    ));
                    changed = true;
                }
                Ok(Event::Finished {
                    snapshot,
                    report,
                    status,
                    error,
                }) => {
                    let activity = std::mem::take(&mut self.snapshot.activity);
                    let previous_report = self.snapshot.report.take();
                    let theme_task = self.snapshot.theme_task.take();
                    let appearance_task = self.snapshot.appearance_task.take();
                    self.snapshot = *snapshot;
                    self.snapshot.theme_task = theme_task;
                    self.snapshot.appearance_task = appearance_task;
                    self.snapshot.status = status;
                    self.snapshot.error = error;
                    self.snapshot.busy = false;
                    self.snapshot.progress = if status == Status::Success { 1.0 } else { 0.0 };
                    self.snapshot.activity = activity;
                    self.snapshot.report = report.or(previous_report);
                    if let Some(saved) = self.snapshot.settings_saved {
                        self.snapshot.activity.push(settings_result(saved).message);
                        self.snapshot.activity.push(
                            match status {
                                Status::Success => "Installation completed / 安装完成",
                                Status::PartialFailure => {
                                    "Installation partially failed / 安装部分失败"
                                }
                                Status::Cancelled => "Installation cancelled / 安装已取消",
                                _ => "Installation not completed / 安装未完成",
                            }
                            .into(),
                        );
                    }
                    self.snapshot.activity.push(match &self.snapshot.error {
                        Some(error) => error.clone(),
                        None => format!("{status:?}"),
                    });
                    changed = true;
                }
                Ok(Event::Failed(error)) => {
                    self.snapshot.error = Some(error);
                    self.snapshot.status = Status::Error;
                    self.snapshot.busy = false;
                    changed = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if self.snapshot.busy && !self.closed {
                        self.snapshot.busy = false;
                        self.snapshot.status = Status::Error;
                        self.snapshot.error = Some("Worker disconnected / 后台任务已断开".into());
                        changed = true;
                    }
                    break;
                }
            }
        }
        if self.snapshot.activity.len() > 200 {
            self.snapshot
                .activity
                .drain(..self.snapshot.activity.len() - 200);
        }
        if let Some(deadline) = self.deadline {
            let now = Instant::now();
            let remaining = deadline.saturating_duration_since(now).as_secs() + 1;
            if self.snapshot.countdown != Some(remaining) {
                self.snapshot.countdown = Some(remaining);
                changed = true;
            }
            if now >= deadline {
                self.stop_countdown();
                self.snapshot.auto_update_due = true;
                changed = true;
            }
        }
        changed
    }

    pub fn request_shutdown(&mut self) {
        if !self.closed {
            self.closed = true;
            self.stop_countdown();
            self.worker.request_shutdown();
        }
    }

    pub fn shutdown(&mut self) -> Result<()> {
        self.request_shutdown();
        self.worker.join()
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.request_shutdown();
        if !self.worker.process_owned {
            let _ = self.worker.join();
        }
    }
}

struct WorkerEnvironment {
    app_update: crate::app_update::Updater,
    live: bool,
    engines: Vec<String>,
    appearance: Option<AppearanceTarget>,
    remote_status: Option<application::status::ConfigStatusSnapshot>,
    fcitx_repositories: Vec<snout::api::ThemeRepository>,
    fcitx_catalog_loaded: bool,
    theme_download: Option<ThemeDownload>,
    settings_saved: Option<bool>,
}

impl WorkerEnvironment {
    fn new(live: bool, manager: &Manager) -> Self {
        let engines = if live {
            config::detect_installed_engines()
        } else {
            Vec::new()
        };
        let appearance = if live {
            application::appearance_target(&manager.rime_dir, &engines)
        } else {
            None
        };
        Self {
            live,
            app_update: crate::app_update::Updater::new(live),
            engines,
            appearance,
            remote_status: None,
            fcitx_repositories: Vec::new(),
            fcitx_catalog_loaded: false,
            theme_download: None,
            settings_saved: None,
        }
    }
}

fn record_version(path: PathBuf, warnings: &mut Vec<String>) -> String {
    if !path.exists() {
        return "Not installed / 未安装".into();
    }
    match std::fs::read(&path)
        .context("read version record")
        .and_then(|bytes| {
            serde_json::from_slice::<UpdateRecord>(&bytes).context("parse version record")
        }) {
        Ok(record) => format!("{} · {}", record.tag, record.apply_time),
        Err(error) => {
            warnings.push(format!("{}: {error}", path.display()));
            "Unavailable / 无法读取".into()
        }
    }
}

fn redact(text: &str, config: &Config) -> String {
    let mut text = text.to_string();
    if !config.github_token.is_empty() {
        text = text.replace(&config.github_token, "[redacted]");
    }
    if config.proxy_address.contains('@') {
        text = text.replace(&config.proxy_address, "[redacted proxy]");
    }
    text
}

fn redact_json(value: &mut serde_json::Value, config: &Config) {
    match value {
        serde_json::Value::String(text) => *text = redact(text, config),
        serde_json::Value::Array(values) => {
            for value in values {
                redact_json(value, config);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values_mut() {
                redact_json(value, config);
            }
        }
        _ => {}
    }
}

fn snapshot(manager: &Manager, environment: &WorkerEnvironment, revision: u64) -> Snapshot {
    let config = &manager.config;
    let diagnosis = config::diagnose_wanxiang(config, &manager.cache_dir, &manager.rime_dir);
    let lang = Lang::from_str(&config.language);
    let mut versions = Vec::new();
    let mut warnings = Vec::new();
    for (name, path) in [
        ("Scheme / 方案", manager.scheme_record_path()),
        ("Dictionary / 词库", manager.dict_record_path()),
        ("Model / 模型", manager.model_record_path()),
    ] {
        let version = record_version(path, &mut warnings);
        versions.push((name.into(), version));
    }
    let fcitx_supported = matches!(environment.appearance, Some(AppearanceTarget::Fcitx5Theme));
    let fcitx_selection = if fcitx_supported {
        fcitx5::current_theme_selection().unwrap_or_else(|error| {
            warnings.push(format!("Fcitx5: {error}"));
            Default::default()
        })
    } else {
        Default::default()
    };
    let mut rounding = |name: &Option<String>| -> Option<bool> {
        match name {
            Some(name) => fcitx5::installed_theme_rounding(name).unwrap_or_else(|error| {
                warnings.push(format!("{name}: {error}"));
                None
            }),
            None => None,
        }
    };
    let light_rounded = rounding(&fcitx_selection.light);
    let dark_rounded = rounding(&fcitx_selection.dark);
    let proxy = match snout::api::effective_proxy(config) {
        Ok(Some(proxy)) => {
            let endpoint = proxy.url.rsplit('@').next().unwrap_or(&proxy.url);
            format!("{:?} · {endpoint}", proxy.source)
        }
        Ok(None) => "Direct / 直连".into(),
        Err(error) => {
            warnings.push(format!(
                "Proxy / 代理: {}",
                redact(&error.to_string(), config)
            ));
            "Invalid proxy settings / 无效代理配置".into()
        }
    };
    let diagnosis = format!(
        "Detected / 检测: {}\nRecord / 记录: {}\nPatch / 补丁: {}\nConfigured / 配置: {}\n{}",
        diagnosis
            .detected_schema
            .map(|schema| schema.display_name_lang(lang))
            .unwrap_or_else(|| "—".into()),
        diagnosis
            .record_schema
            .map(|schema| schema.display_name_lang(lang))
            .unwrap_or_else(|| "—".into()),
        diagnosis
            .custom_patch_schema
            .map(|schema| schema.display_name_lang(lang))
            .unwrap_or_else(|| "—".into()),
        config.schema.display_name_lang(lang),
        diagnosis
            .marker_files
            .iter()
            .map(|(file, exists)| format!("{} {file}", if *exists { "✓" } else { "–" }))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let candidate_size = snout::custom::candidate_page_size(&manager.rime_dir, config.schema)
        .unwrap_or_else(|error| {
            warnings.push(format!("Candidate settings / 候选设置: {error}"));
            None
        });
    let (skins, default_skin) = match &environment.appearance {
        Some(AppearanceTarget::ThemePatch(path)) => (
            patch::read_skin_preset_selections(path).unwrap_or_else(|error| {
                warnings.push(format!("Skin presets / 皮肤预设: {error}"));
                HashSet::new()
            }),
            patch::read_default_skin(path).unwrap_or_else(|error| {
                warnings.push(format!("Default skin / 默认皮肤: {error}"));
                None
            }),
        ),
        _ => (HashSet::new(), None),
    };
    Snapshot {
        app_update: environment.app_update.state.clone(),
        config: Some(config.clone()),
        settings_saved: environment.settings_saved,
        revision,
        paths: Some(Paths {
            config: manager.config_path.clone(),
            rime: manager.rime_dir.clone(),
            cache: manager.cache_dir.clone(),
        }),
        engines: environment.engines.clone(),
        versions,
        diagnosis,
        proxy,
        candidate_size,
        skins,
        default_skin,
        appearance: environment.appearance.clone(),
        remote_status: environment.remote_status.clone(),
        light_rounded,
        dark_rounded,
        fcitx_selection,
        fcitx_repositories: environment.fcitx_repositories.clone(),
        fcitx_catalog_loaded: environment.fcitx_catalog_loaded,
        theme_download: environment.theme_download.clone(),
        fcitx_choices: if fcitx_supported {
            fcitx5::available_theme_choices().unwrap_or_else(|error| {
                warnings.push(format!("Fcitx5 themes / 主题: {error}"));
                fcitx5::builtin_theme_choices()
            })
        } else {
            Vec::new()
        },
        fcitx_supported,
        warnings,
        status: Status::Ready,
        busy: false,
        ..Snapshot::default()
    }
}

fn worker_loop(
    paths: Option<Paths>,
    requests: Receiver<Request>,
    events: &Sender<Event>,
) -> Result<()> {
    let live = paths.is_none();
    let mut manager = match paths {
        Some(paths) => Manager::with_paths(paths.config, paths.rime, paths.cache)?,
        None => Manager::new()?,
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let mut environment = WorkerEnvironment::new(live, &manager);
    let mut revision = 1;
    events.send(Event::Loaded(Box::new(snapshot(
        &manager,
        &environment,
        revision,
    ))))?;
    for request in requests {
        if matches!(request.command, Command::Shutdown) {
            break;
        }
        let report_command = request.command.report_command();
        let result = runtime.block_on(execute(&mut manager, &mut environment, request, events));
        revision += 1;
        let (report, status, error) =
            command_outcome(result, report_command, &manager, &environment)?;
        let snapshot = snapshot(&manager, &environment, revision);
        events.send(Event::Finished {
            snapshot: Box::new(snapshot),
            report,
            status,
            error,
        })?;
    }
    drop(runtime);
    Ok(())
}

fn command_outcome(
    result: Result<(Option<String>, Status)>,
    report_command: Option<&'static str>,
    manager: &Manager,
    environment: &WorkerEnvironment,
) -> Result<(Option<String>, Status, Option<String>)> {
    Ok(match result {
        Ok((report, status)) => (report, status, None),
        Err(error) => {
            let status = if error.downcast_ref::<UpdateCancelled>().is_some() {
                Status::Cancelled
            } else {
                Status::Error
            };
            let completed = snout::updater::completed_results(&error);
            let error = redact(&error.to_string(), &manager.config);
            let report = report_command
                .map(|command| {
                    let mut report =
                        Report::from_results(command, manager.config.schema, completed);
                    report.ok = false;
                    report.error = Some(error.clone());
                    if command == "install" {
                        report
                            .results
                            .insert(0, settings_result(environment.settings_saved == Some(true)));
                    }
                    report_json(report, &manager.config)
                })
                .transpose()?;
            (report, status, Some(error))
        }
    })
}

async fn execute(
    manager: &mut Manager,
    environment: &mut WorkerEnvironment,
    request: Request,
    events: &Sender<Event>,
) -> Result<(Option<String>, Status)> {
    environment.settings_saved =
        matches!(request.command, Command::InstallScheme { .. }).then_some(false);
    request.cancel.checkpoint()?;
    match request.command {
        Command::CheckAppUpdate => {
            environment
                .app_update
                .check(&manager.config, &request.cancel)
                .await?;
        }
        Command::InstallAppUpdate => {
            let install = environment.app_update.install(
                &manager.config,
                &request.cancel,
                |received, total| {
                    let _ = events.send(Event::Progress(UpdateEvent {
                        component: snout::updater::UpdateComponent::Update,
                        phase: snout::updater::UpdatePhase::Downloading,
                        progress: received as f64 / total.max(1) as f64,
                        detail: format!("Desktop update / 程序更新: {received} / {total}"),
                    }));
                },
            );
            tokio::pin!(install);
            loop {
                tokio::select! {
                    result = &mut install => { result?; break; },
                    _ = tokio::time::sleep(Duration::from_millis(100)) => request.cancel.checkpoint()?,
                }
            }
        }
        Command::Refresh => {
            let selected_schema = manager.config.schema;
            *manager = manager.reload()?;
            manager.config.schema = selected_schema;
            *environment = WorkerEnvironment::new(environment.live, manager);
        }
        Command::CheckUpdates => {
            let manager_snapshot = Manager {
                config: manager.config.clone(),
                config_path: manager.config_path.clone(),
                rime_dir: manager.rime_dir.clone(),
                cache_dir: manager.cache_dir.clone(),
            };
            let check = application::status::build_config_status_snapshot(
                Ok(manager_snapshot),
                manager.config.schema,
                Lang::from_str(&manager.config.language),
                manager.rime_dir.clone(),
            );
            tokio::pin!(check);
            loop {
                tokio::select! {
                    status = &mut check => { environment.remote_status = Some(status); break; },
                    _ = tokio::time::sleep(Duration::from_millis(100)) => request.cancel.checkpoint()?,
                }
            }
        }
        Command::Save(config) => application::save_settings(manager, *config)?,
        Command::InstallScheme { config, preserve } => {
            return install_scheme(
                manager,
                environment,
                *config,
                preserve,
                request.cancel,
                |operation, cancel| async move {
                    application::run_update(UpdateAction::All, &operation, cancel, |mut event| {
                        event.detail = redact(&event.detail, &operation.config);
                        let _ = events.send(Event::Progress(event));
                    })
                    .await
                },
            )
            .await;
        }
        Command::AddExclusion(pattern) => manager.add_exclude_pattern(pattern)?,
        Command::EditExclusion(index, pattern) => manager.update_exclude_pattern(index, pattern)?,
        Command::RemoveExclusion(index) => manager.remove_exclude_pattern(index)?,
        Command::ResetExclusions => manager.reset_exclude_patterns()?,
        Command::CandidateSize(size) => {
            snout::custom::set_candidate_page_size(&manager.rime_dir, manager.config.schema, size)?;
            if environment.live {
                snout::deployer::deploy(Lang::from_str(&manager.config.language))?;
            }
        }
        Command::ApplySkins { keys, default } => {
            let Some(AppearanceTarget::ThemePatch(path)) = &environment.appearance else {
                bail!("No Squirrel / Weasel theme target is available / 没有可用的鼠须管或小狼毫主题目标");
            };
            patch::apply_skin_presets(
                path,
                &keys.iter().map(String::as_str).collect::<Vec<_>>(),
                &default,
            )?;
            if environment.live {
                snout::deployer::deploy(Lang::from_str(&manager.config.language))?;
            }
        }
        Command::SearchFcitxThemes => {
            if !environment.live
                || !matches!(environment.appearance, Some(AppearanceTarget::Fcitx5Theme))
            {
                bail!("Fcitx5 requires a live Linux session / Fcitx5 需要 Linux 会话");
            }
            let client = snout::api::Client::new(&manager.config)?;
            environment.fcitx_repositories =
                client.fetch_fcitx5_repositories(&request.cancel).await?;
            environment.fcitx_catalog_loaded = true;
        }
        Command::ImportFcitxTheme(repository) => {
            if !environment.live
                || !matches!(environment.appearance, Some(AppearanceTarget::Fcitx5Theme))
            {
                bail!("Fcitx5 requires a live Linux session / Fcitx5 需要 Linux 会话");
            }
            let client = snout::api::Client::new(&manager.config)?;
            let archive = client
                .download_theme_repository(&repository, &request.cancel)
                .await?;
            let keys = snout::skin::online::import_archive(
                archive.path(),
                &fcitx5::fcitx_theme_root_path()?,
                &repository,
                &request.cancel,
            )?;
            environment.theme_download = Some(ThemeDownload {
                repository_id: repository.id,
                keys,
            });
        }
        Command::FcitxThemes {
            light,
            dark,
            light_rounded,
            dark_rounded,
        } => {
            if !environment.live
                || !matches!(environment.appearance, Some(AppearanceTarget::Fcitx5Theme))
            {
                bail!("Fcitx5 requires a live Linux session / Fcitx5 需要 Linux 会话");
            }
            request.cancel.checkpoint()?;
            let result = application::apply_fcitx5_themes(
                &light,
                &dark,
                light_rounded,
                dark_rounded,
                Lang::from_str(&manager.config.language),
            )
            .await;
            return report_outcome(
                Report::from_results("fcitx5-theme", manager.config.schema, vec![result]),
                &manager.config,
            );
        }
        Command::Export { path, content } => {
            snout::fileutil::atomic_write(&path, content.as_bytes())
                .context("export file / 导出文件")?;
        }
        Command::Update { action, preserve } => {
            if !environment.live
                && !(action == UpdateAction::Dict && manager.config.schema.dict_zip().is_none())
            {
                bail!("Updates are disabled in isolated preview / 隔离预览中已禁用更新");
            }
            let mut operation = Manager {
                config: manager.config.clone(),
                config_path: manager.config_path.clone(),
                rime_dir: manager.rime_dir.clone(),
                cache_dir: manager.cache_dir.clone(),
            };
            operation.config.user_data_policy = if preserve {
                "preserve".into()
            } else {
                "discard".into()
            };
            let results =
                application::run_update(action, &operation, request.cancel, |mut event| {
                    event.detail = redact(&event.detail, &manager.config);
                    let _ = events.send(Event::Progress(event));
                })
                .await?;
            return report_outcome(
                Report::from_results(action.command(), manager.config.schema, results),
                &manager.config,
            );
        }
        Command::Shutdown => {}
    }
    Ok((None, Status::Success))
}

fn settings_result(saved: bool) -> snout::report::ComponentResult {
    snout::report::ComponentResult {
        component: "settings",
        display_name: "Settings / 设置".into(),
        old_version: "-".into(),
        new_version: "-".into(),
        success: saved,
        message: if saved {
            "Settings saved / 设置已保存"
        } else {
            "Settings not saved / 设置未保存"
        }
        .into(),
    }
}

async fn install_scheme<F, Fut>(
    manager: &mut Manager,
    environment: &mut WorkerEnvironment,
    config: Config,
    preserve: bool,
    cancel: CancelSignal,
    install: F,
) -> Result<(Option<String>, Status)>
where
    F: FnOnce(Manager, CancelSignal) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<snout::updater::UpdateResult>>>,
{
    if !environment.live {
        bail!("Updates are disabled in isolated preview / 隔离预览中已禁用更新");
    }
    let mut saved = false;
    let results = save_then_install(manager, config, preserve, cancel, &mut saved, install).await;
    environment.settings_saved = Some(saved);
    let mut report = Report::from_results("install", manager.config.schema, results?);
    // Saving preferences must not count as a successful installation component.
    let status = report_status(&report);
    report.results.insert(0, settings_result(saved));
    Ok((Some(report_json(report, &manager.config)?), status))
}

async fn save_then_install<F, Fut>(
    manager: &mut Manager,
    config: Config,
    preserve: bool,
    cancel: CancelSignal,
    saved: &mut bool,
    install: F,
) -> Result<Vec<snout::updater::UpdateResult>>
where
    F: FnOnce(Manager, CancelSignal) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<snout::updater::UpdateResult>>>,
{
    *saved = false;
    cancel.checkpoint()?;
    application::save_settings(manager, config)?;
    *saved = true;
    cancel.checkpoint()?;
    let mut operation = Manager {
        config: manager.config.clone(),
        config_path: manager.config_path.clone(),
        rime_dir: manager.rime_dir.clone(),
        cache_dir: manager.cache_dir.clone(),
    };
    operation.config.user_data_policy = if preserve { "preserve" } else { "discard" }.into();
    install(operation, cancel).await
}

fn report_status(report: &Report) -> Status {
    if report.ok {
        Status::Success
    } else if report.results.iter().any(|result| result.success) {
        Status::PartialFailure
    } else {
        Status::Error
    }
}

fn report_outcome(report: Report, config: &Config) -> Result<(Option<String>, Status)> {
    let status = report_status(&report);
    Ok((Some(report_json(report, config)?), status))
}

fn report_json(report: Report, config: &Config) -> Result<String> {
    let mut json = serde_json::to_value(report)?;
    redact_json(&mut json, config);
    Ok(serde_json::to_string_pretty(&json)?)
}

pub fn completion(shell: CompletionShell) -> String {
    completion_script(shell)
}

#[cfg(test)]
mod tests {
    use super::*;
    use snout::types::Schema;
    use tempfile::TempDir;

    fn paths(root: &TempDir) -> Paths {
        Paths {
            config: root.path().join("config.json"),
            rime: root.path().join("rime"),
            cache: root.path().join("cache"),
        }
    }

    fn ready(controller: &mut Controller) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while controller.snapshot.busy && Instant::now() < deadline {
            controller.poll();
            thread::sleep(Duration::from_millis(2));
        }
        controller.poll();
        assert!(!controller.snapshot.busy, "worker did not settle");
    }

    fn run(controller: &mut Controller, command: Command) {
        controller.submit(command).expect("submit command");
        ready(controller);
    }

    #[test]
    fn preview_rejects_direct_app_update_commands_before_network_or_installation() {
        let root = TempDir::new().unwrap();
        let mut controller = Controller::start(Some(paths(&root))).unwrap();
        ready(&mut controller);
        assert!(controller.snapshot.app_update.disabled.is_some());
        for command in [Command::CheckAppUpdate, Command::InstallAppUpdate] {
            run(&mut controller, command);
            assert_eq!(controller.snapshot.status, Status::Error);
            assert!(controller
                .snapshot
                .error
                .as_deref()
                .unwrap()
                .contains("preview"));
            assert!(!controller.snapshot.app_update.installed);
            assert!(controller.snapshot.app_update.available.is_none());
        }
        controller.shutdown().unwrap();
    }

    fn install_fixture(root: &TempDir) -> (Manager, WorkerEnvironment) {
        let paths = paths(root);
        let manager = Manager::with_paths(paths.config, paths.rime, paths.cache).unwrap();
        manager.save().unwrap();
        // Mark only the environment live; never detect or touch host input methods.
        let mut environment = WorkerEnvironment::new(false, &manager);
        environment.live = true;
        (manager, environment)
    }

    fn install_json(
        result: Result<(Option<String>, Status)>,
        manager: &Manager,
        environment: &WorkerEnvironment,
    ) -> (serde_json::Value, Status, Option<String>) {
        let (json, status, error) =
            command_outcome(result, Some("install"), manager, environment).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json.unwrap()).unwrap();
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["command"], "install");
        assert!(json.get("config").is_none());
        (json, status, error)
    }

    #[tokio::test]
    async fn saved_settings_survive_install_error_or_cancellation_and_errors_are_redacted() {
        for cancelled in [false, true] {
            let root = TempDir::new().unwrap();
            let (mut manager, mut environment) = install_fixture(&root);
            std::fs::write(manager.scheme_record_path(), "installed A").unwrap();
            let proposed = Config {
                schema: Schema::Ice,
                github_token: "install-secret-token".into(),
                proxy_address: "user:password@localhost:1080".into(),
                ..manager.config.clone()
            };
            let expected = serde_json::to_value(&proposed).unwrap();
            let result = install_scheme(
                &mut manager,
                &mut environment,
                proposed,
                true,
                CancelSignal::new(),
                |operation, cancel| async move {
                    let persisted: serde_json::Value =
                        serde_json::from_slice(&std::fs::read(&operation.config_path).unwrap())
                            .unwrap();
                    assert_eq!(
                        persisted["schema"],
                        serde_json::to_value(Schema::Ice).unwrap()
                    );
                    let message = format!(
                        "{} via {}",
                        operation.config.github_token, operation.config.proxy_address
                    );
                    if cancelled {
                        cancel.cancel();
                        cancel.checkpoint().with_context(|| message.clone())?;
                    }
                    bail!(message)
                },
            )
            .await;
            assert_eq!(environment.settings_saved, Some(true));
            assert_eq!(serde_json::to_value(&manager.config).unwrap(), expected);
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(
                    &std::fs::read(&manager.config_path).unwrap()
                )
                .unwrap(),
                expected
            );
            assert_eq!(
                std::fs::read_to_string(manager.scheme_record_path()).unwrap(),
                "installed A"
            );
            assert!(!manager.rime_dir.exists());
            let (report, status, error) = install_json(result, &manager, &environment);
            assert_eq!(
                status,
                if cancelled {
                    Status::Cancelled
                } else {
                    Status::Error
                }
            );
            assert_eq!(report["ok"], false);
            assert_eq!(report["schema"], serde_json::to_value(Schema::Ice).unwrap());
            assert_eq!(report["results"][0]["component"], "settings");
            assert_eq!(report["results"][0]["success"], true);
            for output in [report.to_string(), error.unwrap()] {
                assert!(!output.contains("install-secret-token"));
                assert!(!output.contains("password"));
                assert!(output.contains("[redacted]"));
                assert!(output.contains("[redacted proxy]"));
            }
        }
    }

    #[tokio::test]
    async fn install_pre_cancel_and_invalid_save_never_enter_install_or_write() {
        for pre_cancelled in [false, true] {
            let root = TempDir::new().unwrap();
            let (mut manager, mut environment) = install_fixture(&root);
            let original = std::fs::read(&manager.config_path).unwrap();
            let mut proposed = manager.config.clone();
            proposed.download_threads = if pre_cancelled { 8 } else { 0 };
            let cancel = CancelSignal::new();
            if pre_cancelled {
                cancel.cancel();
            }
            let result = install_scheme(
                &mut manager,
                &mut environment,
                proposed,
                true,
                cancel,
                |_, _| async { panic!("install must not start") },
            )
            .await;
            let (report, status, _) = install_json(result, &manager, &environment);
            assert_eq!(
                status,
                if pre_cancelled {
                    Status::Cancelled
                } else {
                    Status::Error
                }
            );
            assert_eq!(environment.settings_saved, Some(false));
            assert_eq!(report["results"][0]["success"], false);
            assert_eq!(std::fs::read(&manager.config_path).unwrap(), original);
            assert_eq!(manager.config.download_threads, 4);
            assert!(!manager.rime_dir.exists());
        }
    }

    #[tokio::test]
    async fn install_preserve_overrides_only_operation_config_and_settings_do_not_mask_status() {
        use snout::updater::{BaseUpdater, UpdateComponent};
        for preserve in [false, true] {
            for expected_status in [Status::Success, Status::PartialFailure, Status::Error] {
                let root = TempDir::new().unwrap();
                let (mut manager, mut environment) = install_fixture(&root);
                let proposed = manager.config.clone();
                let result = install_scheme(
                    &mut manager,
                    &mut environment,
                    proposed,
                    preserve,
                    CancelSignal::new(),
                    |operation, _| async move {
                        assert_eq!(
                            operation.config.user_data_policy,
                            if preserve { "preserve" } else { "discard" }
                        );
                        let persisted: Config =
                            serde_json::from_slice(&std::fs::read(&operation.config_path).unwrap())
                                .unwrap();
                        assert_eq!(persisted.user_data_policy, "prompt");
                        let success = BaseUpdater::success_result(
                            UpdateComponent::Scheme,
                            "Scheme",
                            "old",
                            "new",
                            "Installed",
                        );
                        let failure = BaseUpdater::error_result(
                            UpdateComponent::Dict,
                            "Dictionary",
                            "Unavailable",
                        );
                        Ok(match expected_status {
                            Status::Success => vec![success],
                            Status::PartialFailure => vec![success, failure],
                            _ => vec![failure],
                        })
                    },
                )
                .await;
                let (report, status, error) = install_json(result, &manager, &environment);
                assert_eq!(status, expected_status);
                assert!(error.is_none());
                assert_eq!(report["ok"], expected_status == Status::Success);
                assert_eq!(report["results"][0]["success"], true);
                assert_eq!(manager.config.user_data_policy, "prompt");
                if expected_status != Status::Error {
                    assert_eq!(report["results"][1]["component"], "scheme");
                    assert_eq!(report["results"][1]["old_version"], "old");
                    assert_eq!(report["results"][1]["new_version"], "new");
                }
            }
        }
    }

    #[tokio::test]
    async fn cancelled_install_keeps_core_completed_results_separate_from_saved_settings() {
        use snout::updater::{UpdateComponent, UpdatePhase};
        let root = TempDir::new().unwrap();
        let (mut manager, mut environment) = install_fixture(&root);
        let proposed = Config {
            schema: Schema::Ice,
            // Avoid ambient credential discovery when constructing the client.
            github_token: "isolated-test-token".into(),
            ..manager.config.clone()
        };
        let result = install_scheme(
            &mut manager,
            &mut environment,
            proposed,
            true,
            CancelSignal::new(),
            |operation, cancel| async move {
                let signal = cancel.clone();
                snout::updater::update_all(
                    &operation.config.schema,
                    &operation.config,
                    operation.cache_dir.clone(),
                    operation.rime_dir.clone(),
                    cancel,
                    |event| {
                        assert_eq!(
                            event.component,
                            UpdateComponent::Scheme,
                            "no later stage may start"
                        );
                        if event.phase == UpdatePhase::Checking {
                            signal.cancel(); // Before the release request: deterministic, no network.
                        }
                    },
                )
                .await
            },
        )
        .await;
        let error = result.as_ref().unwrap_err();
        assert!(error.downcast_ref::<UpdateCancelled>().is_some());
        assert_eq!(snout::updater::completed_results(error).len(), 1);
        let (report, status, _) = install_json(result, &manager, &environment);
        assert_eq!(status, Status::Cancelled);
        assert_eq!(report["ok"], false);
        assert_eq!(report["results"].as_array().unwrap().len(), 2);
        assert_eq!(report["results"][0]["component"], "settings");
        assert_eq!(report["results"][0]["success"], true);
        assert_eq!(report["results"][1]["component"], "scheme");
        assert_eq!(report["results"][1]["success"], false);
        assert!(!manager.rime_dir.exists());
    }

    #[test]
    fn preview_install_rejects_before_save_and_pre_cancel_marks_settings_unsaved() {
        let root = TempDir::new().unwrap();
        let (manager, _) = install_fixture(&root);
        let original = std::fs::read(&manager.config_path).unwrap();
        let mut controller = Controller::start(Some(paths(&root))).unwrap();
        ready(&mut controller);
        for pre_cancelled in [false, true] {
            let mut config = controller.snapshot.config.clone().unwrap();
            config.schema = Schema::Ice;
            let cancel = CancelSignal::new();
            if pre_cancelled {
                cancel.cancel();
            }
            controller
                .worker
                .sender
                .send(Request {
                    command: Command::InstallScheme {
                        config: Box::new(config),
                        preserve: true,
                    },
                    cancel,
                })
                .unwrap();
            controller.snapshot.busy = true;
            ready(&mut controller);
            assert_eq!(
                controller.snapshot.status,
                if pre_cancelled {
                    Status::Cancelled
                } else {
                    Status::Error
                }
            );
            assert_eq!(controller.snapshot.settings_saved, Some(false));
            assert_eq!(std::fs::read(&manager.config_path).unwrap(), original);
            assert!(!manager.rime_dir.exists());
            assert!(controller
                .snapshot
                .activity
                .iter()
                .any(|line| line == "Settings not saved / 设置未保存"));
            if !pre_cancelled {
                assert!(controller
                    .snapshot
                    .error
                    .as_deref()
                    .unwrap()
                    .contains("isolated preview"));
            }
        }
        run(&mut controller, Command::Refresh);
        assert_eq!(controller.snapshot.settings_saved, None);
    }

    #[test]
    fn poll_activity_explains_saved_stage_and_installation_outcome_without_json() {
        for (saved, status, outcome) in [
            (true, Status::Success, "Installation completed / 安装完成"),
            (
                true,
                Status::PartialFailure,
                "Installation partially failed / 安装部分失败",
            ),
            (
                true,
                Status::Error,
                "Installation not completed / 安装未完成",
            ),
            (
                true,
                Status::Cancelled,
                "Installation cancelled / 安装已取消",
            ),
            (
                false,
                Status::Error,
                "Installation not completed / 安装未完成",
            ),
            (
                false,
                Status::Cancelled,
                "Installation cancelled / 安装已取消",
            ),
        ] {
            let (events, receiver) = mpsc::channel();
            let (sender, _requests) = mpsc::channel();
            let mut controller = Controller {
                receiver,
                worker: Arc::new(Worker {
                    sender,
                    state: Mutex::new(WorkerState {
                        cancel: CancelSignal::new(),
                        closed: false,
                    }),
                    handle: Mutex::new(None),
                    process_owned: false,
                }),
                closed: false,
                deadline: None,
                snapshot: Snapshot {
                    activity: vec!["Earlier operation".into()],
                    ..Snapshot::default()
                },
            };
            events
                .send(Event::Finished {
                    snapshot: Box::new(Snapshot {
                        settings_saved: Some(saved),
                        ..Snapshot::default()
                    }),
                    report: None,
                    status,
                    error: None,
                })
                .unwrap();
            assert!(controller.poll());
            assert_eq!(controller.snapshot.activity[0], "Earlier operation");
            assert_eq!(
                controller.snapshot.activity[1],
                settings_result(saved).message
            );
            assert_eq!(controller.snapshot.activity[2], outcome);
            assert_eq!(controller.snapshot.status, status);
            assert!(!controller.snapshot.busy);
        }
    }

    #[test]
    fn theme_task_result_is_kept_and_preview_rejects_theme_downloads() {
        let root = TempDir::new().unwrap();
        let mut controller = Controller::start(Some(paths(&root))).unwrap();
        ready(&mut controller);
        let repository = snout::api::ThemeRepository {
            id: 42,
            full_name: "owner/bundle".into(),
            default_branch: "main".into(),
            description: None,
            topics: vec![],
            stargazers_count: 0,
        };
        run(&mut controller, Command::ImportFcitxTheme(repository));
        assert_eq!(controller.snapshot.status, Status::Error);
        assert_eq!(
            controller.snapshot.theme_task,
            Some(ThemeTask::Download {
                id: 42,
                name: "owner/bundle".into()
            })
        );
        assert!(controller.snapshot.theme_download.is_none());
        assert!(!root.path().join("rime").exists());
        run(&mut controller, Command::Refresh);
        assert!(controller.snapshot.theme_task.is_none());
    }

    #[test]
    fn settings_use_shared_validation_and_preserve_invalid_drafts_on_disk() {
        let root = TempDir::new().expect("temporary root");
        let mut controller = Controller::start(Some(paths(&root))).expect("controller");
        ready(&mut controller);
        let mut config = controller.snapshot.config.clone().expect("configuration");
        config.download_threads = 0;
        run(&mut controller, Command::Save(Box::new(config)));
        assert_eq!(controller.snapshot.status, Status::Error);
        assert_eq!(
            controller
                .snapshot
                .config
                .as_ref()
                .expect("configuration")
                .download_threads,
            4
        );
        let mut config = controller.snapshot.config.clone().expect("configuration");
        config.language = "en".into();
        config.download_threads = 8;
        config.schema = Schema::Ice;
        run(&mut controller, Command::Save(Box::new(config)));
        assert_eq!(controller.snapshot.status, Status::Success);
        let persisted: Config =
            serde_json::from_slice(&std::fs::read(paths(&root).config).expect("read config"))
                .expect("valid config");
        assert_eq!(persisted.schema, Schema::Ice);
        assert_eq!(persisted.download_threads, 8);
        controller.shutdown().expect("shutdown");
    }

    #[test]
    fn exclusion_crud_and_reset_use_shared_manager() {
        let root = TempDir::new().expect("temporary root");
        let mut controller = Controller::start(Some(paths(&root))).expect("controller");
        ready(&mut controller);
        run(
            &mut controller,
            Command::AddExclusion("personal.txt".into()),
        );
        let index = controller
            .snapshot
            .config
            .as_ref()
            .expect("config")
            .exclude_files
            .len()
            - 1;
        run(
            &mut controller,
            Command::EditExclusion(index, "personal-*.txt".into()),
        );
        assert_eq!(
            controller
                .snapshot
                .config
                .as_ref()
                .expect("config")
                .exclude_files[index],
            "personal-*.txt"
        );
        run(&mut controller, Command::RemoveExclusion(index));
        assert!(!controller
            .snapshot
            .config
            .as_ref()
            .expect("config")
            .exclude_files
            .iter()
            .any(|pattern| pattern.starts_with("personal")));
        run(&mut controller, Command::ResetExclusions);
        assert_eq!(
            controller
                .snapshot
                .config
                .as_ref()
                .expect("config")
                .exclude_files,
            config::default_exclude_patterns()
        );
        controller.shutdown().expect("shutdown");
    }

    #[test]
    fn candidate_apply_reset_and_validation_stay_inside_temporary_rime() {
        let root = TempDir::new().expect("temporary root");
        let mut controller = Controller::start(Some(paths(&root))).expect("controller");
        ready(&mut controller);
        run(&mut controller, Command::CandidateSize(Some(9)));
        assert_eq!(controller.snapshot.candidate_size, Some(9));
        run(&mut controller, Command::CandidateSize(Some(0)));
        assert_eq!(controller.snapshot.status, Status::Error);
        assert_eq!(controller.snapshot.candidate_size, Some(9));
        run(&mut controller, Command::CandidateSize(None));
        assert_eq!(controller.snapshot.candidate_size, None);
        controller.shutdown().expect("shutdown");
    }

    #[test]
    fn optional_corrupt_records_warn_without_locking_out_settings() {
        let root = TempDir::new().expect("temporary root");
        let paths = paths(&root);
        std::fs::create_dir_all(&paths.cache).expect("cache");
        std::fs::write(paths.cache.join("scheme_record.json"), "{broken").expect("corrupt fixture");
        std::fs::create_dir_all(&paths.rime).expect("rime");
        std::fs::write(paths.rime.join("wanxiang.custom.yaml"), "patch: [broken")
            .expect("corrupt optional patch");
        let mut controller = Controller::start(Some(paths)).expect("controller");
        ready(&mut controller);
        assert!(controller.snapshot.config.is_some());
        assert!(!controller.snapshot.warnings.is_empty());
        assert_eq!(controller.snapshot.status, Status::Ready);
        assert!(!controller.snapshot.fcitx_supported);
        controller.shutdown().expect("shutdown");
    }

    #[test]
    fn cancellation_prevents_update_and_shutdown_joins_worker() {
        let root = TempDir::new().expect("temporary root");
        let paths = paths(&root);
        let mut manager =
            Manager::with_paths(paths.config, paths.rime, paths.cache).expect("manager");
        let mut environment = WorkerEnvironment::new(false, &manager);
        let cancel = CancelSignal::new();
        cancel.cancel();
        let (sender, _) = mpsc::channel();
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let error = runtime
            .block_on(execute(
                &mut manager,
                &mut environment,
                Request {
                    command: Command::Update {
                        action: UpdateAction::All,
                        preserve: true,
                    },
                    cancel,
                },
                &sender,
            ))
            .expect_err("pre-cancelled update must not run");
        assert!(error.downcast_ref::<UpdateCancelled>().is_some());
        assert!(!manager.rime_dir.exists());
        let mut controller =
            Controller::start(Some(super::tests::paths(&root))).expect("controller");
        ready(&mut controller);
        controller.shutdown().expect("shutdown joins worker");
        assert!(controller
            .worker
            .handle
            .lock()
            .expect("worker handle")
            .is_none());
        assert!(controller.closed);
    }

    #[test]
    fn busy_operations_are_rejected_and_exports_match_real_cli() {
        let root = TempDir::new().expect("temporary root");
        let mut controller = Controller::start(Some(paths(&root))).expect("controller");
        assert!(controller.submit(Command::Refresh).is_err());
        ready(&mut controller);
        let output = root.path().join("snout.fish");
        let content = completion(CompletionShell::Fish);
        run(
            &mut controller,
            Command::Export {
                path: output.clone(),
                content: content.clone(),
            },
        );
        assert_eq!(
            std::fs::read_to_string(output).expect("exported completion"),
            content
        );
        assert!(content.contains("snout"));
        controller.shutdown().expect("shutdown");
    }

    #[test]
    fn reports_redact_credentials_recursively_and_do_not_include_configuration() {
        let config = Config {
            github_token: "test-secret-token".into(),
            proxy_address: "user:password@localhost:1080".into(),
            ..Config::default()
        };
        let mut json = serde_json::json!({"results": [{"message": "test-secret-token via user:password@localhost:1080"}]});
        redact_json(&mut json, &config);
        let output = serde_json::to_string(&json).expect("JSON");
        assert!(!output.contains("test-secret-token"));
        assert!(!output.contains("password"));
        assert!(output.contains("[redacted]"));
    }

    #[test]
    fn cancelled_update_replaces_previous_success_report() {
        let root = TempDir::new().expect("temporary root");
        let mut controller = Controller::start(Some(paths(&root))).expect("controller");
        ready(&mut controller);
        controller.snapshot.report = Some("previous success".into());
        let cancel = CancelSignal::new();
        cancel.cancel();
        controller
            .worker
            .sender
            .send(Request {
                command: Command::Update {
                    action: UpdateAction::All,
                    preserve: true,
                },
                cancel,
            })
            .expect("send pre-cancelled update");
        controller.snapshot.busy = true;
        ready(&mut controller);
        assert_eq!(controller.snapshot.status, Status::Cancelled);
        let report: serde_json::Value = serde_json::from_str(
            controller
                .snapshot
                .report
                .as_deref()
                .expect("cancelled report"),
        )
        .expect("new JSON, not stale success");
        assert_eq!(report["ok"], false);
        assert_eq!(report["command"], "update");
        assert!(!controller
            .snapshot
            .paths
            .as_ref()
            .expect("paths")
            .rime
            .exists());
        controller.shutdown().expect("shutdown");
    }

    #[test]
    fn preview_rejects_every_effectful_update_before_core_dispatch() {
        let root = TempDir::new().expect("temporary root");
        let paths = paths(&root);
        let marker = root.path().join("hook-must-not-run");
        let mut manager = Manager::with_paths(
            paths.config.clone(),
            paths.rime.clone(),
            paths.cache.clone(),
        )
        .expect("manager");
        manager.config.rime_auto_setup = Some(true);
        manager.config.engine_sync_enabled = true;
        manager.config.pre_update_hook = format!("touch {}", marker.display());
        manager.config.post_update_hook = manager.config.pre_update_hook.clone();
        manager.save().expect("preview config");
        let original = std::fs::read(&paths.config).expect("original config");
        let mut controller = Controller::start(Some(paths.clone())).expect("controller");
        ready(&mut controller);
        for action in [
            UpdateAction::All,
            UpdateAction::Scheme,
            UpdateAction::Dict,
            UpdateAction::Model,
            UpdateAction::RimeSetup,
        ] {
            run(
                &mut controller,
                Command::Update {
                    action,
                    preserve: true,
                },
            );
            assert_eq!(
                controller.snapshot.status,
                Status::Error,
                "{action:?} must be rejected"
            );
            assert!(controller
                .snapshot
                .error
                .as_deref()
                .expect("visible error")
                .contains("disabled in isolated preview"));
            let report: serde_json::Value = serde_json::from_str(
                controller
                    .snapshot
                    .report
                    .as_deref()
                    .expect("structured failure"),
            )
            .expect("report JSON");
            assert_eq!(report["ok"], false);
            assert_eq!(report["command"], action.command());
            assert!(!paths.rime.exists(), "preview must not install anything");
            assert!(!marker.exists(), "preview must not execute hooks");
            assert_eq!(
                std::fs::read(&paths.config).expect("unchanged configuration"),
                original
            );
        }
        controller.shutdown().expect("shutdown");
    }

    #[test]
    fn theme_reports_replace_stale_json_including_failed_and_cancelled_requests() {
        let root = TempDir::new().expect("temporary root");
        let mut controller = Controller::start(Some(paths(&root))).expect("controller");
        ready(&mut controller);
        let theme_command = || Command::FcitxThemes {
            light: "catppuccin-latte-sky".into(),
            dark: "catppuccin-mocha-sky".into(),
            light_rounded: Some(true),
            dark_rounded: Some(false),
        };
        controller.snapshot.report = Some("stale successful update".into());
        controller
            .submit(theme_command())
            .expect("submit preview theme operation");
        assert!(
            controller.snapshot.report.is_none(),
            "new theme operation clears stale results immediately"
        );
        ready(&mut controller);
        assert_eq!(controller.snapshot.status, Status::Error);
        let report: serde_json::Value = serde_json::from_str(
            controller
                .snapshot
                .report
                .as_deref()
                .expect("theme failure report"),
        )
        .expect("theme report JSON");
        assert_eq!(report["command"], "fcitx5-theme");
        assert_eq!(report["ok"], false);
        assert!(report["error"]
            .as_str()
            .expect("visible unavailable error")
            .contains("live Linux session"));
        let previous_error = report["error"].clone();
        let cancel = CancelSignal::new();
        cancel.cancel();
        controller
            .worker
            .sender
            .send(Request {
                command: theme_command(),
                cancel,
            })
            .expect("pre-cancelled theme request");
        controller.snapshot.busy = true;
        ready(&mut controller);
        assert_eq!(controller.snapshot.status, Status::Cancelled);
        let report: serde_json::Value = serde_json::from_str(
            controller
                .snapshot
                .report
                .as_deref()
                .expect("cancelled theme report"),
        )
        .expect("theme cancellation JSON");
        assert_eq!(report["command"], "fcitx5-theme");
        assert_eq!(report["ok"], false);
        assert_ne!(
            report["error"], previous_error,
            "cancellation must replace the previous theme failure"
        );
        controller.shutdown().expect("shutdown");
    }

    #[test]
    fn all_failed_component_report_is_error_without_generic_partial_failure_message() {
        let root = TempDir::new().expect("temporary root");
        let paths = paths(&root);
        let mut manager =
            Manager::with_paths(paths.config, paths.rime, paths.cache).expect("manager");
        manager.config.schema = Schema::Frost;
        let mut environment = WorkerEnvironment::new(false, &manager);
        let (events, _receiver) = mpsc::channel();
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let (report, status) = runtime
            .block_on(execute(
                &mut manager,
                &mut environment,
                Request {
                    command: Command::Update {
                        action: UpdateAction::Dict,
                        preserve: true,
                    },
                    cancel: CancelSignal::new(),
                },
                &events,
            ))
            .expect("side-effect-free unsupported standalone dictionary result");
        assert_eq!(status, Status::Error);
        let report: serde_json::Value =
            serde_json::from_str(report.as_deref().expect("failure report")).expect("JSON");
        assert_eq!(report["ok"], false);
        assert_eq!(report["error"], serde_json::Value::Null);
        assert_eq!(report["results"][0]["success"], false);
        assert!(!manager.rime_dir.exists());
    }

    #[test]
    fn shared_theme_report_helper_preserves_component_fields_and_mixed_status() {
        use snout::updater::{BaseUpdater, UpdateComponent};
        let config = Config::default();
        let theme = BaseUpdater::success_result(
            UpdateComponent::Fcitx5Theme,
            "Fcitx5 theme",
            "-",
            "-",
            "Applied",
        );
        let (json, status) = report_outcome(
            Report::from_results("fcitx5-theme", config.schema, vec![theme]),
            &config,
        )
        .expect("shared theme report");
        assert_eq!(status, Status::Success);
        let report: serde_json::Value =
            serde_json::from_str(json.as_deref().expect("theme JSON")).expect("JSON");
        assert_eq!(report["command"], "fcitx5-theme");
        assert_eq!(report["ok"], true);
        assert_eq!(report["results"][0]["component"], "fcitx5_theme");
        assert_eq!(report["results"][0]["message"], "Applied");
        assert_eq!(report["results"][0]["old_version"], "-");
        assert_eq!(report["results"][0]["new_version"], "-");
        let success =
            BaseUpdater::success_result(UpdateComponent::Scheme, "Scheme", "old", "new", "Updated");
        let failure = BaseUpdater::error_result(UpdateComponent::Dict, "Dictionary", "Unavailable");
        let (json, status) = report_outcome(
            Report::from_results("update", config.schema, vec![success, failure]),
            &config,
        )
        .expect("mixed results");
        assert_eq!(status, Status::PartialFailure);
        let report: serde_json::Value =
            serde_json::from_str(json.as_deref().expect("mixed JSON")).expect("JSON");
        assert_eq!(report["ok"], false);
        assert_eq!(report["error"], serde_json::Value::Null);
    }
}
