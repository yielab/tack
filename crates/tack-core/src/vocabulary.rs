use std::collections::HashMap;

use crate::error::CoreError;
use crate::models::ProjectType;

pub type VocabularyMap = HashMap<String, String>;

/// All valid vocabulary keys that can be customized per project.
pub const VOCABULARY_KEYS: &[&str] = &[
    "sprint",
    "backlog",
    "board",
    "blocker",
    "story_points",
    "assignee",
    "deliverable",
    "phase",
    "milestone",
    "release",
];

/// Returns the default agile vocabulary: the words that are not item types (the types are the
/// project's catalog, `item_types`).
pub fn default_vocabulary() -> VocabularyMap {
    HashMap::from([
        ("sprint".into(), "Sprint".into()),
        ("backlog".into(), "Backlog".into()),
        ("board".into(), "Board".into()),
        ("blocker".into(), "Blocker".into()),
        ("story_points".into(), "Story Points".into()),
        ("assignee".into(), "Assignee".into()),
        ("deliverable".into(), "Deliverable".into()),
        ("phase".into(), "Phase".into()),
        ("milestone".into(), "Milestone".into()),
        ("release".into(), "Release".into()),
    ])
}

/// Returns a vocabulary preset for a specific project type.
pub fn vocabulary_for_type(project_type: &ProjectType) -> VocabularyMap {
    match project_type {
        ProjectType::Construction => HashMap::from([
            ("sprint".into(), "Phase".into()),
            ("backlog".into(), "Pending Work".into()),
            ("board".into(), "Project Board".into()),
            ("blocker".into(), "Hold".into()),
            ("story_points".into(), "Effort Hours".into()),
            ("assignee".into(), "Responsible".into()),
            ("deliverable".into(), "Deliverable".into()),
            ("phase".into(), "Phase".into()),
            ("milestone".into(), "Inspection Point".into()),
            ("release".into(), "Handover".into()),
        ]),
        ProjectType::Homework => HashMap::from([
            ("sprint".into(), "Week".into()),
            ("backlog".into(), "Upcoming".into()),
            ("board".into(), "Planner".into()),
            ("blocker".into(), "Dependency".into()),
            ("story_points".into(), "Effort".into()),
            ("assignee".into(), "Student".into()),
            ("deliverable".into(), "Submission".into()),
            ("phase".into(), "Term".into()),
            ("milestone".into(), "Exam".into()),
            ("release".into(), "Graduation".into()),
        ]),
        ProjectType::Personal => HashMap::from([
            ("sprint".into(), "Cycle".into()),
            ("backlog".into(), "Ideas".into()),
            ("board".into(), "Board".into()),
            ("blocker".into(), "Blocker".into()),
            ("story_points".into(), "Effort".into()),
            ("assignee".into(), "Owner".into()),
            ("deliverable".into(), "Result".into()),
            ("phase".into(), "Phase".into()),
            ("milestone".into(), "Milestone".into()),
            ("release".into(), "Completion".into()),
        ]),
        ProjectType::Maintenance => HashMap::from([
            ("sprint".into(), "Schedule".into()),
            ("backlog".into(), "Queue".into()),
            ("board".into(), "Dashboard".into()),
            ("blocker".into(), "Hold".into()),
            ("story_points".into(), "Hours".into()),
            ("assignee".into(), "Technician".into()),
            ("deliverable".into(), "Report".into()),
            ("phase".into(), "Cycle".into()),
            ("milestone".into(), "Inspection".into()),
            ("release".into(), "Sign-off".into()),
        ]),
        ProjectType::Legal => HashMap::from([
            ("sprint".into(), "Term".into()),
            ("backlog".into(), "Intake".into()),
            ("board".into(), "Docket".into()),
            ("blocker".into(), "Hold".into()),
            ("story_points".into(), "Billable Hours".into()),
            ("assignee".into(), "Counsel".into()),
            ("deliverable".into(), "Brief".into()),
            ("phase".into(), "Stage".into()),
            ("milestone".into(), "Hearing".into()),
            ("release".into(), "Settlement".into()),
        ]),
        ProjectType::Research => HashMap::from([
            ("sprint".into(), "Cycle".into()),
            ("backlog".into(), "Question Bank".into()),
            ("board".into(), "Lab Board".into()),
            ("blocker".into(), "Blocker".into()),
            ("story_points".into(), "Effort".into()),
            ("assignee".into(), "Researcher".into()),
            ("deliverable".into(), "Finding".into()),
            ("phase".into(), "Phase".into()),
            ("milestone".into(), "Milestone".into()),
            ("release".into(), "Publication".into()),
        ]),
        ProjectType::Event => HashMap::from([
            ("sprint".into(), "Milestone Week".into()),
            ("backlog".into(), "Ideas".into()),
            ("board".into(), "Run Sheet".into()),
            ("blocker".into(), "Blocker".into()),
            ("story_points".into(), "Effort".into()),
            ("assignee".into(), "Owner".into()),
            ("deliverable".into(), "Deliverable".into()),
            ("phase".into(), "Phase".into()),
            ("milestone".into(), "Milestone".into()),
            ("release".into(), "Go-Live".into()),
        ]),
        _ => default_vocabulary(),
    }
}

