//! Every route an MCP tool calls, walked through the real router and middleware with a real
//! agent credential. The access table in `agent_access` is closed (a route that is not in it
//! answers an agent `403`), so a tool whose route nobody classified would fail only for an agent
//! credential, and the MCP tests, which talk to a fake server, would not see it.

use crate::common::{send, test_app_with_config};
use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::config::AppConfig;

const OPERATOR: &str = "Bearer operator-token";

/// The tools the MCP server advertises: the table below must name each of them.
const TOOLS_LIST: &str = include_str!("../../../tack-cli/src/mcp/tools_list.json");

/// What an agent credential holding every capability on every project finds at a tool's routes.
enum Reach {
    /// Every route the tool calls is classified for an agent (a `403` here means the table
    /// forgot it).
    Agent,
    /// The tool is for the operator's token by design (`docs/MCP.md`, "Under an agent
    /// credential"); an agent credential is told so.
    OperatorOnly,
}

use Reach::{Agent, OperatorOnly};

/// Each tool, what it reaches and the routes it calls. `{p}`, `{i}`, `{t}` and `{c}` stand for a
/// project, an item, a thread and a change set that exist.
type Tool = (&'static str, Reach, &'static [(&'static str, &'static str)]);

const TOOLS: &[Tool] = &[
    ("list_projects", Agent, &[("GET", "/api/projects")]),
    (
        "get_autonomy",
        OperatorOnly,
        &[("GET", "/api/projects/{p}/autonomy")],
    ),
    ("list_items", Agent, &[("GET", "/api/projects/{p}/items")]),
    (
        "get_item",
        Agent,
        &[
            ("GET", "/api/items/{i}"),
            ("GET", "/api/items/{i}/comments"),
        ],
    ),
    (
        "search_items",
        Agent,
        &[
            ("GET", "/api/search?query=x"),
            ("GET", "/api/projects/{p}/search?query=x"),
        ],
    ),
    // With an agent credential the four writes below are a change set.
    (
        "create_item",
        Agent,
        &[("POST", "/api/projects/{p}/change-sets")],
    ),
    ("create_project", OperatorOnly, &[("POST", "/api/projects")]),
    (
        "create_outline",
        OperatorOnly,
        &[
            ("POST", "/api/projects/{p}/outline"),
            ("POST", "/api/projects/outline"),
        ],
    ),
    (
        "update_item",
        Agent,
        &[
            ("GET", "/api/items/{i}"),
            ("POST", "/api/projects/{p}/change-sets"),
        ],
    ),
    (
        "move_item",
        Agent,
        &[
            ("GET", "/api/items/{i}"),
            ("POST", "/api/projects/{p}/change-sets"),
        ],
    ),
    (
        "add_comment",
        Agent,
        &[
            ("GET", "/api/items/{i}"),
            ("POST", "/api/projects/{p}/change-sets"),
        ],
    ),
    (
        "list_fleets",
        OperatorOnly,
        &[("GET", "/api/runner-fleets")],
    ),
    (
        "list_agent_profiles",
        OperatorOnly,
        &[("GET", "/api/agent-profiles")],
    ),
    (
        "list_executions",
        OperatorOnly,
        &[("GET", "/api/executions")],
    ),
    (
        "get_execution",
        OperatorOnly,
        &[("GET", "/api/executions/{i}")],
    ),
    (
        "cancel_execution",
        OperatorOnly,
        &[("POST", "/api/executions/{i}/cancel")],
    ),
    (
        "create_execution",
        OperatorOnly,
        &[("POST", "/api/executions")],
    ),
    ("digest", Agent, &[("GET", "/api/digest")]),
    (
        "team_activity",
        Agent,
        &[("GET", "/api/teams/writers/activity")],
    ),
    ("list_teams", Agent, &[("GET", "/api/teams")]),
    ("list_roles", Agent, &[("GET", "/api/projects/{p}/roles")]),
    ("get_change_set", Agent, &[("GET", "/api/change-sets/{c}")]),
    (
        "propose",
        Agent,
        &[("POST", "/api/projects/{p}/change-sets")],
    ),
    ("read_thread", Agent, &[("GET", "/api/threads/{t}")]),
    (
        "post_message",
        Agent,
        &[("POST", "/api/threads/{t}/messages")],
    ),
];

