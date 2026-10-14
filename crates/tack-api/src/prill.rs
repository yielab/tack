//! The board's own prill: the one place the API server runs it.
//!
//! The board never produces a pack, only checks the attestation that came with
//! one. `run` is the single way in; `attest_verify` is the check the autonomy
//! levels name (`tack_core::autonomy::Check`, built by the caller).

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Output, Stdio},
    time::Duration,
};

use tokio::process::Command;

/// prill could not be started, or did not finish in time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unavailable;

/// Longest `attest verify` is given before the check fails.
const ATTEST_TIMEOUT: Duration = Duration::from_secs(20);

const NOT_INSTALLED: &str = "prill is not installed on the board";

/// Runs `program args…` with a null stdin and `PATH` as its only variable.
/// A program that cannot start, or does not finish within `timeout`, is
/// [`Unavailable`]; a program that exits non-zero is still an `Output`.
pub async fn run(program: &str, args: &[String], timeout: Duration) -> Result<Output, Unavailable> {
    #[allow(clippy::disallowed_methods)]
    let path = std::env::var("PATH").unwrap_or_default();
    let child = Command::new(program)
        .args(args)
        .env_clear()
        .env("PATH", path)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(timeout, child).await {
        Ok(Ok(output)) => Ok(output),
        _ => Err(Unavailable),
    }
}

/// Whether `envelope` is a valid attestation, signed by `public_key_pem`, of
/// `commit`: `prill attest verify --key <file> --commit <commit> <envelope>`
/// with both documents in a directory only the board can read. Exit 0 is
/// `Ok` with the key id prill names, which the merge record keeps; anything
/// else is `Err` with prill's reason, or a message saying prill is not
/// installed. The caller turns it into the policy's `Check`.
pub async fn attest_verify(
    program: &str,
    envelope: &[u8],
    public_key_pem: &str,
    commit: &str,
) -> Result<String, String> {
    let Some(scratch) = Scratch::new() else {
        return Err("the board could not stage the attestation".to_owned());
    };
    let key = scratch.0.join("prill.pub");
    let envelope_path = scratch.0.join("mrp.intoto.json");
    if write_private(&key, public_key_pem.as_bytes()).is_err()
        || write_private(&envelope_path, envelope).is_err()
    {
        return Err("the board could not stage the attestation".to_owned());
    }
    let args = [
        "attest".to_owned(),
        "verify".to_owned(),
        "--key".to_owned(),
        key.display().to_string(),
        "--commit".to_owned(),
        commit.to_owned(),
        envelope_path.display().to_string(),
    ];
    match run(program, &args, ATTEST_TIMEOUT).await {
        Ok(output) if output.status.success() => {
            // `prill: attestation valid (keyid <id>, subject …)`
            let said = String::from_utf8_lossy(&output.stderr);
            Ok(said
                .split_once("keyid ")
                .and_then(|(_, rest)| rest.split([',', ')']).next())
                .unwrap_or_default()
                .to_owned())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let line = stderr.lines().next().unwrap_or_default();
            let reason = line
                .split_once("attestation invalid: ")
                .map_or(line.trim_start_matches("prill: "), |(_, reason)| reason);
            Err(if reason.is_empty() {
                format!("prill refused the attestation ({})", output.status)
            } else {
                reason.to_owned()
            })
        }
        Err(Unavailable) => Err(NOT_INSTALLED.to_owned()),
    }
}

/// A `0700` directory under the OS temp dir, removed on drop.
pub(crate) struct Scratch(pub(crate) PathBuf);

impl Scratch {
    pub(crate) fn new() -> Option<Self> {
        #[allow(clippy::disallowed_methods)]
        let root = std::env::temp_dir();
        let path = root.join(format!("tack-attest-{}", uuid::Uuid::new_v4()));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(&path).ok().map(|()| Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};

    use super::*;

    const ENVELOPE: &str = include_str!("../tests/fixtures/prill/mrp.intoto.json");
    const KEY: &str = include_str!("../tests/fixtures/prill/prill.pub");
    const HEAD: &str = "ad0eea1455de382abb4f7c41129d524a14ddcbf5";
    const BASE: &str = "d705833ec4b0ee5a4ec97e5c8bb2a88f0521922f";

    /// A prill that answers `attest verify` from the captured outputs: exit 0
    /// for the commit the envelope names, the captured refusal for any other,
    /// and the key and envelope it was handed copied beside it so a test can
    /// see them.
    fn fake_prill(dir: &Path) -> String {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/prill");
        let script = dir.join("prill");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\n\
                 [ \"$1 $2\" = \"attest verify\" ] || exit 2\n\
                 shift 2\n\
                 while [ $# -gt 0 ]; do\n\
                   case \"$1\" in --key) cp \"$2\" '{d}/key.seen'; stat -c %a \"$2\" > '{d}/key.mode'; shift 2;;\n\
                   --commit) commit=\"$2\"; shift 2;;\n\
                   *) cp \"$1\" '{d}/envelope.seen'; shift;; esac\n\
                 done\n\
                 if [ \"$commit\" = {HEAD} ]; then cat '{f}/attest-ok.stderr' >&2; exit 0; fi\n\
                 cat '{f}/attest-other-commit.stderr' >&2; exit 1\n",
                d = dir.display(),
                f = fixtures.display(),
            ),
        )
        .expect("script");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).expect("mode");
        script.display().to_string()
    }

    #[tokio::test]
    async fn attest_verify_passes_fails_and_fails_when_prill_is_not_installed() {
        let dir = tempfile::tempdir().expect("dir");
        let program = fake_prill(dir.path());
        let ok = attest_verify(&program, ENVELOPE.as_bytes(), KEY, HEAD).await;
        assert_eq!(
            ok,
            Ok("f239d6c81feedf3a944a2a940f89f0ca4af0a2f05207185c19997eb09d7476f6".to_owned())
        );
        // The key travelled in a file only the board could read.
        assert_eq!(
            fs::read_to_string(dir.path().join("key.seen")).unwrap(),
            KEY
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("key.mode"))
                .unwrap()
                .trim(),
            "600"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("envelope.seen")).unwrap(),
            ENVELOPE
        );

        let wrong = attest_verify(&program, ENVELOPE.as_bytes(), KEY, BASE).await;
        assert_eq!(wrong, Err("subject_mismatch".to_owned()));

        let missing = attest_verify("tack-no-such-prill", ENVELOPE.as_bytes(), KEY, HEAD).await;
        assert_eq!(
            missing,
            Err("prill is not installed on the board".to_owned())
        );
    }
}
