import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { Button, EmptyState, Field, Skeleton } from '../../../shared/ui';
import { toast } from '../../../shared/ui/toast';
import { IconSettings } from '../../../shared/ui/icons';
import { agentProfilesApi, type AgentProfileSummary } from '../../../shared/execution';
import { parseOptionalJsonObject } from './format';

/**
 * Create/list UI for `agent_profiles` (`GET`/`POST /agent-profiles`) — the
 * instructions/tool-policy/limits bundle an execution request snapshots at
 * creation time (III.1.2's `agent-profile id AND resolved profile
 * snapshot`). `tool_policy`/`limits` are opaque JSON on the wire
 * (`AgentProfileSummary.tool_policy: unknown`) — this panel doesn't
 * interpret them, only round-trips whatever JSON object the operator types.
 */
const AgentProfilesPanel: Component = () => {
  const [profiles, { refetch, mutate }] = createResource(() => agentProfilesApi.list());

  const [showForm, setShowForm] = createSignal(false);
  const [name, setName] = createSignal('');
  const [instructions, setInstructions] = createSignal('');
  const [toolPolicyRaw, setToolPolicyRaw] = createSignal('');
  const [limitsRaw, setLimitsRaw] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  const rows = (): AgentProfileSummary[] => profiles()?.data.data ?? [];

  const submit = async (e: Event) => {
    e.preventDefault();
    if (!name().trim() || !instructions().trim()) return;
    const parsedPolicy = parseOptionalJsonObject(toolPolicyRaw(), 'Tool policy');
    if (!parsedPolicy.ok) {
      toast.error(parsedPolicy.error);
      return;
    }
    const parsedLimits = parseOptionalJsonObject(limitsRaw(), 'Limits');
    if (!parsedLimits.ok) {
      toast.error(parsedLimits.error);
      return;
    }
    setSaving(true);
    try {
      const created = await agentProfilesApi.create({
        name: name().trim(),
        instructions: instructions().trim(),
        tool_policy: parsedPolicy.value,
        limits: parsedLimits.value,
      });
      toast.success(`Created agent profile "${created.name}"`);
      mutate((prev) =>
        prev
          ? {
              ...prev,
              data: {
                ...prev.data,
                data: [
                  ...prev.data.data,
                  {
                    agent_profile_id: created.agent_profile_id,
                    name: created.name,
                    instructions: instructions().trim(),
                    tool_policy: parsedPolicy.value,
                    limits: parsedLimits.value,
                  },
                ],
              },
            }
          : prev,
      );
      setName('');
      setInstructions('');
      setToolPolicyRaw('');
      setLimitsRaw('');
      setShowForm(false);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to create agent profile');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="space-y-4">
      <Show when={profiles.loading}>
        <Skeleton height="60px" />
      </Show>

      <Show when={!profiles.loading && profiles.error === undefined}>
        <Show
          when={rows().length > 0}
          fallback={
            <div class="rounded-[28px] border-2 border-dashed" style={{ 'border-color': 'var(--color-border-light)' }}>
              <EmptyState
                icon={<IconSettings size={26} />}
                title="No agent profiles yet"
                description="An agent profile bundles the instructions and tool/limits policy an execution request snapshots at creation."
              />
            </div>
          }
        >
          <ul class="space-y-2">
            <For each={rows()}>
              {(profile) => (
                <li class="flex flex-col gap-1.5 rounded-[28px] bg-panel px-5 py-[18px]">
                  <div class="flex flex-wrap items-center gap-2">
                    <span class="text-[15px] font-bold" style={{ color: 'var(--color-text-primary)' }}>
                      {profile.name}
                    </span>
                    <span class="font-mono text-[11px]" style={{ color: 'var(--color-text-tertiary)' }}>
                      {profile.agent_profile_id}
                    </span>
                  </div>
                  <p class="text-[12.5px]" style={{ color: 'var(--color-text-secondary)' }}>{profile.instructions}</p>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </Show>

      <Show when={profiles.error !== undefined}>
        <div class="text-[13px]" style={{ color: 'var(--color-danger-600)' }}>
          Couldn't load agent profiles.{' '}
          <button type="button" class="font-bold underline" onClick={() => void refetch()}>
            Retry
          </button>
        </div>
      </Show>

      <Show
        when={showForm()}
        fallback={
          <Button variant="ghost" size="sm" onClick={() => setShowForm(true)}>
            + Create agent profile
          </Button>
        }
      >
        <form onSubmit={(e) => void submit(e)} class="max-w-md space-y-3 rounded-[28px] bg-panel px-5 py-[18px]">
          <Field label="Name" required placeholder="reviewer" value={name()} onInput={(e) => setName(e.currentTarget.value)} />
          <Field
            label="Instructions"
            required
            placeholder="Review the diff for correctness and style."
            value={instructions()}
            onInput={(e) => setInstructions(e.currentTarget.value)}
          />
          <Field
            label="Tool policy (JSON object, optional)"
            placeholder="{}"
            value={toolPolicyRaw()}
            onInput={(e) => setToolPolicyRaw(e.currentTarget.value)}
          />
          <Field
            label="Limits (JSON object, optional)"
            placeholder="{}"
            value={limitsRaw()}
            onInput={(e) => setLimitsRaw(e.currentTarget.value)}
          />
          <div class="flex gap-2">
            <Button type="submit" loading={saving()} disabled={saving() || !name().trim() || !instructions().trim()}>
              Create
            </Button>
            <Button type="button" variant="ghost" onClick={() => setShowForm(false)} disabled={saving()}>
              Cancel
            </Button>
          </div>
        </form>
      </Show>
    </div>
  );
};

export default AgentProfilesPanel;
