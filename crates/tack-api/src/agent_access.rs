//! Which routes an agent credential may call, and the scope each one needs (ADR 0034,
//! decision 2). The table is closed: a route that is not here answers an agent `403`, so a new
//! route is the operator's until someone adds it on purpose. The operator is never asked.
//! An agent reads through the routes below and changes the board only through a change set
//! (decision 3): the routes that write an item, a comment or a dependency directly answer it `403`.
//!
//! [`classify`] is pure; [`authorize`] adds the one lookup it needs (an item's project).

use axum::http::Method;
use tack_core::principal::{Capability, Principal, may, missing_scope};
use uuid::Uuid;

use crate::error::ApiError;
use crate::router::AppState;

/// What a request needs from an agent that makes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// The path names a project.
    Project(Uuid, Capability),
    /// The path names a project and the call proposes a change set: `propose`, which `act`
    /// includes (a credential that may change the board directly may also ask a person to).
    Proposal(Uuid),
    /// The path names an item; its project decides.
    Item(Uuid, Capability),
    /// The path names a change set; its project decides.
    ChangeSet(Uuid, Capability),
    /// Not about one project (the list of teams): the capability on at least one.
    Anywhere(Capability),
    /// About every project (the global search): the capability on every one.
    Everywhere(Capability),
    /// The path names a thread; its project decides, and a thread kept for one credential is
    /// that credential's alone.
    Thread(Uuid, Capability),
    /// The handler filters its answer by the principal's scopes (the project list).
    Filtered,
    /// A direct write to the board: an agent makes the same change by proposing a change set
    /// (ADR 0034, decision 3), so the route answers `403` and names that one.
    ChangeSetOnly,
    /// No agent credential may call it.
    OperatorOnly,
}

