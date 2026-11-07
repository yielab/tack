//! Redaction primitives shared by [`super::process`] and the local harnesses.
//! Credentials, prompt bodies, query strings and environment values must
//! never reach a log line or a harness event. Two independent mechanisms
//! enforce this: **structural avoidance** ([`RedactedEnv`] gives a
//! `Debug`-safe stand-in so a future `tracing::debug!(?spec)` can't
//! accidentally print a secret) and **content scrubbing**
//! ([`SecretMaterial`] replaces every exact occurrence of a known secret in
//! text a harness produced) — needed because a buggy or malicious harness
//! can echo a credential into its own stdout/stderr, which flows through as
//! ordinary captured output; scrubbing catches it before it is ever stored.

use std::collections::BTreeSet;

/// A set of exact secret values to strip from any text a harness produced.
/// Every registered value is an opaque byte string — no normalization,
/// casing, or partial-match heuristic, because a heuristic match is also a
/// heuristic *miss*. Callers seed this with the enrollment/runner
/// credential, resolved secret-reference env values, and the prompt body.
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

/// The secrets in a CLI's login file (`~/.claude/.credentials.json`,
/// `~/.codex/auth.json`): every string under a key that names a token, a key
/// or a secret, however deep. A file that is absent or is not JSON gives none.
/// The values are the CLI's own, so a run that prints one is leaking the
/// person's login.
pub fn login_secrets(file: &std::path::Path) -> Vec<String> {
    // Shorter than this is a flag or a word (`"max"`), not a credential.
    const LEAST: usize = 8;
    fn collect(value: &serde_json::Value, secret: bool, found: &mut Vec<String>) {
        match value {
            serde_json::Value::String(text) if secret && text.len() >= LEAST => {
                found.push(text.clone());
            }
            serde_json::Value::Object(map) => {
                for (key, inner) in map {
                    let key = key.to_ascii_lowercase();
                    let named = ["token", "key", "secret"]
                        .iter()
                        .any(|word| key.contains(word));
                    collect(inner, secret || named, found);
                }
            }
            serde_json::Value::Array(items) => {
                items.iter().for_each(|inner| collect(inner, secret, found));
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    if let Some(value) = std::fs::read(file)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    {
        collect(&value, false, &mut found);
    }
    found
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
    }

    #[test]
    fn empty_secret_values_are_never_registered() {
        let mut material = SecretMaterial::new();
        material.register("");
        assert_eq!(material.scrub("nothing secret here"), "nothing secret here");
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
    fn login_secrets_reads_the_tokens_of_a_login_file_and_nothing_else() {
        let dir = tempfile::tempdir().expect("directory");
        let file = dir.path().join("login.json");
        std::fs::write(
            &file,
            r#"{"claudeAiOauth":{"accessToken":"canary-access-token","refreshToken":"canary-refresh-token",
                "expiresAt":1790000000000,"scopes":["user:inference"],"subscriptionType":"maxplan"},
               "OPENAI_API_KEY":"sk-canary-key","tokens":{"id_token":"canary-id.jwt"},"short":"abc",
               "last_refresh":"2026-11-01T00:00:00Z"}"#,
        )
        .expect("login file");
        let mut found = login_secrets(&file);
        found.sort();
        assert_eq!(
            found,
            [
                "canary-access-token",
                "canary-id.jwt",
                "canary-refresh-token",
                "sk-canary-key"
            ]
        );
        assert!(login_secrets(&dir.path().join("absent.json")).is_empty());
        std::fs::write(&file, "not json").expect("file");
        assert!(login_secrets(&file).is_empty());
    }

    #[test]
    fn redact_query_strips_query_fragment_keeps_bare_urls_unchanged() {
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
