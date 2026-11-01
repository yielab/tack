import { type Accessor, type Component, For, Show, createSignal } from 'solid-js';
import { HelpHint } from '../../../../shared/ui';
import { HELP } from '../../../../shared/help/texts';
import type { Project, UpdateProject } from '../../../../shared/types';
import type { AgentsOnBoard as Setting, RungLevel } from '../../../../shared/api/changeSets';
import { Block, Heading, Note, inputStyle } from './parts';

/** The levels in the order they widen, each with what an agent may do there. */
export const AGENT_LEVELS: ReadonlyArray<{ value: RungLevel; name: string; sentence: string }> = [
  { value: 'read', name: 'Read', sentence: 'Agents read the board and talk; they change nothing.' },
  { value: 'propose', name: 'Propose', sentence: 'Changes wait for you to apply them.' },
  { value: 'act', name: 'Act', sentence: 'Everything but new items applies by itself: who does it, priority, move, dependencies, comments and due dates. New items wait for you.' },
  { value: 'create', name: 'Create', sentence: 'New items apply by themselves too, up to the two limits below. Off until you choose it.' },
];

/** The most one change set can hold, and the most a day's limit may be: the server refuses more. */
export const MAX_CREATES_PER_CHANGE_SET = 100;
export const MAX_CREATES_PER_DAY = 10_000;

const OFF: Setting = { level: 'propose', creates_per_change_set: 0, creates_per_day: 0 };

/** Why a level cannot be chosen now, or null when it can. A rung is never hidden, only said to be off. */
export function levelOffReason(project: Project | undefined): string | null {
  return project ? null : 'The project has not loaded yet';
}

/**
 * Agents on the board: how far an agent may go. Four rungs, the current one marked, and at Create
 * the two limits on new items. Every change saves at once (`PATCH /projects/{id}`, `agents_on_board`).
 * The stricter of this rung and the agent's own credential wins, and the screen says so.
 */
const AgentsOnBoard: Component<{
  project: Accessor<Project | undefined>;
  save: (patch: UpdateProject) => void | Promise<void>;
}> = (props) => {
  const current = (): Setting => props.project()?.agents_on_board ?? OFF;
  const [saving, setSaving] = createSignal(false);
  const reasonOff = () => levelOffReason(props.project());
  const put = async (patch: Partial<Setting>) => {
    setSaving(true);
    try {
      await props.save({ agents_on_board: { ...current(), ...patch } });
    } finally {
      setSaving(false);
    }
  };
  const capsOff = () => current().level !== 'create';
  const clamp = (raw: string, max: number) => Math.min(max, Math.max(0, Math.trunc(Number(raw) || 0)));

  return (
    <Block id="automation-agents-rung" testid="rung-agents">
      <div class="flex items-center gap-2">
        <Heading>Agents on the board</Heading>
        <HelpHint label="Agents on the board" help={HELP.ladderAgents} />
      </div>
      <div role="radiogroup" aria-label="Agents on the board" class="flex flex-col gap-2">
        <For each={AGENT_LEVELS}>
          {(l) => (
            <label
              data-testid="agents-rung"
              data-level={l.value}
              data-state={current().level === l.value ? 'current' : 'open'}
              class="flex items-start gap-2 text-sm"
              style={{ color: reasonOff() ? 'var(--color-text-tertiary)' : 'var(--color-text-primary)' }}
            >
              <input
                type="radio"
                name="agents-on-board"
                value={l.value}
                checked={current().level === l.value}
                disabled={!!reasonOff() || saving()}
                onChange={() => void put({ level: l.value })}
              />
              <span>
                <span class="font-semibold">{l.name}</span> — {l.sentence}
                <Show when={current().level === l.value}>
                  <span class="ml-2 text-caption font-semibold" data-testid="agents-rung-mark" style={{ color: 'var(--color-accent-ink)' }}>
                    Set here
                  </span>
                </Show>
              </span>
            </label>
          )}
        </For>
        <Show when={reasonOff()}>
          {(why) => <Note testid="agents-rung-reason">{why()}</Note>}
        </Show>
      </div>

      <div class="flex flex-wrap items-center gap-3 text-sm" style={{ color: capsOff() ? 'var(--color-text-tertiary)' : 'var(--color-text-primary)' }}>
        <label class="flex items-center gap-2 whitespace-nowrap">
          New items per change set
          <input
            type="number"
            min="0"
            max={MAX_CREATES_PER_CHANGE_SET}
            aria-label="New items per change set"
            class="w-20 rounded-full border px-3.5 py-2 text-sm"
            style={inputStyle}
            disabled={capsOff() || saving()}
            value={current().creates_per_change_set}
            onChange={(e) => void put({ creates_per_change_set: clamp(e.currentTarget.value, MAX_CREATES_PER_CHANGE_SET) })}
          />
        </label>
        <label class="flex items-center gap-2 whitespace-nowrap">
          New items per day
          <input
            type="number"
            min="0"
            max={MAX_CREATES_PER_DAY}
            aria-label="New items per day"
            class="w-24 rounded-full border px-3.5 py-2 text-sm"
            style={inputStyle}
            disabled={capsOff() || saving()}
            value={current().creates_per_day}
            onChange={(e) => void put({ creates_per_day: clamp(e.currentTarget.value, MAX_CREATES_PER_DAY) })}
          />
        </label>
      </div>
      <Note testid="agents-caps-note">
        <Show
          when={capsOff()}
          fallback={<>Both start at 0, so Create behaves like Act until you raise them. The day counts in UTC.</>}
        >
          Used only at Create. Both are 0 until you set them, so Create is off by default.
        </Show>
      </Note>
      <Note testid="agents-credential-wins">
        The stricter of this rung and an agent&apos;s own credential wins: a credential that may only propose never goes beyond Propose, whatever the rung says.
      </Note>
    </Block>
  );
};

export default AgentsOnBoard;
