import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { A } from '@solidjs/router';
import { api } from '../../shared/api';
import { agentProfilesApi } from '../../shared/execution';
import { HARNESS_KINDS } from '../../shared/runWithAgent/shared';
import { toast } from '../../shared/ui/toast';
import { Button, Field, Select } from '../../shared/ui';
import { AGENT_LEVELS } from '../../shared/agents/levels';
import { triggerSentence } from '../../shared/agents/triggerText';
import { AddManagerError, type AddManagerResult, addManager } from './managerSetup';

const NEW_PROFILE = '__new__';
const MUTED = { color: 'var(--color-text-secondary)' };

/** Why a manager cannot be added yet, or undefined when it can. */
export function addManagerReason(args: { project: string; profile: string; name: string }): string | undefined {
  if (!args.project) return 'Choose the project it looks after';
  if (args.profile === NEW_PROFILE && !args.name.trim()) return 'Give the new profile a name';
  return undefined;
}

/**
 * Add a manager: one agent profile that looks after one project, read from the digest, changing the board only
 * through change sets and talking with you in a thread. It makes nothing new: the profile (or one you choose), a
 * credential that may read, propose and converse, the thread, three triggers and the MCP configuration for the agent
 * to read. Each can be changed one by one afterwards.
 */
const AddManager: Component = () => {
  const [projects] = createResource(() => api.projects.list().catch(() => []));
  const [profiles] = createResource(() => agentProfilesApi.list().then((r) => r.data.data).catch(() => []));
  const [project, setProject] = createSignal('');
  const [profile, setProfile] = createSignal('');
  const [name, setName] = createSignal('Manager');
  const [agent, setAgent] = createSignal(HARNESS_KINDS[0].value);
  const [busy, setBusy] = createSignal(false);
  const [done, setDone] = createSignal<AddManagerResult | null>(null);
  const [failure, setFailure] = createSignal<{ step: string; message: string } | null>(null);
  const [projectOfDone, setProjectOfDone] = createSignal('');

  const chosenProject = () => project() || (projects() ?? [])[0]?.id || '';
  const chosenProfile = () => profile() || ((profiles() ?? []).length > 0 ? (profiles() ?? [])[0].agent_profile_id : NEW_PROFILE);
  const reason = () => addManagerReason({ project: chosenProject(), profile: chosenProfile(), name: name() });

  const submit = async () => {
    if (reason() || busy()) return;
    setBusy(true);
    setFailure(null);
    setDone(null);
    setProjectOfDone(chosenProject());
    try {
      const result = await addManager(
        {
          projectId: chosenProject(),
          profile: chosenProfile() === NEW_PROFILE ? { kind: 'new', name: name().trim(), agent: agent() } : { kind: 'existing', id: chosenProfile() },
          utcOffsetMinutes: -new Date().getTimezoneOffset(),
          apiUrl: window.location.origin,
        },
        setDone,
      );
      setDone(result);
    } catch (error) {
      if (error instanceof AddManagerError) {
        setDone(error.done);
        setFailure({ step: error.step, message: error.message });
      } else {
        setFailure({ step: 'start', message: error instanceof Error ? error.message : String(error) });
      }
    } finally {
      setBusy(false);
    }
  };

  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast.success('Copied.');
    } catch {
      toast.error('Could not copy; select the text and copy it by hand.');
    }
  };

  const rungSentence = (level: string | undefined) =>
    AGENT_LEVELS.find((l) => l.value === level) ?? AGENT_LEVELS[1];

  return (
    <section class="flex flex-col gap-3 rounded-card bg-panel px-5 py-5" data-testid="add-manager">
      <h2 class="text-xl leading-tight" style={{ color: 'var(--color-text-primary)' }}>
        Add a manager
      </h2>
      <p class="text-small" style={MUTED}>
        A manager is an agent profile that looks after one project: it reads the digest of every team, says who should do
        what, and changes the board only through change sets that wait for you. This sets up everything it needs in one go.
        None of it is special: the profile, the credential, the thread and the triggers are the same ones you can make by hand
        and change one by one.
      </p>

      <form
        class="flex flex-col gap-3 rounded-xl px-3.5 py-3"
        style={{ 'background-color': 'var(--color-bg-app)' }}
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <Select
          label="Project it looks after"
          data-testid="manager-project"
          value={chosenProject()}
          disabled={(projects() ?? []).length === 0}
          options={(projects() ?? []).map((p) => ({ value: p.id, label: p.name }))}
          onChange={(e) => setProject(e.currentTarget.value)}
          hint={(projects() ?? []).length === 0 ? 'There are no projects yet.' : 'It reads this project only; you can widen its credential later in Settings.'}
        />
        <Select
          label="Agent profile"
          data-testid="manager-profile"
          value={chosenProfile()}
          options={[
            ...(profiles() ?? []).map((p) => ({ value: p.agent_profile_id, label: p.name })),
            { value: NEW_PROFILE, label: 'Make a new profile for it' },
          ]}
          onChange={(e) => setProfile(e.currentTarget.value)}
          hint="Any agent will do: a docket team, Claude Code, Codex or opencode."
        />
        <Show when={chosenProfile() === NEW_PROFILE}>
          <Field label="Name" value={name()} onInput={(e) => setName(e.currentTarget.value)} hint="Its credential and its comments are signed with this name." />
          <Select
            label="Agent"
            data-testid="manager-agent"
            value={agent()}
            options={HARNESS_KINDS.map((h) => ({ value: h.value, label: h.label }))}
            onChange={(e) => setAgent(e.currentTarget.value)}
          />
        </Show>
        <div class="flex">
          <Button type="submit" loading={busy()} disabled={!!reason() || busy()} reason={reason()} data-testid="manager-add">
            Add manager
          </Button>
        </div>
      </form>

      <Show when={failure()}>
        {(f) => (
          <p role="alert" data-testid="manager-failure" class="text-small" style={{ color: 'var(--color-danger-600)' }}>
            Stopped at the {f().step}: {f().message}. What was made is listed below; finish the rest by hand in Settings.
          </p>
        )}
      </Show>

      <Show when={done()}>
        {(result) => (
          <div class="flex flex-col gap-3" data-testid="manager-result">
            <ul class="flex list-none flex-col gap-1 p-0 text-small" style={{ color: 'var(--color-text-primary)' }}>
              <Show when={result().profileName}>
                <li data-testid="manager-made-profile">Profile: {result().profileName}</li>
              </Show>
              <Show when={result().credential}>
                {(c) => (
                  <li data-testid="manager-made-credential">
                    Credential: {c().credential.name}, which may read, propose and converse on the project. It expires after 90 days.
                  </li>
                )}
              </Show>
              <Show when={result().thread}>
                {(t) => (
                  <li data-testid="manager-made-thread">
                    Thread: <A class="underline" href={`/projects/${projectOfDone()}/threads?open=${t().id}`}>{t().title}</A>
                  </li>
                )}
              </Show>
              <For each={result().triggers}>
                {(t) => (
                  <li data-testid="manager-made-trigger">
                    Trigger {t.name}: {triggerSentence(t)}
                  </li>
                )}
              </For>
              <Show when={result().rung !== undefined}>
                <li data-testid="manager-rung">
                  Agents on the board is at {rungSentence(result().rung).name}: {rungSentence(result().rung).sentence}{' '}
                  <A class="underline" href={`/projects/${projectOfDone()}/settings?tab=automation`}>Change it in Settings</A>
                </li>
              </Show>
            </ul>

            <Show when={result().credential && result().mcpConfig}>
              <div
                class="flex flex-col gap-2 rounded-xl px-3.5 py-3"
                style={{ 'background-color': 'var(--color-bg-app)' }}
                role="status"
                data-testid="manager-mcp"
              >
                <p class="text-small font-semibold" style={{ color: 'var(--color-text-primary)' }}>
                  Give this to the agent
                </p>
                <p class="text-xs" style={MUTED}>
                  Paste it into the agent&apos;s MCP configuration (for Claude Code, a <code>.mcp.json</code> file in the folder
                  it works in). It holds the credential&apos;s secret, which is shown only now; Tack keeps a hash.
                </p>
                <pre class="overflow-x-auto font-mono text-xs" style={{ color: 'var(--color-text-primary)' }}>{result().mcpConfig}</pre>
                <div class="flex gap-2">
                  <Button size="sm" onClick={() => void copy(result().mcpConfig!)}>
                    Copy
                  </Button>
                  <Button size="sm" variant="secondary" onClick={() => setDone(null)}>
                    I have copied it
                  </Button>
                </div>
              </div>
            </Show>
          </div>
        )}
      </Show>
    </section>
  );
};

export default AddManager;
