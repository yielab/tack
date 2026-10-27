import { type Accessor, type Component, For, Show } from 'solid-js';
import { toast } from '../../../../shared/ui/toast';
import { FieldShell, HelpHint, LoadError, Switch } from '../../../../shared/ui';
import { HELP } from '../../../../shared/help/texts';
import type { AutonomyDomain, AutonomySettings } from '../../../../shared/api/autonomy';
import { Heading, Note, SECTION, inputClass, inputStyle } from './parts';

const LEVELS = [
  { value: 'L3', sentence: 'You merge every change yourself.' },
  { value: 'L4', sentence: 'Tack merges changes that stay inside the domains below.' },
  { value: 'L5', sentence: 'Tack merges any change that passes its checks.' },
] as const;

const PROVISIONAL = 'provisional_tokens_per_run';

/** The verifier's L4 example for the stages it must run. */
const PRILL_STAGES = ['criteria', 'suite', 'anti_tautology', 'static_analysis'];

const list = (text: string) => text.split(/[,\n]/).map((x) => x.trim()).filter(Boolean);

/** The day bar turns amber when this share of today's limit is counted or reserved. */
const DAY_WARNING = 0.8;

const BUDGETS = [
  { key: 'tokens_per_day', label: 'Tokens per day', integer: true },
  { key: 'auto_merges_per_day', label: 'Automatic merges per day', integer: true },
  { key: 'queue_items_max', label: 'Items waiting at once', integer: true },
] as const;


/**
 * Rung 3, Merge: how much Tack may merge by itself. Every field saves on change
 * (`PUT /projects/{id}/autonomy`, the whole object, through the ladder's `save`).
 */
