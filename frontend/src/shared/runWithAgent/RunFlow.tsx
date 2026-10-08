import { type Accessor, type Component, For, Show, createSignal } from 'solid-js';
import type { SetStoreFunction } from 'solid-js/store';
import { A } from '@solidjs/router';
import { Button, Field, FieldShell, HelpHint, Select, Badge } from '../ui';
import { HELP } from '../help/texts';
import type { AgentProfileSummary, AggregatedModelCombination } from '../execution';
import type { Project } from '../types';
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
  /** A click on a profile pill, as opposed to the dialog's own choice. */
  onProfilePicked: () => void;
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
  project: Accessor<Project | undefined>;
  /** What the agent will read, from `GET /items/{id}/agent-context`. */
  agentContext: Accessor<string | undefined>;
  /** The model as the summary line names it. */
  modelLabel: Accessor<string>;
  decisionsAttested: Accessor<boolean>;
  /** The selected runner's own config has a verifier / pushes branches. */
  verifyConfigured: Accessor<boolean>;
  pushConfigured: Accessor<boolean>;
  errors: Accessor<string[]>;
  submitting: Accessor<boolean>;
  canSubmit: Accessor<boolean>;
  onClose: () => void;
}

const PILL = { 'background-color': 'var(--color-bg-app)', color: 'var(--color-text-primary)' };
const LINK = { color: 'var(--color-accent-ink)' };
const SUMMARY_CLASS = 'cursor-pointer font-heading text-lg';
const KIND_ORDER = ['implementer', 'reviewer', 'researcher', 'planner'];

/** The home prefix of an absolute path, shown as `~`. */
const tilde = (path: string) => path.replace(/^\/(?:home|Users)\/[^/]+(?=\/|$)/, '~');

