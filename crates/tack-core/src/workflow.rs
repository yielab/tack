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
            let allowed = transitions.iter().any(|t| t.from == from && t.to == to);
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
        if let Some(status_def) = self.statuses.iter().find(|s| s.name == status)
            && let Some(limit) = status_def.wip_limit
            && current_count >= limit
        {
            warn!(status, limit, current_count, "WIP limit exceeded");
            return Err(CoreError::WipLimitExceeded {
                column: status.to_string(),
                limit,
                current: current_count,
            });
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

    /// Return the name of the first Done-category status (by order), or None.
    pub fn find_first_done_status(&self) -> Option<&str> {
        self.statuses
            .iter()
            .filter(|s| s.category == StatusCategory::Done)
            .min_by_key(|s| s.order)
            .map(|s| s.name.as_str())
    }

    /// Return true if `status` maps to the Done category in this workflow.
    pub fn is_done_status(&self, status: &str) -> bool {
        self.statuses
            .iter()
            .any(|s| s.name == status && s.category == StatusCategory::Done)
    }

    /// Pure decision: should the parent item be marked complete?
    /// The caller is responsible for querying whether all siblings are done.
    pub fn should_complete_parent(all_siblings_done: bool) -> bool {
        all_siblings_done
    }

    /// Validate workflow shape: must have at least one status in each category.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.statuses.is_empty() {
            return Err(CoreError::InvalidWorkflow(
                "workflow must have at least one status".to_string(),
            ));
        }
        let has_todo = self
            .statuses
            .iter()
            .any(|s| s.category == StatusCategory::Todo);
        let has_in_progress = self
            .statuses
            .iter()
            .any(|s| s.category == StatusCategory::InProgress);
        let has_done = self
            .statuses
            .iter()
            .any(|s| s.category == StatusCategory::Done);

        if !has_todo {
            return Err(CoreError::InvalidWorkflow(
                "workflow must include at least one 'todo' status".to_string(),
            ));
        }
        if !has_in_progress {
            return Err(CoreError::InvalidWorkflow(
                "workflow must include at least one 'in_progress' status".to_string(),
            ));
        }
        if !has_done {
            return Err(CoreError::InvalidWorkflow(
                "workflow must include at least one 'done' status".to_string(),
            ));
        }

        // Duplicate status names are not allowed
        let mut seen = std::collections::HashSet::new();
        for s in &self.statuses {
            if !seen.insert(s.name.as_str()) {
                return Err(CoreError::InvalidWorkflow(format!(
                    "duplicate status name '{}'",
                    s.name
                )));
            }
        }

        Ok(())
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
            StatusDef {
                name: "Backlog".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 0,
            },
            StatusDef {
                name: "To Do".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 1,
            },
            StatusDef {
                name: "In Progress".into(),
                category: StatusCategory::InProgress,
                wip_limit: Some(5),
                order: 2,
            },
            StatusDef {
                name: "In Review".into(),
                category: StatusCategory::InProgress,
                wip_limit: Some(3),
                order: 3,
            },
            StatusDef {
                name: "Done".into(),
                category: StatusCategory::Done,
                wip_limit: None,
                order: 4,
            },
        ],
        transitions: None, // Allow all transitions by default
    }
}

pub fn kanban_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Kanban,
        statuses: vec![
            StatusDef {
                name: "Queue".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 0,
            },
            StatusDef {
                name: "In Progress".into(),
                category: StatusCategory::InProgress,
                wip_limit: Some(3),
                order: 1,
            },
            StatusDef {
                name: "Review".into(),
                category: StatusCategory::InProgress,
                wip_limit: Some(2),
                order: 2,
            },
            StatusDef {
                name: "Done".into(),
                category: StatusCategory::Done,
                wip_limit: None,
                order: 3,
            },
        ],
        transitions: None,
    }
}