const Merge: Component<{
  settings: Accessor<AutonomySettings | null | undefined>;
  /** Set when the settings did not load: they are not "unavailable". */
  loadError: Accessor<unknown>;
  onRetry: () => void;
  save: (patch: Partial<AutonomySettings>) => Promise<void>;
}> = (props) => {
  const settings = props.settings;
  const save = props.save;
  const blocked = () => settings()?.automatic_merge_blocked_by ?? [];
  const blockedWhy = () => blocked().join('. ');
  const saveDomain = (index: number, patch: Partial<AutonomyDomain>) =>
    save({ domains: (settings()?.domains ?? []).map((d, i) => (i === index ? { ...d, ...patch } : d)) });
  const addDomain = () =>
    save({ domains: [...(settings()?.domains ?? []), { name: `domain-${(settings()?.domains?.length ?? 0) + 1}`, path_globs: [], item_types: [], level: 'L3' }] });
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
    return save({ budgets: { ...(settings()?.budgets ?? {}), [key]: n } });
  };

  return (
    <div class={SECTION} id="automation-autonomy" data-testid="rung-merge">
      <div class="flex items-center gap-2">
        <Heading>Merge</Heading>
        <HelpHint label="Autonomy" help={HELP.autonomyLevel} />
      </div>
      <Show
        when={settings()}
        fallback={
          <Show when={props.loadError()} fallback={<Note>Loading the autonomy settings…</Note>}>
            {(error) => <LoadError what="the autonomy settings" error={error()} onRetry={props.onRetry} />}
          </Show>
        }
      >
        {(s) => (
          <>
            <div class="flex flex-col gap-2">
              <For each={LEVELS}>
                {(l) => {
                  const off = () => l.value !== 'L3' && blocked().length > 0;
                  return (
                    <label class="flex items-start gap-2 text-sm" style={{ color: off() ? 'var(--color-text-tertiary)' : 'var(--color-text-primary)' }}>
                      <input
                        type="radio"
                        name="autonomy-level"
                        value={l.value}
                        checked={s().level === l.value}
                        disabled={off()}
                        onChange={() => void save({ level: l.value })}
                      />
                      <span>
                        <span class="font-semibold">{l.value}</span> — {l.sentence}
                      </span>
                    </label>
                  );
                }}
              </For>
              <Show when={blocked().length > 0}>
                <p data-testid="autonomy-blocked" class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                  L4 and L5 are off: {blockedWhy()}
                </p>
              </Show>
            </div>

            <div class="flex items-center gap-2">
              <Switch
                ariaLabel="Merge when moved to Done"
                checked={s().merge_on_done}
                onChange={(checked) => void save({ merge_on_done: checked })}
                style={{ color: 'var(--color-text-primary)' }}
              >
                Merge when moved to Done
              </Switch>
              <HelpHint label="Merge when moved to Done" help={HELP.mergeOnDone} />
            </div>

            <FieldShell label="Domains">
              <div class="flex flex-col gap-3">
                <For each={s().domains} fallback={<Note>No domains yet. L4 merges only inside a domain.</Note>}>
                  {(d, i) => (
                    <div data-testid="autonomy-domain" class="flex flex-col gap-1.5 rounded-item border p-3" style={{ 'border-color': 'var(--color-border-medium)' }}>
                      <input
                        class={inputClass}
                        style={inputStyle}
                        aria-label="Domain name"
                        value={d.name}
                        onChange={(e) => void saveDomain(i(), { name: e.currentTarget.value.trim() })}
                      />
                      <input
                        class={inputClass}
                        style={inputStyle}
                        aria-label="Paths it covers"
                        placeholder="docs/**, README.md"
                        value={d.path_globs.join(', ')}
                        onChange={(e) => void saveDomain(i(), { path_globs: list(e.currentTarget.value) })}
                      />
                      <input
                        class={inputClass}
                        style={inputStyle}
                        aria-label="Item types it covers"
                        placeholder="Any type"
                        value={(d.item_types ?? []).join(', ')}
                        onChange={(e) => void saveDomain(i(), { item_types: list(e.currentTarget.value) })}
                      />
                      <div class="flex flex-wrap items-center gap-3">
                        <span data-testid="domain-counts" class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                          {d.merged ?? 0} merged · {d.undone ?? 0} undone
                        </span>
                        <select
                          aria-label="Domain level"
                          class="rounded-full border px-3 py-1 text-sm"
                          style={inputStyle}
                          onChange={(e) => void saveDomain(i(), { level: e.currentTarget.value })}
                        >
                          <For each={LEVELS}>
                            {(l) => (
                              <option value={l.value} selected={d.level === l.value} disabled={l.value !== 'L3' && blocked().length > 0}>
                                {l.value}
                              </option>
                            )}
                          </For>
                        </select>
                        <button
                          type="button"
                          class="text-xs underline"
                          style={{ color: 'var(--color-text-secondary)' }}
                          onClick={() => void save({ domains: s().domains.filter((_, j) => j !== i()) })}
                        >
                          Remove
                        </button>
                      </div>
                    </div>
                  )}
                </For>
                <div>
                  <button type="button" class="rounded-full border px-3 py-1 text-sm" style={inputStyle} onClick={() => void addDomain()}>
                    Add a domain
                  </button>
                </div>
              </div>
            </FieldShell>

            <FieldShell label="Stages the verifier must run" help={HELP.verifyRequire}>
              <div class="flex flex-col gap-1.5">
                <input
                  class={inputClass}
                  style={inputStyle}
                  aria-label="Stages the verifier must run"
                  value={s().verify_require.join(', ')}
                  onChange={(e) => void save({ verify_require: list(e.currentTarget.value) })}
                />
                <div>
                  <button
                    type="button"
                    class="rounded-full border px-3 py-1 text-sm"
                    style={inputStyle}
                    onClick={() => void save({ verify_require: PRILL_STAGES })}
                  >
                    Fill the example stages
                  </button>
                </div>
              </div>
            </FieldShell>

            <FieldShell label="Public key">
              <textarea
                aria-label="Public key"
                rows={3}
                class="w-full resize-none rounded-item border px-3.5 py-2 text-sm"
                style={{ ...inputStyle, 'font-family': 'var(--font-mono)' }}
                value={s().attestation_public_key ?? ''}
                onChange={(e) => void save({ attestation_public_key: e.currentTarget.value.trim() || null })}
              />
              <Note>The key the signed pack is checked against, in PEM form.</Note>
            </FieldShell>

            <FieldShell label="Times a rejected change goes back to the agent" help={HELP.loopCap}>
              <input
                type="number"
                min="0"
                class={inputClass}
                style={inputStyle}
                aria-label="Times a rejected change goes back to the agent"
                value={s().verify_loop_max_attempts}
                onChange={(e) => void save({ verify_loop_max_attempts: Math.max(0, Math.trunc(Number(e.currentTarget.value) || 0)) })}
              />
            </FieldShell>

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
                    <label class="flex flex-col gap-1 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
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
              <Note>
                Applies to a run whose agent profile has no token limit of its own. The default, 2 000 000 tokens, is the
                cap the SWE-Effi benchmark gives one software task. Empty means such a run has no limit. Once a profile has
                5 successful runs, its form proposes a limit from them.
              </Note>
            </FieldShell>
          </>
        )}
      </Show>
    </div>
  );
};

export default Merge;
