//! What each step of a settled task was made of, as the run keeps it: one entry per hop of the
//! task record, in order, written from the hop's `promptManifest` and `artifact`. The value is
//! the run's `steps`; the verifier's `trajectory` is the same value stored as one artifact.

use serde_json::{Value, json};

use super::wire::{PromptManifest, TaskHop};

/// The `steps` of a task's hops. A hop with no manifest has `prompts: []`, which the screen words
/// as "recorded no prompt", and an empty list for what the agent assumed or asked means it said
/// nothing; nothing here is invented for a key the hop does not have.
pub(super) fn steps(hops: &[TaskHop]) -> Vec<Value> {
    hops.iter().map(step).collect()
}

fn step(hop: &TaskHop) -> Value {
    let manifest = hop.prompt_manifest.as_ref();
    let artifact = hop.artifact.as_ref();
    let text = |value: Option<&str>| value.filter(|value| !value.is_empty()).map(str::to_owned);
    let prompts = manifest.map_or_else(Vec::new, prompts);
    json!({
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
    })
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
