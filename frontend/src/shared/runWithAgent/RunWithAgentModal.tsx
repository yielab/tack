import { type Component, createSignal, createResource, createMemo, createEffect, Show, For } from 'solid-js';
import { Modal, Button, Field, Select, Badge } from '../ui';
import { toast } from '../ui/toast';
import {
  fleetsApi,
  agentProfilesApi,
  modelProfilesApi,
  runnersApi,
  type FleetSummary,
  type AgentProfileSummary,
  type ModelProfileSummary,
  type RunnerCapabilities,
  type RunnerSummary,
} from '../execution';
import { useExecutionStore } from '../state/executionContext';
import {
  HARNESS_KINDS,
  buildCreateExecutionInput,
  generateIdempotencyKey,
  resolveDefaultProvenance,
  gateHarnessModelSelection,
  type RunWithAgentFormValues,
} from './shared';

export interface RunWithAgentModalProps {
  isOpen: boolean;
  onClose: () => void;
  itemId: string;
  itemTitle: string;
  /** Called after a successful create, with the new request id — so a host
   *  (e.g. item-detail) can switch straight to the Execution tab. Never
   *  required: every surface already sees the new request appear via the
   *  shared store without this callback (this card's acceptance bar). */
  onCreated?: (requestId: string) => void;
  /**
   * Injectable runner capability snapshots for the submit gate
   * (`shared.ts#gateHarnessModelSelection`), overriding the live `GET
   * /runners` fetch below. Exposed as a prop purely so tests can inject
   * deterministic fixture data without a network round-trip and prove the
   * gate is load-bearing, not permanently permissive or permanently
   * blocked — every real call site (Board/item-detail/Sprint) omits it and
   * gets live data instead (card III-E6: `GET /runners` didn't exist when
   * this prop was first built as the only way to test the gate at all —
   * see this component's own body for the adapter now that it does).
   */
  capabilities?: () => RunnerCapabilities[];
}

/**
 * Adapts one `GET /runners` row into the `RunnerCapabilities` shape
 * `gateHarnessModelSelection` expects. The two shapes differ in exactly one
 * way: `RunnerCapabilities` (`docs/contracts/runner-v1/capabilities.json`'s
 * *standalone* report shape) nests `protocol_version`/`runner_version`
 * inside the capability payload itself, while `agent_runners` stores them
 * as sibling columns next to the *embedded* snapshot
 * (`EmbeddedCapabilitySnapshot` — see that type's own doc comment in
 * `crates/tack-orch/src/execution/capabilities.rs` for why enroll/refresh
 * use a different, sibling-field shape than a standalone report). Returns
 * `null` for a runner with no valid, structurally complete parsed snapshot
 * — never enrolled (the column default is the literal `'{}'`, which parses
 * as valid JSON but has no `harnesses`/`concurrency`/`features`/`limits`
 * fields at all — a real, reproduced bug this check fixes: iterating a
 * missing `harnesses` on such a row threw downstream in
 * `capabilities.ts#harnessProbeStatus`), a pending-enrollment runner, or a
 * genuinely corrupt stored value. Skipped from the gate's input entirely —
 * the same "unknown, not a fabricated empty capability" treatment
 * `capabilities.ts` already gives a `probe_error`red harness, never a
 * structural `{}` standing in for "no data" (III.2 rule 7).
 */
function runnerSummaryToCapabilities(runner: RunnerSummary): RunnerCapabilities | null {
  const snapshot = runner.capability_snapshot as
    | Omit<RunnerCapabilities, 'protocol_version' | 'runner_version'>
    | null;
  if (!snapshot || !Array.isArray(snapshot.harnesses) || !snapshot.concurrency || !snapshot.limits) {
    return null;
  }
  return {
    ...snapshot,
    protocol_version: runner.protocol_version,
    runner_version: runner.runner_version ?? '',
  };
}

/**
 * The ONE shared "Run with agent" modal (TODO.md III-E4) — Board,
 * item-detail, and Sprint all mount this exact component rather than each
 * building their own form, which is what makes "all three surfaces create
 * the same payload shape" true by construction rather than by convention.
 *
 * Vocabulary/visual distinctness from the legacy Docket "dispatch" feature
 * (`shared/dispatch/**`, III.0's vocabulary rule): this modal's title, every
 * label, and its submit button all say "run" / "agent", never "dispatch" —
 * and it is a visually different modal (its own title bar, its own field
 * set) from `features/sprints/DispatchSprintModal.tsx`, not a themed
 * variant of it. The two features are deliberately unaware of each other.
 */