/// Resolve a vocabulary key to its display label for a given project vocabulary.
/// Falls back to the default vocabulary if the key is not overridden.
pub fn resolve(vocab: &VocabularyMap, key: &str) -> String {
    vocab.get(key).cloned().unwrap_or_else(|| {
        default_vocabulary()
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_string())
    })
}

/// Validate that all keys in a vocabulary map are recognized.
pub fn validate(vocab: &VocabularyMap) -> Result<(), CoreError> {
    for key in vocab.keys() {
        if !VOCABULARY_KEYS.contains(&key.as_str()) {
            return Err(CoreError::InvalidVocabularyKey(key.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_vocabulary_has_all_keys() {
        let vocab = default_vocabulary();
        for key in VOCABULARY_KEYS {
            assert!(vocab.contains_key(*key), "Missing default key: {key}");
        }
    }

    #[test]
    fn test_construction_vocabulary_renames() {
        let vocab = vocabulary_for_type(&ProjectType::Construction);
        assert_eq!(vocab.get("milestone").unwrap(), "Inspection Point");
        assert_eq!(vocab.get("sprint").unwrap(), "Phase");
    }

    #[test]
    fn test_new_domain_vocabularies_rename_and_are_complete() {
        for ptype in [
            ProjectType::Legal,
            ProjectType::Research,
            ProjectType::Event,
        ] {
            let vocab = vocabulary_for_type(&ptype);
            // Every domain vocabulary must define all known keys and validate clean.
            for key in VOCABULARY_KEYS {
                assert!(vocab.contains_key(*key), "{ptype} missing key: {key}");
            }
            assert!(
                validate(&vocab).is_ok(),
                "{ptype} vocabulary failed validation"
            );
        }
        assert_eq!(
            vocabulary_for_type(&ProjectType::Legal)
                .get("deliverable")
                .unwrap(),
            "Brief"
        );
        assert_eq!(
            vocabulary_for_type(&ProjectType::Research)
                .get("release")
                .unwrap(),
            "Publication"
        );
        assert_eq!(
            vocabulary_for_type(&ProjectType::Event)
                .get("board")
                .unwrap(),
            "Run Sheet"
        );
    }

    #[test]
    fn test_resolve_falls_back_to_default() {
        let custom: VocabularyMap = HashMap::from([("sprint".into(), "Iteration".into())]);
        assert_eq!(resolve(&custom, "sprint"), "Iteration");
        assert_eq!(resolve(&custom, "backlog"), "Backlog"); // fallback
    }

    #[test]
    fn test_validate_rejects_unknown_keys() {
        let bad: VocabularyMap = HashMap::from([("nonsense_key".into(), "Whatever".into())]);
        assert!(validate(&bad).is_err());
    }
}