pub fn simple_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Simple,
        statuses: vec![
            StatusDef {
                name: "To Do".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 0,
            },
            StatusDef {
                name: "Doing".into(),
                category: StatusCategory::InProgress,
                wip_limit: None,
                order: 1,
            },
            StatusDef {
                name: "Done".into(),
                category: StatusCategory::Done,
                wip_limit: None,
                order: 2,
            },
        ],
        transitions: None,
    }
}

pub fn construction_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Construction,
        statuses: vec![
            StatusDef {
                name: "Permit".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 0,
            },
            StatusDef {
                name: "Procurement".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 1,
            },
            StatusDef {
                name: "Build".into(),
                category: StatusCategory::InProgress,
                wip_limit: None,
                order: 2,
            },
            StatusDef {
                name: "Inspect".into(),
                category: StatusCategory::InProgress,
                wip_limit: None,
                order: 3,
            },
            StatusDef {
                name: "Handover".into(),
                category: StatusCategory::Done,
                wip_limit: None,
                order: 4,
            },
        ],
        transitions: Some(vec![
            Transition {
                from: "Permit".into(),
                to: "Procurement".into(),
            },
            Transition {
                from: "Procurement".into(),
                to: "Build".into(),
            },
            Transition {
                from: "Build".into(),
                to: "Inspect".into(),
            },
            Transition {
                from: "Inspect".into(),
                to: "Handover".into(),
            },
            Transition {
                from: "Inspect".into(),
                to: "Build".into(),
            }, // rework
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── initial_status ───────────────────────────────────────

    #[test]
    fn initial_status_scrum_is_backlog() {
        assert_eq!(scrum_workflow().initial_status().unwrap(), "Backlog");
    }

    #[test]
    fn initial_status_simple_is_todo() {
        assert_eq!(simple_workflow().initial_status().unwrap(), "To Do");
    }

    #[test]
    fn initial_status_construction_is_permit() {
        assert_eq!(construction_workflow().initial_status().unwrap(), "Permit");
    }

    #[test]
    fn initial_status_kanban_is_queue() {
        assert_eq!(kanban_workflow().initial_status().unwrap(), "Queue");
    }

    #[test]
    fn initial_status_empty_workflow_returns_err() {
        let wf = WorkflowConfig {
            workflow_type: WorkflowType::Custom,
            statuses: vec![],
            transitions: None,
        };
        assert!(wf.initial_status().is_err());
    }

    // ── status_names ─────────────────────────────────────────

    #[test]
    fn status_names_scrum_ordered() {
        let names = scrum_workflow().status_names();
        assert_eq!(
            names,
            vec!["Backlog", "To Do", "In Progress", "In Review", "Done"]
        );
    }

    // ── validate_transition (scrum — open, any → any if both exist) ──

    #[test]
    fn scrum_allows_any_valid_status_pair() {
        let wf = scrum_workflow();
        assert!(wf.validate_transition("Backlog", "In Progress").is_ok());
        assert!(wf.validate_transition("In Progress", "Done").is_ok());
        assert!(wf.validate_transition("Done", "Backlog").is_ok()); // reopen
        assert!(wf.validate_transition("In Review", "Backlog").is_ok()); // send back
    }

    #[test]
    fn scrum_rejects_unknown_from_status() {
        let wf = scrum_workflow();
        assert!(wf.validate_transition("Nonexistent", "Done").is_err());
    }

    #[test]
    fn scrum_rejects_unknown_to_status() {
        let wf = scrum_workflow();
        assert!(wf.validate_transition("Backlog", "Shipped").is_err());
    }

    // ── validate_transition (construction — strict linear) ──────────

    #[test]
    fn construction_allows_each_forward_step() {
        let wf = construction_workflow();
        assert!(wf.validate_transition("Permit", "Procurement").is_ok());
        assert!(wf.validate_transition("Procurement", "Build").is_ok());
        assert!(wf.validate_transition("Build", "Inspect").is_ok());
        assert!(wf.validate_transition("Inspect", "Handover").is_ok());
    }

    #[test]
    fn construction_allows_rework_inspect_to_build() {
        let wf = construction_workflow();
        assert!(wf.validate_transition("Inspect", "Build").is_ok());
    }

    #[test]
    fn construction_rejects_skipping_stages() {
        let wf = construction_workflow();
        assert!(wf.validate_transition("Permit", "Handover").is_err());
        assert!(wf.validate_transition("Permit", "Build").is_err());
        assert!(wf.validate_transition("Procurement", "Handover").is_err());
        assert!(wf.validate_transition("Build", "Permit").is_err()); // can't go back to start
    }

    // ── check_wip_limit ──────────────────────────────────────

    #[test]
    fn wip_under_limit_ok() {
        let wf = scrum_workflow(); // In Progress limit = 5
        assert!(wf.check_wip_limit("In Progress", 4).is_ok());
    }

    #[test]
    fn wip_exactly_at_limit_fails() {
        let wf = scrum_workflow(); // In Progress limit = 5
        assert!(wf.check_wip_limit("In Progress", 5).is_err());
    }

    #[test]
    fn wip_over_limit_fails() {
        let wf = scrum_workflow();
        assert!(wf.check_wip_limit("In Progress", 99).is_err());
    }

    #[test]
    fn wip_no_limit_set_always_ok() {
        let wf = scrum_workflow(); // Backlog has no WIP limit
        assert!(wf.check_wip_limit("Backlog", 10_000).is_ok());
    }

    #[test]
    fn wip_unknown_status_ok() {
        // Unknown status has no limit entry — should not error
        let wf = scrum_workflow();
        assert!(wf.check_wip_limit("Nonexistent", 99).is_ok());
    }

    #[test]
    fn kanban_in_progress_limit_three() {
        let wf = kanban_workflow(); // In Progress limit = 3
        assert!(wf.check_wip_limit("In Progress", 2).is_ok());
        assert!(wf.check_wip_limit("In Progress", 3).is_err());
    }

    // ── find_first_done_status ────────────────────────────────

    #[test]
    fn done_status_scrum_is_done() {
        assert_eq!(scrum_workflow().find_first_done_status(), Some("Done"));
    }

    #[test]
    fn done_status_simple_is_done() {
        assert_eq!(simple_workflow().find_first_done_status(), Some("Done"));
    }

    #[test]
    fn done_status_kanban_is_done() {
        assert_eq!(kanban_workflow().find_first_done_status(), Some("Done"));
    }

    #[test]
    fn done_status_construction_is_handover() {
        assert_eq!(
            construction_workflow().find_first_done_status(),
            Some("Handover")
        );
    }

    #[test]
    fn done_status_none_when_no_done_category() {
        let wf = WorkflowConfig {
            workflow_type: WorkflowType::Custom,
            statuses: vec![StatusDef {
                name: "Open".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 0,
            }],
            transitions: None,
        };
        assert_eq!(wf.find_first_done_status(), None);
    }

    // ── is_done_status ────────────────────────────────────────

    #[test]
    fn is_done_status_true_for_done() {
        assert!(scrum_workflow().is_done_status("Done"));
    }

    #[test]
    fn is_done_status_false_for_in_progress() {
        assert!(!scrum_workflow().is_done_status("In Progress"));
    }

    #[test]
    fn is_done_status_false_for_unknown() {
        assert!(!scrum_workflow().is_done_status("Nonexistent"));
    }

    #[test]
    fn construction_handover_is_done() {
        assert!(construction_workflow().is_done_status("Handover"));
        assert!(!construction_workflow().is_done_status("Build"));
    }

    // ── should_complete_parent ───────────────────────────────

    #[test]
    fn should_complete_parent_true_when_all_siblings_done() {
        assert!(WorkflowConfig::should_complete_parent(true));
    }

    #[test]
    fn should_complete_parent_false_when_siblings_incomplete() {
        assert!(!WorkflowConfig::should_complete_parent(false));
    }
}
