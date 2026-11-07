//! Who is calling (ADR 0034, decision 2), as pure values and one pure check.
//!
//! Every request carries a [`Principal`]: its kind, an id, and the [`Scope`]s it holds. A scope
//! is a list of projects (or every project) times what may be done there. Nothing here touches
//! the store, a clock or HTTP; the API resolves a principal from a credential and asks
//! [`may`] before it serves a route.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What kind of caller a request is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum PrincipalKind {
    /// The person who holds Tack's token (ADR 0004): every scope.
    Operator,
    /// An agent holding a credential: only its scopes.
    Agent,
    /// A runner holding a runner credential (its own protocol, never a scope here).
    Runner,
    /// Tack itself: a loop, a rule, a background use case.
    System,
}

impl PrincipalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PrincipalKind::Operator => "operator",
            PrincipalKind::Agent => "agent",
            PrincipalKind::Runner => "runner",
            PrincipalKind::System => "system",
        }
    }
}

/// What a principal may do inside a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Capability {
    /// Read the project's items, comments, dependencies and board.
    Read,
    /// Propose changes that wait for a person.
    Propose,
    /// Change the project directly: items, comments, dependencies.
    Act,
    /// Talk in a thread about the project.
    Converse,
}

impl Capability {
    /// Every capability, in the order a screen lists them.
    pub const ALL: [Capability; 4] = [
        Capability::Read,
        Capability::Propose,
        Capability::Act,
        Capability::Converse,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Read => "read",
            Capability::Propose => "propose",
            Capability::Act => "act",
            Capability::Converse => "converse",
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Capability {
    type Err = ();
    fn from_str(text: &str) -> Result<Self, ()> {
        Capability::ALL
            .into_iter()
            .find(|capability| capability.as_str() == text)
            .ok_or(())
    }
}

/// A set of projects times a set of capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Scope {
    /// True: every project, including the ones made after the scope was granted. `projects`
    /// is then ignored.
    #[serde(default)]
    pub all_projects: bool,
    /// The projects the scope names when `all_projects` is false.
    #[serde(default)]
    pub projects: Vec<Uuid>,
    pub capabilities: Vec<Capability>,
}

impl Scope {
    /// Every project, every capability: the operator's.
    pub fn everything() -> Scope {
        Scope {
            all_projects: true,
            projects: Vec::new(),
            capabilities: Capability::ALL.to_vec(),
        }
    }

    fn covers_project(&self, project: Uuid) -> bool {
        self.all_projects || self.projects.contains(&project)
    }

    fn grants(&self, project: Uuid, capability: Capability) -> bool {
        self.covers_project(project) && self.capabilities.contains(&capability)
    }

    /// Why a scope cannot be granted, or `None` when it can.
    pub fn problem(&self) -> Option<&'static str> {
        if self.capabilities.is_empty() {
            return Some("choose at least one of read, propose, act and converse");
        }
        if !self.all_projects && self.projects.is_empty() {
            return Some("choose at least one project, or every project");
        }
        None
    }
}

/// The caller of a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Principal {
    pub kind: PrincipalKind,
    /// Who, within the kind: the operator's token label, an agent credential's id.
    pub id: String,
    pub scopes: Vec<Scope>,
}

impl Principal {
    /// The person holding Tack's token (`token:<hash prefix>`), or the local one (`local`).
    pub fn operator(id: impl Into<String>) -> Principal {
        Principal {
            kind: PrincipalKind::Operator,
            id: id.into(),
            scopes: vec![Scope::everything()],
        }
    }

    /// An agent credential with the scopes it was issued.
    pub fn agent(credential_id: impl Into<String>, scopes: Vec<Scope>) -> Principal {
        Principal {
            kind: PrincipalKind::Agent,
            id: credential_id.into(),
            scopes,
        }
    }

    /// A runner holding a runner credential: it acts through its own protocol, so no scope.
    pub fn runner(runner_id: impl Into<String>) -> Principal {
        Principal {
            kind: PrincipalKind::Runner,
            id: runner_id.into(),
            scopes: Vec::new(),
        }
    }

    /// Tack itself, or a route that needs no caller (health, the schema): no scope.
    pub fn system() -> Principal {
        Principal {
            kind: PrincipalKind::System,
            id: "tack".into(),
            scopes: Vec::new(),
        }
    }

    /// The principal a recorded [`label`](Principal::label) names, for a fact that keeps only
    /// who caused it (a change set's proposer): the kind and id, and no scopes, since nothing
    /// is asked of it any more. `None` when the text is not `<kind>:<id>` of a known kind.
    pub fn from_label(label: &str) -> Option<Principal> {
        let (kind, id) = label.split_once(':')?;
        let kind = match kind {
            "operator" => PrincipalKind::Operator,
            "agent" => PrincipalKind::Agent,
            "runner" => PrincipalKind::Runner,
            "system" => PrincipalKind::System,
            _ => return None,
        };
        Some(Principal {
            kind,
            id: id.to_owned(),
            scopes: Vec::new(),
        })
    }