const RunWithAgentModal: Component<RunWithAgentModalProps> = (props) => {
  const store = useExecutionStore();

  const [fleets] = createResource(
    () => (props.isOpen ? 'open' : undefined),
    () => fleetsApi.list().then((r) => r.data.data),
  );
  const [agentProfiles] = createResource(
    () => (props.isOpen ? 'open' : undefined),
    () => agentProfilesApi.list().then((r) => r.data.data),
  );
  const [modelProfiles] = createResource(
    () => (props.isOpen ? 'open' : undefined),
    () => modelProfilesApi.list().then((r) => r.data.data),
  );
  // Live capability data (card III-E6) — skipped entirely when a caller
  // (a test) injects `props.capabilities`, so no network call happens and
  // the injected fixture stays the only source of truth for that case.
  const [liveRunners] = createResource(
    () => (props.isOpen && !props.capabilities ? 'open' : undefined),
    () => runnersApi.list().then((r) => r.data.data),
  );
  const capabilities = (): RunnerCapabilities[] => {
    if (props.capabilities) return props.capabilities();
    if (liveRunners.error !== undefined) return [];
    return (liveRunners() ?? [])
      .map(runnerSummaryToCapabilities)
      .filter((c): c is RunnerCapabilities => c !== null);
  };

  // Resources throw once errored — read through a safe accessor everywhere
  // (same pattern, and same reasoning, as `DispatchSprintModal.tsx#dryRunData`
  // and `ItemDetailDrawer.tsx#agentActivityData`: calling an errored Solid
  // resource accessor from within a reactive computation aborts that batch).
  const fleetsData = (): FleetSummary[] => (fleets.error !== undefined ? [] : (fleets() ?? []));
  const agentProfilesData = (): AgentProfileSummary[] =>
    agentProfiles.error !== undefined ? [] : (agentProfiles() ?? []);
  const modelProfilesData = (): ModelProfileSummary[] =>
    (modelProfiles.error !== undefined ? [] : (modelProfiles() ?? [])).filter((p) => p.enabled);

  const [selectorKind, setSelectorKind] = createSignal<'fleet' | 'exact_runner'>('fleet');
  const [selectorId, setSelectorId] = createSignal('');
  const [agentProfileId, setAgentProfileId] = createSignal('');
  const [harnessKind, setHarnessKind] = createSignal(HARNESS_KINDS[0].value);
  const [modelMode, setModelMode] = createSignal<'auto' | 'profile'>('auto');
  const [modelProfileId, setModelProfileId] = createSignal('');
  const [timeoutSeconds, setTimeoutSeconds] = createSignal(3600);
  const [allowNetwork, setAllowNetwork] = createSignal(false);
  const [toolsText, setToolsText] = createSignal('');
  const [repoKind, setRepoKind] = createSignal('git');
  const [repoRemote, setRepoRemote] = createSignal('');
  const [repoBaseRevision, setRepoBaseRevision] = createSignal('main');
  const [repoSubdirectory, setRepoSubdirectory] = createSignal('');
  const [submitting, setSubmitting] = createSignal(false);
  const [idempotencyKey, setIdempotencyKey] = createSignal(generateIdempotencyKey());

  // Reset to a fresh form (including a fresh idempotency key) every time the
  // modal opens for a — possibly different — item, so a leftover selection
  // from a previous run is never silently resubmitted against a new item.
  createEffect(() => {
    if (!props.isOpen) return;
    setSelectorKind('fleet');
    setSelectorId('');
    setAgentProfileId('');
    setHarnessKind(HARNESS_KINDS[0].value);
    setModelMode('auto');
    setModelProfileId('');
    setTimeoutSeconds(3600);
    setAllowNetwork(false);
    setToolsText('');
    setRepoKind('git');
    setRepoRemote('');
    setRepoBaseRevision('main');
    setRepoSubdirectory('');
    setIdempotencyKey(generateIdempotencyKey());
  });

  const selectedAgentProfile = createMemo(() =>
    agentProfilesData().find((p) => p.agent_profile_id === agentProfileId()),
  );
  const selectedModelProfile = createMemo(() =>
    modelProfilesData().find((p) => p.model_profile_id === modelProfileId()),
  );

  const modelProvider = (): string | null =>
    modelMode() === 'profile' ? (selectedModelProfile()?.model_provider ?? null) : null;
  const modelId = (): string | null => (modelMode() === 'profile' ? (selectedModelProfile()?.model_id ?? null) : null);

  const agentProfileProvenance = () => resolveDefaultProvenance();
  const modelProvenance = () => resolveDefaultProvenance();

  const combinationGate = createMemo(() =>
    gateHarnessModelSelection(capabilities(), harnessKind(), modelProvider(), modelId()),
  );

  // Structural (non-capability) validation — every reason is shown, never a
  // silently-disabled control (TODO.md III.2 rule 7).
  const structuralErrors = createMemo((): string[] => {
    const errors: string[] = [];
    if (!selectorId().trim()) {
      errors.push(selectorKind() === 'fleet' ? 'Select a fleet.' : 'Enter a runner id.');
    }
    if (!agentProfileId()) errors.push('Select an agent profile.');
    if (!harnessKind()) errors.push('Select a harness.');
    if (modelMode() === 'profile' && !modelProfileId()) errors.push('Select a model, or switch to Auto.');
    if (!repoRemote().trim()) errors.push('Enter a repository remote.');
    if (!repoBaseRevision().trim()) errors.push('Enter a base revision.');
    if (!Number.isFinite(timeoutSeconds()) || timeoutSeconds() <= 0) errors.push('Timeout must be a positive number of seconds.');
    return errors;
  });

  const canSubmit = createMemo(
    () => structuralErrors().length === 0 && combinationGate().allowed && !submitting(),
  );

  const buildValues = (): RunWithAgentFormValues => {
    const profile = selectedAgentProfile();
    return {
      itemId: props.itemId,
      selectorKind: selectorKind(),
      selectorId: selectorId().trim(),
      agentProfileId: agentProfileId(),
      agentProfileSnapshot: {
        name: profile?.name ?? '',
        instructions: profile?.instructions ?? '',
        tool_policy: profile?.tool_policy ?? {},
      },
      harnessKind: harnessKind(),
      modelProvider: modelProvider(),
      modelId: modelId(),
      timeoutSeconds: timeoutSeconds(),
      allowNetwork: allowNetwork(),
      tools: toolsText()
        .split(',')
        .map((t) => t.trim())
        .filter(Boolean),
      repository: {
        kind: repoKind(),
        remote: repoRemote().trim(),
        baseRevision: repoBaseRevision().trim(),
        subdirectory: repoSubdirectory().trim() || null,
      },
      idempotencyKey: idempotencyKey(),
    };
  };

  const submit = async (e: Event) => {
    e.preventDefault();
    if (!canSubmit()) return;
    setSubmitting(true);
    try {
      const input = buildCreateExecutionInput(buildValues());
      const result = await store.create(input);
      toast.success(result.replayed ? 'Reused an existing run for this item.' : 'Run started.');
      props.onCreated?.(result.request_id);
      props.onClose();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to start the run.');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal isOpen={props.isOpen} onClose={props.onClose} title={`Run with agent: ${props.itemTitle}`} size="lg">
      <form class="space-y-5" onSubmit={submit}>
        {/* ── Target ──────────────────────────────────────────────────── */}
        <fieldset class="space-y-2">
          <legend class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            Target
          </legend>
          <div class="flex gap-4 text-sm" style={{ color: 'var(--color-text-primary)' }}>
            <label class="flex items-center gap-1.5">
              <input
                type="radio"
                name="selector-kind"
                checked={selectorKind() === 'fleet'}
                onChange={() => {
                  setSelectorKind('fleet');
                  setSelectorId('');
                }}
              />
              Fleet
            </label>
            <label class="flex items-center gap-1.5">
              <input
                type="radio"
                name="selector-kind"
                checked={selectorKind() === 'exact_runner'}
                onChange={() => {
                  setSelectorKind('exact_runner');
                  setSelectorId('');
                }}
              />
              Exact runner
            </label>
          </div>
          <Show
            when={selectorKind() === 'fleet'}
            fallback={
              <Field
                label="Runner id"
                value={selectorId()}
                onInput={(e) => setSelectorId(e.currentTarget.value)}
                placeholder="runner id"
                hint="No runner directory endpoint exists yet (see docs/agent-handoffs/part-iii/III-E2.md, Gap 1) — enter an id you already know, e.g. from `tack runner enroll`."
              />
            }
          >
            <Select
              label="Fleet"
              value={selectorId()}
              onInput={(e) => setSelectorId(e.currentTarget.value)}
              options={[
                { value: '', label: fleets.loading ? 'Loading…' : 'Select a fleet' },
                ...fleetsData().map((f) => ({ value: f.fleet_id, label: f.name })),
              ]}
            />
          </Show>
        </fieldset>

        {/* ── Agent ───────────────────────────────────────────────────── */}
        <fieldset class="space-y-3">
          <legend class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            Agent
          </legend>
          <div>
            <Select
              label="Agent profile"
              value={agentProfileId()}
              onInput={(e) => setAgentProfileId(e.currentTarget.value)}
              options={[
                { value: '', label: agentProfiles.loading ? 'Loading…' : 'Select an agent profile' },
                ...agentProfilesData().map((p) => ({ value: p.agent_profile_id, label: p.name })),
              ]}
            />
            <DefaultProvenanceNote provenance={agentProfileProvenance()} />
          </div>

          <Select
            label="Harness"
            value={harnessKind()}
            onInput={(e) => setHarnessKind(e.currentTarget.value)}
            options={HARNESS_KINDS.map((h) => ({ value: h.value, label: h.label }))}
          />

          <div class="space-y-2">
            <div class="flex gap-4 text-sm" style={{ color: 'var(--color-text-primary)' }}>
              <label class="flex items-center gap-1.5">
                <input type="radio" name="model-mode" checked={modelMode() === 'auto'} onChange={() => setModelMode('auto')} />
                Auto (let the runner decide)
              </label>
              <label class="flex items-center gap-1.5">
                <input
                  type="radio"
                  name="model-mode"
                  checked={modelMode() === 'profile'}
                  onChange={() => setModelMode('profile')}
                />
                Choose a model
              </label>
            </div>
            <Show when={modelMode() === 'profile'}>
              <Select
                label="Model"
                value={modelProfileId()}
                onInput={(e) => setModelProfileId(e.currentTarget.value)}
                options={[
                  { value: '', label: modelProfiles.loading ? 'Loading…' : 'Select a model' },
                  ...modelProfilesData().map((p) => ({
                    value: p.model_profile_id,
                    label: `${p.name} — ${p.model_provider} / ${p.model_id}`,
                  })),
                ]}
              />
            </Show>
            <DefaultProvenanceNote provenance={modelProvenance()} />
            <CombinationGateNote gate={combinationGate()} />
          </div>
        </fieldset>

        {/* ── Repository ──────────────────────────────────────────────── */}
        <fieldset class="space-y-3">
          <legend class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            Repository
          </legend>
          <div class="grid grid-cols-2 gap-3">
            <Field label="Kind" value={repoKind()} onInput={(e) => setRepoKind(e.currentTarget.value)} />
            <Field
              label="Base revision"
              value={repoBaseRevision()}
              onInput={(e) => setRepoBaseRevision(e.currentTarget.value)}
              placeholder="main"
            />
          </div>
          <Field
            label="Remote"
            value={repoRemote()}
            onInput={(e) => setRepoRemote(e.currentTarget.value)}
            placeholder="git@github.com:org/repo.git"
          />
          <Field
            label="Subdirectory"
            value={repoSubdirectory()}
            onInput={(e) => setRepoSubdirectory(e.currentTarget.value)}
            hint="Optional."
          />
        </fieldset>

        {/* ── Permissions & budget ────────────────────────────────────── */}
        <fieldset class="space-y-3">
          <legend class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            Permissions &amp; budget
          </legend>
          <Field
            label="Timeout (seconds)"
            type="number"
            min="1"
            value={timeoutSeconds()}
            onInput={(e) => setTimeoutSeconds(Number(e.currentTarget.value))}
          />
          <Field
            label="Allowed tools"
            value={toolsText()}
            onInput={(e) => setToolsText(e.currentTarget.value)}
            hint="Comma-separated. Leave blank for none."
          />
          <label class="flex items-center gap-2 text-sm" style={{ color: 'var(--color-text-primary)' }}>
            <input type="checkbox" checked={allowNetwork()} onChange={(e) => setAllowNetwork(e.currentTarget.checked)} />
            Allow network access
          </label>
        </fieldset>

        <Show when={structuralErrors().length > 0}>
          <ul class="space-y-1 text-xs" style={{ color: 'var(--color-danger-600)' }}>
            <For each={structuralErrors()}>{(msg) => <li>{msg}</li>}</For>
          </ul>
        </Show>

        <div class="flex justify-end gap-2 border-t pt-3" style={{ 'border-color': 'var(--color-border-light)' }}>
          <Button type="button" variant="secondary" onClick={props.onClose} disabled={submitting()}>
            Cancel
          </Button>
          <Button type="submit" loading={submitting()} disabled={!canSubmit()}>
            Run
          </Button>
        </div>
      </form>
    </Modal>
  );
};

const DefaultProvenanceNote: Component<{ provenance: ReturnType<typeof resolveDefaultProvenance> }> = (props) => (
  <p class="mt-1 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
    {props.provenance.status === 'resolved' ? props.provenance.description : props.provenance.reason}
  </p>
);

const CombinationGateNote: Component<{ gate: ReturnType<typeof gateHarnessModelSelection> }> = (props) => (
  <p class="flex items-start gap-1.5 text-xs" style={{ color: props.gate.advisory ? 'var(--color-warning-700)' : props.gate.allowed ? 'var(--color-success-700)' : 'var(--color-danger-600)' }}>
    <Show when={!props.gate.allowed}>
      <Badge tone="danger">Unsupported</Badge>
    </Show>
    <Show when={props.gate.allowed && props.gate.advisory}>
      <Badge tone="warning">Unverified</Badge>
    </Show>
    <Show when={props.gate.allowed && !props.gate.advisory}>
      <Badge tone="success">Supported</Badge>
    </Show>
    <span>{props.gate.reason}</span>
  </p>
);

export default RunWithAgentModal;