async fn call(
    app: &Router,
    authorization: &str,
    method: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    send(app, method, uri, body, &[("authorization", authorization)]).await
}

#[tokio::test]
async fn every_route_an_mcp_tool_calls_is_classified_for_an_agent_credential() {
    let (app, _) = test_app_with_config(AppConfig {
        api_token: Some("operator-token".into()),
        ..AppConfig::default()
    })
    .await;
    let (_, project) = call(
        &app,
        OPERATOR,
        "POST",
        "/api/projects",
        json!({"name": "My SaaS", "project_type": "software"}),
    )
    .await;
    let p = project["id"].as_str().unwrap().to_owned();
    let (_, item) = call(
        &app,
        OPERATOR,
        "POST",
        &format!("/api/projects/{p}/items"),
        json!({"title": "Existing"}),
    )
    .await;
    let i = item["id"].as_str().unwrap().to_owned();
    let (_, profile) = call(
        &app,
        OPERATOR,
        "POST",
        "/api/agent-profiles",
        json!({"name": "Manager profile", "instructions": "help"}),
    )
    .await;
    let (status, thread) = call(
        &app,
        OPERATOR,
        "POST",
        "/api/threads",
        json!({
            "project_id": p,
            "profile": profile["agent_profile_id"],
            "title": "Report",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{thread}");
    let t = thread["id"].as_str().unwrap().to_owned();

    // Every capability on every project: whatever the tool needs, this credential holds it.
    let (status, issued) = call(
        &app,
        OPERATOR,
        "POST",
        "/api/agent-credentials",
        json!({"name": "Everything", "scopes": [{
            "all_projects": true,
            "capabilities": ["read", "propose", "act", "converse"],
        }]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{issued}");
    let agent = format!("Bearer {}", issued["secret"].as_str().unwrap());

    let (status, proposed) = call(
        &app,
        &agent,
        "POST",
        &format!("/api/projects/{p}/change-sets"),
        json!({"reason": "r", "operations": [{"op": "create", "title": "T", "acceptance": ["a"]}]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{proposed}");
    let c = proposed["change_set"]["id"].as_str().unwrap().to_owned();

    // The table names exactly the tools the server advertises, so a new tool cannot skip it.
    let listed: Vec<Value> = serde_json::from_str(TOOLS_LIST).unwrap();
    let mut advertised: Vec<&str> = listed.iter().map(|t| t["name"].as_str().unwrap()).collect();
    let mut walked: Vec<&str> = TOOLS.iter().map(|(name, ..)| *name).collect();
    advertised.sort_unstable();
    walked.sort_unstable();
    assert_eq!(
        walked, advertised,
        "add the new tool to TOOLS with the routes it calls"
    );

    for (tool, reach, routes) in TOOLS {
        for (method, template) in *routes {
            let uri = template
                .replace("{p}", &p)
                .replace("{i}", &i)
                .replace("{t}", &t)
                .replace("{c}", &c);
            let body = match *method {
                "GET" => Value::Null,
                _ if uri.ends_with("/change-sets") => json!({
                    "reason": "r",
                    "operations": [{"op": "create", "title": "T", "acceptance": ["a"]}],
                }),
                _ => json!({"body": "hello"}),
            };
            let (status, got) = call(&app, &agent, method, &uri, body).await;
            match reach {
                Agent => assert_ne!(
                    status,
                    StatusCode::FORBIDDEN,
                    "{tool}: {method} {uri} is refused to a credential that holds every \
                     capability; classify it in agent_access: {got}"
                ),
                OperatorOnly => {
                    assert_eq!(
                        status,
                        StatusCode::FORBIDDEN,
                        "{tool}: {method} {uri}: {got}"
                    );
                    assert!(
                        got.to_string()
                            .contains("not available to an agent credential"),
                        "{tool}: {method} {uri}: {got}"
                    );
                }
            }
        }
    }
}
