use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::error::CoreError;
use crate::models::ProjectType;

/// Full workflow configuration for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    pub workflow_type: WorkflowType,
    pub statuses: Vec<StatusDef>,
    pub transitions: Option<Vec<Transition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowType {
    Scrum,
    Kanban,
    Mixed,
    Simple,
    Construction,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusDef {
    pub name: String,
    pub category: StatusCategory,
    pub wip_limit: Option<usize>,
    pub order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StatusCategory {
    Todo,
    InProgress,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    pub from: String,
    pub to: String,
}

impl WorkflowConfig {
    /// Validate that a status transition is allowed.
    pub fn validate_transition(&self, from: &str, to: &str) -> Result<(), CoreError> {
        // Verify both statuses exist
        let from_exists = self.statuses.iter().any(|s| s.name == from);
        let to_exists = self.statuses.iter().any(|s| s.name == to);

        if !from_exists {
            return Err(CoreError::InvalidTransition {
                from: from.to_string(),
                to: to.to_string(),
            });
        }
        if !to_exists {
            return Err(CoreError::InvalidTransition {
                from: from.to_string(),
                to: to.to_string(),
            });
        }

        // If explicit transitions defined, enforce them
        if let Some(ref transitions) = self.transitions {
            let allowed = transitions
                .iter()
                .any(|t| t.from == from && t.to == to);
            if !allowed {
                debug!(from, to, "Transition not in allowed list");
                return Err(CoreError::InvalidTransition {
                    from: from.to_string(),
                    to: to.to_string(),
                });
            }
        }

        Ok(())
    }

    /// Check WIP limits for a given status column.
    pub fn check_wip_limit(&self, status: &str, current_count: usize) -> Result<(), CoreError> {
        if let Some(status_def) = self.statuses.iter().find(|s| s.name == status) {
            if let Some(limit) = status_def.wip_limit {
                if current_count >= limit {
                    warn!(
                        status,
                        limit,
                        current_count,
                        "WIP limit exceeded"
                    );
                    return Err(CoreError::WipLimitExceeded {
                        column: status.to_string(),
                        limit,
                        current: current_count,
                    });
                }
            }
        }
        Ok(())
    }

    /// Get the initial (first) status for new items.
    pub fn initial_status(&self) -> Result<String, CoreError> {
        self.statuses
            .iter()
            .min_by_key(|s| s.order)
            .map(|s| s.name.clone())
            .ok_or(CoreError::EmptyWorkflow)
    }

    /// Get all status names in order.
    pub fn status_names(&self) -> Vec<String> {
        let mut statuses = self.statuses.clone();
        statuses.sort_by_key(|s| s.order);
        statuses.into_iter().map(|s| s.name).collect()
    }
}

// ─── Preset Workflows ───────────────────────────────────────

pub fn workflow_for_type(project_type: &ProjectType) -> WorkflowConfig {
    match project_type {
        ProjectType::Software | ProjectType::Web | ProjectType::Mobile => scrum_workflow(),
        ProjectType::Construction => construction_workflow(),
        ProjectType::Personal | ProjectType::Homework => simple_workflow(),
        ProjectType::Maintenance => kanban_workflow(),
        ProjectType::Custom => simple_workflow(),
    }
}

pub fn scrum_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Scrum,
        statuses: vec![
            StatusDef { name: "Backlog".into(), category: StatusCategory::Todo, wip_limit: None, order: 0 },
            StatusDef { name: "To Do".into(), category: StatusCategory::Todo, wip_limit: None, order: 1 },
            StatusDef { name: "In Progress".into(), category: StatusCategory::InProgress, wip_limit: Some(5), order: 2 },
            StatusDef { name: "In Review".into(), category: StatusCategory::InProgress, wip_limit: Some(3), order: 3 },
            StatusDef { name: "Done".into(), category: StatusCategory::Done, wip_limit: None, order: 4 },
        ],
        transitions: None, // Allow all transitions by default
    }
}

