use crate::RunnerError;

/// Harness identities are explicit, avoiding a bare ambiguous `provider` key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessKind {
    Codex,
    ClaudeCode,
    OpenCode,
    Other(String),
}

impl HarnessKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
            Self::OpenCode => "opencode",
            Self::Other(value) => value,
        }
    }
}

/// The registry is deliberately empty until Wave 3 adapters are added. It is
/// total: unknown or not-yet-installed harnesses return a stable typed error.
#[derive(Debug, Default)]
pub struct HarnessRegistry;

impl HarnessRegistry {
    pub fn require_supported(&self, harness: &HarnessKind) -> Result<(), RunnerError> {
        Err(RunnerError::UnsupportedHarness {
            harness: harness.as_str().to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry_returns_typed_unsupported_for_every_harness() {
        let registry = HarnessRegistry;
        for harness in [
            HarnessKind::Codex,
            HarnessKind::ClaudeCode,
            HarnessKind::OpenCode,
            HarnessKind::Other("future-harness".into()),
        ] {
            let result = registry.require_supported(&harness);
            assert!(matches!(
                result,
                Err(RunnerError::UnsupportedHarness { .. })
            ));
        }
    }
}
