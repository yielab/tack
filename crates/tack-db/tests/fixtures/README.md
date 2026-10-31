# Database fixtures

`tack-0.1.0-beta.9.sqlite` is a database written by the published `v0.1.0-beta.9` binary
(77 migrations applied), used by `tests/migrations/upgrade_from_beta9.rs` to prove that a
database from a release migrates to the current schema with its data and ends up with the schema
a new install gets.

It was made on 2026-10-27 like this, and holds only what was created through that binary's API:

1. `git worktree add --detach <dir> v0.1.0-beta.9`, then `cargo build -p tack-cli` there.
2. `tack serve` on a free loopback port with a throwaway `TACK_DATABASE_URL`, `TACK_STORAGE_DIR`,
   `TACK_API_TOKEN`, `TACK_LOCAL_RUNNER_CONFIG=''` and no keychain.
3. Through `/api`: one project ("Upgrade fixture") with five items (an epic with a child, a task in
   a sprint with a role, a task blocked by another, a bug), three comments, one dependency, one
   sprint, one role, one template saved from the project ("Release checklist"), one agent profile,
   one runner fleet, one pending runner (no credential) and one queued execution request.
4. Stop the server, `PRAGMA wal_checkpoint(TRUNCATE)`, then `VACUUM INTO` a clean single file.

The built-in templates, the vocabulary and the workflow are the ones beta.9 seeds. Nothing in it is
a secret or a local path; the repository is `https://example.test/fixture.git`.

Do not regenerate it with a newer binary: its point is to be old.
