#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UpdateComponent {
    Update,
    Scheme,
    Dict,
    Model,
    ModelPatch,
    Deploy,
    Fcitx5Theme,
    Fcitx5Setup,
    Sync,
    Hook,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePhase {
    Starting,
    Checking,
    Downloading,
    Verifying,
    Extracting,
    Saving,
    Applying,
    Deploying,
    Syncing,
    Running,
    Cancelling,
    Cancelled,
    Finished,
}

#[derive(Debug, Clone)]
pub struct UpdateEvent {
    pub component: UpdateComponent,
    pub phase: UpdatePhase,
    pub progress: f64,
    pub detail: String,
}
