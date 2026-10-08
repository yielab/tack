import { createStore } from 'solid-js/store';
import { type Component, createSignal, createResource, createMemo, createEffect, Show } from 'solid-js';
import { A } from '@solidjs/router';
import { Modal } from '../ui';
import { toast } from '../ui/toast';
import { BOOK } from '../help/routes';
import { api } from '../api';
import {
  fleetsApi, agentProfilesApi, runnersApi, listModelCombinationsForHarness, listReportedHarnessKinds,
  type AgentProfileSummary, type AggregatedModelCombination, type RunnerCapabilities, type RunnerSummary,
} from '../execution';
import { useExecutionStore } from '../state/executionContext';
import {
  HARNESS_KINDS, buildCreateExecutionInput, generateIdempotencyKey, gateHarnessModelSelection, isActiveRunnerState,
  shouldHideTargetPicker, isExecutionOff, describeProjectModelDefault, projectDefaultModelPair,
  isDecisionsAttested, resolveAutoModelPolicy,
  type RunWithAgentFormValues, type RunWithAgentModalProps,
} from './shared';
import RunFlow, { CUSTOM_MODEL_VALUE, STATIC_MODEL_PREFIX } from './RunFlow';
import { ACCEPTED_MODELS } from './models';

const initialForm = () => ({
  selectorKind: 'fleet' as 'fleet' | 'exact_runner', selectorId: '', agentProfileId: '', harnessKind: HARNESS_KINDS[0].value,
  modelMode: 'auto' as 'project' | 'choose' | 'auto', modelModeInitialized: false, chooseIndex: '', viaGateway: false,
  customModelId: '', timeoutSeconds: 3600, allowNetwork: false, approvals: 'auto' as 'auto' | 'ask', toolsText: '',
  branch: '',
  idempotencyKey: generateIdempotencyKey(),
  verify: true, pushBranch: true,
});
export type RunForm = ReturnType<typeof initialForm>;

/** Adapts one `GET /runners` row to the `RunnerCapabilities` shape the gate
 *  expects (protocol/runner version live in sibling columns there). `null` for
 *  a runner with no complete parsed snapshot — skipped, never faked. */
function runnerSummaryToCapabilities(runner: RunnerSummary): RunnerCapabilities | null {
  const snap = runner.capability_snapshot as Omit<RunnerCapabilities, 'protocol_version' | 'runner_version'> | null;
  if (!snap || !Array.isArray(snap.harnesses) || !snap.concurrency || !snap.limits) return null;
  return { ...snap, protocol_version: runner.protocol_version, runner_version: runner.runner_version ?? '' };
}

/** The ONE shared "Run with agent" modal — Board, item-detail and Sprint all
 *  mount it, so all three create the same payload. State and gates live here;
 *  the body is `RunFlow`. */
