# evidence-v1

What an attempt changed, read from its workspace before the workspace is deleted, for every
harness kind. The runner stages three files from a scratch directory outside the workspace and
uploads each through the ordinary artifact manifest-then-content path:

| File | Artifact kind | Media type | Content |
|---|---|---|---|
| `changes.patch` | `patch` | `text/x-diff` | `git diff --cached --binary <base_revision>`, cut at 8 MiB (`patch.truncated`) |
| `files.json` | `files` | `application/json` | `[{path, op}]`, `op` one of `added`, `modified`, `deleted`, `renamed` |
| `evidence.json` | `evidence` | `application/json` | the manifest below; `schema.json` is its shape |

Capture first runs `git add -A` in the disposable clone (excluding `.tack-runner/` and the
runner's own markers), which is how untracked files and deletions enter one diff. A workspace
that git cannot read, or a provisioner with no repository, yields only `evidence.json` with
`captured: false` and a `reason`; the attempt's outcome never changes. A cancelled attempt is
captured too. A renamed file is listed under its new path.

`attempt-evidence.example.json` is the frozen example. `crates/tack-runner/tests/evidence_contract.rs`
round-trips it through `tack_runner::evidence::AttemptEvidence` and pins its bytes (FNV-1a 64).
Changing the shape means changing the example first, then the type, then the pin.
