use std::collections::HashMap;

use crate::client::TackClient;

pub type VocabMap = HashMap<String, String>;

/// Fetch a project's vocabulary map. Returns an empty map on any error so callers
/// always get a usable value.
pub fn fetch(client: &TackClient, project_id: &str) -> VocabMap {
    client
        .get(&format!("/projects/{project_id}"))
        .ok()
        .and_then(|v| serde_json::from_value(v["vocabulary"].clone()).ok())
        .unwrap_or_default()
}

/// Look up a canonical key (e.g. "task") in the vocabulary, falling back to the
/// key itself if not present.
pub fn term<'a>(vocab: &'a VocabMap, key: &'a str) -> &'a str {
    vocab.get(key).map(String::as_str).unwrap_or(key)
}
