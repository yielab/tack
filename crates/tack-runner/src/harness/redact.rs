//! Redaction primitives shared by [`super::process`] and [`super::event_sink`].
//!
//! Rule 12 (`TODO.md` III.2): credentials, prompt bodies, query strings and
//! complete environment values must never reach a log line or a harness
//! event. Two independent mechanisms enforce this:
//!
//! 1. **Structural avoidance** — [`RedactedEnv`] and [`PromptSummary`] give
//!    [`std::fmt::Debug`]-safe stand-ins for values that must never be
//!    formatted directly, so a future `tracing::debug!(?spec)` cannot
//!    accidentally print a secret merely by existing.
//! 2. **Content scrubbing** — [`SecretMaterial`] replaces every exact
//!    occurrence of a known secret value inside text a harness produced.
//!    Structural avoidance alone is not enough: the *harness itself* can
//!    echo a credential or prompt fragment into its own stdout/stderr (a
//!    buggy or malicious harness, or a verbose debug flag), and that text
//!    flows through [`super::process`]/[`super::event_sink`] as ordinary
//!    captured output. Scrubbing is applied to that captured text before it
//!    is ever stored, so a canary planted in credentials/env/prompt cannot
//!    surface even via that path.

use std::collections::BTreeSet;

/// A set of exact secret values to strip from any text a harness produced.
///
/// Every registered value is treated as an opaque byte string: no
/// normalization, casing, or partial-match heuristic is applied, because a
/// heuristic match is also a heuristic *miss*. Callers seed this with every
/// value that must never survive into captured output: the enrollment/runner
/// credential, resolved secret-reference values placed into the child's
/// environment, and the prompt body (or a distinctive fragment of it) handed
/// to the harness.
#[derive(Debug, Clone, Default)]
pub struct SecretMaterial {
    // Longest-first so a secret that is a substring of another registered
    // secret is not partially masked, leaving a residual fragment behind.
    values: Vec<String>,
}

const REDACTED_PLACEHOLDER: &str = "[REDACTED]";

impl SecretMaterial {
    pub fn new() -> Self {
        Self { values: Vec::new() }
    }

    /// Registers one secret value. Empty strings are ignored: they match
    /// everywhere and would corrupt unrelated output instead of protecting
    /// anything.
    pub fn register(&mut self, value: impl Into<String>) -> &mut Self {
        let value = value.into();
        if !value.is_empty() && !self.values.iter().any(|existing| existing == &value) {
            self.values.push(value);
            self.values.sort_by_key(|b| std::cmp::Reverse(b.len()));
        }
        self
    }

    pub fn register_all<I, S>(&mut self, values: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for value in values {
            self.register(value);
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Replaces every occurrence of every registered secret with
    /// `[REDACTED]`. Byte-exact substring replacement, so it works
    /// regardless of the surrounding structure (plain text, JSON, a stray
    /// `key=value` pair a harness printed for "debugging").
    pub fn scrub(&self, text: &str) -> String {
        if self.values.is_empty() {
            return text.to_owned();
        }
        let mut output = text.to_owned();
        for secret in &self.values {
            if output.contains(secret.as_str()) {
                output = output.replace(secret.as_str(), REDACTED_PLACEHOLDER);
            }
        }
        output
    }

    /// Scrubs every string leaf of a JSON value in place (object keys are
    /// left alone; only string values are inspected, since keys are shaped
    /// by the harness's own schema, not secret material).
    pub fn scrub_json(&self, value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(text) => *text = self.scrub(text),
            serde_json::Value::Array(items) => {
                for item in items {
                    self.scrub_json(item);
                }
            }
            serde_json::Value::Object(map) => {
                for (_, item) in map.iter_mut() {
                    self.scrub_json(item);
                }
            }
            serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
            }
        }
    }
}

/// A `Debug`-safe stand-in for an environment map. Formatting it prints only
/// the sorted key set, never a value — matching how
/// [`crate::config::EnrollmentCredential`] already redacts `Debug`/`Display`.
pub struct RedactedEnv<'a>(pub &'a std::collections::BTreeMap<String, String>);

impl std::fmt::Debug for RedactedEnv<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let keys: BTreeSet<&str> = self.0.keys().map(String::as_str).collect();
        formatter
            .debug_struct("RedactedEnv")
            .field("keys", &keys)
            .field("values", &"[REDACTED]")
            .finish()
    }
}

/// A safe, non-reversible stand-in for a prompt body: byte length plus a
/// truncated SHA-256 fingerprint (via [`super::sha256`]), enough to correlate
/// "was this the same prompt" across logs without ever printing the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptSummary {
    pub byte_len: usize,
    pub sha256_prefix: String,
}

