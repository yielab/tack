//! What each step of a settled task was made of, as the run keeps it: one entry per hop of the
//! task record, in order, written from the hop's `promptManifest` and `artifact`. The value is
//! the run's `steps`; the verifier's `trajectory` is the same value stored as one artifact.

use serde_json::{Value, json};

use super::wire::{PromptManifest, TaskHop};

/// The `steps` of a task's hops. A hop with no manifest has `prompts: []`, which the screen words
/// as "recorded no prompt", and an empty list for what the agent assumed or asked means it said
/// nothing; nothing here is invented for a key the hop does not have. `hop_times` is whether this
/// docket lists `hopTimes`: only then are `startedAt`, `finishedAt` and `cycle` read, and a hop
/// that has none of them (no agent turn, or recorded before) keeps the keys out of its step.
pub(super) fn steps(hops: &[TaskHop], hop_times: bool) -> Vec<Value> {
    hops.iter().map(|hop| step(hop, hop_times)).collect()
}

/// What a task handed off as its result: the summary of its last step, when it wrote one.
pub(super) fn result(hops: &[TaskHop]) -> Option<String> {
    hops.last()?
        .artifact
        .as_ref()?
        .summary
        .as_deref()
        .filter(|summary| !summary.is_empty())
        .map(str::to_owned)
}

fn step(hop: &TaskHop, hop_times: bool) -> Value {
    let manifest = hop.prompt_manifest.as_ref();
    let artifact = hop.artifact.as_ref();
    let text = |value: Option<&str>| value.filter(|value| !value.is_empty()).map(str::to_owned);
    let prompts = manifest.map_or_else(Vec::new, prompts);
    let mut step = json!({
        "role": text(hop.role.as_deref()),
        // The model the hop was served by; the manifest names the one it was planned for.
        "model": text(hop.model.as_deref())
            .or_else(|| text(manifest.and_then(|manifest| manifest.model.as_deref()))),
        "class": text(manifest.and_then(|manifest| manifest.class.as_deref())),
        "family": text(manifest.and_then(|manifest| manifest.family.as_deref())),
        "verdict": text(artifact.and_then(|artifact| artifact.verdict.as_deref()))
            .or_else(|| text(hop.verdict.as_deref())),
        "ok": hop.ok,
        "summary": text(artifact.and_then(|artifact| artifact.summary.as_deref())),
        "prompts": prompts,
        "assumptions": artifact.map_or_else(Vec::new, |artifact| artifact.assumptions.clone()),
        "questions": artifact.map_or_else(Vec::new, |artifact| artifact.questions.clone()),
    });
    if hop_times && let Some(object) = step.as_object_mut() {
        if let Some(started) = text(hop.started_at.as_deref()) {
            object.insert("startedAt".to_owned(), json!(started));
        }
        if let Some(finished) = text(hop.finished_at.as_deref()) {
            object.insert("finishedAt".to_owned(), json!(finished));
        }
        if let Some(cycle) = hop.cycle {
            object.insert("cycle".to_owned(), json!(cycle));
        }
    }
    step
}

fn prompts(manifest: &PromptManifest) -> Vec<Value> {
    manifest
        .layers
        .iter()
        .map(|layer| {
            json!({
                "layer": layer.layer,
                "name": layer.name,
                "version": layer.version,
                "sha256": layer.sha256,
                "scope": layer.scope,
                "tokens": layer.tokens,
            })
        })
        .collect()
}
