import { type Component, createResource, createSignal, For, Show } from 'solid-js';
import { api } from '../../shared/api';
import type {
  AgentCredential,
  CredentialCapability,
  CredentialScope,
  IssuedCredential,
} from '../../shared/api/credentials';
import { toast } from '../../shared/ui/toast';
import { Badge, Button, Field, createConfirm } from '../../shared/ui';

/** What a scope may name, in the order a screen lists them, and why one cannot be picked yet. */
export const CAPABILITIES: { value: CredentialCapability; label: string; hint: string; reason?: string }[] = [
  { value: 'read', label: 'Read', hint: 'See the project, its items, comments and dependencies' },
  {
    value: 'propose',
    label: 'Propose',
    hint: 'Put changes in front of a person',
  },
  { value: 'act', label: 'Act', hint: 'Create and change items, comments and dependencies' },
  {
    value: 'converse',
    label: 'Converse',
    hint: 'Talk in a thread about the project',
  },
];

const label = (value: CredentialCapability) => CAPABILITIES.find((c) => c.value === value)?.label ?? value;

const day = (iso: string) => new Date(iso).toLocaleDateString();

/** "Active", "Expired" or "Revoked", from the credential's own dates. */
export function credentialState(c: AgentCredential, now = Date.now()): 'active' | 'expired' | 'revoked' {
  if (c.revoked_at) return 'revoked';
  return new Date(c.expires_at).getTime() <= now ? 'expired' : 'active';
}

const CARD = 'flex flex-col gap-3 rounded-card bg-panel px-5 py-5';
const CARD_TITLE = 'text-xl leading-tight';
const MUTED = { color: 'var(--color-text-secondary)' };