/** The dialog as a pre-flight: what will run and where, with everything optional folded away. */
const RunFlow: Component<RunFlowProps> = (props) => {
  const [advanced, setAdvanced] = createSignal(false);
  const tools = () => props.form.toolsText.split(',').map((t) => t.trim()).filter(Boolean);
  const toggleTool = (name: string, on: boolean) =>
    props.setForm('toolsText', (on ? [...tools(), name] : tools().filter((t) => t !== name)).join(', '));

  const automation = () => `/projects/${props.projectId}/settings?tab=automation`;
  const folder = () => tilde(props.project()?.repository ?? '');
  const where = () => {
    const mode = props.project()?.workspace_mode;
    return mode === 'in_place' ? `in ${folder()}`
      : mode === 'clone' ? `in a fresh clone of ${folder()}`
        : `on a new branch of ${folder()}`;
  };
  const profileName = () => props.profiles().find((p) => p.agent_profile_id === props.form.agentProfileId)?.name ?? 'Choose a profile';
  const sortedProfiles = () => {
    const rank = (p: AgentProfileSummary) => { const i = KIND_ORDER.indexOf(p.kind ?? 'custom'); return i < 0 ? KIND_ORDER.length : i; };
    return [...props.profiles()].sort((a, b) => rank(a) - rank(b));
  };
  const blocker = (): { text: string; href?: string } | undefined => {
    if (props.project()?.code_origin === 'none') return { text: "Choose where this project's code is →", href: automation() };
    if (props.rows.runner() === 'missing') return { text: 'Turn on agent execution →', href: '/agents' };
    if (!props.gate().allowed || (props.form.modelMode === 'choose' && props.form.chooseIndex === '')) return { text: 'Choose a model for this run' };
  };

  return (
    <>
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

      <p data-testid="run-summary" class="rounded-[20px] px-4 py-3 text-sm" style={PILL}>
        <A href={automation()} class="font-semibold hover:underline" style={LINK}>{profileName()}</A>
        {' · '}
        <A href={automation()} class="hover:underline" style={LINK}>{props.modelLabel()}</A>
        <Show when={props.project()?.repository}>
          {' · '}
          <A href={automation()} class="hover:underline" style={LINK}>{where()}</A>
        </Show>
      </p>

      <details>
        <summary class={SUMMARY_CLASS} style={{ color: 'var(--color-text-primary)' }}>What the agent will read</summary>
        <pre class="mt-2 max-h-64 overflow-auto whitespace-pre-wrap rounded-[20px] px-4 py-3 text-xs" style={PILL}>{props.agentContext() ?? 'Not available.'}</pre>
      </details>

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
        <FieldShell label="Profile" help={HELP.profile}>
        <div role="group" aria-label="Profile" class="flex flex-wrap gap-2">
          <For each={sortedProfiles()}>
            {(p) => (
              <button
                type="button"
                title={p.summary}
                aria-pressed={props.form.agentProfileId === p.agent_profile_id}
                class="rounded-full px-4 py-1.5 text-sm font-semibold"
                style={props.form.agentProfileId === p.agent_profile_id
                  ? { 'background-color': 'var(--color-accent-soft)', color: 'var(--color-accent-ink)' }
                  : PILL}
                onClick={() => { props.setForm('agentProfileId', p.agent_profile_id); props.onProfilePicked(); }}
              >
                {p.name}
              </button>
            )}
          </For>
        </div>
        </FieldShell>
      </Show>
      <Show when={props.rows.profile() === 'missing'}>
        <Prerequisite state="missing" label="An agent profile exists" href="/agents" fixLabel="Add an agent profile" />
      </Show>
      <Show when={props.rows.harness() === 'missing'}>
        <Prerequisite state="missing" label="The agent is installed and signed in" href="/agents" fixLabel="Check the agent" />
      </Show>

      <div class="space-y-2">
        <div class="flex items-center gap-2">
        <label class="flex flex-1 cursor-pointer items-center gap-2.5 rounded-full px-4 py-2.5 text-sm" style={PILL}>
          <input
            type="checkbox"
            role="switch"
            name="approvals"
            class="h-4 w-4"
            style={{ 'accent-color': 'var(--color-primary-600)' }}
            checked={props.form.approvals === 'ask'}
            disabled={!props.decisionsAttested()}
            onChange={(e) => props.setForm('approvals', e.currentTarget.checked ? 'ask' : 'auto')}
          />
          Ask before each action
        </label>
        <HelpHint label="Ask before each action" help={HELP.askBeforeEachAction} />
        </div>
        <Show when={!props.decisionsAttested()}>
          <p class="pl-4 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>This agent can't pause to ask — it always decides on its own.</p>
        </Show>
        <div class="flex items-center gap-2">
        <label class="flex flex-1 cursor-pointer items-center gap-2.5 rounded-full px-4 py-2.5 text-sm" style={PILL}>
          <input
            type="checkbox"
            role="switch"
            class="h-4 w-4"
            style={{ 'accent-color': 'var(--color-primary-600)' }}
            checked={props.form.allowNetwork}
            onChange={(e) => props.setForm('allowNetwork', e.currentTarget.checked)}
          />
          Allow network access
        </label>
        <HelpHint label="Allow network access" help={HELP.allowNetwork} />
        </div>
      </div>

      <details open={advanced()} onToggle={(e) => setAdvanced(e.currentTarget.open)} class="space-y-3">
        <summary class={SUMMARY_CLASS} style={{ color: 'var(--color-text-primary)' }}>Advanced for this run</summary>
        <div class="mt-3 space-y-3">
          <Select
            label="Agent"
            value={props.form.harnessKind}
            onInput={(e) => props.setForm('harnessKind', e.currentTarget.value)}
            options={props.harnessOptions().map((h) => ({ value: h.value, label: h.label }))}
          />
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
              help={HELP.model}
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
          <Field
            label="Branch"
            value={props.form.branch}
            onInput={(e) => props.setForm('branch', e.currentTarget.value)}
            placeholder={props.project()?.default_branch ?? 'main'}
            hint="Leave blank to use the project's."
          />
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
          <Field
            label="Timeout (seconds)"
            type="number"
            min="1"
            value={props.form.timeoutSeconds}
            onInput={(e) => props.setForm('timeoutSeconds', Number(e.currentTarget.value))}
          />
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
        </div>
      </details>

      <Show when={props.errors().length > 0}>
        <ul
          class="space-y-1 rounded-[20px] px-4 py-3 text-xs font-medium"
          style={{ 'background-color': 'var(--color-danger-100)', color: 'var(--color-danger-600)' }}
        >
          <For each={props.errors()}>{(msg) => <li>{msg}</li>}</For>
        </ul>
      </Show>

      <div class="flex items-center justify-end gap-2 pt-1">
        <Button type="button" variant="secondary" onClick={props.onClose} disabled={props.submitting()}>
          Cancel
        </Button>
        <Show
          when={blocker()}
          fallback={
            <Button type="submit" loading={props.submitting()} disabled={!props.canSubmit()}>
              Run
            </Button>
          }
        >
          {(b) => (
            <Show
              when={b().href}
              fallback={
                <button type="button" class="text-sm font-semibold hover:underline" style={LINK} onClick={() => setAdvanced(true)}>
                  {b().text}
                </button>
              }
            >
              {(href) => <A href={href()} class="text-sm font-semibold hover:underline" style={LINK}>{b().text}</A>}
            </Show>
          )}
        </Show>
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
