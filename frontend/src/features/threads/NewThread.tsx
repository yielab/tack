import { type Component, Show, createSignal } from 'solid-js';
import { A } from '@solidjs/router';
import { Button, Field, Select } from '../../shared/ui';
import { api } from '../../shared/api';
import type { Thread } from '../../shared/api/threads';
import type { AgentProfileSummary } from '../../shared/execution';

/**
 * Starts a thread in the project: a title and the agent profile that answers. With no profile the form
 * stays, disabled, and says where to make one.
 */
const NewThread: Component<{
  projectId: string;
  profiles: readonly AgentProfileSummary[];
  onStarted: (thread: Thread) => void;
  onCancel?: () => void;
}> = (props) => {
  const [title, setTitle] = createSignal('');
  const [profile, setProfile] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [problem, setProblem] = createSignal<string | null>(null);
  const chosen = () => profile() || props.profiles[0]?.agent_profile_id || '';
  const why = (): string | undefined =>
    props.profiles.length === 0
      ? 'There is no agent profile yet. Make one on the Agents page.'
      : title().trim() === ''
        ? 'Give the thread a title.'
        : undefined;

  const start = async () => {
    if (why() || busy()) return;
    setBusy(true);
    setProblem(null);
    try {
      const thread = await api.threads.create({ project_id: props.projectId, profile: chosen(), title: title().trim() });
      props.onStarted(thread);
    } catch (error) {
      setProblem(`The thread was not started. ${error instanceof Error ? error.message : ''}`.trim());
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      data-testid="new-thread"
      class="flex min-w-0 flex-col gap-3 rounded-item p-3"
      style={{ border: '1px solid var(--color-border-light)' }}
      onSubmit={(e) => {
        e.preventDefault();
        void start();
      }}
    >
      <Field
        label="Title"
        value={title()}
        disabled={props.profiles.length === 0}
        onInput={(e) => setTitle(e.currentTarget.value)}
        placeholder="What this conversation is about"
        hint="Only to tell your threads apart."
      />
      <Select
        label="Who answers"
        data-testid="new-thread-profile"
        value={chosen()}
        disabled={props.profiles.length === 0}
        options={props.profiles.map((p) => ({ value: p.agent_profile_id, label: p.name }))}
        onChange={(e) => setProfile(e.currentTarget.value)}
        hint="Each message you write runs this profile once, in this project."
      />
      <Show when={props.profiles.length === 0}>
        <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          {why()} <A href="/agents" class="underline">Open Agents</A>
        </p>
      </Show>
      <Show when={problem()}>
        <p role="alert" data-testid="new-thread-problem" class="text-sm" style={{ color: 'var(--color-danger-600)' }}>
          {problem()}
        </p>
      </Show>
      <div class="flex flex-wrap items-center gap-2">
        <Button type="submit" size="sm" data-testid="new-thread-start" disabled={!!why()} reason={why()} loading={busy()}>
          Start thread
        </Button>
        <Show when={props.onCancel}>
          <Button type="button" size="sm" variant="ghost" onClick={() => props.onCancel?.()}>
            Cancel
          </Button>
        </Show>
      </div>
    </form>
  );
};

export default NewThread;
