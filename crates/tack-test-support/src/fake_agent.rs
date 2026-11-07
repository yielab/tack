//! A stand-in for the `claude` binary, for tests that run the real server and the real runner
//! and need only the agent to be a script: it answers the runner's probe, optionally asks for
//! one tool permission the way Claude Code does (`can_use_tool` over stream-json), writes the
//! files its behaviour names into its working directory and ends with a `result` line.
//!
//! Only the agent is fake. Everything between the request and the finished attempt is the
//! product. No model is called and no tokens are spent.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// What the fake agent does when it is run.
#[derive(Debug, Clone, Default)]
pub struct FakeAgent {
    /// Files written into the working directory, as (relative path, content). Written when the
    /// agent is allowed to work: always, or after the tool it asked for was allowed.
    pub writes: Vec<(String, String)>,
    /// Ask for permission to use this tool (name, command) before working.
    pub asks: Option<(String, String)>,
}

impl FakeAgent {
    /// Writes the executable `claude` into `dir` and returns its path. Put `dir` on the
    /// runner's `PATH`.
    pub fn install(&self, dir: &Path) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("claude");
        std::fs::write(&path, self.script())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        }
        Ok(path)
    }

    fn script(&self) -> String {
        let mut work = String::new();
        for (path, content) in &self.writes {
            let parent = Path::new(path)
                .parent()
                .and_then(Path::to_str)
                .unwrap_or("");
            if !parent.is_empty() {
                let _ = writeln!(work, "mkdir -p {}", quoted(parent));
            }
            let _ = writeln!(work, "printf '%s' {} > {}", quoted(content), quoted(path));
        }
        let ask = match &self.asks {
            None => work,
            Some((tool, command)) => {
                let request = serde_json::json!({
                    "type": "control_request",
                    "request_id": "fake-request-1",
                    "request": {
                        "subtype": "can_use_tool",
                        "tool_name": tool,
                        "input": {"command": command},
                        "description": command,
                    },
                });
                format!(
                    "printf '%s\\n' {request}\n\
                     read -r answer\n\
                     case \"$answer\" in\n\
                     *'\"behavior\":\"allow\"'*)\n{work};;\n\
                     esac\n",
                    request = quoted(&request.to_string()),
                )
            }
        };
        // `claude -p` reads its prompt from stdin until it ends. A run that asks (or that the
        // runner turned into one) keeps stdin open for the answers, so there the prompt is one
        // line and reading to the end would never return.
        let drain = "for arg in \"$@\"; do [ \"$arg\" = --permission-prompt-tool ] && conversation=1; done\n\
                     if [ -n \"${conversation:-}\" ]; then read -r _prompt; else cat >/dev/null; fi\n";
        format!(
            "#!/bin/sh\n\
             case \"${{1:-}}\" in\n\
             --version|-v) echo '2.1.302 (Claude Code)'; exit 0;;\n\
             auth|login|debug) exit 1;;\n\
             esac\n\
             {drain}{ask}\
             printf '%s\\n' '{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"done\",\"permission_denials\":[],\"usage\":{{\"input_tokens\":2,\"output_tokens\":18}}}}'\n"
        )
    }
}

/// `text` as one single-quoted shell word.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}
