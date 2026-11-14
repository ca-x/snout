use super::UpdateAction;
use crate::i18n::Lang;
use crate::types::Config;
use crate::updater::{UpdateComponent, UpdateEvent, UpdatePhase};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    Pending,
    Active,
    Complete,
    Failed,
    Cancelled,
    Skipped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Check,
    Download,
    Verify,
    Install,
}

impl Stage {
    pub const ALL: [Self; 4] = [Self::Check, Self::Download, Self::Verify, Self::Install];

    pub fn label(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Self::Check, Lang::Zh) => "检查",
            (Self::Download, Lang::Zh) => "下载",
            (Self::Verify, Lang::Zh) => "校验",
            (Self::Install, Lang::Zh) => "安装与应用",
            (Self::Check, _) => "Check",
            (Self::Download, _) => "Download",
            (Self::Verify, _) => "Verify",
            (Self::Install, _) => "Install & apply",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentProgress {
    pub component: UpdateComponent,
    pub state: StepState,
    pub stage: Stage,
    visited: Vec<Stage>,
}

impl ComponentProgress {
    pub fn step_state(&self, stage: Stage) -> StepState {
        if self.stage == stage
            && matches!(
                self.state,
                StepState::Active | StepState::Failed | StepState::Cancelled
            )
        {
            self.state
        } else if self.visited.contains(&stage) {
            StepState::Complete
        } else if matches!(
            self.state,
            StepState::Complete | StepState::Skipped | StepState::Cancelled | StepState::Failed
        ) {
            StepState::Skipped
        } else {
            StepState::Pending
        }
    }

