//! The verifier set: labelled diffs that ask whether a verifier signs work it should not and
//! rejects work it should sign.
//!
//! Each case is a directory holding `diff.patch` (a change to a regression task's seed),
//! `reference.yaml` (what the verifier is told about the task: the instruction and the files the
//! work may touch) and `label.yaml` (the answer key). A script makes the cases from the
//! tasks' reference solutions ([`generate`]), and a person confirms every label by setting
//! `confirmed: true` in it; nothing is scored against a label nobody has read.

pub mod generate;
pub mod mutate;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// What the work deserves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expected {
    Merge,
    Reject,
}

/// What the generator did to the reference solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// The task's reference solution as it is.
    Reference,
    /// The reference solution with a change no check can see.
    Variant,
    /// Work a check of the task refuses.
    Mutant,
}

/// What running the task's checks on the diff said; this is the generator's proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proof {
    pub hidden_tests_passed: bool,
    /// The tests the agent can see; `null` when the task names none.
    pub visible_tests_passed: Option<bool>,
    /// Every touched file is among those the task allows; `null` when it allows any.
    pub within_scope: Option<bool>,
}

/// The answer key of a case (`label.yaml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Label {
    pub expected: Expected,
    /// A person read the diff and agrees with `expected`.
    pub confirmed: bool,
    pub kind: Kind,
    /// The family of the mutation (`logic`, `weakened_tests`, `test_config`, `scope`).
    #[serde(default)]
    pub family: Option<String>,
    pub operator: String,
    pub reason: String,
    pub proof: Proof,
    pub seed: u64,
}

/// What the verifier is told about the task (`reference.yaml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub task: String,
    pub task_version: u32,
    pub instruction: String,
    #[serde(default)]
    pub constraints: Vec<String>,
    /// The files the work may touch; absent when the task allows any.
    #[serde(default)]
    pub allowed_files: Option<Vec<String>>,
}

/// A case read from disk.
#[derive(Debug, Clone)]
pub struct Case {
    pub id: String,
    pub dir: PathBuf,
    pub reference: Reference,
    pub label: Label,
}

impl Case {
    pub fn diff_path(&self) -> PathBuf {
        self.dir.join("diff.patch")
    }
}

/// Every case under `dir`, in id order.
pub fn load_set(dir: &Path) -> Result<Vec<Case>> {
    let mut cases = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let path = entry?.path();
        if !path.join("label.yaml").is_file() {
            continue;
        }
        let id = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        let read = |name: &str| -> Result<String> {
            std::fs::read_to_string(path.join(name))
                .with_context(|| format!("case {id}: read {name}"))
        };
        let label: Label = serde_saphyr::from_str(&read("label.yaml")?)
            .with_context(|| format!("case {id}: label.yaml"))?;
        let reference: Reference = serde_saphyr::from_str(&read("reference.yaml")?)
            .with_context(|| format!("case {id}: reference.yaml"))?;
        if !path.join("diff.patch").is_file() {
            bail!("case {id}: diff.patch is missing");
        }
        cases.push(Case {
            id,
            dir: path,
            reference,
            label,
        });
    }
    cases.sort_by(|a, b| a.id.cmp(&b.id));
    if cases.is_empty() {
        bail!("no cases under {}", dir.display());
    }
    Ok(cases)
}

/// The cases whose label no person has confirmed.
pub fn unconfirmed(cases: &[Case]) -> Vec<&Case> {
    cases.iter().filter(|c| !c.label.confirmed).collect()
}
