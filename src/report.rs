use crate::types::Schema;
use crate::updater::{UpdateComponent, UpdateResult};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema_version: u8,
    pub ok: bool,
    pub command: &'static str,
    pub schema: Option<Schema>,
    pub results: Vec<ComponentResult>,
    pub error: Option<String>,
}

impl Report {
    pub fn from_results(command: &'static str, schema: Schema, results: Vec<UpdateResult>) -> Self {
        Self {
            schema_version: 1,
            ok: results.iter().all(|result| result.success),
            command,
            schema: Some(schema),
            results: results.into_iter().map(ComponentResult::from).collect(),
            error: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ComponentResult {
    pub component: &'static str,
    pub display_name: String,
    pub old_version: String,
    pub new_version: String,
    pub success: bool,
    pub message: String,
}

impl From<UpdateResult> for ComponentResult {
    fn from(result: UpdateResult) -> Self {
        Self {
            component: component_id(result.kind),
            display_name: result.component,
            old_version: result.old_version,
            new_version: result.new_version,
            success: result.success,
            message: result.message,
        }
    }
}

fn component_id(component: UpdateComponent) -> &'static str {
    match component {
        UpdateComponent::Update => "update",
        UpdateComponent::Scheme => "scheme",
        UpdateComponent::Dict => "dict",
        UpdateComponent::Model => "model",
        UpdateComponent::ModelPatch => "model_patch",
        UpdateComponent::Deploy => "deploy",
        UpdateComponent::Fcitx5Theme => "fcitx5_theme",
        UpdateComponent::Fcitx5Setup => "fcitx5_setup",
        UpdateComponent::Sync => "sync",
        UpdateComponent::Hook => "hook",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_ids_are_stable() {
        assert_eq!(component_id(UpdateComponent::Dict), "dict");
        assert_eq!(component_id(UpdateComponent::Fcitx5Theme), "fcitx5_theme");
        assert_eq!(component_id(UpdateComponent::Fcitx5Setup), "fcitx5_setup");
    }
}