const RunWithAgentModal: Component<RunWithAgentModalProps> = (props) => {
  const store = useExecutionStore();

  const open = () => (props.isOpen ? 'open' : undefined);
  const [project] = createResource(() => (props.isOpen ? props.projectId : undefined), (id) => api.projects.get(id));
  const [item] = createResource(() => (props.isOpen ? props.itemId : undefined), (id) => api.items.get(id));
  const [agentContext] = createResource(
    () => (props.isOpen ? props.itemId : undefined),
    (id) => api.briefs.agentContext(id).then((r) => r.text, () => undefined),
  );
  const [fleets] = createResource(open, () => fleetsApi.list().then((r) => r.data.data));
  const [agentProfiles, { refetch: refetchAgentProfiles }] = createResource(open, () => agentProfilesApi.list().then((r) => r.data.data));
  const [liveRunners] = createResource(open, () => runnersApi.list().then((r) => r.data.data));

  const runnersData = (): RunnerSummary[] => (liveRunners.error !== undefined ? [] : (liveRunners() ?? []));
  const activeRunners = (): RunnerSummary[] => runnersData().filter((r) => isActiveRunnerState(r.state));

  const capabilities = (): RunnerCapabilities[] => props.capabilities?.() ?? targetCapabilities();

  const fleetsData = () => (fleets.error !== undefined ? [] : (fleets() ?? []));
  const agentProfilesData = (): AgentProfileSummary[] =>
    agentProfiles.error !== undefined ? [] : (agentProfiles() ?? []);

  const settled = () => !liveRunners.loading && liveRunners.error === undefined;
  const executionOff = createMemo(() => settled() && isExecutionOff(activeRunners().length));

  const [creatingProfile, setCreatingProfile] = createSignal(false);
  const [submitting, setSubmitting] = createSignal(false);
  const [form, setForm] = createStore<RunForm>(initialForm());
  // Set only by a click on a profile pill: an auto-selected profile is not remembered.
  const [profilePicked, setProfilePicked] = createSignal(false);

  // A fresh form (and idempotency key) on every open, so a leftover selection
  // is never resubmitted against another item.
  let settingsApplied = false;
  createEffect(() => { if (props.isOpen) { settingsApplied = false; setProfilePicked(false); setForm(initialForm()); } });

  createEffect(() => {
    if (!props.isOpen || form.selectorId) return;
    const runners = activeRunners();
    if (runners.length === 1 && fleetsData().length === 0) {
      setForm('selectorKind', 'exact_runner');
      setForm('selectorId', runners[0].runner_id);
    }
  });


  const targetOptions = createMemo(() => [
    ...fleetsData().map((f) => ({ value: `fleet:${f.fleet_id}`, label: f.name })),
    ...activeRunners().map((r) => ({ value: `exact_runner:${r.runner_id}`, label: r.name })),
  ]);

  const targetCapabilities = createMemo((): RunnerCapabilities[] => {
    const kind = form.selectorKind;
    const id = form.selectorId;
    if (!id) return [];
    const runners = activeRunners().filter((r) => (kind === 'exact_runner' ? r.runner_id === id : r.fleet_ids.includes(id)));
    return runners.map(runnerSummaryToCapabilities).filter((c): c is RunnerCapabilities => c !== null);
  });

  const harnessOptions = createMemo(() => {
    const reported = liveRunners.loading ? [] : listReportedHarnessKinds(targetCapabilities());
    const filtered = HARNESS_KINDS.filter((h) => reported.includes(h.value));
    return filtered.length > 0 ? filtered : HARNESS_KINDS;
  });
  // The select can only show a listed harness; keep the form on one, so what is shown is what is sent.
  createEffect(() => {
    const options = harnessOptions();
    if (!options.some((h) => h.value === form.harnessKind)) setForm('harnessKind', options[0].value);
  });

  const selectedAgentProfile = createMemo(() => agentProfilesData().find((p) => p.agent_profile_id === form.agentProfileId));

  createEffect(() => {
    if (!props.isOpen || form.agentProfileId) return;
    const profiles = agentProfilesData();
    // The project's default profile, else the built-in Implementer (ADR 0074
    // decision 5), else the only profile there is.
    const preferred =
      profiles.find((p) => p.agent_profile_id === project()?.default_profile_id) ??
      profiles.find((p) => p.kind === 'implementer');
    if (preferred) setForm('agentProfileId', preferred.agent_profile_id);
    else if (profiles.length === 1) setForm('agentProfileId', profiles[0].agent_profile_id);
  });

  const createDefaultProfile = async () => {
    setCreatingProfile(true);
    try {
      const result = await agentProfilesApi.create({ name: 'Default', instructions: 'Complete the requested change and summarize what changed.', tool_policy: {} });
      await refetchAgentProfiles();
      setForm('agentProfileId', result.agent_profile_id);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to create the default agent profile.');
    } finally {
      setCreatingProfile(false);
    }
  };

  const projectDefaultLabel = createMemo(() => describeProjectModelDefault(project()?.default_model ?? null));

  createEffect(() => {
    if (!props.isOpen || form.modelModeInitialized || project.loading) return;
    setForm('modelMode', projectDefaultLabel() ? 'project' : 'auto');
    setForm('modelModeInitialized', true);
  });

  const modelCombos = createMemo<AggregatedModelCombination[]>(() => listModelCombinationsForHarness(targetCapabilities(), form.harnessKind));

  const targetHarnessCapability = createMemo(() =>
    targetCapabilities().flatMap((c) => c.harnesses).find((h) => h.harness_kind === form.harnessKind));
  // The measured list is the fallback: ids the runner already reports are not repeated.
  const staticModels = createMemo(() => {
    const reported = new Set(modelCombos().map((c) => c.model_id));
    return (ACCEPTED_MODELS[form.harnessKind] ?? []).filter((id) => !reported.has(id));
  });
  // A harness that needs a model opens on "Specific model", unless the project already names one.
  createEffect(() => {
    if (targetHarnessCapability()?.model_selection === 'required' && !projectDefaultLabel()) setForm('modelMode', 'choose');
  });
  // Start from the project's values, then overlay what this task remembered.
  createEffect(() => {
    if (!props.isOpen || settingsApplied || project.loading || item.loading || liveRunners.loading || agentProfiles.loading) return;
    settingsApplied = true;
    const p = project();
    const saved = item.error === undefined ? item()?.run_settings : null;
    if (p?.default_harness) setForm('harnessKind', p.default_harness);
    if (typeof p?.push_after_run === 'boolean') setForm('pushBranch', p.push_after_run);
    if (!saved) return;
    if (typeof saved.harness === 'string') setForm('harnessKind', saved.harness);
    if (typeof saved.agent_profile_id === 'string') setForm('agentProfileId', saved.agent_profile_id);
    if (typeof saved.branch === 'string') setForm('branch', saved.branch);
    if (typeof saved.push === 'boolean') setForm('pushBranch', saved.push);
    if (typeof saved.timeout_seconds === 'number') setForm('timeoutSeconds', saved.timeout_seconds);
    if (Array.isArray(saved.tools)) setForm('toolsText', saved.tools.join(', '));
    if (typeof saved.network === 'boolean') setForm('allowNetwork', saved.network);
    if (saved.approvals === 'auto' || saved.approvals === 'ask') setForm('approvals', saved.approvals);
    if (typeof saved.verify === 'boolean') setForm('verify', saved.verify);
    if (typeof saved.model_id === 'string') {
      const id = saved.model_id;
      const comboIndex = modelCombos().findIndex((c) => c.model_id === id && c.model_provider === saved.model_provider);
      setForm('modelModeInitialized', true);
      setForm('modelMode', 'choose');
      if (comboIndex >= 0) setForm('chooseIndex', String(comboIndex));
      else if (staticModels().includes(id)) setForm('chooseIndex', STATIC_MODEL_PREFIX + id);
      else { setForm('chooseIndex', CUSTOM_MODEL_VALUE); setForm('customModelId', id); }
    }
  });

  const decisionsAttested = createMemo(() => isDecisionsAttested(targetHarnessCapability()));

  const verifyConfigured = createMemo(() => capabilities().some((c) => c.verify_configured === true));
  const pushConfigured = createMemo(() => capabilities().some((c) => c.push_configured === true));

  createEffect(() => { if (!decisionsAttested() && form.approvals === 'ask') setForm('approvals', 'auto'); });

  const nativeProvider = () => targetHarnessCapability()?.native_provider;
  const gatewayProvider = () => targetHarnessCapability()?.providers?.find((p) => p !== nativeProvider());
  /** The provider is the harness's own, or the gateway's when chosen; never typed. */
  const derivedProvider = (): string | null =>
    (form.viaGateway ? gatewayProvider() : undefined) ?? nativeProvider() ?? null;
  const throughOptions = createMemo(() => {
    const native = nativeProvider();
    const gateway = gatewayProvider();
    if (!native || !gateway) return [];
    return [
      { value: native, label: native.charAt(0).toUpperCase() + native.slice(1) },
      { value: gateway, label: gateway === 'vercel-ai-gateway' ? 'Vercel AI Gateway' : gateway },
    ];
  });

  const modelPart = (key: 'provider' | 'id'): string | null => {
    if (form.modelMode === 'project') return projectDefaultModelPair(project()?.default_model ?? null)[key];
    if (form.modelMode !== 'choose') return null;
    if (form.chooseIndex === CUSTOM_MODEL_VALUE) return key === 'provider' ? derivedProvider() : (form.customModelId.trim() || null);
    if (form.chooseIndex.startsWith(STATIC_MODEL_PREFIX)) {
      return key === 'provider' ? derivedProvider() : form.chooseIndex.slice(STATIC_MODEL_PREFIX.length);
    }
    const combo = modelCombos()[Number(form.chooseIndex)];
    return (key === 'provider' ? combo?.model_provider : combo?.model_id) ?? null;
  };
  const modelProvider = () => modelPart('provider');
  const modelId = () => modelPart('id');

  const autoModelResolution = createMemo(() =>
    resolveAutoModelPolicy(
      selectedAgentProfile()?.limits,
      project()?.default_model ?? null,
      form.selectorKind === 'fleet' ? fleetsData().find((f) => f.fleet_id === form.selectorId)?.default_policy : null,
    ),
  );

  const combinationGate = createMemo(() =>
    gateHarnessModelSelection(capabilities(), form.harnessKind, modelProvider(), modelId(), autoModelResolution(), `/projects/${props.projectId}/settings?tab=automation`));

  const structuralErrors = createMemo((): string[] => {
    const errors: string[] = [];
    if (!form.selectorId.trim()) errors.push('Select where this runs.');
    if (!form.agentProfileId) errors.push('Choose a profile.');
    if (!form.harnessKind) errors.push('Select an agent.');
    if (form.modelMode === 'choose' && form.chooseIndex === '') errors.push('Select a model, or use the agent\'s default.');
    if (form.modelMode === 'choose' && form.chooseIndex === CUSTOM_MODEL_VALUE && !form.customModelId.trim()) errors.push('Enter a model id, or pick one from the list.');
    if (!Number.isFinite(form.timeoutSeconds) || form.timeoutSeconds <= 0) errors.push('Timeout must be a positive number of seconds.');
    return errors;
  });

  const state = (missing: boolean): 'ok' | 'missing' => (missing ? 'missing' : 'ok');
  const rows = {
    runner: () => state(settled() && activeRunners().length === 0),
    harness: () => {
      const cap = targetHarnessCapability();
      return state(settled() && !!form.selectorId && (!cap || cap.probe_error != null));
    },
    profile: () => state(!agentProfiles.loading && agentProfilesData().length === 0),
    model: () => state(!combinationGate().allowed),
  };
  const missingRow = () => Object.values(rows).some((r) => r() === 'missing');

  const canSubmit = createMemo(() => project()?.code_origin !== 'none' && structuralErrors().length === 0 && combinationGate().allowed && !missingRow() && !submitting());

  const buildValues = (): RunWithAgentFormValues => {
    const profile = selectedAgentProfile();
    return {
      itemId: props.itemId,
      selectorKind: form.selectorKind,
      selectorId: form.selectorId.trim(),
      agentProfileId: form.agentProfileId,
      agentProfileSnapshot: { name: profile?.name ?? '', instructions: profile?.instructions ?? '', tool_policy: profile?.tool_policy ?? {} },
      harnessKind: form.harnessKind,
      modelProvider: modelProvider(),
      modelId: modelId(),
      timeoutSeconds: form.timeoutSeconds,
      allowNetwork: form.allowNetwork,
      approvals: form.approvals,
      tools: form.toolsText.split(',').map((t) => t.trim()).filter(Boolean),
      // The server fills the snapshot from the project; only a branch override sends one.
      repository: form.branch.trim()
        ? { kind: 'git', remote: project()?.repository ?? '', baseRevision: form.branch.trim(), subdirectory: null }
        : undefined,
      idempotencyKey: form.idempotencyKey,
      verify: verifyConfigured() ? form.verify : undefined,
      pushBranch: pushConfigured() ? form.pushBranch : undefined,
    };
  };

  /** The keys that differ from the project's value (or the form's own default). */
  const settingsDiff = (): Record<string, unknown> => {
    const p = project();
    const d = initialForm();
    const diff: Record<string, unknown> = {};
    if (form.harnessKind !== (p?.default_harness || d.harnessKind)) diff.harness = form.harnessKind;
    const savedProfile = item.error === undefined && typeof item()?.run_settings?.agent_profile_id === 'string';
    if (form.agentProfileId && (profilePicked() || savedProfile)) diff.agent_profile_id = form.agentProfileId;
    if (form.modelMode === 'choose' && modelProvider() && modelId()) { diff.model_provider = modelProvider(); diff.model_id = modelId(); }
    const branch = form.branch.trim();
    if (branch && branch !== p?.default_branch) diff.branch = branch;
    if (form.pushBranch !== (p?.push_after_run ?? d.pushBranch)) diff.push = form.pushBranch;
    if (form.timeoutSeconds !== d.timeoutSeconds) diff.timeout_seconds = form.timeoutSeconds;
    const tools = form.toolsText.split(',').map((t) => t.trim()).filter(Boolean);
    if (tools.length > 0) diff.tools = tools;
    if (form.allowNetwork !== d.allowNetwork) diff.network = form.allowNetwork;
    if (form.approvals !== d.approvals) diff.approvals = form.approvals;
    if (form.verify !== d.verify) diff.verify = form.verify;
    return diff;
  };

  // The run already started: failing to remember must not fail it.
  const rememberSettings = async () => {
    const diff = settingsDiff();
    const had = item.error === undefined && !!item()?.run_settings;
    if (Object.keys(diff).length === 0 && !had) return;
    try {
      const run_settings = Object.keys(diff).length === 0 ? null : diff;
      await api.items.update(props.itemId, { run_settings });
    } catch {
      toast.error('The run started, but its settings could not be saved on the task.');
    }
  };

  const submit = async (e: Event) => {
    e.preventDefault();
    if (!canSubmit()) return;
    setSubmitting(true);
    try {
      const input = buildCreateExecutionInput(buildValues());
      const result = await store.create(input);
      toast.success(result.replayed ? 'Reused an existing run for this item.' : 'Run started.');
      await rememberSettings();
      props.onCreated?.(result.request_id);
      props.onClose();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to start the run.');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal isOpen={props.isOpen} onClose={props.onClose} title={`Run with agent: ${props.itemTitle}`} size="lg" help={BOOK.runDialog}>
      <Show
        when={!executionOff()}
        fallback={
          <div class="flex flex-col items-center gap-4 py-8 text-center">
            <p class="font-heading text-xl" style={{ color: 'var(--color-text-primary)' }}>
              Agent execution is off.
            </p>
            <A
              href="/agents"
              class="inline-flex items-center gap-1 rounded-full px-4 py-2 text-sm font-semibold focus:outline-none focus-visible:ring-2"
              style={{
                'background-color': 'var(--color-accent-soft)',
                color: 'var(--color-accent-ink)',
                '--tw-ring-color': 'var(--color-focus-ring)',
              }}
            >
              Turn it on
            </A>
          </div>
        }
      >
        <form class="space-y-6" onSubmit={submit}>
          <RunFlow
            form={form}
            setForm={setForm}
            onProfilePicked={() => setProfilePicked(true)}
            projectId={props.projectId}
            hideTargetPicker={() => shouldHideTargetPicker(activeRunners().length, fleetsData().length)}
            runnersLoading={() => liveRunners.loading}
            targetOptions={targetOptions}
            target={() => (form.selectorId ? `${form.selectorKind}:${form.selectorId}` : '')}
            onTarget={(value) => {
              const [kind, id] = value.split(/:(.*)/s);
              const ok = kind === 'fleet' || kind === 'exact_runner';
              if (ok) setForm('selectorKind', kind);
              setForm('selectorId', ok ? (id ?? '') : '');
            }}
            rows={rows}
            profilesLoading={() => agentProfiles.loading}
            profiles={agentProfilesData}
            creatingProfile={creatingProfile}
            createDefaultProfile={createDefaultProfile}
            harnessOptions={harnessOptions}
            projectDefaultLabel={projectDefaultLabel}
            modelCombos={modelCombos}
            throughOptions={throughOptions}
            through={derivedProvider}
            staticModels={staticModels}
            gate={combinationGate}
            project={project}
            agentContext={() => agentContext()}
            modelLabel={() => modelId() ?? "the agent's default"}
            decisionsAttested={decisionsAttested}
            verifyConfigured={verifyConfigured}
            pushConfigured={pushConfigured}
            errors={structuralErrors}
            submitting={submitting}
            canSubmit={canSubmit}
            onClose={props.onClose}
          />
        </form>
      </Show>
    </Modal>
  );
};

export default RunWithAgentModal;