impl PromptSummary {
    pub fn of(prompt: &str) -> Self {
        let digest = super::sha256::sha256_hex(prompt.as_bytes());
        Self {
            byte_len: prompt.len(),
            sha256_prefix: digest[..16].to_owned(),
        }
    }
}

impl std::fmt::Display for PromptSummary {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "prompt({} bytes, sha256:{}…)",
            self.byte_len, self.sha256_prefix
        )
    }
}

/// Strips a query string (and its leading `?`) from a URL-shaped string.
/// Query strings are a common place for a leaked API key (`?api_key=...`),
/// so any URL that might reach a log or event must go through this first.
/// Text with no `?` is returned unchanged; a fragment (`#...`) after the
/// query string is dropped along with it, since it is positionally part of
/// the same trailing segment and just as unsafe to assume is public.
pub fn redact_query(url: &str) -> String {
    match url.find('?') {
        Some(index) => url[..index].to_owned(),
        None => url.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn scrub_removes_every_occurrence_of_every_registered_secret() {
        let mut material = SecretMaterial::new();
        material.register("canary-credential-xyz");
        material.register("canary-prompt-body");

        let text =
            "leaked=canary-credential-xyz also here canary-credential-xyz and canary-prompt-body";
        let scrubbed = material.scrub(text);

        assert!(!scrubbed.contains("canary-credential-xyz"));
        assert!(!scrubbed.contains("canary-prompt-body"));
        assert_eq!(scrubbed.matches(REDACTED_PLACEHOLDER).count(), 3);
    }

    #[test]
    fn scrub_is_a_no_op_when_nothing_is_registered() {
        let material = SecretMaterial::new();
        assert_eq!(material.scrub("nothing secret here"), "nothing secret here");
        assert!(material.is_empty());
    }

    #[test]
    fn empty_secret_values_are_never_registered() {
        let mut material = SecretMaterial::new();
        material.register("");
        assert!(material.is_empty());
    }

    #[test]
    fn longer_secrets_are_scrubbed_before_shorter_overlapping_ones() {
        let mut material = SecretMaterial::new();
        // "canary" is a substring of "canary-extended"; scrubbing the short
        // value first would leave "-extended[REDACTED]" instead of one clean
        // placeholder for the longer secret.
        material.register("canary");
        material.register("canary-extended");

        let scrubbed = material.scrub("token=canary-extended");
        assert_eq!(scrubbed, "token=[REDACTED]");
    }

    #[test]
    fn scrub_json_walks_arrays_and_objects_but_leaves_keys_alone() {
        let mut material = SecretMaterial::new();
        material.register("canary-value");
        let mut value = serde_json::json!({
            "canary-value": "outer",
            "nested": {"text": "contains canary-value here"},
            "list": ["canary-value", 1, null, true]
        });

        material.scrub_json(&mut value);

        assert_eq!(value["canary-value"], "outer", "keys are not scrubbed");
        assert_eq!(value["nested"]["text"], "contains [REDACTED] here");
        assert_eq!(value["list"][0], "[REDACTED]");
        assert_eq!(value["list"][1], 1);
        assert_eq!(value["list"][2], serde_json::Value::Null);
        assert_eq!(value["list"][3], true);
    }

    #[test]
    fn redacted_env_debug_never_contains_a_value() {
        let mut env = BTreeMap::new();
        env.insert("TACK_TEST_SECRET".to_owned(), "canary-env-value".to_owned());
        env.insert("OTHER".to_owned(), "also-secret".to_owned());

        let formatted = format!("{:?}", RedactedEnv(&env));

        assert!(formatted.contains("TACK_TEST_SECRET"), "keys are visible");
        assert!(!formatted.contains("canary-env-value"));
        assert!(!formatted.contains("also-secret"));
    }

    #[test]
    fn prompt_summary_never_contains_the_prompt_text() {
        let prompt = "do the canary-prompt-body thing";
        let summary = PromptSummary::of(prompt);

        assert_eq!(summary.byte_len, prompt.len());
        assert_eq!(summary.sha256_prefix.len(), 16);
        assert!(!format!("{summary}").contains("canary-prompt-body"));
        assert!(!format!("{summary:?}").contains("canary-prompt-body"));
    }

    #[test]
    fn prompt_summary_is_deterministic_and_distinguishes_content() {
        assert_eq!(PromptSummary::of("same"), PromptSummary::of("same"));
        assert_ne!(PromptSummary::of("a"), PromptSummary::of("b"));
    }

    #[test]
    fn redact_query_strips_query_and_fragment_but_keeps_bare_urls_unchanged() {
        assert_eq!(
            redact_query("https://example.invalid/v1/models?api_key=canary-xyz"),
            "https://example.invalid/v1/models"
        );
        assert_eq!(
            redact_query("https://example.invalid/path"),
            "https://example.invalid/path"
        );
        assert_eq!(redact_query("no-query-here"), "no-query-here");
    }
}
