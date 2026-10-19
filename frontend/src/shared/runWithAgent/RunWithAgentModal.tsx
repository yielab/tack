import { createStore } from 'solid-js/store';
import { type Component, createSignal, createResource, createMemo, createEffect, onCleanup, Show } from 'solid-js';
import { A } from '@solidjs/router';
import { Modal } from '../ui';
import { toast } from '../ui/toast';
import { BOOK } from '../help/routes';
import { api, ApiError } from '../api';
import type { AgentContextRun } from '../api/briefs';
import {
  fleetsApi, agentProfilesApi, runnersApi, listReportedHarnessKinds,
  type AgentProfileSummary, type RunnerCapabilities, type RunnerSummary,
} from '../execution';
import { useExecutionStore } from '../state/executionContext';
import {
  HARNESS_KINDS, buildCreateExecutionInput, generateIdempotencyKey, gateHarnessModelSelection, isActiveRunnerState,
  isAnsweringRunner, silentRunnerLabel, profileAgents, shouldHideTargetPicker, isExecutionOff, resolveRunModel,
  isDecisionsAttested, profileNetworkFor, profileToolsFor,
  type RunWithAgentFormValues, type RunWithAgentModalProps,
} from './shared';
import RunFlow from './RunFlow';
import { summarizeModels } from './models';
import { agentSafety } from '../help/texts';

const LIMIT_DEBOUNCE_MS = 300;

/** A profile's `limits` as the dialog reads them: a token limit and a time limit, each only when it is a positive whole number. */
function profileLimits(limits: unknown): { tokens: number | null; timeoutSeconds: number | null } {
  const l = (limits ?? {}) as { budgets?: { tokens?: unknown }; timeout_seconds?: unknown };
  const positive = (v: unknown) => (typeof v === 'number' && Number.isInteger(v) && v > 0 ? v : null);
  return { tokens: positive(l.budgets?.tokens), timeoutSeconds: positive(l.timeout_seconds) };
}