    /// The operator (a person with the token) decides without scopes. Tack itself is a `System`
    /// principal, not the operator, and holds no scopes of its own.
    pub fn is_operator(&self) -> bool {
        self.kind == PrincipalKind::Operator
    }

    /// The form recorded on a write and sent as `x-tack-principal`: `<kind>:<id>`.
    pub fn label(&self) -> String {
        format!("{}:{}", self.kind.as_str(), self.id)
    }

    /// True when the principal holds `capability` on at least one project. A route that is not
    /// about one project (the list of teams) asks this.
    pub fn may_anywhere(&self, capability: Capability) -> bool {
        self.scopes
            .iter()
            .any(|scope| scope.capabilities.contains(&capability) && scope.has_any_project())
    }

    /// True when the principal holds `capability` on every project, now and later.
    pub fn may_everywhere(&self, capability: Capability) -> bool {
        self.scopes
            .iter()
            .any(|scope| scope.all_projects && scope.capabilities.contains(&capability))
    }
}

impl Scope {
    fn has_any_project(&self) -> bool {
        self.all_projects || !self.projects.is_empty()
    }
}

/// Whether `principal` holds `capability` on `project`. The operator holds every scope; an
/// agent only the ones it was issued; Tack itself and a runner none.
pub fn may(principal: &Principal, project: Uuid, capability: Capability) -> bool {
    principal
        .scopes
        .iter()
        .any(|scope| scope.grants(project, capability))
}

/// The words of the 403 for a call `principal` may not make: the scope it lacks, by name.
pub fn missing_scope(project: Uuid, capability: Capability) -> String {
    format!(
        "This credential lacks the scope `{capability}` on project {project}; \
         add it in Settings, Credentials"
    )
}

/// How long an agent credential lives before it must be rotated.
pub const AGENT_CREDENTIAL_LIFETIME_DAYS: i64 = 90;

/// The most scopes one credential may hold.
pub const MAX_SCOPES_PER_CREDENTIAL: usize = 20;

/// What a credential is in now: only an active one authenticates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum CredentialState {
    Active,
    Expired,
    Revoked,
}

/// An agent credential as Tack shows it: never the secret, never its hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AgentCredential {
    pub id: Uuid,
    /// What a comment or a change of this credential's is signed with.
    pub name: String,
    pub scopes: Vec<Scope>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl AgentCredential {
    pub fn state(&self, now: chrono::DateTime<chrono::Utc>) -> CredentialState {
        if self.revoked_at.is_some() {
            CredentialState::Revoked
        } else if self.expires_at <= now {
            CredentialState::Expired
        } else {
            CredentialState::Active
        }
    }

    /// The principal a request holding this credential is.
    pub fn principal(&self) -> Principal {
        Principal::agent(self.id.to_string(), self.scopes.clone())
    }
}

