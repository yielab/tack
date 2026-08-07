# III-A2 handoff

- Base SHA / branch / final SHA: `1d71785` / `agent/iii-a2-atomic` / the commit containing this handoff.
- Files changed (must equal ownership list): `crates/tack-core/src/models.rs`; `crates/tack-db/src/repo/items.rs`; `crates/tack-api/src/handlers/items.rs`; focused item-concurrency tests; `frontend/src/shared/api/{client,items}.ts`; `frontend/src/features/item-detail/ItemDetailDrawer.tsx`; focused frontend API test; this handoff.
- Contract fixtures consumed: none (this is a Phase-50 repair of the existing item HTTP contract).
- Behavior implemented: PATCH now has one `BEGIN IMMEDIATE` transaction for its version guard, WIP decision, status timestamp bookkeeping, fields and one version increment. PATCH and GET use coherent item/version snapshots; successful PATCH returns the matching ETag. `description`, `assignee`, and `estimate` now distinguish absent from JSON `null`, so null clears the column. The browser fetches/caches the ETag before every item mutation, sends `If-Match`, stores the returned ETag, and presents 412 as a deliberate refresh-and-review retry.
- Tests added and exact commands/results: `cargo test -p tack-api --test item_concurrency_test` — 11 passed; `cargo test -p tack-db --test version_concurrency_test --test integration_test --no-fail-fast` — 31 passed; `cargo check -p tack-api` — passed; `git diff --check` — passed.
- Failure/adversarial case proved: same-ETag racers yield exactly one 200/one 412; a WIP-rejected multi-field PATCH leaves every field and version unchanged; a SQLite `BEFORE UPDATE` injected failure leaves a multi-field PATCH wholly unchanged; nullable field clears and body/ETag version agreement are asserted.
- Schema/API/contract change requested from another owner: generated OpenAPI/TypeScript schema still describes these nullable PATCH fields correctly and is intentionally untouched per III.2 rule 5. No route or migration change is needed.
- Known limitations or `not_measured` fields: frontend `npm test -- --run src/shared/api/resources.test.ts` and `npm run type-check` could not run because this isolated worktree has no `frontend/node_modules` (`vitest: not found`).
- Secrets/logging review: no credentials, prompts, query strings, or complete environment values were added to logs/tests.
- Safe merge order and likely conflicts: merge after/with A1 only if its handler edits are rebased; this changes the owned item handler/repository/model and frontend shared API surfaces, not router/migrations/openapi.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.