const initialForm = () => ({
  selectorKind: 'fleet' as 'fleet' | 'exact_runner', selectorId: '', agentProfileId: '', harnessKind: HARNESS_KINDS[0].value,
  model: '', timeoutSeconds: 3600, tokenLimit: '', tokenLimitEdited: false, timeoutEdited: false, allowNetwork: false, approvals: 'auto' as 'auto' | 'ask', toolsText: '',
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
  const [fleets] = createResource(open, () => fleetsApi.list().then((r) => r.data.data));
  const [agentProfiles, { refetch: refetchAgentProfiles }] = createResource(open, () => agentProfilesApi.list().then((r) => r.data.data));
  const [liveRunners] = createResource(open, () => runnersApi.list().then((r) => r.data.data));

  const runnersData = (): RunnerSummary[] => (liveRunners.error !== undefined ? [] : (liveRunners() ?? []));
  const activeRunners = (): RunnerSummary[] => runnersData().filter((r) => isActiveRunnerState(r.state));
  // The ones whose heartbeat is current: the only machines a run can be sent to.
  const answeringRunners = (): RunnerSummary[] => activeRunners().filter(isAnsweringRunner);

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
  // What a click on a profile did to the agent; shown only while that choice stands.
  const [agentSwitch, setAgentSwitch] = createSignal<{ profileId: string; harness: string; text: string } | null>(null);
  // The profile the server would choose for this item and why; the dialog does not choose it.
  const [resolved] = createResource(
    () => (props.isOpen ? props.itemId : undefined),
    (id) => api.briefs.agentContext(id).then((r) => ({ id: r.profile_id ?? null, source: r.profile_source ?? null }), () => null),
  );
  // Whether something can test what the agent delivers: the brief's acceptance checks.
  // A card with no brief has none; any other failure says nothing rather than guess.
  const [checkedCard] = createResource(
    () => (props.isOpen ? props.itemId : undefined),
    (id) => api.briefs.get(id).then(
      (brief) => (brief.acceptance?.length ?? 0) > 0,
      (error) => (error instanceof ApiError && error.status === 404 ? false : undefined),
    ),
  );
  const [agentContext] = createResource(
    () => (props.isOpen ? { id: props.itemId, profileId: form.agentProfileId } : undefined),
    ({ id, profileId }) => api.briefs.agentContext(id, profileId || undefined).then((r) => ({ text: r.text, limits: profileLimits(r.limits) }), () => undefined),
  );
  // What the profile limits a run to; the fields start from it until the person edits them.
  createEffect(() => {
    const limits = agentContext()?.limits;
    if (!limits) return;
    if (!form.tokenLimitEdited) setForm('tokenLimit', limits.tokens === null ? '' : String(limits.tokens));
    const saved = item.error === undefined ? item()?.run_settings?.timeout_seconds : undefined;
    if (!form.timeoutEdited && typeof saved !== 'number') setForm('timeoutSeconds', limits.timeoutSeconds ?? initialForm().timeoutSeconds);
  });

  // A fresh form (and idempotency key) on every open, so a leftover selection
  // is never resubmitted against another item.
  let settingsApplied = false;
  createEffect(() => {
    if (!props.isOpen) return;
    settingsApplied = false;
    setProfilePicked(false);
    setAgentSwitch(null);
    setForm(initialForm());
    if (props.initialTokenLimit) setForm({ tokenLimit: String(props.initialTokenLimit), tokenLimitEdited: true });
  });

  createEffect(() => {
    if (!props.isOpen || form.selectorId) return;
    const runners = answeringRunners();
    if (runners.length === 1 && fleetsData().length === 0) {
      setForm('selectorKind', 'exact_runner');
      setForm('selectorId', runners[0].runner_id);
    }
  });


  const targetOptions = createMemo(() => [
    ...fleetsData().map((f) => ({ value: `fleet:${f.fleet_id}`, label: f.name })),
    ...activeRunners().map((r) => isAnsweringRunner(r)
      ? { value: `exact_runner:${r.runner_id}`, label: r.name }
      : { value: `exact_runner:${r.runner_id}`, label: silentRunnerLabel(r.name, r.last_heartbeat_at), disabled: true }),
  ]);
  // One answering machine and no group: nothing to choose, so the field is a line saying where it runs.
  const fixedTarget = (): string | null => {
    if (!shouldHideTargetPicker(activeRunners().length, fleetsData().length)) return null;
    const runner = answeringRunners()[0];
    if (!runner) return null;
    return runner.name.startsWith('This computer') ? 'Runs on this computer' : `Runs on ${runner.name}`;
  };

  const targetCapabilities = createMemo((): RunnerCapabilities[] => {
    const kind = form.selectorKind;
    const id = form.selectorId;
    if (!id) return [];
    const runners = answeringRunners().filter((r) => (kind === 'exact_runner' ? r.runner_id === id : r.fleet_ids.includes(id)));
    return runners.map(runnerSummaryToCapabilities).filter((c): c is RunnerCapabilities => c !== null);
  });

  const harnessOptions = createMemo(() => {
    const reported = liveRunners.loading ? [] : listReportedHarnessKinds(targetCapabilities());
    // Every harness the target reports is offered: the ones `HARNESS_KINDS`
    // names, with their labels, then any other reported kind under its wire
    // name (docket, opencode), so a runner that reports one of those is not
    // hidden behind a list that predates their adapters.
    const named = HARNESS_KINDS.filter((h) => reported.includes(h.value));
    const unnamed = reported
      .filter((kind) => !HARNESS_KINDS.some((h) => h.value === kind))
      .map((kind) => ({ value: kind, label: kind }));
    const offered = [...named, ...unnamed];
    return offered.length > 0 ? offered : HARNESS_KINDS;
  });
  // The select can only show a listed harness; keep the form on one, so what is shown is what is sent.
  createEffect(() => {
    const options = harnessOptions();
    if (!options.some((h) => h.value === form.harnessKind)) setForm('harnessKind', options[0].value);
  });

  const selectedAgentProfile = createMemo(() => agentProfilesData().find((p) => p.agent_profile_id === form.agentProfileId));
  const timeoutDefault = () => agentContext()?.limits.timeoutSeconds ?? initialForm().timeoutSeconds;
  const typedTools = () => form.toolsText.split(',').map((t) => t.trim()).filter(Boolean);
  const profileTools = createMemo(() => profileToolsFor(selectedAgentProfile()?.tool_policy, form.harnessKind));
  // A blank field means the profile's tools; an empty list would leave the agent none.
  const runTools = () => (typedTools().length > 0 ? typedTools() : profileTools());

  createEffect(() => {
    if (!props.isOpen || form.agentProfileId) return;
    const profiles = agentProfilesData();
    const chosen = resolved()?.id;
    if (chosen && profiles.some((p) => p.agent_profile_id === chosen)) setForm('agentProfileId', chosen);
  });

  /** Where the shown profile came from, while it is still the server's choice. */
  const profileSource = () => (resolved()?.id === form.agentProfileId ? resolved()?.source ?? null : null);

  /** A click on a profile picks the agent it is written for, when this computer offers that agent. */
  const pickProfile = (profile: AgentProfileSummary) => {
    setProfilePicked(true);
    const agents = profileAgents(profile).filter((kind) => harnessOptions().some((h) => h.value === kind));
    if (agents.length === 0 || agents.includes(form.harnessKind)) return;
    const label = (kind: string) => harnessOptions().find((h) => h.value === kind)?.label ?? kind;
    const [next, was] = [agents[0], form.harnessKind];
    setForm('harnessKind', next);
    setAgentSwitch({ profileId: profile.agent_profile_id, harness: next, text: `${profile.name} runs on ${label(next)}, so the agent is now ${label(next)} instead of ${label(was)}.` });
  };

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

  // The project's default model applies only to runs of the project's default agent.
  const projectDefaultLabel = createMemo(() => resolveRunModel(null, form.harnessKind, project()));

  const targetHarnessCapability = createMemo(() =>
    targetCapabilities().flatMap((c) => c.harnesses).find((h) => h.harness_kind === form.harnessKind));
  // The reports of the chosen agent across the runners the run may land on.
  const chosenHarnessReports = createMemo(() =>
    capabilities().flatMap((c) => c.harnesses).filter((h) => h.harness_kind === form.harnessKind));
  const modelsReport = createMemo(() => summarizeModels(chosenHarnessReports()));
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
    if (typeof saved.network === 'boolean') { networkChosen = true; setForm('allowNetwork', saved.network); }
    if (saved.approvals === 'auto' || saved.approvals === 'ask') { approvalsChosen = true; setForm('approvals', saved.approvals); }
    if (typeof saved.verify === 'boolean') setForm('verify', saved.verify);
    if (typeof saved.model === 'string') setForm('model', saved.model);
  });

  // The Web tools box starts at what the chosen profile's role needs; a value the task remembers or
  // the person sets wins, and until then it follows the profile.
  const defaultNetwork = () => profileNetworkFor(selectedAgentProfile()?.tool_policy);
  let networkChosen = false;
  createEffect(() => { if (props.isOpen) networkChosen = false; });
  createEffect(() => { if (!networkChosen) setForm('allowNetwork', defaultNetwork()); });

  const decisionsAttested = createMemo(() => isDecisionsAttested(targetHarnessCapability()));

  const verifyConfigured = createMemo(() => capabilities().some((c) => c.verify_configured === true));
  const pushConfigured = createMemo(() => capabilities().some((c) => c.push_configured === true));

  // Where the agent is not sandboxed, it asks first; a value the task remembers or the person
  // picks wins over this default. A fleet is unsandboxed if any of its runners' chosen harness is.
  const unsandboxed = () => chosenHarnessReports().some((h) => h.sandbox === 'none');
  const sandboxLine = (): { text: string; unsandboxed: boolean } | null => {
    const safety = agentSafety(chosenHarnessReports());
    return safety && { text: safety.text, unsandboxed: safety.tone !== 'ok' };
  };
  const defaultApprovals = (): 'auto' | 'ask' => (unsandboxed() ? 'ask' : 'auto');
  let approvalsChosen = false;
  createEffect(() => { if (props.isOpen) approvalsChosen = false; });
  createEffect(() => { if (!approvalsChosen) setForm('approvals', defaultApprovals()); });
  createEffect(() => { if (!decisionsAttested() && form.approvals === 'ask') setForm('approvals', 'auto'); });

  /** What this form asks for; `null` names none, and the server fills the project's default. */
  const requestedModel = (): string | null => form.model.trim() || null;
  const runModel = () => resolveRunModel(requestedModel(), form.harnessKind, project());

  const modelGate = createMemo(() =>
    gateHarnessModelSelection(capabilities(), form.harnessKind, runModel(), `/projects/${props.projectId}/settings?tab=automation`));

  const structuralErrors = createMemo((): string[] => {
    const errors: string[] = [];
    if (!form.selectorId.trim()) errors.push('Select where this runs.');
    if (!form.agentProfileId) errors.push('Choose a profile.');
    if (!form.harnessKind) errors.push('Select an agent.');
    if (!Number.isFinite(form.timeoutSeconds) || form.timeoutSeconds <= 0) errors.push('Timeout must be a positive number of seconds.');
    if (form.tokenLimitEdited && tokenLimitValue() === undefined) errors.push('Token limit must be a whole number above zero, or empty for no limit.');
    return errors;
  });

  /** What the field asks for: `null` is no limit, `undefined` is a value that is no limit and no number. */
  const tokenLimitValue = (): number | null | undefined => {
    const text = form.tokenLimit.trim();
    if (text === '') return null;
    const n = Number(text);
    return Number.isInteger(n) && n > 0 ? n : undefined;
  };

  // The sentence about the run's token limit is the server's, asked for what the dialog holds now:
  // the agent and the limit. The typed value waits 300 ms, and the composed text is not refetched.
  const [limitInput, setLimitInput] = createSignal({ edited: false, text: '' });
  createEffect(() => {
    const next = { edited: form.tokenLimitEdited, text: form.tokenLimit };
    const timer = setTimeout(() => setLimitInput(next), LIMIT_DEBOUNCE_MS);
    onCleanup(() => clearTimeout(timer));
  });
  const [limitSentence] = createResource(
    () => {
      // No profile chosen yet: the dialog cannot run, and the profile it settles on may change the answer.
      if (!props.isOpen || !form.agentProfileId) return undefined;
      const { edited, text } = limitInput();
      const typed = text.trim();
      const run: AgentContextRun = { harness: form.harnessKind || undefined };
      if (edited && typed === '') run.noLimit = true;
      else if (edited) {
        const n = Number(typed);
        if (!Number.isInteger(n) || n <= 0) return undefined;
        run.tokens = n;
      }
      return { id: props.itemId, profileId: form.agentProfileId, run };
    },
    ({ id, profileId, run }) => api.briefs.agentContext(id, profileId, run).then((r) => r.limit?.sentence, () => undefined),
  );
  // An invalid field shows no sentence: the validation message speaks.
  const limitLine = () => (form.tokenLimitEdited && tokenLimitValue() === undefined ? undefined : limitSentence());

  const state = (missing: boolean): 'ok' | 'missing' => (missing ? 'missing' : 'ok');
  const rows = {
    runner: () => state(settled() && activeRunners().length === 0),
    harness: () => {
      const cap = targetHarnessCapability();
      return state(settled() && !!form.selectorId && (!cap || cap.probe_error != null));
    },
    profile: () => state(!agentProfiles.loading && agentProfilesData().length === 0),
    model: () => state(!modelGate().allowed),
  };
  const missingRow = () => Object.values(rows).some((r) => r() === 'missing');

  const canSubmit = createMemo(() => project()?.code_origin !== 'none' && structuralErrors().length === 0 && modelGate().allowed && !missingRow() && !submitting());

  const buildValues = (): RunWithAgentFormValues => {
    return {
      itemId: props.itemId,
      selectorKind: form.selectorKind,
      selectorId: form.selectorId.trim(),
      agentProfileId: form.agentProfileId,
      harnessKind: form.harnessKind,
      model: requestedModel(),
      timeoutSeconds: form.timeoutSeconds,
      // Sent only once the person changed it, so an untouched field leaves the choice to the profile.
      tokenLimit: form.tokenLimitEdited ? tokenLimitValue() : undefined,
      allowNetwork: form.allowNetwork,
      approvals: form.approvals,
      tools: runTools(),
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
    if (requestedModel()) diff.model = requestedModel();
    const branch = form.branch.trim();
    if (branch && branch !== p?.default_branch) diff.branch = branch;
    if (form.pushBranch !== (p?.push_after_run ?? d.pushBranch)) diff.push = form.pushBranch;
    if (form.timeoutSeconds !== timeoutDefault()) diff.timeout_seconds = form.timeoutSeconds;
    if (typedTools().length > 0) diff.tools = typedTools();
    if (form.allowNetwork !== defaultNetwork()) diff.network = form.allowNetwork;
    if (form.approvals !== defaultApprovals()) diff.approvals = form.approvals;
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
            onProfilePicked={pickProfile}
            agentNotice={() => { const s = agentSwitch(); return s && s.profileId === form.agentProfileId && s.harness === form.harnessKind ? s.text : null; }}
            projectId={props.projectId}
            fixedTarget={fixedTarget}
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
            checkedCard={() => (checkedCard.loading ? undefined : checkedCard())}
            profilesLoading={() => agentProfiles.loading}
            profiles={agentProfilesData}
            creatingProfile={creatingProfile}
            createDefaultProfile={createDefaultProfile}
            harnessOptions={harnessOptions}
            projectDefaultLabel={projectDefaultLabel}
            harnessReports={chosenHarnessReports}
            gate={modelGate}
            project={project}
            agentContext={() => agentContext()?.text}
            profileLimit={() => agentContext()?.limits.tokens ?? null}
            limitSentence={limitLine}
            profileSource={profileSource}
            profileTools={profileTools}
            harnessLabel={() => harnessOptions().find((h) => h.value === form.harnessKind)?.label ?? form.harnessKind}
            modelLabel={() => runModel() ?? (modelsReport().requiresModel ? 'no model chosen' : "the agent's default")}
            decisionsAttested={decisionsAttested}
            sandboxLine={sandboxLine}
            onApprovals={() => { approvalsChosen = true; }}
            onNetwork={() => { networkChosen = true; }}
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
