//! The operator of a trial: answers every decision the run raises, by the API, with the
//! decision token, as the task's script says.

use std::collections::HashSet;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::Api;
use crate::task::DecisionScript;

/// One decision the run raised and what was answered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub decision_id: String,
    pub kind: String,
    pub prompt: String,
    pub options: Vec<String>,
    /// The option the operator chose.
    pub answered: String,
}

pub struct Operator {
    script: DecisionScript,
    seen: HashSet<String>,
    pub decisions: Vec<DecisionRecord>,
}

impl Operator {
    pub fn new(script: DecisionScript) -> Self {
        Self {
            script,
            seen: HashSet::new(),
            decisions: Vec::new(),
        }
    }

    /// Answers every decision of `request_id` that is still pending; returns how many.
    pub fn answer_pending(&mut self, api: &Api, request_id: &str) -> Result<usize> {
        let attempts = api.get(&format!("/api/executions/{request_id}/attempts"))?;
        let mut answered = 0;
        for attempt in attempts["data"].as_array().into_iter().flatten() {
            let Some(number) = attempt["attempt_number"].as_u64() else {
                continue;
            };
            let listed = api.get(&format!(
                "/api/executions/{request_id}/attempts/{number}/decisions"
            ))?;
            for decision in listed["data"].as_array().into_iter().flatten() {
                if decision["state"] != "pending" {
                    continue;
                }
                let (Some(attempt_id), Some(decision_id)) = (
                    decision["attempt_id"].as_str(),
                    decision["decision_id"].as_str(),
                ) else {
                    continue;
                };
                if !self.seen.insert(decision_id.to_owned()) {
                    continue;
                }
                let options: Vec<String> = decision["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|o| o["option_id"].as_str().map(str::to_owned))
                    .collect();
                let choice = self.choose(&options);
                api.post_as_decider(
                    &format!("/api/attempts/{attempt_id}/decisions/{decision_id}/resolve"),
                    &json!({"answer": {"option_id": choice}}),
                )?;
                self.decisions.push(record(decision, &options, choice));
                answered += 1;
            }
        }
        Ok(answered)
    }

    /// By convention a question lists its safe default last (`deny`), and a permission to go
    /// ahead is `allow_once`; a question that follows neither gets its first option on approval
    /// and its last on refusal.
    fn choose<'a>(&self, options: &'a [String]) -> &'a str {
        let find = |wanted: &str| options.iter().find(|o| *o == wanted);
        let picked = match self.script {
            DecisionScript::ApproveOnce => find("allow_once").or(options.first()),
            DecisionScript::Deny => find("deny").or(options.last()),
        };
        picked.map_or("deny", String::as_str)
    }
}

fn record(decision: &Value, options: &[String], choice: &str) -> DecisionRecord {
    let text = |key: &str| decision[key].as_str().unwrap_or_default().to_owned();
    DecisionRecord {
        decision_id: text("decision_id"),
        kind: text("kind"),
        prompt: text("prompt"),
        options: options.to_vec(),
        answered: choice.to_owned(),
    }
}
