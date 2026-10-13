//! What a run takes from the person's own opencode setup: the provider, the
//! default models and where the login lives. Nothing else of that setup is
//! read into a run.

use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use serde_json::{Map, Value};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(20);

/// The person's providers and models, and the directory their login is in.
#[derive(Debug)]
pub(super) struct Login {
    /// `provider`, `model` and `small_model` as the person's config resolves
    /// them; a key the config does not set is absent.
    pub(super) copied: Map<String, Value>,
    pub(super) data_dir: PathBuf,
}

impl Login {
    pub(super) fn model(&self) -> Option<&str> {
        self.copied.get("model").and_then(Value::as_str)
    }

    /// Every string under a copied `apiKey`, `api_key` or `headers` key.
    pub(super) fn secrets(&self) -> Vec<String> {
        let mut found = Vec::new();
        collect_secrets(&Value::Object(self.copied.clone()), false, &mut found);
        found
    }
}

fn collect_secrets(value: &Value, secret: bool, found: &mut Vec<String>) {
    match value {
        Value::String(text) if secret => found.push(text.clone()),
        Value::Object(map) => {
            for (key, inner) in map {
                let secret = secret || matches!(key.as_str(), "apiKey" | "api_key" | "headers");
                collect_secrets(inner, secret, found);
            }
        }
        Value::Array(items) => items
            .iter()
            .for_each(|inner| collect_secrets(inner, secret, found)),
        _ => {}
    }
}

/// Runs `opencode debug config` and `opencode debug paths` as the runner's
/// own user, against its real `HOME`.
pub(super) fn read(
    program: &Path,
    path: Option<&std::ffi::OsStr>,
    home: &Path,
) -> Result<Login, String> {
    let config = run(program, &["debug", "config"], path, home)?;
    let config: Value = serde_json::from_str(&config)
        .map_err(|_| "`opencode debug config` did not print JSON".to_owned())?;
    let copied = ["provider", "model", "small_model"]
        .into_iter()
        .filter_map(|key| Some((key.to_owned(), config.get(key)?.clone())))
        .collect();
    let paths = run(program, &["debug", "paths"], path, home)?;
    let data_dir = paths
        .lines()
        .find_map(|line| {
            let (name, value) = line.trim().split_once(char::is_whitespace)?;
            (name == "data").then(|| PathBuf::from(value.trim()))
        })
        .ok_or_else(|| "`opencode debug paths` did not name a data directory".to_owned())?;
    Ok(Login { copied, data_dir })
}

fn run(
    program: &Path,
    args: &[&str],
    path: Option<&std::ffi::OsStr>,
    home: &Path,
) -> Result<String, String> {
    let command = format!("opencode {}", args.join(" "));
    let mut child = Command::new(program);
    child
        .args(args)
        .env_clear()
        .env("HOME", home)
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = path {
        child.env("PATH", path);
    }
    let mut child = child
        .spawn()
        .map_err(|error| format!("`{command}` could not start: {error}"))?;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(stdout) = stdout.as_mut() {
            let _ = stdout.read_to_string(&mut text);
        }
        text
    });
    let err = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(stderr) = stderr.as_mut() {
            let _ = stderr.take(8192).read_to_string(&mut text);
        }
        text
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() >= COMMAND_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => return Err(format!("`{command}` failed: {error}")),
        }
    };
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    match status {
        Some(status) if status.success() => Ok(stdout),
        Some(status) => {
            let reason = stderr
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map_or_else(
                    || format!("exited with {status}"),
                    |line| line.chars().take(120).collect(),
                );
            Err(format!("`{command}` failed: {reason}"))
        }
        None => Err(format!("`{command}` failed: timed out")),
    }
}
