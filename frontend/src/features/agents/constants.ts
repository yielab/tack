// Every console-step command string these steps introduce, in one place —
// so a docs page can cite this file instead of a string embedded in JSX.

/** Per-harness vendor install command, shown when step 2's probe reports the
 *  binary absent from `PATH`. Verified against the vendor's own published
 *  package (`@anthropic-ai/claude-code`, `@openai/codex`, the `opencode`
 *  Homebrew formula) rather than guessed. Keyed by the wire `harness_kind`
 *  values `shared/runWithAgent/shared.ts#HARNESS_KINDS` already establishes.
 *  A harness with no one-line command (docket) is in
 *  {@link HARNESS_INSTALL_NOTE} instead. */
export const HARNESS_INSTALL_COMMAND: Readonly<Record<string, string>> = {
  'claude-code': 'npm install -g @anthropic-ai/claude-code',
  codex: 'npm install -g @openai/codex',
  opencode: 'brew install opencode',
};

/** Per-harness install hint for a harness with no one-line install command:
 *  a sentence to read, not a command to paste. The wording is the user
 *  guide's (`docs/book/src/user-guide/agent-runners.md`, "Choosing a
 *  harness"). */
export const HARNESS_INSTALL_NOTE: Readonly<Record<string, string>> = {
  docket: 'From the docket project, following its own install instructions.',
};
