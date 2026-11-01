//! The tools that discover where a run can happen: fleets and agent profiles.

use serde_json::{Value, json};

use super::projection::data_list;
use super::tool::{Kind, Spec, Tool, call};
use crate::client::TackClient;

pub static LIST_FLEETS: Tool = Tool {
    name: "list_fleets",
    spec: Spec {
        description: "List runner fleets.",
        input_schema: || json!({ "type": "object", "properties": {} }),
    },
    kind: Kind::Read,
    handler: list_fleets,
};

fn list_fleets(client: &TackClient, _args: &Value) -> Result<Value, String> {
    let fleets = call(client.get("/runner-fleets"))?;
    Ok(data_list("fleets", &fleets))
}

pub static LIST_AGENT_PROFILES: Tool = Tool {
    name: "list_agent_profiles",
    spec: Spec {
        description: "List agent profiles (instructions, tool policy, limits).",
        input_schema: || json!({ "type": "object", "properties": {} }),
    },
    kind: Kind::Read,
    handler: list_agent_profiles,
};

fn list_agent_profiles(client: &TackClient, _args: &Value) -> Result<Value, String> {
    let profiles = call(client.get("/agent-profiles"))?;
    Ok(data_list("agent_profiles", &profiles))
}
