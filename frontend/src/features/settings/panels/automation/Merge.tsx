import { type Accessor, type Component, For, Show } from 'solid-js';
import { FieldShell, HelpHint, LoadError, Switch } from '../../../../shared/ui';
import { HELP } from '../../../../shared/help/texts';
import type { AutonomyDomain, AutonomySettings } from '../../../../shared/api/autonomy';
import { Block, Heading, LEVELS, Note, PUBLIC_KEY_ID, inputClass, inputStyle } from './parts';

/** The verifier's L4 example for the stages it must run. */
const PRILL_STAGES = ['criteria', 'suite', 'anti_tautology', 'static_analysis'];

const list = (text: string) => text.split(/[,\n]/).map((x) => x.trim()).filter(Boolean);

/**
 * Merge: how much Tack may merge by itself. Every field saves on change
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
  const saveDomain = (index: number, patch: Partial<AutonomyDomain>) =>
    save({ domains: (settings()?.domains ?? []).map((d, i) => (i === index ? { ...d, ...patch } : d)) });
  const addDomain = () =>
    save({ domains: [...(settings()?.domains ?? []), { name: `domain-${(settings()?.domains?.length ?? 0) + 1}`, path_globs: [], item_types: [], level: 'L3' }] });

  return (
    <Block id="automation-autonomy" testid="rung-merge">
      <div class="flex items-center gap-2">
        <Heading>Merge rules</Heading>
        <HelpHint label="Autonomy" help={HELP.autonomyLevel} />
      </div>
      <Note>The level is chosen on the ladder above.</Note>
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
                id={PUBLIC_KEY_ID}
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
          </>
        )}
      </Show>
    </Block>
  );
};

export default Merge;
