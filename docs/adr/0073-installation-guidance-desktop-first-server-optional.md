# ADR 0073: installation guidance — desktop-first onboarding, server-optional path

**Status: WITHDRAWN as an ADR 2026-10-03 — the work stays, as tasks R1 and R2 of `docs/plans/phase-66.md`.** Complements ADR 0072 (onboarding clarity). Not yet linked to the plan; waiting on the user's review.

**Decide:** approve that the **README, installation docs, and quickstart guides make it clear that Tack has two entry points with different use cases**, and direct most users to the **desktop app first**:

1. **Desktop App** (primary, "what most people want initially"): `tack` binary as a native desktop application (Tauri + embedded server), installed via Homebrew, Windows installer, or `cargo build`. Single binary, full UI, runs locally. This is the default recommendation.

2. **CLI Server** (secondary, "for specific cases"): `tack serve` as a CLI with web UI accessed in a browser. For remote deployment, CI/CD integration, infrastructure where a desktop app is not suitable, or headless environments. Explicitly documented as optional, not the primary path.

The docs and README currently treat both as equal entry points, or bury the desktop app as an option, which leads most users to default to unclear CLI setup and struggle. This ADR is not a code change — it is a **documentation and messaging strategy**.

**Why now:** QA on Phase 66 found that users don't know which installation path to take. The README mentions desktop bundles in release notes but emphasizes the `cargo serve` CLI. Most users want a single app they can launch; few need a server. The confusion cascades: users set up a server they don't need, then the harness setup dialog (ADR 0072) is even more complex because they are running in a terminal instead of a UI.

**If you do nothing:** every new user guesses wrong, wastes time on server setup, and then struggles with harness configuration. The installation docs support both equally, so nobody knows which to choose. Desktop app adoption stays low because the path is not obvious.

## The decisions, in short

| # | Decision | Why |
|---|---|---|
| 1 | **Desktop app is the primary (first-choice) installation.** README, docs/INSTALL.md, and the website lead with the desktop app as "the standard way to use Tack." The CLI server path is clearly documented, but positioned as "for specific use cases" and linked from a separate section. | Most users want a single app to launch; it is the default mental model. Encouraging it first reduces confusion and gets users to the UI faster. |
| 2 | **README has a "Quick Start" section, not "Installation."** "Quick Start" (desktop app, two commands or one click) appears before any "Installation Options" section. This trains the eye to the recommended path. | Section order is UX. Leading with "Install" implies choice; "Quick Start" implies "here's what you do next." |
| 3 | **Installation Options section has three clear subsections:** "Desktop App (Recommended)" with install commands for macOS/Windows/Linux; "CLI Server (For Infrastructure)" with use cases, one-liner setup, and a link to CONFIG.md; "From Source" for developers. No single "choose your adventure" — instead, "here's the recommended path" and "here are alternatives." | Users scan vertically, not horizontally. Putting the recommended first with explicit labeling reduces decision paralysis. |
| 4 | **The docs homepage (or next landing page) mentions desktop first.** If there is a website or docs site built from the book, its first sentence includes "Tack is a desktop app that manages your work …" not "Tack is a project manager with CLI and server …" The desktop-first framing is consistent across docs/INSTALL.md, README, website, and the Tauri window's title bar and icon. | Consistency in messaging; first impression shapes expectations. |
| 5 | **The harness configuration docs (docs/book/src/user-guide/agent-runners.md) assume desktop app.** Examples show "Click 'Run with agent'" and "The Configuration tab" (from ADR 0072). A separate "Running agents from the CLI server" section explains the differences (no GUI, use MCP tool instead, POST /api/executions). | Readers assume the recommended path; alternatives are marked as such. |
| 6 | **The release notes call out desktop bundles as the primary artifact.** When a new version ships, the release notes lead with "Download the app for macOS/Windows/Linux" with direct links to .dmg, .exe, .AppImage or Homebrew. The GitHub Releases page lists `tack-desktop-*` before `tack-cli-*` or `tack-runner-*`. | First place users look is the release notes; download links must be obvious. |
| 7 | **A troubleshooting section in README names the two paths and their gotchas.** "I installed via `cargo serve`, can I use the desktop app?" (Yes, separate install). "I have the desktop app, can I run a harness on a remote machine?" (Yes, configure the runner remotely, then point the app at it). "How do I know which one I have?" (Check: `tack --version` → CLI, or open the app and look at the title bar). | Users get stuck; these are the common confusions. Naming them upfront saves debugging time. |

## Evidence

