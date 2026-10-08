import { type Accessor, type Component, For, Show } from 'solid-js';
import type { SetStoreFunction } from 'solid-js/store';
import { A } from '@solidjs/router';
import { Button, Field, Select, Badge } from '../ui';
import type { AgentProfileSummary, AggregatedModelCombination } from '../execution';
import { gateHarnessModelSelection } from './shared';
import RadioRow from './RadioRow';
import type { RunForm } from './RunWithAgentModal';
import Prerequisite, { type PrerequisiteState } from './Prerequisite';

/** "Other…": a model id typed by hand. */
export const CUSTOM_MODEL_VALUE = '__custom__';
/** Prefix of a Select value naming an id from the measured list rather than a reported combination. */
export const STATIC_MODEL_PREFIX = 'static:';

/** Claude Code's file, shell, web and sub-agent tools, from the names `claude mcp serve`
 *  lists (docs/plans/measurements/tools-claude-code.md). The rest of that list depends on
 *  the install's own features, so it is left to the free-text field beside this one. */
const CLAUDE_CODE_TOOLS = [
  'Agent', 'Bash', 'Read', 'Edit', 'Write', 'NotebookEdit', 'WebFetch', 'WebSearch',
];

/** Help under the tools field, per harness (docs/plans/measurements/tools-<harness>.md). */
const TOOLS_HELP: Record<string, string> = {
  'claude-code': 'Claude Code lists its tools; tick the ones to allow. The list can differ by version, so you can also type names, comma-separated.',
  codex: 'Codex exposes no list of its tools. Type names, comma-separated. Leave blank for none.',
  opencode: 'OpenCode exposes no list of its tools. Type names, comma-separated. Leave blank for none.',
  docket: 'Docket exposes no list of its tools. Type names, comma-separated. Leave blank for none.',
};

export interface RunFlowProps {
  form: RunForm;
  setForm: SetStoreFunction<RunForm>;
  itemTitle: string;
  hasBrief: Accessor<boolean>;
  projectId: string;
  hideTargetPicker: Accessor<boolean>;
  runnersLoading: Accessor<boolean>;
  targetOptions: Accessor<ReadonlyArray<{ value: string; label: string }>>;
  target: Accessor<string>;
  onTarget: (value: string) => void;
  rows: { runner: Accessor<PrerequisiteState>; harness: Accessor<PrerequisiteState>; profile: Accessor<PrerequisiteState>; model: Accessor<PrerequisiteState> };
  profilesLoading: Accessor<boolean>;
  profiles: Accessor<AgentProfileSummary[]>;
  creatingProfile: Accessor<boolean>;
  createDefaultProfile: () => void;
  harnessOptions: Accessor<ReadonlyArray<{ value: string; label: string }>>;
  projectDefaultLabel: Accessor<string | null | undefined>;
  modelCombos: Accessor<AggregatedModelCombination[]>;
  /** Native provider and gateway, when the harness lists both; empty otherwise. */
  throughOptions: Accessor<ReadonlyArray<{ value: string; label: string }>>;
  through: Accessor<string | null>;
  /** Measured ids for this harness that the runner did not already report. */
  staticModels: Accessor<string[]>;
  gate: Accessor<ReturnType<typeof gateHarnessModelSelection>>;
  repoSummary: Accessor<string>;
  decisionsAttested: Accessor<boolean>;
  /** The selected runner's own config has a verifier / pushes branches. */
  verifyConfigured: Accessor<boolean>;
  pushConfigured: Accessor<boolean>;
  errors: Accessor<string[]>;
  submitting: Accessor<boolean>;
  canSubmit: Accessor<boolean>;
  onClose: () => void;
}

const LEGEND_CLASS = 'mb-1 font-heading text-lg';
const legendStyle = { color: 'var(--color-text-primary)' };
const PILL = { 'background-color': 'var(--color-bg-app)', color: 'var(--color-text-primary)' };

