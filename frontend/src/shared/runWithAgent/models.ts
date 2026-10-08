/** Model ids each harness's CLI accepted when measured (docs/plans/measurements/models-<harness>.md).
 *  codex: its account refused every id, so no list. opencode: ids are `<provider>/<model>` and
 *  depend on the operator's configured providers, so no static list. Both offer "Other…" only. */
export const ACCEPTED_MODELS: Record<string, readonly string[]> = {
  'claude-code': [
    'claude-sonnet-5-5', 'claude-opus-5-5', 'claude-fable-5-1', 'claude-haiku-4-5-20251001', 'claude-sonnet-4-5',
    'sonnet', 'opus', 'haiku',
  ],
  codex: [],
  opencode: [],
};
