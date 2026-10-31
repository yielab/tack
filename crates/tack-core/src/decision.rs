//! Who settled a decision.

use serde::{Deserialize, Serialize};

/// The `resolved_by` of a settled decision: `{"kind": "operator", "subject_id": <principal>}`
/// for an operator's answer, `{"kind": "system", "subject_id": "expiry"}` for a decision that
/// passed its `expires_at`. Not a closed enum on purpose: nothing in `docs/contracts/runner-v1/`
/// fixes this shape, so it stays an open two-field object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DecisionResolvedBy {
    #[cfg_attr(feature = "openapi", schema(example = "operator"))]
    pub kind: String,
    pub subject_id: String,
}

impl DecisionResolvedBy {
    /// An operator's answer, by the principal that gave it.
    pub fn operator(subject_id: impl Into<String>) -> Self {
        Self {
            kind: "operator".to_string(),
            subject_id: subject_id.into(),
        }
    }

    /// The fail-closed expiry of a decision nobody answered in time.
    pub fn expiry() -> Self {
        Self {
            kind: "system".to_string(),
            subject_id: "expiry".to_string(),
        }
    }
}