pub fn kanban_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Kanban,
        statuses: vec![
            StatusDef { name: "Queue".into(), category: StatusCategory::Todo, wip_limit: None, order: 0 },
            StatusDef { name: "In Progress".into(), category: StatusCategory::InProgress, wip_limit: Some(3), order: 1 },
            StatusDef { name: "Review".into(), category: StatusCategory::InProgress, wip_limit: Some(2), order: 2 },
            StatusDef { name: "Done".into(), category: StatusCategory::Done, wip_limit: None, order: 3 },
        ],
        transitions: None,
    }
}

pub fn simple_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Simple,
        statuses: vec![
            StatusDef { name: "To Do".into(), category: StatusCategory::Todo, wip_limit: None, order: 0 },
            StatusDef { name: "Doing".into(), category: StatusCategory::InProgress, wip_limit: None, order: 1 },
            StatusDef { name: "Done".into(), category: StatusCategory::Done, wip_limit: None, order: 2 },
        ],
        transitions: None,
    }
}

pub fn construction_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Construction,
        statuses: vec![
            StatusDef { name: "Permit".into(), category: StatusCategory::Todo, wip_limit: None, order: 0 },
            StatusDef { name: "Procurement".into(), category: StatusCategory::Todo, wip_limit: None, order: 1 },
            StatusDef { name: "Build".into(), category: StatusCategory::InProgress, wip_limit: None, order: 2 },
            StatusDef { name: "Inspect".into(), category: StatusCategory::InProgress, wip_limit: None, order: 3 },
            StatusDef { name: "Handover".into(), category: StatusCategory::Done, wip_limit: None, order: 4 },
        ],
        transitions: Some(vec![
            Transition { from: "Permit".into(), to: "Procurement".into() },
            Transition { from: "Procurement".into(), to: "Build".into() },
            Transition { from: "Build".into(), to: "Inspect".into() },
            Transition { from: "Inspect".into(), to: "Handover".into() },
            Transition { from: "Inspect".into(), to: "Build".into() }, // rework
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_status_scrum() {
        let wf = scrum_workflow();
        assert_eq!(wf.initial_status().unwrap(), "Backlog");
    }

    #[test]
    fn test_initial_status_simple() {
        let wf = simple_workflow();
        assert_eq!(wf.initial_status().unwrap(), "To Do");
    }

    #[test]
    fn test_valid_transition_no_restrictions() {
        let wf = scrum_workflow();
        assert!(wf.validate_transition("Backlog", "In Progress").is_ok());
    }

    #[test]
    fn test_invalid_status_transition() {
        let wf = scrum_workflow();
        assert!(wf.validate_transition("Backlog", "Nonexistent").is_err());
    }

    #[test]
    fn test_construction_enforces_transitions() {
        let wf = construction_workflow();
        assert!(wf.validate_transition("Permit", "Procurement").is_ok());
        assert!(wf.validate_transition("Permit", "Handover").is_err());
        assert!(wf.validate_transition("Inspect", "Build").is_ok()); // rework
    }

    #[test]
    fn test_wip_limit_exceeded() {
        let wf = scrum_workflow();
        assert!(wf.check_wip_limit("In Progress", 4).is_ok());
        assert!(wf.check_wip_limit("In Progress", 5).is_err());
    }

    #[test]
    fn test_wip_limit_no_limit() {
        let wf = scrum_workflow();
        assert!(wf.check_wip_limit("Backlog", 1000).is_ok());
    }

    #[test]
    fn test_status_names_ordered() {
        let wf = scrum_workflow();
        let names = wf.status_names();
        assert_eq!(names, vec!["Backlog", "To Do", "In Progress", "In Review", "Done"]);
    }

    #[test]
    fn test_empty_workflow_error() {
        let wf = WorkflowConfig {
            workflow_type: WorkflowType::Custom,
            statuses: vec![],
            transitions: None,
        };
        assert!(wf.initial_status().is_err());
    }
}
