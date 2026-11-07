# Contributing to Tack

Thanks for your interest in Tack. This guide gets you from a clone to a pull request: setting up,
the checks a change must pass, and how changes are reviewed. All participation is governed by the
[Code of Conduct](CODE_OF_CONDUCT.md); project decisions follow the
[governance model](GOVERNANCE.md) (one maintainer).

How the code is organised, and the recipes for new code, are in the book's
[Developer Guide](https://yielab.github.io/tack/developer/index.html), not here.

## Report a bug or ask for a feature

Issues are tracked on [GitHub Issues](https://github.com/yielab/tack/issues).

- **Bugs:** open a [bug report](https://github.com/yielab/tack/issues/new?template=bug_report.yml)
  with the Tack version or commit, your OS, the steps to reproduce, what you expected and what
  happened. A minimal reproduction is the most helpful thing you can give.
- **Features and ideas:** open a
  [feature request](https://github.com/yielab/tack/issues/new?template=feature_request.yml), or
  start a thread in [Discussions](https://github.com/yielab/tack/discussions) if it is open-ended.
  Describe the problem, not only the solution; it helps keep Tack small and focused.
- **Security vulnerabilities:** do not open a public issue. Follow [SECURITY.md](SECURITY.md).

For anything beyond a trivial fix, open (or find) an issue before writing code, so the approach is
agreed first.

## Set up

You need [Rust](https://rustup.rs/) (1.94 or later), [Node.js](https://nodejs.org/) 22 or later,
git, and `cargo-nextest` (`cargo install cargo-nextest --locked`). No database, Docker or other
service is needed: SQLite is embedded.

```bash
git clone https://github.com/yielab/tack.git
cd tack
./scripts/setup-git.sh       # once per clone: the pre-push hook and the merge driver for generated files
make dev                     # API on :3210 plus the Vite dev server with hot reload
```

`make help` lists the other targets. The ones you will use most:

| Command | Does |
| --- | --- |
| `make dev` | The server and the web UI with hot reload. |
| `cargo run -p tack-cli -- serve` | The server alone, at <http://127.0.0.1:3210>. |
| `make test` | `cargo nextest run --workspace`. Details and filters: [docs/TESTING.md](docs/TESTING.md). |
| `make e2e` | Browser tests (Playwright). `make e2e-install` downloads the browsers once. |
| `make coverage` | Rust and frontend coverage against CI's floors. |
| `make deny` | The license and duplicate-dependency check CI runs. |
| `make build` | The release `tack` binary with the UI embedded. |
| `make desktop` | The Tauri desktop app (a workspace of its own). |

`rust-toolchain.toml` pins the exact compiler you and CI use; rustup installs it the first time you
run `cargo` here. Do not override it. A distro or Homebrew `cargo` earlier on `PATH` ignores the
pin, so the pre-push hook puts rustup's first and refuses any other version. Dependabot proposes
bumping the pin.

`setup-git.sh` also installs the `tack-generated` merge driver for `Cargo.lock`,
`frontend/package-lock.json`, `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts` and the
book's `developer/api-reference.md`. Each is a pure function of sources tracked elsewhere, so the
driver takes your side without a conflict and `.githooks/post-merge` regenerates it. By hand:
`./scripts/regen-generated.sh`. Never edit those files yourself.

## Make a change

1. **Discuss first** for non-trivial work and link the pull request to the issue. Docs, tests and
   small self-contained fixes can go straight to a pull request.
2. **Branch off `develop`** and open the pull request against `develop`. Keep one logical change per
   pull request.
3. **Write tests** for new behaviour and for a bug fix (a test that fails before the change and
   passes after). Where a test lives and how big it may be: [docs/TESTING.md](docs/TESTING.md).
4. **Classify the change.** Does it touch what the agent reads or receives: the prompt and the fence
   around a card's text, what a run is given, a profile's default tools and approvals, the plan
   instructions, the brief sent to the verifier, the manager's digest and threads, the onboarding
   skill? A change to what the agent reads or receives names its suite, its threshold and its baseline, and writes the task first when none captures it. Put that in an EDD block at the top of the pull request; the lane and how to run it are in
   [docs/TESTING.md](docs/TESTING.md#evals). A change that touches none of those needs a test, not an eval.
5. **Run the gate before pushing.** The pre-push hook is the definition of the local gate, so run
   it rather than a list of commands typed from memory:

   ```bash
   ./.githooks/pre-push
   cargo nextest run --workspace
   ```

   The hook checks formatting (the desktop workspace too), clippy with warnings denied, the crate
   dependency graph, the boundary test suites, that the generated files are current, and, when
   `frontend/node_modules` exists, the TypeScript build and the frontend tests your push touches.
   It does not run the whole suite, so run `nextest` yourself. It runs automatically on `git push`
   once `setup-git.sh` has run.

   If you touched the frontend, CI also runs `cd frontend && npm run type-check && npm test`,
   `npm run lint:tokens` (no raw color literals; colors come from `--color-*` tokens) and
   `npm run build` (the entry bundle stays under 30 KB gzipped).
6. **Update the docs and the changelog.** If you changed behaviour, configuration or the API, change
   the matching docs in the same pull request and add a line under `[Unreleased]` in
   [CHANGELOG.md](CHANGELOG.md) (Added, Changed, Removed, Fixed or Security). The changelog is
   written by hand for someone upgrading: say what they can now do or what behaves differently, not
   which file moved. When you change an API response shape, update the frontend types and test mocks
   with it.
7. **Describe the pull request:** what changed and why, how you tested it, and any follow-ups. The
   [template](.github/PULL_REQUEST_TEMPLATE.md) has the checklist.
8. **Write the commit subject as one plain sentence after a verb:** `Add:` for something new,
   `Fix:` for a bug, `Change:` for different behaviour, `Clean:` for code or docs removed with no
   change in behaviour, `Chore:` for regenerated files and housekeeping, `Docs:` for documentation only, `Release:` for a version bump. Say what a user
   or maintainer can now do or no longer suffers, for example `Fix: an open "Run with agent" dialog
   survives the board's realtime refetch`. No AI-attribution lines (no `Co-Authored-By` trailers).
9. **Review.** The maintainer reviews, may ask for changes, and merges once CI is green. Every job
   in `.github/workflows/ci.yml` that runs on a pull request must pass.

By submitting a pull request you agree that your contribution is licensed under the project's
[MIT License](LICENSE).

## Decisions and scope

A non-trivial design choice is recorded as an ADR under `docs/adr/`, not argued out across commit
messages or pull-request comments. Write it for someone who was not in the room: the first screen
says, in plain language, what is being decided, why it needs deciding now, and what stays blocked if
nobody accepts it. The rejected alternatives and the evidence go in a separate section below it.

Do not build a mechanism (a column, a trait, a config flag, a route, a module) that nothing in the
same change calls. Code with no caller is unverified surface with a maintenance cost. Build for the
second concrete case you have, not a hypothetical third.

## Branches and releases

| Branch | Role |
| --- | --- |
| `main` | The release branch. Tagged releases (`vX.Y.Z`) are cut from it. |
| `develop` | The integration branch. Work lands here first. |
| `feat/…`, `fix/…`, `docs/…` | Short-lived topic branches for one change. |

Rules on `main` and `develop` require green CI on pull requests and forbid force-pushes and
deletion. They require no reviewer approval, and the maintainer can push to `develop` directly,
because there is one maintainer and the pre-push hook is the gate for that path; revisit both
settings the day a second person gets write access. CI runs on pushes to `main`, `develop` and
`claude/**` and on every pull request. `tack branch <item-id>` makes a topic-branch name from a
Tack item if you track your work in Tack.

Tagging and publishing a release is the maintainer's step, written in
[docs/LAUNCH-CHECKLIST.md](docs/LAUNCH-CHECKLIST.md).

## Code style

- `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings` clean; the hook
  catches both.
- `tack-core`, `tack-protocol` and `tack-orch` do no I/O. `sqlx` appears only in `tack-db`, and
  `axum` only in `tack-api`. `scripts/check-crate-graph.sh` enforces the crate edges; the rules and
  the reasons are in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
- Prefer returning `Result` over panicking.
- Unsupported is typed, unknown is explicit, unmeasured is nullable. No `unimplemented!()`, and no
  zero standing in for "unknown": never render `$0.00` for money nobody measured; the literal is
  `Not measured`.
- Errors, logging and the secrets-in-backups rule: [Adding Features](https://yielab.github.io/tack/developer/adding-features.html#conventions-every-change-follows).

### Comments

Comments explain the code, never the project's history. Write what the code does when the name
doesn't say it, why a non-obvious choice was made, what breaks if you change it, and what isn't true
yet (an unwired column, a mechanism with no caller). Don't write dates, attributions,
commented-out code, or a narrative of how the code got here: `git log` holds that.
`scripts/check-comments.sh` (in CI) enforces the one part a script can check: a comment that cites a
filename must cite a file that still exists. When it fires, repoint the citation or restate the fact
it stood in for; deleting the comment is almost always the wrong fix.