/** The dialog's body as a flow, top to bottom: who runs it, what it gets, how far it may go, what happens after. */
const RunFlow: Component<RunFlowProps> = (props) => {
  const tools = () => props.form.toolsText.split(',').map((t) => t.trim()).filter(Boolean);
  const toggleTool = (name: string, on: boolean) =>
    props.setForm('toolsText', (on ? [...tools(), name] : tools().filter((t) => t !== name)).join(', '));

  return (
    <>
      {/* ── Who runs it ─────────────────────────────────────────────── */}
      <fieldset class="space-y-3">
        <legend class={LEGEND_CLASS} style={legendStyle}>Who runs it</legend>
        <Show when={!props.hideTargetPicker()}>
          <Select
            label="Machine or group"
            value={props.target()}
            onInput={(e) => props.onTarget(e.currentTarget.value)}
            options={[
              { value: '', label: props.runnersLoading() ? 'Loading…' : 'Select where this runs' },
              ...props.targetOptions(),
            ]}
          />
        </Show>
        <Prerequisite state={props.rows.runner()} label="A runner is connected" href="/agents" fixLabel="Connect a runner" />
        <Show
          when={props.profilesLoading() || props.profiles().length > 0}
          fallback={
            <div class="flex flex-wrap items-center justify-between gap-3 rounded-[20px] px-4 py-3" style={PILL}>
              <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>No agent profile exists yet.</p>
              <Button size="sm" variant="secondary" loading={props.creatingProfile()} onClick={props.createDefaultProfile}>
                Create default profile
              </Button>
            </div>
          }
        >
          <Select
            label="Agent profile"
            value={props.form.agentProfileId}
            onInput={(e) => props.setForm('agentProfileId', e.currentTarget.value)}
            options={[
              { value: '', label: props.profilesLoading() ? 'Loading…' : 'Select an agent profile' },
              ...props.profiles().map((p) => ({ value: p.agent_profile_id, label: p.name })),
            ]}
          />
        </Show>
        <Prerequisite state={props.rows.profile()} label="An agent profile exists" href="/agents" fixLabel="Add an agent profile" />
        <Select
          label="Harness"
          value={props.form.harnessKind}
          onInput={(e) => props.setForm('harnessKind', e.currentTarget.value)}
          options={props.harnessOptions().map((h) => ({ value: h.value, label: h.label }))}
        />
        <Prerequisite state={props.rows.harness()} label="The harness is installed and signed in" href="/agents" fixLabel="Check the harness" />
        <div class="flex flex-col gap-2">
          <RadioRow name="model-mode" checked={props.form.modelMode !== 'choose'} onChange={() => props.setForm('modelMode', props.projectDefaultLabel() ? 'project' : 'auto')}>
            The agent's default (recommended)
          </RadioRow>
          <Show when={props.projectDefaultLabel()}>
            {(label) => (
              <p class="pl-7 text-xs" style={{ color: 'var(--color-text-secondary)' }}>Project default — {label()}</p>
            )}
          </Show>
          <RadioRow name="model-mode" checked={props.form.modelMode === 'choose'} onChange={() => props.setForm('modelMode', 'choose')}>
            Specific model
          </RadioRow>
        </div>
        <Show when={props.form.modelMode === 'choose'}>
          <Select
            label="Model"
            value={props.form.chooseIndex}
            onInput={(e) => props.setForm('chooseIndex', e.currentTarget.value)}
            options={[
              { value: '', label: 'Select a model' },
              ...props.modelCombos().map((c, i) => ({
                value: String(i),
                label: `${c.model_provider} / ${c.model_id} (${c.supportingRunnerCount} runner${c.supportingRunnerCount === 1 ? '' : 's'})`,
              })),
              ...props.staticModels().map((id) => ({ value: `${STATIC_MODEL_PREFIX}${id}`, label: id })),
              { value: CUSTOM_MODEL_VALUE, label: 'Other…' },
            ]}
          />
          <Show when={props.form.chooseIndex === CUSTOM_MODEL_VALUE}>
            <Field label="Model id" value={props.form.customModelId} onInput={(e) => props.setForm('customModelId', e.currentTarget.value)} />
            <Show when={props.throughOptions().length > 0}>
              <Select
                label="Through"
                value={props.through() ?? ''}
                onInput={(e) => props.setForm('viaGateway', e.currentTarget.value !== props.throughOptions()[0].value)}
                options={props.throughOptions().map((o) => ({ value: o.value, label: o.label }))}
              />
            </Show>
          </Show>
        </Show>
        <CombinationGateNote gate={props.gate()} />
        <Prerequisite
          state={props.rows.model()}
          label="A model is set for this run"
          href={props.gate().fix?.href ?? `/projects/${props.projectId}/settings?tab=automation`}
          fixLabel="Set the project's model"
        />
      </fieldset>

      {/* ── What it gets ────────────────────────────────────────────── */}
      <fieldset class="space-y-3">
        <legend class={LEGEND_CLASS} style={legendStyle}>What it gets</legend>
        <p class="rounded-[20px] px-4 py-2.5 text-sm" style={PILL}>
          <span class="text-xs font-semibold" style={{ color: 'var(--color-text-secondary)' }}>Item: </span>
          {props.itemTitle}
        </p>
        <Show
          when={props.hasBrief()}
          fallback={
            <p class="rounded-[20px] px-4 py-2.5 text-xs" style={{ ...PILL, color: 'var(--color-text-secondary)' }}>
              This item has no brief. The agent gets its title and description.
            </p>
          }
        >
          <Prerequisite state="ok" label="The item's brief goes with the title and description" />
        </Show>
        <Show
          when={props.form.repoExpanded}
          fallback={
            <div class="flex items-center justify-between gap-3 rounded-full py-2 pl-4 pr-2 text-xs" style={{ ...PILL, color: 'var(--color-text-secondary)' }}>
              <span class="min-w-0 break-words">{props.repoSummary()}</span>
              <Button type="button" size="sm" variant="secondary" onClick={() => props.setForm('repoExpanded', true)}>
                Change for this run
              </Button>
            </div>
          }
        >
          <div class="grid grid-cols-2 gap-3">
            <Field label="Kind" value={props.form.repoKind} onInput={(e) => props.setForm('repoKind', e.currentTarget.value)} />
            <Field label="Base revision" value={props.form.repoBaseRevision} onInput={(e) => props.setForm('repoBaseRevision', e.currentTarget.value)} placeholder="main" />
          </div>
          <Field label="Remote" value={props.form.repoRemote} onInput={(e) => props.setForm('repoRemote', e.currentTarget.value)} placeholder="git@github.com:org/repo.git" />
          <Field label="Subdirectory" value={props.form.repoSubdirectory} onInput={(e) => props.setForm('repoSubdirectory', e.currentTarget.value)} hint="Optional." />
        </Show>
      </fieldset>

      {/* ── How far it may go ───────────────────────────────────────── */}
      <fieldset class="space-y-3">
        <legend class={LEGEND_CLASS} style={legendStyle}>How far it may go</legend>
        <div class="space-y-2">
          <p id="run-approvals-label" class="text-xs font-semibold" style={{ color: 'var(--color-text-secondary)' }}>Approvals</p>
          <div role="radiogroup" aria-labelledby="run-approvals-label" class="grid grid-cols-2 gap-2">
            <RadioRow name="approvals" checked={props.form.approvals === 'auto'} onChange={() => props.setForm('approvals', 'auto')}>
              Automatic
            </RadioRow>
            <RadioRow name="approvals" checked={props.form.approvals === 'ask'} disabled={!props.decisionsAttested()} onChange={() => props.setForm('approvals', 'ask')}>
              Ask me
            </RadioRow>
          </div>
          <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
            <Show when={props.decisionsAttested()} fallback="This harness can't pause to ask — it always decides on its own.">
              Automatic lets the agent decide on its own. Ask me pauses before each tool call and waits for
              your answer in the run's decision inbox.
            </Show>
          </p>
        </div>
        <Show when={props.form.harnessKind === 'claude-code'}>
          <div class="grid grid-cols-2 gap-x-3 gap-y-1 text-xs sm:grid-cols-3" role="group" aria-label="Suggested tools">
            <For each={CLAUDE_CODE_TOOLS}>
              {(name) => (
                <label class="flex items-center gap-1.5">
                  <input type="checkbox" checked={tools().includes(name)} onChange={(e) => toggleTool(name, e.currentTarget.checked)} />
                  {name}
                </label>
              )}
            </For>
          </div>
        </Show>
        <Field
          label="Allowed tools"
          value={props.form.toolsText}
          onInput={(e) => props.setForm('toolsText', e.currentTarget.value)}
          hint={TOOLS_HELP[props.form.harnessKind] ?? 'Comma-separated. Leave blank for none.'}
        />
        <label class="flex cursor-pointer items-center gap-2.5 rounded-full px-4 py-2.5 text-sm" style={PILL}>
          <input
            type="checkbox"
            class="h-4 w-4"
            style={{ 'accent-color': 'var(--color-primary-600)' }}
            checked={props.form.allowNetwork}
            onChange={(e) => props.setForm('allowNetwork', e.currentTarget.checked)}
          />
          Allow network access
        </label>
        <Field
          label="Timeout (seconds)"
          type="number"
          min="1"
          value={props.form.timeoutSeconds}
          onInput={(e) => props.setForm('timeoutSeconds', Number(e.currentTarget.value))}
        />
      </fieldset>

      {/* ── What happens after ──────────────────────────────────────── */}
      <fieldset class="space-y-3">
        <legend class={LEGEND_CLASS} style={legendStyle}>What happens after</legend>
        <Prerequisite
          state={props.verifyConfigured() ? 'ok' : 'deferred'}
          label="Verify the result"
          checked={props.form.verify}
          onToggle={(on) => props.setForm('verify', on)}
          reason="This runner has no verifier set up. Add a [verify] section to its config to turn it on."
        />
        <Prerequisite
          state={props.pushConfigured() ? 'ok' : 'deferred'}
          label="Push the branch"
          checked={props.form.pushBranch}
          onToggle={(on) => props.setForm('pushBranch', on)}
          reason="This runner does not push branches. Set push_branches in its [git] config to turn it on."
        />
        <Prerequisite
          state="deferred"
          label="Open a pull request"
          reason={props.pushConfigured() ? 'Not chosen per run: Tack opens one for the pushed branch when the item is linked to a GitHub issue and a GitHub token is set.' : 'Needs a pushed branch, and this runner does not push branches.'}
        />
      </fieldset>

      <Show when={props.errors().length > 0}>
        <ul
          class="space-y-1 rounded-[20px] px-4 py-3 text-xs font-medium"
          style={{ 'background-color': 'var(--color-danger-100)', color: 'var(--color-danger-600)' }}
        >
          <For each={props.errors()}>{(msg) => <li>{msg}</li>}</For>
        </ul>
      </Show>

      <div class="flex justify-end gap-2 pt-1">
        <Button type="button" variant="secondary" onClick={props.onClose} disabled={props.submitting()}>
          Cancel
        </Button>
        <Button type="submit" loading={props.submitting()} disabled={!props.canSubmit()}>
          Run
        </Button>
      </div>
    </>
  );
};

const CombinationGateNote: Component<{ gate: ReturnType<typeof gateHarnessModelSelection> }> = (props) => (
  <div class="space-y-1.5">
    <p class="flex items-start gap-2 text-xs" style={{ color: props.gate.advisory ? 'var(--color-warning-700)' : props.gate.allowed ? 'var(--color-success-700)' : 'var(--color-danger-600)' }}>
      <Show when={!props.gate.allowed}><Badge tone="danger">Unsupported</Badge></Show>
      <Show when={props.gate.allowed && props.gate.advisory}><Badge tone="warning">Unverified</Badge></Show>
      <Show when={props.gate.allowed && !props.gate.advisory}><Badge tone="success">Supported</Badge></Show>
      <span class="pt-0.5">{props.gate.reason}</span>
    </p>
    <Show when={props.gate.fix}>
      {(fix) => (
        <A href={fix().href} class="inline-flex items-center gap-1 text-xs font-semibold hover:underline" style={{ color: 'var(--color-accent-ink)' }}>
          {fix().label}
        </A>
      )}
    </Show>
  </div>
);

export default RunFlow;
