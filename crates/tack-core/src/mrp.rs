//! The Merge-Readiness Pack (MRP): what an independent verifier says about
//! one attempt — each brief criterion's status, the checks it ran, a risk
//! tier and a recommendation. Tack drafts the contract as its consumer and
//! never produces a pack itself. Pure — no database, no handler.
//! `docs/contracts/mrp-v1/` is the wire shape; `tests/mrp_contract.rs` pins
//! its fixtures.

use serde::{Deserialize, Serialize};

/// Criterion evaluation in the pack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CriterionStatus {
    Passed,
    Failed,
    Manual,
    Skipped,
}

impl std::fmt::Display for CriterionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Passed => write!(f, "passed"),
            Self::Failed => write!(f, "failed"),
            Self::Manual => write!(f, "manual"),
            Self::Skipped => write!(f, "skipped"),
        }
    }
}

/// Criterion evaluation in a pack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Criterion {
    /// Criterion id from the brief
    pub id: String,
    /// Criterion kind
    pub kind: String,
    /// Criterion status: passed|failed|manual|skipped
    pub status: CriterionStatus,
    /// Path to evidence file if applicable
    pub evidence_ref: Option<String>,
    /// Human-readable note about this criterion
    pub note: Option<String>,
}

/// Verification run details.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerifySection {
    /// Verification command that was run
    pub command: String,
    /// Exit code of the verifier
    pub exit_code: i32,
    /// Last N lines of verifier output
    pub output_tail: Option<String>,
    /// Seconds elapsed during verification
    pub duration_s: Option<f64>,
}

/// Mutation testing results.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MutationSection {
    /// What was mutated
    pub scope: String,
    /// Mutation score (percent)
    pub score: f64,
    /// Number of mutations killed
    pub killed: u32,
    /// Number of mutations that survived
    pub survived: u32,
}

/// Static analysis results.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StaticAnalysisCounts {
    /// Number of errors
    pub error: u32,
    /// Number of warnings
    pub warning: u32,
    /// Number of notes
    pub note: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StaticAnalysisSection {
    /// SHA256 of the SARIF results file
    pub sarif_sha256: Option<String>,
    /// Error/warning/note counts
    pub counts: StaticAnalysisCounts,
}

/// Judge verdict for a criterion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JudgeRubricEntry {
    /// Brief criterion id
    pub criterion_id: String,
    /// Judge's verdict for this criterion
    pub verdict: JudgeVerdict,
    /// Explanation for the verdict
    pub reason: Option<String>,
}

/// A judge's verdict on one criterion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum JudgeVerdict {
    Pass,
    Fail,
}

impl std::fmt::Display for JudgeVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pass => write!(f, "pass"),
            Self::Fail => write!(f, "fail"),
        }
    }
}

/// Model judge section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JudgeSection {
    /// The judge's model family, reported by the producer
    pub model_family: String,
    /// `true` in every v1 pack: the contract only admits a blind judge
    pub blind: bool,
    /// Judge's verdict per criterion
    pub rubric: Vec<JudgeRubricEntry>,
}

/// Risk assessment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RiskTier {
    Low,
    Medium,
    High,
}

impl std::fmt::Display for RiskTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskSection {
    /// Risk tier assessment
    pub tier: RiskTier,
    /// Reasons for the risk tier
    pub reasons: Vec<String>,
}

/// Merge recommendation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MergeDecision {
    Merge,
    Review,
    Reject,
}

impl std::fmt::Display for MergeDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Merge => write!(f, "merge"),
            Self::Review => write!(f, "review"),
            Self::Reject => write!(f, "reject"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecommendationSection {
    /// Recommended decision
    pub decision: MergeDecision,
    /// Explanation for the recommendation
    pub rationale: String,
}

/// Verifier token usage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageSection {
    /// Input tokens used by verifier
    pub tokens_in: u64,
    /// Output tokens used by verifier
    pub tokens_out: u64,
}

/// Signature proof.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SignatureSection {
    None,
    InToto { statement_sha256: String },
}

/// Producer identification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProducedBySection {
    /// Name of the program that produced this pack
    pub program: String,
    /// Version of the producing program
    pub version: String,
}

/// The Merge Readiness Pack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergeReadinessPack {
    /// Schema version
    pub v: String,
    /// The attempt this pack certifies, as the evidence names it (`att_…`)
    pub attempt_id: String,
    /// SHA256 hash of the evidence directory
    pub evidence_sha256: String,
    /// SHA256 hash of the brief (null if no brief)
    pub brief_sha256: Option<String>,
    /// Status of each brief criterion
    pub criteria: Vec<Criterion>,
    /// Verification run details (null if verify was not run)
    pub verify: Option<VerifySection>,
    /// Mutation testing results (null if mutation test was not run)
    pub mutation: Option<MutationSection>,
    /// Static analysis results (null if not run)
    pub static_analysis: Option<StaticAnalysisSection>,
    /// Judge verdict (null if judge was not run)
    pub judge: Option<JudgeSection>,
    /// Risk tier assessment
    pub risk: RiskSection,
    /// Merge recommendation
    pub recommendation: RecommendationSection,
    /// Token usage of the verifier itself
    pub usage: UsageSection,
    /// Signature proof
    pub signature: SignatureSection,
    /// Producer identification
    pub produced_by: ProducedBySection,
}