    pub fn label(&self, lang: Lang) -> String {
        match self.state {
            StepState::Pending => if lang == Lang::Zh {
                "等待中"
            } else {
                "Waiting"
            }
            .into(),
            StepState::Active => self.stage.label(lang).into(),
            StepState::Complete => if lang == Lang::Zh {
                "已完成"
            } else {
                "Completed"
            }
            .into(),
            StepState::Failed => if lang == Lang::Zh {
                "未完成，请重试"
            } else {
                "Failed — retry"
            }
            .into(),
            StepState::Cancelled => if lang == Lang::Zh {
                "已取消"
            } else {
                "Cancelled"
            }
            .into(),
            StepState::Skipped => if lang == Lang::Zh {
                "未执行"
            } else {
                "Not run"
            }
            .into(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct OperationProgress {
    pub components: Vec<ComponentProgress>,
    pub finished: bool,
}

impl OperationProgress {
    pub fn new(components: impl IntoIterator<Item = UpdateComponent>) -> Self {
        Self {
            components: components
                .into_iter()
                .map(|component| ComponentProgress {
                    component,
                    state: StepState::Pending,
                    stage: Stage::Check,
                    visited: Vec::new(),
                })
                .collect(),
            finished: false,
        }
    }

    pub fn for_action(action: UpdateAction, config: &Config) -> Self {
        let components = match action {
            UpdateAction::All => {
                let mut components = vec![UpdateComponent::Scheme];
                if config.schema.dict_zip().is_some() {
                    components.push(UpdateComponent::Dict);
                }
                if config.schema.supports_model_patch() && config.model_patch_enabled {
                    components.push(UpdateComponent::Model);
                }
                components.push(UpdateComponent::Deploy);
                components
            }
            UpdateAction::Scheme => vec![UpdateComponent::Scheme],
            UpdateAction::Dict => vec![UpdateComponent::Dict],
            UpdateAction::Model => vec![UpdateComponent::Model],
            UpdateAction::RimeSetup => vec![UpdateComponent::Fcitx5Setup],
        };
        Self::new(components)
    }

    pub fn observe(&mut self, event: &UpdateEvent) {
        if self.finished
            || (event.component == UpdateComponent::Update
                && !self.components.is_empty()
                && !self
                    .components
                    .iter()
                    .any(|entry| entry.component == UpdateComponent::Update))
        {
            return;
        }
        if !self
            .components
            .iter()
            .any(|entry| entry.component == event.component)
        {
            self.components.push(ComponentProgress {
                component: event.component,
                state: StepState::Pending,
                stage: Stage::Check,
                visited: Vec::new(),
            });
        }
        let entry = self
            .components
            .iter_mut()
            .find(|entry| entry.component == event.component)
            .expect("component was inserted");
        match event.phase {
            UpdatePhase::Finished => entry.state = StepState::Complete,
            UpdatePhase::Cancelled => entry.state = StepState::Cancelled,
            UpdatePhase::Cancelling => {}
            phase => {
                entry.stage = match phase {
                    UpdatePhase::Starting | UpdatePhase::Checking => Stage::Check,
                    UpdatePhase::Downloading => Stage::Download,
                    UpdatePhase::Verifying => Stage::Verify,
                    _ => Stage::Install,
                };
                if !entry.visited.contains(&entry.stage) {
                    entry.visited.push(entry.stage);
                }
                entry.state = StepState::Active;
            }
        }
    }

    pub fn finish(
        &mut self,
        results: impl IntoIterator<Item = (UpdateComponent, bool)>,
        cancelled: bool,
        failed: bool,
    ) {
        for (component, success) in results {
            if !self
                .components
                .iter()
                .any(|entry| entry.component == component)
            {
                self.components.push(ComponentProgress {
                    component,
                    state: StepState::Pending,
                    stage: Stage::Check,
                    visited: Vec::new(),
                });
            }
            let entry = self
                .components
                .iter_mut()
                .find(|entry| entry.component == component)
                .expect("component was inserted");
            entry.state = if success {
                StepState::Complete
            } else {
                StepState::Failed
            };
        }
        if failed
            && !cancelled
            && self
                .components
                .iter()
                .all(|entry| entry.state == StepState::Pending)
        {
            if let Some(entry) = self.components.first_mut() {
                entry.state = StepState::Failed;
            }
        }
        for entry in &mut self.components {
            if matches!(entry.state, StepState::Active | StepState::Pending) {
                entry.state = if cancelled {
                    StepState::Cancelled
                } else if failed && entry.state == StepState::Active {
                    StepState::Failed
                } else {
                    StepState::Skipped
                };
            }
        }
        self.finished = true;
    }
}

pub fn component_label(component: UpdateComponent, lang: Lang) -> &'static str {
    let (en, zh) = match component {
        UpdateComponent::Update => ("Program", "程序"),
        UpdateComponent::Scheme => ("Scheme", "方案"),
        UpdateComponent::Dict => ("Dictionary", "词库"),
        UpdateComponent::Model => ("Language model", "语言模型"),
        UpdateComponent::ModelPatch => ("Model integration", "模型接入"),
        UpdateComponent::Deploy => ("Deploy", "部署"),
        UpdateComponent::Fcitx5Theme => ("Theme", "主题"),
        UpdateComponent::Fcitx5Setup => ("Rime integration", "Rime 集成"),
        UpdateComponent::Sync => ("Sync", "同步"),
        UpdateComponent::Hook => ("Hooks", "钩子"),
    };
    if lang == Lang::Zh {
        zh
    } else {
        en
    }
}

pub fn component_from_id(id: &str) -> Option<UpdateComponent> {
    Some(match id {
        "update" | "application" => UpdateComponent::Update,
        "scheme" => UpdateComponent::Scheme,
        "dict" => UpdateComponent::Dict,
        "model" => UpdateComponent::Model,
        "model_patch" => UpdateComponent::ModelPatch,
        "deploy" => UpdateComponent::Deploy,
        "fcitx5_theme" => UpdateComponent::Fcitx5Theme,
        "fcitx5_setup" => UpdateComponent::Fcitx5Setup,
        "sync" => UpdateComponent::Sync,
        "hook" => UpdateComponent::Hook,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(component: UpdateComponent, phase: UpdatePhase) -> UpdateEvent {
        UpdateEvent {
            component,
            phase,
            progress: 0.5,
            detail: String::new(),
        }
    }

    #[test]
    fn program_update_observes_real_phases() {
        let mut progress = OperationProgress::new([UpdateComponent::Update]);
        progress.observe(&event(UpdateComponent::Update, UpdatePhase::Downloading));
        assert_eq!(
            progress.components[0].step_state(Stage::Download),
            StepState::Active
        );
        progress.observe(&event(UpdateComponent::Update, UpdatePhase::Verifying));
        progress.finish([], false, true);
        assert_eq!(
            progress.components[0].step_state(Stage::Verify),
            StepState::Failed
        );
    }

    #[test]
    fn fast_current_component_skips_download_instead_of_faking_work() {
        let mut progress = OperationProgress::new([UpdateComponent::Scheme]);
        progress.observe(&event(UpdateComponent::Scheme, UpdatePhase::Checking));
        progress.observe(&event(UpdateComponent::Scheme, UpdatePhase::Finished));
        progress.finish([(UpdateComponent::Scheme, true)], false, false);
        assert_eq!(
            progress.components[0].step_state(Stage::Check),
            StepState::Complete
        );
        assert_eq!(
            progress.components[0].step_state(Stage::Download),
            StepState::Skipped
        );
    }

    #[test]
    fn mixed_failure_and_cancellation_do_not_turn_waiting_steps_green() {
        let mut progress = OperationProgress::new([
            UpdateComponent::Scheme,
            UpdateComponent::Dict,
            UpdateComponent::Model,
        ]);
        progress.observe(&event(UpdateComponent::Scheme, UpdatePhase::Checking));
        progress.observe(&event(UpdateComponent::Dict, UpdatePhase::Downloading));
        progress.finish([(UpdateComponent::Scheme, false)], true, false);
        assert_eq!(progress.components[0].state, StepState::Failed);
        assert_eq!(
            progress.components[1].step_state(Stage::Download),
            StepState::Cancelled
        );
        assert_eq!(progress.components[2].state, StepState::Cancelled);
        progress.observe(&event(UpdateComponent::Model, UpdatePhase::Finished));
        assert_eq!(progress.components[2].state, StepState::Cancelled);
    }
}