| Fact | Locator (QA, 2026-09-30) |
|---|---|
| README emphasizes CLI/server setup more than desktop app | `README.md:1-50` mentions "single-binary," "runs locally," but leads with `cargo build` and `tack serve`, not app download |
| Desktop app is available but buried | v0.1.0-beta.9 releases include `tack-desktop-*` but README does not link to them or explain when to use them |
| Users don't know which to install | QA feedback: confusion on whether to use CLI or app; some users download the app but then try to `cargo serve` in the terminal |
| Installation docs treat both as equal | `docs/INSTALL.md` (if it exists) or README has a flat list: "Install via Homebrew" / "Install from source" / "Run via Docker" / "Run the CLI" — no hierarchy |
| Harness configuration assumes the UI | ADR 0072 and existing docs (DecisionInbox.tsx, RunWithAgentModal) reference the desktop UI; no parallel docs for CLI users |
| Desktop app is production-ready | Phase 65 shipped v0.1.0-beta.9 with "the first release with the desktop bundles" (phase-65.md, 2026-09-21) |

## Related decisions

| Earlier decision | How this ADR aligns |
|---|---|
| Phase 65: "v0.1.0-beta.9 shipped the first release with the desktop bundles" | Bundles exist; this ADR surfaces them. |
| ADR 0072 (harness configuration): assumes desktop UI | This ADR supports the desktop-first path that 0072 designs for. |
| CLAUDE.md "desktop app direction": "Tack becomes a desktop app supervising a background service" (ADR 0062, accepted 2026-09-03) | This ADR is the public-facing articulation of that decision. |

## What this is NOT

- **Not a code change.** No new features, no architecture. Pure documentation.
- **Not removing the CLI.** The server path remains, documented and supported.
- **Not changing the build or release process.** Only the messaging in the docs.
- **Not a migration from CLI users.** Existing CLI users can continue; new users are guided to the app.

## Cut and deferred

| Item | Status | Why |
|---|---|---|
| A guided setup wizard in the desktop app | Deferred | Trigger: Phase 66 ADR 0072 onboarding is implemented; then a wizard can guide through the Configuration tab. |
| A "Welcome" landing page on first launch | Deferred | Trigger: same; the onboarding checklist (ADR 0072 Decision 1) is the foundation. |
| Comparison matrix (Desktop vs. CLI) | Deferred | Trigger: the CLI gains features or constraints that are worth documenting side-by-side (e.g., MCP-only, no GUI). |

## Questions for the maintainer (blocking Wave 0.1.1)

1. **Should the website / docs landing page exist before Phase 66 ships, or after?** This ADR assumes there is a place to implement "desktop-first" messaging (a docs home, a website, or at least a clear README structure). If none exists, when should it be built?

2. **Are there CLI-only use cases that should be documented explicitly?** (e.g., "use the CLI server if you are deploying to a team's infrastructure"). What are the canonical use cases?

3. **Should the desktop app have a built-in "open server" option** to bridge both paths? (e.g., a checkbox "Use a remote server" in Settings, defaulting to localhost).

## Amendments

*(Appended by later readers, dated. The text above is never rewritten.)*

**2026-10-02 — review against `develop` at `7718420`.** The premise does not hold on the tree.
`README.md:116-150` ("Get started") already leads with **Desktop app** and the
`.AppImage`/`.deb`/`.dmg`/`.msi` links, then the one-liner, then Homebrew and Windows (commit
`d6dbb55`, 2026-09-27, "one-screen Install section"); `README.md:1-50` leads with the board
and the agents, not with `cargo build`. `docs/INSTALL.md` does not exist and nothing points to
it. Decision 5 assumes ADR 0072's Configuration tab, which is not built (ADR 0072, amendment
of the same date). Decision 7's "point the app at a remote server" names a setting the
desktop crate does not have (`rg -i "remote|server_url" crates/tack-desktop/src` finds only
the database path). Decisions 1–4 are the current state; decision 6 is one line in
`docs/LAUNCH-CHECKLIST.md`; of decision 7 the true answers remain. What survives is R1 in
`docs/plans/phase-66.md`: a Haiku doc task, two files. Recommendation: reject as an ADR —
there is no decision left to take — and keep R1 (plan question 6). The three questions fall
away; question 3, a remote-server option in the app, would be a product ADR of its own with
its trigger.

**2026-10-03 — the maintainer: this is not an ADR.** It is an improvement of the pitch and the
voice: the desktop app is the default, and the text says what the server form is for. The
landing page already exists and needs a content refactor and the release download links.
Withdrawn as a decision record; R1 (README, launch checklist) and R2 (the landing page, in the
studio site's repository) carry the work.