/// Render the pack as Markdown for a pull-request body. One heading per
/// section, one table row per criterion.
pub fn render_markdown(pack: &MergeReadinessPack) -> String {
    let mut out = String::new();

    // Title
    out.push_str("# Merge Readiness Assessment\n\n");

    // Summary
    out.push_str(&format!(
        "**Decision:** {}\n\n",
        pack.recommendation.decision
    ));
    out.push_str(&format!("**Risk:** {}\n\n", pack.risk.tier));
    out.push_str(&format!("{}\n\n", pack.recommendation.rationale));

    // Criteria table
    if !pack.criteria.is_empty() {
        out.push_str("## Criteria\n\n");
        out.push_str("| Criterion | Kind | Status | Note |\n");
        out.push_str("|-----------|------|--------|------|\n");
        for criterion in &pack.criteria {
            let note = criterion.note.as_deref().unwrap_or("");
            out.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                criterion.id, criterion.kind, criterion.status, note
            ));
        }
        out.push('\n');
    }

    // Risk reasons
    if !pack.risk.reasons.is_empty() {
        out.push_str("## Risk Assessment\n\n");
        for reason in &pack.risk.reasons {
            out.push_str(&format!("- {}\n", reason));
        }
        out.push('\n');
    }

    // Verify section
    if let Some(verify) = &pack.verify {
        out.push_str("## Verification\n\n");
        out.push_str(&format!("**Command:** `{}`\n\n", verify.command));
        out.push_str(&format!("**Exit code:** {}\n\n", verify.exit_code));
        if let Some(duration) = verify.duration_s {
            out.push_str(&format!("**Duration:** {:.2}s\n\n", duration));
        }
        if let Some(output) = &verify.output_tail {
            out.push_str("**Output (tail):**\n\n");
            out.push_str("```\n");
            out.push_str(output);
            out.push_str("```\n\n");
        }
    }

    // Mutation section
    if let Some(mutation) = &pack.mutation {
        out.push_str("## Mutation Testing\n\n");
        out.push_str(&format!("**Score:** {:.1}%\n\n", mutation.score));
        out.push_str(&format!("**Killed:** {}\n\n", mutation.killed));
        out.push_str(&format!("**Survived:** {}\n\n", mutation.survived));
    }

    // Static analysis section
    if let Some(analysis) = &pack.static_analysis {
        out.push_str("## Static Analysis\n\n");
        out.push_str("| Severity | Count |\n|----------|-------|\n");
        out.push_str(&format!("| Error | {} |\n", analysis.counts.error));
        out.push_str(&format!("| Warning | {} |\n", analysis.counts.warning));
        out.push_str(&format!("| Note | {} |\n", analysis.counts.note));
        out.push('\n');
    }

    // Judge section
    if let Some(judge) = &pack.judge {
        out.push_str("## Judge Assessment\n\n");
        out.push_str(&format!(
            "**Model:** {} (blind: {})\n\n",
            judge.model_family, judge.blind
        ));
        out.push_str("| Criterion | Verdict | Reason |\n");
        out.push_str("|-----------|---------|--------|\n");
        for entry in &judge.rubric {
            let reason = entry.reason.as_deref().unwrap_or("");
            out.push_str(&format!(
                "| `{}` | {} | {} |\n",
                entry.criterion_id, entry.verdict, reason
            ));
        }
        out.push('\n');
    }

    // Usage section
    out.push_str("## Usage\n\n");
    out.push_str(&format!(
        "**Tokens:** {} in, {} out\n\n",
        pack.usage.tokens_in, pack.usage.tokens_out
    ));

    // Producer
    out.push_str("## Produced By\n\n");
    out.push_str(&format!(
        "{} {}\n",
        pack.produced_by.program, pack.produced_by.version
    ));

    out
}

/// A summary line for the pack (e.g., for a PR title or log entry).
pub fn summary_line(pack: &MergeReadinessPack) -> String {
    match pack.recommendation.decision {
        MergeDecision::Merge => format!("Ready to merge ({} risk)", pack.risk.tier),
        MergeDecision::Review => format!("Needs review ({} risk)", pack.risk.tier),
        MergeDecision::Reject => format!("Not ready ({} risk)", pack.risk.tier),
    }
}
