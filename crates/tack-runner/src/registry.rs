/// Harness identities are explicit, avoiding a bare ambiguous `provider` key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessKind {
    Codex,
    ClaudeCode,
    Other(String),
}

impl HarnessKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
            Self::Other(value) => value,
        }
    }
}
