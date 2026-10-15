import { type Accessor, type Component, For, Show } from 'solid-js';
import { HelpHint } from '../../../../shared/ui';
import { HELP } from '../../../../shared/help/texts';
import type { Project, UpdateProject, Role } from '../../../../shared/types';
import { Heading, Note, SECTION, inputClass, inputStyle } from './parts';

interface Props {
  project: Accessor<Project | undefined>;
  roles: Accessor<Role[]>;
  statuses: Accessor<{ name: string; order: number; category?: string }[]>;
  save: (patch: UpdateProject) => void | Promise<void>;
}

export const NO_PROFILE_REASON = 'Choose a profile for at least one role in Teams';

/** Rung 2: a task starts by itself when it reaches a status. */
const Start: Component<Props> = (props) => {
  const hasProfile = () => props.roles().some((r) => !!r.agent_profile_id) || !!props.project()?.default_profile_id;
  const status = () =>
    props.project()?.auto_start_status ??
    (props.statuses().find((s) => s.category === 'todo') ?? props.statuses()[0])?.name ?? '';

  return (
    <div class={SECTION} id="automation-start" data-testid="rung-start">
      <div class="flex items-center gap-2">
        <Heading>2. Start</Heading>
        <HelpHint label="Start" help={HELP.ladderStart} />
      </div>
      <label class="flex items-center gap-2 text-sm" style={{ color: hasProfile() ? 'var(--color-text-primary)' : 'var(--color-text-tertiary)' }}>
        <input
          type="checkbox"
          role="switch"
          aria-label="Start tasks by themselves"
          checked={!!props.project()?.auto_start_enabled}
          disabled={!hasProfile()}
          onChange={(e) => void props.save({ auto_start_enabled: e.currentTarget.checked })}
        />
        Start a task by itself
      </label>
      <Show when={!hasProfile()}>
        <Note testid="start-reason">{NO_PROFILE_REASON}</Note>
      </Show>
      <div class="flex flex-wrap items-center gap-3 text-sm" style={{ color: 'var(--color-text-primary)' }}>
        <label class="flex items-center gap-2">
          When a task reaches
          <select
            aria-label="Start status"
            class="rounded-full border px-3 py-1 text-sm"
            style={inputStyle}
            disabled={!hasProfile()}
            onChange={(e) => void props.save({ auto_start_status: e.currentTarget.value })}
          >
            <For each={props.statuses()}>{(s) => <option value={s.name} selected={s.name === status()}>{s.name}</option>}</For>
          </select>
        </label>
        <label class="flex items-center gap-2">
          Keep at most
          <input
            type="number"
            min="1"
            aria-label="Runs open at once"
            class={`${inputClass} w-20`}
            style={inputStyle}
            disabled={!hasProfile()}
            value={props.project()?.auto_start_max_open ?? 1}
            onChange={(e) => void props.save({ auto_start_max_open: Math.max(1, Math.trunc(Number(e.currentTarget.value) || 1)) })}
          />
          runs open
        </label>
      </div>
      <Show when={props.project()?.auto_start_disabled_reason}>
        {(why) => <Note testid="start-disabled-reason">{why()}</Note>}
      </Show>
    </div>
  );
};

export default Start;