/** Settings: the keys the operator hands to agents. */
const CredentialsSettings: Component = () => {
  const [list, { refetch }] = createResource(() => api.credentials.list());
  const [projects] = createResource(() => api.projects.list());
  const [ask, ConfirmHost] = createConfirm();

  const [name, setName] = createSignal('');
  const [everyProject, setEveryProject] = createSignal(false);
  const [picked, setPicked] = createSignal<string[]>([]);
  const [caps, setCaps] = createSignal<CredentialCapability[]>(['read']);
  const [busy, setBusy] = createSignal(false);
  const [shown, setShown] = createSignal<IssuedCredential | null>(null);

  const projectName = (id: string) => projects()?.find((p) => p.id === id)?.name ?? id;
  const scopeText = (scope: CredentialScope) =>
    `${scope.all_projects ? 'Every project' : (scope.projects ?? []).map(projectName).join(', ')}: ${scope.capabilities.map(label).join(', ')}`;

  const noProjects = () => !projects.loading && (projects() ?? []).length === 0;
  const reason = (): string | undefined => {
    if (!name().trim()) return 'Give the credential a name';
    if (!everyProject() && picked().length === 0) {
      return noProjects() ? 'Create a project first, or choose every project' : 'Choose a project, or every project';
    }
    if (caps().length === 0) return 'Choose at least one thing it may do';
    return undefined;
  };

  const toggle = <T,>(list: T[], value: T, on: boolean) =>
    on ? [...list.filter((v) => v !== value), value] : list.filter((v) => v !== value);

  const issue = async () => {
    setBusy(true);
    try {
      const scope: CredentialScope = {
        all_projects: everyProject(),
        projects: everyProject() ? [] : picked(),
        capabilities: caps(),
      };
      setShown(await api.credentials.issue(name().trim(), [scope]));
      setName('');
      setPicked([]);
      setEveryProject(false);
      setCaps(['read']);
      await refetch();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Could not issue the credential');
    } finally {
      setBusy(false);
    }
  };

  const rotate = async (c: AgentCredential) => {
    if (
      !(await ask({
        title: `Rotate ${c.name}?`,
        message: 'The current secret stops working at once. The agent needs the new one.',
        confirmLabel: 'Rotate',
      }))
    ) {
      return;
    }
    try {
      setShown(await api.credentials.rotate(c.id));
      await refetch();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Could not rotate the credential');
    }
  };

  const revoke = async (c: AgentCredential) => {
    if (
      !(await ask({
        title: `Revoke ${c.name}?`,
        message: 'The agent loses its access for good. What it already wrote stays, signed with its name.',
        confirmLabel: 'Revoke',
        danger: true,
      }))
    ) {
      return;
    }
    try {
      await api.credentials.revoke(c.id);
      await refetch();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Could not revoke the credential');
    }
  };

  const copy = async (secret: string) => {
    try {
      await navigator.clipboard.writeText(secret);
      toast.success('Copied.');
    } catch {
      toast.error('Could not copy; select the secret and copy it by hand.');
    }
  };

  return (
    <section class={CARD} data-testid="credentials-settings">
      <ConfirmHost />
      <h2 class={CARD_TITLE} style={{ color: 'var(--color-text-primary)' }}>
        Agent credentials
      </h2>
      <p class="text-small" style={MUTED}>
        A credential is a key you hand to an agent so it can use Tack on its own. It reaches only the projects
        you name, does only what you tick, signs what it writes with its name, and expires after 90 days.
        Resolving a decision is never part of it.
      </p>

      <Show when={shown()}>
        {(issued) => (
          <div
            class="flex flex-col gap-2 rounded-xl px-3.5 py-3"
            style={{ 'background-color': 'var(--color-bg-app)' }}
            role="status"
            data-testid="credential-secret"
          >
            <p class="text-small font-semibold" style={{ color: 'var(--color-text-primary)' }}>
              Secret for {issued().credential.name}
            </p>
            <code class="break-all font-mono text-xs" style={{ color: 'var(--color-text-primary)' }}>
              {issued().secret}
            </code>
            <p class="text-xs" style={MUTED}>
              This is the only time it is shown. Copy it now; Tack keeps only a hash.
            </p>
            <div class="flex gap-2">
              <Button size="sm" onClick={() => void copy(issued().secret)}>
                Copy
              </Button>
              <Button size="sm" variant="secondary" onClick={() => setShown(null)}>
                I have copied it
              </Button>
            </div>
          </div>
        )}
      </Show>

      <Show
        when={(list() ?? []).length > 0}
        fallback={
          <p class="text-small" style={{ color: 'var(--color-text-tertiary)' }}>
            {list.loading ? 'Loading…' : 'No credentials yet.'}
          </p>
        }
      >
        <ul class="flex flex-col" data-testid="credential-list">
          <For each={list()}>
            {(c) => {
              const state = () => credentialState(c);
              return (
                <li
                  class="flex flex-wrap items-center gap-x-3 gap-y-1 border-t py-2.5 text-small"
                  style={{ 'border-color': 'var(--color-border-light)', color: 'var(--color-text-secondary)' }}
                >
                  <div class="flex min-w-0 flex-1 flex-col gap-0.5">
                    <span class="flex items-center gap-2">
                      <span class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>
                        {c.name}
                      </span>
                      <Badge tone={state() === 'active' ? 'success' : state() === 'expired' ? 'warning' : 'neutral'}>
                        {state()}
                      </Badge>
                    </span>
                    <For each={c.scopes}>{(scope) => <span class="text-xs">{scopeText(scope)}</span>}</For>
                    <span class="text-xs">
                      {state() === 'revoked' ? `Revoked ${day(c.revoked_at!)}` : `Expires ${day(c.expires_at)}`}
                      {' · '}
                      {c.last_used_at ? `Last used ${day(c.last_used_at)}` : 'Never used'}
                    </span>
                  </div>
                  <span class="flex items-start gap-1.5">
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={state() === 'revoked'}
                      reason={state() === 'revoked' ? 'Revoked for good; issue a new one' : undefined}
                      onClick={() => void rotate(c)}
                    >
                      Rotate
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={state() === 'revoked'}
                      reason={state() === 'revoked' ? 'Already revoked' : undefined}
                      onClick={() => void revoke(c)}
                    >
                      Revoke
                    </Button>
                  </span>
                </li>
              );
            }}
          </For>
        </ul>
      </Show>

      <form
        class="flex flex-col gap-3 rounded-xl px-3.5 py-3"
        style={{ 'background-color': 'var(--color-bg-app)' }}
        onSubmit={(e) => {
          e.preventDefault();
          if (!reason()) void issue();
        }}
      >
        <p class="text-micro font-bold uppercase tracking-widest" style={{ color: 'var(--color-text-tertiary)' }}>
          Issue a credential
        </p>
        <Field
          label="Name"
          placeholder="Reviewer"
          hint="What its comments and changes are signed with."
          value={name()}
          onInput={(e) => setName(e.currentTarget.value)}
        />

        <fieldset class="flex flex-col gap-1.5">
          <legend class="mb-1 text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            Projects
          </legend>
          <label class="flex items-center gap-2 text-small" style={{ color: 'var(--color-text-primary)' }}>
            <input
              type="checkbox"
              checked={everyProject()}
              onChange={(e) => setEveryProject(e.currentTarget.checked)}
            />
            Every project
          </label>
          <Show when={everyProject()}>
            <p class="text-xs" style={MUTED}>
              It reaches the {(projects() ?? []).length} project{(projects() ?? []).length === 1 ? '' : 's'} that
              exist today and every project made after.
            </p>
          </Show>
          <For each={projects() ?? []}>
            {(p) => (
              <label
                class="flex items-center gap-2 text-small"
                style={{ color: everyProject() ? 'var(--color-text-tertiary)' : 'var(--color-text-primary)' }}
              >
                <input
                  type="checkbox"
                  disabled={everyProject()}
                  checked={everyProject() || picked().includes(p.id)}
                  onChange={(e) => setPicked(toggle(picked(), p.id, e.currentTarget.checked))}
                />
                {p.name}
              </label>
            )}
          </For>
          <Show when={noProjects()}>
            <p class="text-xs" style={MUTED}>
              There are no projects yet.
            </p>
          </Show>
        </fieldset>

        <fieldset class="flex flex-col gap-1.5">
          <legend class="mb-1 text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            It may
          </legend>
          <For each={CAPABILITIES}>
            {(c) => (
              <label
                class="flex flex-wrap items-center gap-x-2 text-small"
                style={{ color: c.reason ? 'var(--color-text-tertiary)' : 'var(--color-text-primary)' }}
              >
                <input
                  type="checkbox"
                  disabled={!!c.reason}
                  checked={caps().includes(c.value)}
                  aria-describedby={c.reason ? `cap-${c.value}-reason` : undefined}
                  onChange={(e) => setCaps(toggle(caps(), c.value, e.currentTarget.checked))}
                />
                <span class="font-semibold">{c.label}</span>
                <span class="text-xs">{c.hint}</span>
                <Show when={c.reason}>
                  <span id={`cap-${c.value}-reason`} class="text-xs" style={MUTED}>
                    ({c.reason})
                  </span>
                </Show>
              </label>
            )}
          </For>
        </fieldset>

        <div class="flex">
          <Button type="submit" loading={busy()} disabled={!!reason() || busy()} reason={reason()}>
            Issue credential
          </Button>
        </div>
      </form>
    </section>
  );
};

export default CredentialsSettings;
