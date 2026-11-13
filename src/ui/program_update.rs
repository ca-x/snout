use crate::self_update::{Release, TerminalUpdater};
use crate::types::{CancelSignal, Config};
use std::sync::mpsc;
use tokio::task::JoinHandle;

enum Event {
    Checked(Result<Option<Release>, String>),
    Progress(u64, u64),
    Installed(Result<(), String>),
}

pub(super) struct ProgramUpdate {
    pub message: String,
    pub release: Option<Release>,
    pub disabled: bool,
    pub busy: bool,
    pub installing: bool,
    pub installed: bool,
    pub confirm: bool,
    receiver: Option<mpsc::Receiver<Event>>,
    task: Option<JoinHandle<()>>,
    cancel: CancelSignal,
}

impl ProgramUpdate {
    pub fn new() -> Self {
        let reason = TerminalUpdater::current()
            .err()
            .map(|error| error.to_string());
        Self {
            message: reason
                .clone()
                .unwrap_or_else(|| "Check program updates / 检查程序更新".into()),
            disabled: reason.is_some(),
            release: None,
            busy: false,
            installing: false,
            installed: false,
            confirm: false,
            receiver: None,
            task: None,
            cancel: CancelSignal::new(),
        }
    }

    pub fn check(&mut self, config: Config) {
        if self.busy || self.disabled || self.installed {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let cancel = CancelSignal::new();
        let task_cancel = cancel.clone();
        self.task = Some(tokio::spawn(async move {
            let result = async {
                TerminalUpdater::current()?
                    .check(&config, &task_cancel)
                    .await
            }
            .await
            .map_err(|error: anyhow::Error| error.to_string());
            let _ = sender.send(Event::Checked(result));
        }));
        self.receiver = Some(receiver);
        self.cancel = cancel;
        self.busy = true;
        self.confirm = false;
        self.message = "Checking program updates… / 正在检查程序更新…".into();
    }

    pub fn install(&mut self, config: Config) {
        if self.busy || self.disabled || !self.confirm {
            return;
        }
        let Some(release) = self.release.clone() else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        let cancel = CancelSignal::new();
        let task_cancel = cancel.clone();
        self.task = Some(tokio::spawn(async move {
            let result = async {
                TerminalUpdater::current()?
                    .install(&release, &config, &task_cancel, |received, total| {
                        let _ = sender.send(Event::Progress(received, total));
                    })
                    .await
            }
            .await
            .map_err(|error: anyhow::Error| error.to_string());
            let _ = sender.send(Event::Installed(result));
        }));
        self.receiver = Some(receiver);
        self.cancel = cancel;
        self.busy = true;
        self.installing = true;
        self.confirm = false;
        self.message = "Downloading and verifying… / 正在下载并校验…".into();
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        if let Some(receiver) = &self.receiver {
            while let Ok(event) = receiver.try_recv() {
                changed = true;
                match event {
                    Event::Checked(result) => {
                        self.busy = false;
                        match result {
                            Ok(release) => {
                                self.message = if let Some(release) = &release {
                                    format!("New program version / 程序新版: {}", release.version)
                                } else {
                                    "You are up to date. / 当前已是最新版本。".into()
                                };
                                self.release = release;
                            }
                            Err(error) => self.message = error,
                        }
                    }
                    Event::Progress(received, total) => {
                        self.message = format!("Downloading / 下载: {received} / {total}")
                    }
                    Event::Installed(result) => {
                        self.busy = false;
                        self.installing = false;
                        match result {
                            Ok(()) => {
                                self.installed = true;
                                self.release = None;
                                self.message = "Program updated. Press Enter to exit, then run snout again. / 程序已更新，按 Enter 退出后重新运行 snout。".into();
                            }
                            Err(error) => self.message = error,
                        }
                    }
                }
            }
        }
        changed
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub async fn shutdown(&mut self) {
        self.cancel();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for ProgramUpdate {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn program_install_requires_confirmation_and_respects_distribution_policy() {
        let mut update = ProgramUpdate::new();
        update.disabled = false;
        update.install(Config::default());
        assert!(!update.busy, "no confirmation means no task");
        update.confirm = true;
        update.disabled = true;
        update.install(Config::default());
        assert!(!update.busy, "disabled distribution overrides confirmation");
        assert!(update.task.is_none());
        update.shutdown().await;
    }
}
