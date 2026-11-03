## What

<!-- One-sentence description of the change. -->

## Why

<!-- Motivation: bug fix, new feature, refactor? -->

## Related issue

<!-- "Closes #123", or "N/A" for a small self-contained fix (docs, tests, typo) that
     CONTRIBUTING.md's PR process says can skip the discuss-first step. -->

## How to test

<!-- Steps to verify this works — curl commands, UI flow, or test name. A new test you
     added counts; this section shouldn't just restate the checklist below. -->

## Checklist

- [ ] `.githooks/pre-push` passes (fmt, clippy, crate graph, boundary suites, generated files)
- [ ] `cargo nextest run --workspace` passes (never `cargo test --workspace`; see
      `docs/TESTING.md`)
- [ ] Frontend changed? `cd frontend && npm run type-check && npm test` passes, and
      `npm run gen:api` produces no diff in `schema.gen.ts` if the API response shape
      changed
- [ ] Docs updated if this changes behavior, config, or the API, and a line added under
      `[Unreleased]` in `CHANGELOG.md` (it is written by hand)
- [ ] Commit messages have no AI-attribution trailers (see `CONTRIBUTING.md`)