/// The path without the `/api` prefix, split into segments.
fn segments(path: &str) -> Vec<&str> {
    path.strip_prefix("/api")
        .unwrap_or(path)
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn id(segment: &str) -> Option<Uuid> {
    Uuid::parse_str(segment).ok()
}

/// The access `method path` needs.
pub fn classify(method: &Method, path: &str) -> Access {
    use Capability::Read;
    let get = method == Method::GET;
    let segments = segments(path);
    let access = match (segments.as_slice(), get) {
        (["projects"], true) => Some(Access::Filtered),
        (["teams"], true) => Some(Access::Anywhere(Read)),
        (["digest"], true) => Some(Access::Filtered),
        // Needs `read` on one project; the handler keeps the tasks of the projects the caller may
        // read, like the digest.
        (["teams", _, "activity"], true) => Some(Access::Anywhere(Read)),
        (["search"], true) => Some(Access::Everywhere(Read)),
        (["projects", project], true) => id(project).map(|p| Access::Project(p, Read)),
        (
            [
                "projects",
                project,
                "items" | "search" | "roles" | "auto-start",
            ],
            true,
        )
        | (["projects", project, "items", "tree"], true) => {
            id(project).map(|p| Access::Project(p, Read))
        }
        (["projects", project, "items"], false) if method == Method::POST => {
            id(project).map(|_| Access::ChangeSetOnly)
        }
        (["projects", project, "change-sets"], true) => {
            id(project).map(|p| Access::Project(p, Read))
        }
        (["projects", project, "change-sets"], false) if method == Method::POST => {
            id(project).map(Access::Proposal)
        }
        (["threads", thread], true) => id(thread).map(|t| Access::Thread(t, Capability::Converse)),
        (["threads", thread, "messages"], false) if method == Method::POST => {
            id(thread).map(|t| Access::Thread(t, Capability::Converse))
        }
        (["change-sets", change_set], true) => id(change_set).map(|c| Access::ChangeSet(c, Read)),
        (["items", item], true) => id(item).map(|i| Access::Item(i, Read)),
        (["items", item], false) if method == Method::PATCH => {
            id(item).map(|_| Access::ChangeSetOnly)
        }
        (["items", item, "comments" | "dependencies" | "brief"], true) => {
            id(item).map(|i| Access::Item(i, Read))
        }
        (["items", item, "comments" | "dependencies"], false) if method == Method::POST => {
            id(item).map(|_| Access::ChangeSetOnly)
        }
        _ => None,
    };
    access.unwrap_or(Access::OperatorOnly)
}

/// The name an agent's writes are signed with: its credential's. `None` for any other caller.
pub async fn signature(
    state: &AppState,
    principal: &Principal,
) -> Result<Option<String>, ApiError> {
    if principal.kind != tack_core::principal::PrincipalKind::Agent {
        return Ok(None);
    }
    let Ok(id) = Uuid::parse_str(&principal.id) else {
        return Ok(None);
    };
    Ok(state
        .repo
        .get_agent_credential(id)
        .await?
        .map(|credential| credential.name))
}

fn refusal(text: String) -> ApiError {
    ApiError::Forbidden(text)
}

/// Lets the request through, or the `403` that names what the credential lacks. Only called for
/// an agent.
pub async fn authorize(
    state: &AppState,
    principal: &Principal,
    method: &Method,
    path: &str,
) -> Result<(), ApiError> {
    match classify(method, path) {
        Access::OperatorOnly => Err(refusal(
            "This route is not available to an agent credential; it needs the operator's token."
                .into(),
        )),
        Access::ChangeSetOnly => Err(refusal(
            "An agent changes the board only through a change set: POST \
             /api/projects/{project_id}/change-sets with the operations and a reason."
                .into(),
        )),
        Access::Filtered => Ok(()),
        Access::Anywhere(capability) if principal.may_anywhere(capability) => Ok(()),
        Access::Anywhere(capability) => Err(refusal(format!(
            "This credential lacks the scope `{capability}` on any project."
        ))),
        Access::Everywhere(capability) if principal.may_everywhere(capability) => Ok(()),
        Access::Everywhere(capability) => Err(refusal(format!(
            "This credential lacks the scope `{capability}` on every project; search one project instead."
        ))),
        Access::Project(project, capability) => {
            if may(principal, project, capability) {
                Ok(())
            } else {
                Err(refusal(missing_scope(project, capability)))
            }
        }
        Access::Proposal(project) => {
            if may(principal, project, Capability::Propose)
                || may(principal, project, Capability::Act)
            {
                Ok(())
            } else {
                Err(refusal(missing_scope(project, Capability::Propose)))
            }
        }
        Access::Thread(thread, capability) => {
            let found = state.repo.get_thread(thread).await?;
            // As for an item: the answer does not tell a missing thread from one the credential
            // cannot reach.
            let Some(found) = found else {
                return Err(refusal(format!(
                    "This credential lacks the scope `{capability}` on the project of that thread."
                )));
            };
            let home = found.scope.home();
            if let Some(credential) = found.scope.credential()
                && principal.id != credential.to_string()
            {
                return Err(refusal(
                    "That thread belongs to another credential.".to_owned(),
                ));
            }
            if may(principal, home, capability) {
                Ok(())
            } else {
                Err(refusal(missing_scope(home, capability)))
            }
        }
        Access::ChangeSet(set, capability) => {
            let project = state.repo.get_change_set(set).await?.map(|set| set.project);
            match project {
                Some(project) if may(principal, project, capability) => Ok(()),
                Some(project) => Err(refusal(missing_scope(project, capability))),
                // As for an item: the answer does not tell a missing change set from one in a
                // project the credential cannot see.
                None => Err(refusal(format!(
                    "This credential lacks the scope `{capability}` on the project of that change set."
                ))),
            }
        }
        Access::Item(item, capability) => {
            let project = state.repo.get_item(item).await?.map(|item| item.project_id);
            match project {
                Some(project) if may(principal, project, capability) => Ok(()),
                Some(project) => Err(refusal(missing_scope(project, capability))),
                // The same answer for an item that is not there and one in a project the
                // credential cannot see, so the answer does not tell which.
                None => Err(refusal(format!(
                    "This credential lacks the scope `{capability}` on the project of that item."
                ))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some() -> (Uuid, String) {
        let id = Uuid::new_v4();
        (id, id.to_string())
    }

    #[test]
    fn reads_need_read_and_direct_writes_are_for_change_sets() {
        let (project, p) = some();
        let (item, i) = some();
        assert_eq!(
            classify(&Method::GET, &format!("/api/projects/{p}/items")),
            Access::Project(project, Capability::Read)
        );
        assert_eq!(
            classify(&Method::GET, &format!("/api/projects/{p}/auto-start")),
            Access::Project(project, Capability::Read)
        );
        assert_eq!(
            classify(&Method::GET, &format!("/api/items/{i}/dependencies")),
            Access::Item(item, Capability::Read)
        );
        // Writing an item, a comment or a dependency directly is a change set's job.
        for (method, path) in [
            (Method::POST, format!("/api/projects/{p}/items")),
            (Method::PATCH, format!("/api/items/{i}")),
            (Method::POST, format!("/api/items/{i}/comments")),
            (Method::POST, format!("/api/items/{i}/dependencies")),
        ] {
            assert_eq!(
                classify(&method, &path),
                Access::ChangeSetOnly,
                "{method} {path}"
            );
        }
        // A change set has no operation that removes a dependency.
        assert_eq!(
            classify(
                &Method::DELETE,
                &format!("/api/items/{i}/dependencies/{}", Uuid::new_v4())
            ),
            Access::OperatorOnly
        );
    }

    #[test]
    fn an_agent_proposes_with_propose_and_reads_change_sets_with_read() {
        let (project, p) = some();
        let (set, c) = some();
        assert_eq!(
            classify(&Method::POST, &format!("/api/projects/{p}/change-sets")),
            Access::Proposal(project)
        );
        assert_eq!(
            classify(&Method::GET, &format!("/api/projects/{p}/change-sets")),
            Access::Project(project, Capability::Read)
        );
        assert_eq!(
            classify(&Method::GET, &format!("/api/change-sets/{c}")),
            Access::ChangeSet(set, Capability::Read)
        );
        // Deciding is the person's.
        for path in [
            format!("/api/change-sets/{c}/apply"),
            format!("/api/change-sets/{c}/reject"),
            format!("/api/change-sets/{c}/operations/0"),
        ] {
            assert_eq!(
                classify(&Method::POST, &path),
                Access::OperatorOnly,
                "{path}"
            );
            assert_eq!(classify(&Method::PATCH, &path), Access::OperatorOnly);
        }
    }

    #[test]
    fn everything_else_is_the_operators() {
        let (_, p) = some();
        let (_, i) = some();
        for (method, path) in [
            (Method::DELETE, format!("/api/projects/{p}")),
            (Method::PATCH, format!("/api/projects/{p}")),
            (Method::DELETE, format!("/api/items/{i}")),
            (Method::POST, "/api/executions".to_string()),
            (Method::GET, "/api/executions".to_string()),
            (Method::POST, format!("/api/items/{i}/subtasks-from-plan")),
            (Method::GET, "/api/agent-credentials".to_string()),
            (Method::POST, "/api/agent-credentials".to_string()),
            (Method::GET, "/api/backup".to_string()),
            (Method::POST, format!("/api/projects/{p}/import-csv")),
            (Method::GET, "/api/projects/not-a-uuid".to_string()),
            (Method::POST, "/api/decisions/x/resolve".to_string()),
        ] {
            assert_eq!(
                classify(&method, &path),
                Access::OperatorOnly,
                "{method} {path}"
            );
        }
    }

    #[test]
    fn the_prefix_is_optional_and_lookalikes_are_not_matched() {
        let (project, p) = some();
        assert_eq!(
            classify(&Method::GET, &format!("/projects/{p}")),
            Access::Project(project, Capability::Read)
        );
        assert_eq!(
            classify(&Method::GET, &format!("/api/projects/{p}/items/evil")),
            Access::OperatorOnly
        );
    }

    #[test]
    fn project_list_and_teams_are_filtered_or_anywhere() {
        assert_eq!(classify(&Method::GET, "/api/projects"), Access::Filtered);
        assert_eq!(
            classify(&Method::GET, "/api/teams"),
            Access::Anywhere(Capability::Read)
        );
        assert_eq!(
            classify(&Method::GET, "/api/search"),
            Access::Everywhere(Capability::Read)
        );
        // The digest filters its own answer by the caller's scopes.
        assert_eq!(classify(&Method::GET, "/api/digest"), Access::Filtered);
        // A team's activity crosses projects: it needs `read` on one, and the handler keeps the
        // tasks of the projects the caller may read.
        assert_eq!(
            classify(&Method::GET, "/api/teams/writers/activity"),
            Access::Anywhere(Capability::Read)
        );
        assert_eq!(
            classify(&Method::POST, "/api/teams/writers/activity"),
            Access::OperatorOnly
        );
        // Letting a team spend its budget again is a person's.
        assert_eq!(
            classify(&Method::POST, "/api/teams/docket/writers/resume"),
            Access::OperatorOnly
        );
    }
}