/// Why a credential's name or scopes cannot be accepted, or `None` when they can. The name is the
/// words an agent's writes are signed with, so the operator's own word is not available.
pub fn credential_problem(name: &str, scopes: &[Scope]) -> Option<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Some("the name must be 1 to 100 characters".into());
    }
    if name.eq_ignore_ascii_case("operator") || name.eq_ignore_ascii_case("system") {
        return Some(format!("`{name}` is a name only Tack uses; choose another"));
    }
    if scopes.is_empty() {
        return Some("give the credential at least one scope".into());
    }
    if scopes.len() > MAX_SCOPES_PER_CREDENTIAL {
        return Some(format!(
            "a credential holds at most {MAX_SCOPES_PER_CREDENTIAL} scopes"
        ));
    }
    scopes
        .iter()
        .find_map(|scope| scope.problem())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent_on(project: Uuid, capabilities: &[Capability]) -> Principal {
        Principal::agent(
            "cred-1",
            vec![Scope {
                all_projects: false,
                projects: vec![project],
                capabilities: capabilities.to_vec(),
            }],
        )
    }

    #[test]
    fn the_operator_may_do_everything_everywhere() {
        let operator = Principal::operator("local");
        for capability in Capability::ALL {
            assert!(may(&operator, Uuid::new_v4(), capability));
            assert!(operator.may_everywhere(capability));
        }
        assert_eq!(operator.label(), "operator:local");
    }

    #[test]
    fn a_label_names_the_principal_it_was_made_from() {
        for principal in [
            Principal::operator("token:ab12"),
            Principal::agent("cred-1", Vec::new()),
            Principal::runner("runner-1"),
            Principal::system(),
        ] {
            let named = Principal::from_label(&principal.label()).expect("a label of a kind");
            assert_eq!((named.kind, named.id), (principal.kind, principal.id));
        }
        assert_eq!(Principal::from_label("thread:7"), None);
        assert_eq!(Principal::from_label("operator"), None);
    }

    #[test]
    fn an_agent_may_only_what_its_scope_names_on_the_projects_it_names() {
        let (mine, other) = (Uuid::new_v4(), Uuid::new_v4());
        let agent = agent_on(mine, &[Capability::Read, Capability::Propose]);
        assert!(may(&agent, mine, Capability::Read));
        assert!(may(&agent, mine, Capability::Propose));
        assert!(!may(&agent, mine, Capability::Act));
        assert!(!may(&agent, other, Capability::Read));
        assert!(agent.may_anywhere(Capability::Read));
        assert!(!agent.may_anywhere(Capability::Act));
        assert!(!agent.may_everywhere(Capability::Read));
        assert_eq!(agent.label(), "agent:cred-1");
    }

    #[test]
    fn every_project_covers_a_project_made_later() {
        let agent = Principal::agent(
            "cred-2",
            vec![Scope {
                all_projects: true,
                projects: Vec::new(),
                capabilities: vec![Capability::Read],
            }],
        );
        assert!(may(&agent, Uuid::new_v4(), Capability::Read));
        assert!(!may(&agent, Uuid::new_v4(), Capability::Act));
        assert!(agent.may_everywhere(Capability::Read));
    }

    #[test]
    fn scopes_add_up_and_never_cross() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let agent = Principal::agent(
            "cred-3",
            vec![
                Scope {
                    all_projects: false,
                    projects: vec![a],
                    capabilities: vec![Capability::Read],
                },
                Scope {
                    all_projects: false,
                    projects: vec![b],
                    capabilities: vec![Capability::Act],
                },
            ],
        );
        assert!(may(&agent, a, Capability::Read));
        assert!(!may(&agent, a, Capability::Act));
        assert!(may(&agent, b, Capability::Act));
        assert!(!may(&agent, b, Capability::Read));
    }

    #[test]
    fn tack_itself_and_a_scopeless_caller_may_do_nothing_through_scopes() {
        let system = Principal::system();
        assert!(!may(&system, Uuid::new_v4(), Capability::Read));
        assert!(!system.may_anywhere(Capability::Read));
    }

    #[test]
    fn a_scope_without_capabilities_or_projects_is_refused() {
        let nothing = Scope {
            all_projects: false,
            projects: vec![Uuid::new_v4()],
            capabilities: vec![],
        };
        assert!(nothing.problem().is_some());
        let nowhere = Scope {
            all_projects: false,
            projects: vec![],
            capabilities: vec![Capability::Read],
        };
        assert!(nowhere.problem().is_some());
        assert!(Scope::everything().problem().is_none());
    }

    #[test]
    fn the_refusal_names_the_missing_scope() {
        let project = Uuid::new_v4();
        let text = missing_scope(project, Capability::Act);
        assert!(text.contains("`act`") && text.contains(&project.to_string()));
    }

    #[test]
    fn a_credential_is_active_until_it_expires_or_is_revoked() {
        use chrono::{Duration, Utc};
        let now = Utc::now();
        let mut credential = AgentCredential {
            id: Uuid::new_v4(),
            name: "Reviewer".into(),
            scopes: vec![Scope::everything()],
            created_at: now,
            expires_at: now + Duration::days(AGENT_CREDENTIAL_LIFETIME_DAYS),
            last_used_at: None,
            revoked_at: None,
        };
        assert_eq!(credential.state(now), CredentialState::Active);
        assert_eq!(
            credential.state(now + Duration::days(91)),
            CredentialState::Expired
        );
        credential.revoked_at = Some(now);
        assert_eq!(credential.state(now), CredentialState::Revoked);
        assert_eq!(
            credential.principal().label(),
            format!("agent:{}", credential.id)
        );
    }

    #[test]
    fn names_and_scopes_are_checked() {
        let ok = vec![Scope::everything()];
        assert!(credential_problem("Reviewer", &ok).is_none());
        assert!(credential_problem("  ", &ok).is_some());
        assert!(credential_problem("Operator", &ok).is_some());
        assert!(credential_problem("Reviewer", &[]).is_some());
    }

    #[test]
    fn capabilities_parse_from_their_words() {
        for capability in Capability::ALL {
            assert_eq!(capability.as_str().parse(), Ok(capability));
        }
        assert!("manage".parse::<Capability>().is_err());
    }
}
