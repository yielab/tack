import { type Component, For, Show, createSignal, createUniqueId } from 'solid-js';
import { Badge, Button } from '../../shared/ui';
import Field from '../../shared/ui/Field';
import { toast } from '../../shared/ui/toast';
import CopyFix from '../../shared/ui/CopyFix';
import {
  MEMORY_DECIDE_REASON,
  learntNewestFirst,
  memoryCommands,
  teamsApi,
  type MemoryEntry,
  type TeamReport,
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

/** What the sheet needs to decide an entry from the board: who runs the team, whether it can, and where the
 *  report it answers with goes. */
export interface MemoryDeciding {
  harness: string | null;
  canDecide: boolean;
  onDecided: (report: TeamReport | null) => void;
}

/** An entry waiting for a person: why it waits, the two commands that decide it, and the buttons that decide
 *  it from the board when the agent offers a way (disabled otherwise, with the reason). Refusing asks for the
 *  reason first; the agent keeps it with the entry. */
const Waiting: Component<{ team: string; entry: MemoryEntry; deciding: MemoryDeciding }> = (props) => {
  const commands = () => memoryCommands(props.team, props.entry.id);
  const reasonId = createUniqueId();
  const [busy, setBusy] = createSignal(false);
  const [asking, setAsking] = createSignal(false);
  const [why, setWhy] = createSignal('');
  const [failure, setFailure] = createSignal<string | null>(null);
  const enabled = () => props.deciding.canDecide && !!props.deciding.harness;

  const decide = async (decision: 'approve' | 'refuse') => {
    const harness = props.deciding.harness;
    if (!harness || busy()) return;
    setBusy(true);
    setFailure(null);
    try {
      const decided = await teamsApi.decideMemory(harness, props.team, props.entry.id, decision, decision === 'refuse' ? why().trim() : undefined);
      toast.success(decision === 'approve' ? `${props.team} will use "${props.entry.subject}"` : `${props.team} will not use "${props.entry.subject}"`);
      props.deciding.onDecided(decided.report ?? null);
    } catch (err) {
      setFailure(err instanceof Error ? err.message : 'The entry could not be decided.');
    } finally {
      setBusy(false);
    }
  };

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
        <Button
          type="button"
          size="sm"
          variant="secondary"
          loading={busy() && !asking()}
          disabled={!enabled() || busy()}
          aria-describedby={enabled() ? undefined : reasonId}
          aria-label={`Approve ${props.entry.subject}`}
          onClick={() => void decide('approve')}
        >
          Approve
        </Button>
        <Button
          type="button"
          size="sm"
          variant="secondary"
          disabled={!enabled() || busy() || asking()}
          aria-describedby={enabled() ? undefined : reasonId}
          aria-label={`Refuse ${props.entry.subject}`}
          onClick={() => setAsking(true)}
        >
          Refuse
        </Button>
        <Show when={!enabled()}>
          <span id={reasonId} data-testid="team-memory-decide-reason" class="text-caption" style={MUTED}>
            {MEMORY_DECIDE_REASON}
          </span>
        </Show>
      </span>
      <Show when={asking()}>
        <form
          data-testid="team-memory-refuse-form"
          class="flex min-w-0 flex-col gap-1.5"
          onSubmit={(event) => {
            event.preventDefault();
            if (why().trim()) void decide('refuse');
          }}
        >
          <Field
            label={`Why ${props.team} should not keep this`}
            required
            value={why()}
            onInput={(event) => setWhy(event.currentTarget.value)}
            hint="The team keeps your reason with the entry."
          />
          <span class="flex flex-wrap items-center gap-2">
            <Button type="submit" size="sm" variant="danger" loading={busy()} disabled={!why().trim() || busy()} aria-label={`Confirm refusing ${props.entry.subject}`}>
              Refuse it
            </Button>
            <Button type="button" size="sm" variant="ghost" disabled={busy()} onClick={() => { setAsking(false); setWhy(''); }}>
              Keep waiting
            </Button>
          </span>
        </form>
      </Show>
      <Show when={failure()}>
        {(text) => (
          <p role="alert" data-testid="team-memory-refused" class="text-caption" style={{ color: 'var(--color-text-primary)', 'overflow-wrap': 'anywhere' }}>
            {text()}
          </p>
        )}
      </Show>
    </li>
  );
};

/**
 * What a team remembers about its work: what it has learnt (accepted, newest first, a stale one marked) and
 * what it waits to learn, each entry with the docket commands that decide it. A person decides from the
 * board when the agent offers a way, and at the agent's own command otherwise; the buttons say which.
 */
const TeamMemory: Component<{ team: string; memory: TeamMemoryReport; deciding: MemoryDeciding }> = (props) => {
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
            <For each={props.memory.pending}>{(entry) => <Waiting team={props.team} entry={entry} deciding={props.deciding} />}</For>
          </ul>
        </Show>
      </section>
    </div>
  );
};

export default TeamMemory;
