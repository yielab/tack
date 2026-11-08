import { type Accessor, type Component, For, Show } from 'solid-js';
import { toast } from '../../../../shared/ui/toast';
import { FieldShell } from '../../../../shared/ui';
import { HELP } from '../../../../shared/help/texts';
import type { AutonomySettings } from '../../../../shared/api/autonomy';
import { Block, DEFAULT_PROVISIONAL_TOKENS, Note, inputClass, inputStyle, spaced } from './parts';

const PROVISIONAL = 'provisional_tokens_per_run';

/** The day bar turns amber when this share of today's limit is counted or reserved. */
const DAY_WARNING = 0.8;

const BUDGETS = [
  { key: 'tokens_per_day', label: 'Tokens per day', integer: true },
  { key: 'auto_merges_per_day', label: 'Automatic merges per day', integer: true },
  { key: 'queue_items_max', label: 'Items waiting at once', integer: true },
] as const;

/**
 * Limits: what the project may spend or queue. Tokens only. Every field saves on
 * change (`PUT /projects/{id}/autonomy`, the whole object, through the ladder's `save`).
 */
const Limits: Component<{
  settings: Accessor<AutonomySettings | null | undefined>;
  save: (patch: Partial<AutonomySettings>) => Promise<void>;
}> = (props) => {
  const settings = props.settings;
  /** Today's token limit, what was counted, what open runs hold back and what is left; null without a daily limit. */
  const day = () => {
    const s = settings();
    const limit = s?.budgets?.tokens_per_day;
    if (!s || !limit || s.day_room === null || s.day_room === undefined) return null;
    const counted = s.day_counted ?? 0;
    const reserved = s.day_reserved ?? 0;
    return { limit, counted, reserved, room: s.day_room, runs: s.day_open_runs ?? 0, warn: (counted + reserved) / limit >= DAY_WARNING };
  };
  const share = (n: number, limit: number) => `${Math.min(100, (n / limit) * 100)}%`;
  const saveBudget = (key: (typeof BUDGETS)[number]['key'] | typeof PROVISIONAL, raw: string, integer: boolean) => {
    const text = raw.trim();
    const n = text === '' ? null : integer ? Math.trunc(Number(text)) : Number(text);
    if (n !== null && (!Number.isFinite(n) || n < 0)) return void toast.error('A limit is a number from 0 up, or empty for none');
    return props.save({ budgets: { ...(settings()?.budgets ?? {}), [key]: n } });
  };

  return (
    <Block id="automation-limits-fields" testid="limits-fields">
      <Show when={settings()} fallback={<Note>The limits load with the autonomy settings.</Note>}>
        {(s) => (
          <>
            <FieldShell label="Daily limits" help={HELP.budgets}>
              <Show when={day()}>
                {(d) => (
                  <div data-testid="day-bar" class="flex flex-col gap-1">
                    <div
                      role="img"
                      aria-label={`${d().counted.toLocaleString()} counted and ${d().reserved.toLocaleString()} reserved of ${d().limit.toLocaleString()} tokens today`}
                      class="flex h-2 overflow-hidden rounded-full"
                      style={{ 'background-color': 'var(--color-border-medium)' }}
                    >
                      <div style={{ width: share(d().counted, d().limit), 'background-color': d().warn ? 'var(--color-warning-600)' : 'var(--color-text-secondary)' }} />
                      <div style={{ width: share(d().reserved, d().limit), 'background-color': d().warn ? 'var(--color-warning-600)' : 'var(--color-text-tertiary)', opacity: 0.55 }} />
                    </div>
                    <p data-testid="day-bar-text" class="text-xs" style={{ color: d().warn ? 'var(--color-warning-600)' : 'var(--color-text-secondary)' }}>
                      Today: {d().counted.toLocaleString()} counted · {d().reserved.toLocaleString()} reserved by {d().runs} open {d().runs === 1 ? 'run' : 'runs'} ·{' '}
                      {d().room.toLocaleString()} left of {d().limit.toLocaleString()}
                    </p>
                    <Note>Tack holds new runs when today's limit is reserved.</Note>
                  </div>
                )}
              </Show>
              <div class="grid grid-cols-2 gap-2">
                <For each={BUDGETS}>
                  {(b) => (
                    <label class="flex min-w-0 flex-col gap-1 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                      {b.label}
                      <input
                        type="number"
                        min="0"
                        class={inputClass}
                        style={inputStyle}
                        aria-label={b.label}
                        placeholder="No limit"
                        value={s().budgets?.[b.key] ?? ''}
                        onChange={(e) => void saveBudget(b.key, e.currentTarget.value, b.integer)}
                      />
                    </label>
                  )}
                </For>
              </div>
            </FieldShell>

            <FieldShell label="Limit for a run whose profile sets none">
              <label class="flex flex-col gap-1 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                Provisional tokens per run
                <input
                  type="number"
                  min="0"
                  class={inputClass}
                  style={inputStyle}
                  aria-label="Provisional tokens per run"
                  placeholder="No limit"
                  value={s().budgets?.provisional_tokens_per_run ?? ''}
                  onChange={(e) => void saveBudget(PROVISIONAL, e.currentTarget.value, true)}
                />
              </label>
              <Note testid="provisional-default">
                Default: <span class="whitespace-nowrap">{spaced(DEFAULT_PROVISIONAL_TOKENS)}</span> tokens, the cap the SWE-Effi benchmark gives one software task.
              </Note>
              <Note>
                Applies to a run whose agent profile has no token limit of its own. Empty means such a run has no limit.
                Once a profile has 5 successful runs, its form proposes a limit from them.
              </Note>
            </FieldShell>
          </>
        )}
      </Show>
    </Block>
  );
};

export default Limits;
