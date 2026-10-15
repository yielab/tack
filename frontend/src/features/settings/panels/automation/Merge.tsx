import { type Accessor, type Component, For, Show } from 'solid-js';
import { toast } from '../../../../shared/ui/toast';
import { FieldShell, HelpHint } from '../../../../shared/ui';
import { HELP } from '../../../../shared/help/texts';
import type { AutonomyDomain, AutonomySettings } from '../../../../shared/api/autonomy';
import { Heading, Note, SECTION, inputClass, inputStyle } from './parts';

const LEVELS = [
  { value: 'L3', sentence: 'You merge every change yourself.' },
  { value: 'L4', sentence: 'Tack merges changes that stay inside the domains below.' },
  { value: 'L5', sentence: 'Tack merges any change that passes its checks.' },
] as const;

/** prill's L4 example for the stages it must run. */
const PRILL_STAGES = ['criteria', 'suite', 'anti_tautology', 'static_analysis'];

const list = (text: string) => text.split(/[,\n]/).map((x) => x.trim()).filter(Boolean);

const BUDGETS = [
  { key: 'tokens_per_day', label: 'Tokens per day', integer: true },
  { key: 'usd_per_day', label: 'Dollars per day', integer: false },
  { key: 'auto_merges_per_day', label: 'Automatic merges per day', integer: true },
  { key: 'queue_items_max', label: 'Items waiting at once', integer: true },
] as const;


/**
 * Rung 3, Merge: how much Tack may merge by itself. Every field saves on change
 * (`PUT /projects/{id}/autonomy`, the whole object, through the ladder's `save`).
 */
const Merge: Component<{ settings: Accessor<AutonomySettings | null | undefined>; save: (patch: Partial<AutonomySettings>) => Promise<void> }> = (props) => {
  const settings = props.settings;
  const save = props.save;
  const blocked = () => settings()?.automatic_merge_blocked_by ?? [];
  const blockedWhy = () => blocked().join('. ');
  const saveDomain = (index: number, patch: Partial<AutonomyDomain>) =>
    save({ domains: (settings()?.domains ?? []).map((d, i) => (i === index ? { ...d, ...patch } : d)) });
  const addDomain = () =>
    save({ domains: [...(settings()?.domains ?? []), { name: `domain-${(settings()?.domains?.length ?? 0) + 1}`, path_globs: [], item_types: [], level: 'L3' }] });
  const saveBudget = (key: (typeof BUDGETS)[number]['key'], raw: string, integer: boolean) => {
    const text = raw.trim();
    const n = text === '' ? null : integer ? Math.trunc(Number(text)) : Number(text);
    if (n !== null && (!Number.isFinite(n) || n < 0)) return void toast.error('A limit is a number from 0 up, or empty for none');
    return save({ budgets: { ...(settings()?.budgets ?? {}), [key]: n } });
  };

  return (
    <div class={SECTION} id="automation-autonomy" data-testid="rung-merge">
      <div class="flex items-center gap-2">
        <Heading>3. Merge</Heading>
        <HelpHint label="Autonomy" help={HELP.autonomyLevel} />
      </div>
      <Show when={settings()} fallback={<Note>Autonomy settings are not available yet.</Note>}>
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
              <label class="flex items-center gap-2 text-sm" style={{ color: 'var(--color-text-primary)' }}>
                <input
                  type="checkbox"
                  role="switch"
                  aria-label="Merge when moved to Done"
                  checked={s().merge_on_done}
                  onChange={(e) => void save({ merge_on_done: e.currentTarget.checked })}
                />
                Merge when moved to Done
              </label>
              <HelpHint label="Merge when moved to Done" help={HELP.mergeOnDone} />
            </div>

            <FieldShell label="Domains">
              <div class="flex flex-col gap-3">
                <For each={s().domains} fallback={<Note>No domains yet. L4 merges only inside a domain.</Note>}>
                  {(d, i) => (
                    <div data-testid="autonomy-domain" class="flex flex-col gap-1.5 rounded-[20px] border p-3" style={{ 'border-color': 'var(--color-border-medium)' }}>
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

            <FieldShell label="Stages prill must run" help={HELP.verifyRequire}>
              <div class="flex flex-col gap-1.5">
                <input
                  class={inputClass}
                  style={inputStyle}
                  aria-label="Stages prill must run"
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
                    Fill prill's example
                  </button>
                </div>
              </div>
            </FieldShell>

            <FieldShell label="Public key">
              <textarea
                aria-label="Public key"
                rows={3}
                class="w-full resize-none rounded-[20px] border px-3.5 py-2 text-sm"
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
          </>
        )}
      </Show>
    </div>
  );
};

export default Merge;
