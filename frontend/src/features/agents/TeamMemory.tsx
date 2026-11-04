import { type Component, For, Show, createUniqueId } from 'solid-js';
import { Badge, Button } from '../../shared/ui';
import CopyFix from '../../shared/ui/CopyFix';
import {
  MEMORY_DECIDE_REASON,
  learntNewestFirst,
  memoryCommands,
  type MemoryEntry,
  type TeamMemory as TeamMemoryReport,
} from '../../shared/execution';
import DisabledLine from './DisabledLine';

const MUTED = { color: 'var(--color-text-secondary)' } as const;

/** What an entry is about and what it says; a path or a command is shown as written. */
const EntryText: Component<{ entry: MemoryEntry }> = (props) => (
  <span class="flex min-w-0 flex-col gap-0.5">
    <span class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5">
      <Badge tone="neutral">{props.entry.kind}</Badge>
      <code class="min-w-0 font-mono text-caption" style={{ 'overflow-wrap': 'anywhere', color: 'var(--color-text-primary)' }}>
        {props.entry.subject}
      </code>
      <Show when={props.entry.stale}>
        <Badge tone="warning">Stale: what it names is gone</Badge>
      </Show>
    </span>
    <span class="text-small" style={{ 'overflow-wrap': 'anywhere', color: 'var(--color-text-primary)' }}>{props.entry.note}</span>
  </span>
);

/** An entry waiting for a person: why it waits, the two commands that decide it, and the buttons that will
 *  decide it from the board once the agent offers a route (disabled until then, with the reason). */
const Waiting: Component<{ team: string; entry: MemoryEntry }> = (props) => {
  const commands = () => memoryCommands(props.team, props.entry.id);
  const reasonId = createUniqueId();
  return (
    <li data-testid={`team-memory-entry-${props.entry.id}`} class="flex min-w-0 flex-col gap-1.5 py-2">
      <EntryText entry={props.entry} />
      <Show when={props.entry.reason}>
        <span class="text-caption" style={MUTED}>Held back: {props.entry.reason}</span>
      </Show>
      <span class="flex min-w-0 flex-col gap-1 text-caption" style={MUTED}>
        <span>To approve it, run:</span>
        <CopyFix fix={commands().approve} what={`approving ${props.entry.subject}`} />
        <span>To refuse it, run:</span>
        <CopyFix fix={commands().refuse} what={`refusing ${props.entry.subject}`} />
      </span>
      <span class="flex flex-wrap items-center gap-2">
        <Button type="button" size="sm" variant="secondary" disabled aria-describedby={reasonId} aria-label={`Approve ${props.entry.subject}`}>
          Approve
        </Button>
        <Button type="button" size="sm" variant="secondary" disabled aria-describedby={reasonId} aria-label={`Refuse ${props.entry.subject}`}>
          Refuse
        </Button>
        <span id={reasonId} data-testid="team-memory-decide-reason" class="text-caption" style={MUTED}>
          {MEMORY_DECIDE_REASON}
        </span>
      </span>
    </li>
  );
};

/**
 * What a team remembers about its work: what it has learnt (accepted, newest first, a stale one marked) and
 * what it waits to learn, each entry with the docket commands that decide it. A person decides at the
 * agent's own command; the board's buttons stay disabled until the agent offers a route.
 */
const TeamMemory: Component<{ team: string; memory: TeamMemoryReport }> = (props) => {
  const learntHeading = createUniqueId();
  const waitingHeading = createUniqueId();
  return (
    <div data-testid="team-memory-sections" class="flex min-w-0 flex-col gap-3">
      <section data-testid="team-memory-learnt" aria-labelledby={learntHeading} class="flex min-w-0 flex-col gap-1">
        <h5 id={learntHeading} class="text-micro font-bold uppercase tracking-widest" style={{ color: 'var(--color-text-tertiary)' }}>
          What it has learnt
        </h5>
        <Show when={props.memory.accepted.length > 0} fallback={<DisabledLine>Nothing yet</DisabledLine>}>
          <ul class="flex flex-col gap-1.5">
            <For each={learntNewestFirst(props.memory)}>
              {(entry) => (
                <li data-testid={`team-memory-entry-${entry.id}`} data-stale={entry.stale ? 'yes' : undefined}>
                  <EntryText entry={entry} />
                </li>
              )}
            </For>
          </ul>
        </Show>
      </section>
      <section data-testid="team-memory-waiting" aria-labelledby={waitingHeading} class="flex min-w-0 flex-col gap-1">
        <h5 id={waitingHeading} class="text-micro font-bold uppercase tracking-widest" style={{ color: 'var(--color-text-tertiary)' }}>
          What it waits to learn
        </h5>
        <Show when={props.memory.pending.length > 0} fallback={<DisabledLine>Nothing waits for you</DisabledLine>}>
          <ul class="flex flex-col">
            <For each={props.memory.pending}>{(entry) => <Waiting team={props.team} entry={entry} />}</For>
          </ul>
        </Show>
      </section>
    </div>
  );
};

export default TeamMemory;
