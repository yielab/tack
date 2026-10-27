import { type Accessor, type Component, For, Show, createResource, createSignal } from 'solid-js';
import type { SetStoreFunction } from 'solid-js/store';
import { A } from '@solidjs/router';
import { Button, Field, FieldShell, HelpHint, Select, Badge } from '../ui';
import type { HarnessCapability } from '../execution/types';
import ModelField from './ModelField';
import { summarizeModels } from './models';
import { HELP } from '../help/texts';
import type { AgentProfileSummary } from '../execution';
import type { Project } from '../types';
import { api } from '../api';
import { installLine } from './install';
import { LEVEL_NEEDS_DOCKET, gateHarnessModelSelection } from './shared';
import type { RunForm } from './RunWithAgentModal';
import Prerequisite, { type PrerequisiteState } from './Prerequisite';

/** Claude Code's file, shell, web and sub-agent tools, from the names `claude mcp serve`
 *  lists (docs/plans/measurements/tools-claude-code.md). The rest of that list depends on
 *  the install's own features, so it is left to the free-text field beside this one. */
const CLAUDE_CODE_TOOLS = [
  'Agent', 'Bash', 'Read', 'Edit', 'Write', 'NotebookEdit', 'WebFetch', 'WebSearch',
];

/** Help under the tools field, per harness (docs/plans/measurements/tools-<harness>.md). */
const TOOLS_HELP: Record<string, string> = {
  'claude-code': 'Claude Code lists its tools; tick the ones to allow. The list can differ by version, so you can also type names, comma-separated.',
  codex: 'Codex exposes no list of its tools. Type names, comma-separated.',
  opencode: 'OpenCode exposes no list of its tools. Type names, comma-separated.',
  docket: 'Docket exposes no list of its tools. Type names, comma-separated.',
};

/** A tool list as a sentence: `*` is every tool. */
const describeTools = (tools: string[]) => (tools.includes('*') ? 'all tools' : tools.join(', '));

export interface RunFlowProps {
  form: RunForm;
  setForm: SetStoreFunction<RunForm>;
  /** A click on a profile pill, as opposed to the dialog's own choice. */
  onProfilePicked: (profile: AgentProfileSummary) => void;
  /** Said when the click replaced the agent; null otherwise. */
  agentNotice: Accessor<string | null>;
  projectId: string;
  /** The one line standing for the machine field when there is nothing to choose; null when there is. */
  fixedTarget: Accessor<string | null>;
  runnersLoading: Accessor<boolean>;
  targetOptions: Accessor<ReadonlyArray<{ value: string; label: string; disabled?: boolean }>>;
  target: Accessor<string>;
  onTarget: (value: string) => void;
  rows: { runner: Accessor<PrerequisiteState>; harness: Accessor<PrerequisiteState>; profile: Accessor<PrerequisiteState>; model: Accessor<PrerequisiteState> };
  /** Whether the card's brief has an acceptance check; undefined while it loads. Never blocks the run. */
  checkedCard?: Accessor<boolean | undefined>;
  profilesLoading: Accessor<boolean>;
  profiles: Accessor<AgentProfileSummary[]>;
  creatingProfile: Accessor<boolean>;
  createDefaultProfile: () => void;
  harnessOptions: Accessor<ReadonlyArray<{ value: string; label: string }>>;
  projectDefaultLabel: Accessor<string | null | undefined>;
  /** Every runner's report of the chosen agent: its models, its default, its documentation. */
  harnessReports: Accessor<HarnessCapability[]>;
  gate: Accessor<ReturnType<typeof gateHarnessModelSelection>>;
  project: Accessor<Project | undefined>;
  /** What the agent will read, from `GET /items/{id}/agent-context`. */
  agentContext: Accessor<string | undefined>;
  /** The token limit the selected profile sets; null when it sets none. */
  profileLimit: Accessor<number | null>;
  /** The server's sentence for the run's token limit and who applies it, as the dialog stands; absent while the field is invalid. */
  limitSentence?: Accessor<string | undefined>;
  /** Where the shown profile came from (`GET /items/{id}/agent-context`'s `profile_source`); null once the person picked another. */
  profileSource?: Accessor<string | null | undefined>;
  /** The tools the selected profile gives the selected agent; a blank tools field uses these. */
  profileTools: Accessor<string[]>;
  harnessLabel: Accessor<string>;
  /** The run carries a level, which only a docket run takes: every other agent is shown disabled, with that reason. */
  levelNeedsDocket?: boolean;
  /** The model as the summary line names it. */
  modelLabel: Accessor<string>;
  decisionsAttested: Accessor<boolean>;
  /** What the chosen agent's sandbox is, in words; null when the runner does not say. */
  sandboxLine: Accessor<{ text: string; unsandboxed: boolean } | null>;
  onApprovals: () => void;
  /** The person set the Web tools box, so a profile change no longer fills it. */
  onNetwork: () => void;
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

/** The dialog's sentence for the server's `profile_source`. */
export function describeProfileSource(source: string | null | undefined): string | null {
  if (!source) return null;
  if (source.startsWith('role:')) return `Chosen by the role ${source.slice(5)}`;
  switch (source) {
    case 'project_default': return 'Project default';
    case 'run_settings': return 'Saved on this task';
    case 'implementer': return 'The built-in Implementer';
    case 'ambiguous': return 'More than one role has a profile, so the project default is used';
    default: return null;
  }
}

/** The home prefix of an absolute path, shown as `~`. */
const tilde = (path: string) => path.replace(/^\/(?:home|Users)\/[^/]+(?=\/|$)/, '~');

/** The dialog as a pre-flight: what will run and where, with everything optional folded away. */
const RunFlow: Component<RunFlowProps> = (props) => {
  const [advanced, setAdvanced] = createSignal(false);
  // Only a folder on this computer can be read by the board; for anything else the runner decides.
  const [folderCheck] = createResource(
    () => {
      const p = props.project();
      return p && (p.code_origin === 'folder' || p.code_origin === 'new_folder') ? p.repository || null : null;
    },
    (path) => api.projects.checkFolder(path).catch(() => null),
  );
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
  const blocker = (): { text: string; href: string } | undefined => {
    if (props.project()?.code_origin === 'none') return { text: "Choose where this project's code is →", href: automation() };
    if (props.rows.runner() === 'missing') return { text: 'Turn on agent execution →', href: '/agents' };
  };
  const models = () => summarizeModels(props.harnessReports());
  // Only an agent that must be told a model blocks the run here; the dialog says so in place of Run.
  const modelBlocked = () => models().requiresModel && !props.gate().allowed;
  const modelReason = () => models().requiresModelReason ?? props.gate().reason;

  return (
    <>
      <Show
        when={props.fixedTarget()}
        fallback={
        <Select
          label="Machine or group"
          value={props.target()}
          onInput={(e) => props.onTarget(e.currentTarget.value)}
          options={[
            { value: '', label: props.runnersLoading() ? 'Loading…' : 'Select where this runs' },
            ...props.targetOptions(),
          ]}
        />
        }
      >
        {(line) => <p data-testid="run-target-fixed" class="px-4 text-sm" style={{ color: 'var(--color-text-secondary)' }}>{line()}</p>}
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

      <Show when={describeProfileSource(props.profileSource?.())}>
        {(text) => (
          <p data-testid="profile-source" class="-mt-3 px-4 text-xs" style={{ color: 'var(--color-text-secondary)' }}>{text()}</p>
        )}
      </Show>

      <Show when={!props.profilesLoading() && props.form.agentProfileId && props.form.toolsText.trim() === '' && props.profileTools().length === 0}>
        <p data-testid="no-tools" class="rounded-[20px] px-4 py-3 text-sm" style={{ 'background-color': 'var(--color-warning-100)', color: 'var(--color-warning-700)' }}>
          {profileName()} gives {props.harnessLabel()} no tools, so it can only answer in text: it cannot read or change files.{' '}
          <A href="/agents" class="font-semibold underline">Give the profile tools</A>, or name some under Advanced.
        </p>
      </Show>

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
        <div role="group" aria-label="Profile" class="space-y-3">
          <ProfileGroup
            title={props.profiles().some((p) => p.builtin) ? 'Your profiles' : undefined}
            profiles={sortedProfiles().filter((p) => !p.builtin)}
            selected={props.form.agentProfileId}
            onPick={(p) => { props.setForm('agentProfileId', p.agent_profile_id); props.onProfilePicked(p); }}
          />
          <ProfileGroup
            title={props.profiles().some((p) => !p.builtin) ? 'Built in' : undefined}
            profiles={sortedProfiles().filter((p) => p.builtin)}
            selected={props.form.agentProfileId}
            onPick={(p) => { props.setForm('agentProfileId', p.agent_profile_id); props.onProfilePicked(p); }}
          />
        </div>
        </FieldShell>
        <Show when={props.agentNotice()}>
          {(text) => <p data-testid="profile-agent-notice" class="-mt-3 px-4 text-xs" style={{ color: 'var(--color-text-secondary)' }}>{text()}</p>}
        </Show>
      </Show>
      <Show when={props.rows.profile() === 'missing'}>
        <Prerequisite state="missing" label="An agent profile exists" href="/agents" fixLabel="Add an agent profile" />
      </Show>
      <Show when={props.rows.harness() === 'missing'}>
        <Prerequisite state="missing" label="The agent is installed and signed in" href="/agents" fixLabel="Check the agent" />
      </Show>
      <Show when={props.checkedCard?.() === false}>
        <Prerequisite state="notice" label="This card has no acceptance check: nothing tests what the agent delivers, so a person reviews it" />
      </Show>
      <Show when={props.project()}>
        {(project) => (
          <p data-testid="run-dialog-install" class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
            {installLine(project().prepare_command, folderCheck(), props.profiles().find((p) => p.agent_profile_id === props.form.agentProfileId)?.runs_commands)}
          </p>
        )}
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
            onChange={(e) => { props.onApprovals(); props.setForm('approvals', e.currentTarget.checked ? 'ask' : 'auto'); }}
          />
          Ask before each action
        </label>
        <HelpHint label="Ask before each action" help={HELP.askBeforeEachAction} />
        </div>
        <Show when={props.sandboxLine()}>
          {(line) => (
            <div class="flex items-center gap-2 pl-4 text-xs" data-testid="sandbox-line" style={{ color: 'var(--color-text-tertiary)' }}>
              <span>{line().text}</span>
              <HelpHint label="Sandbox" help={HELP.sandbox} />
            </div>
          )}
        </Show>
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
            onChange={(e) => { props.onNetwork(); props.setForm('allowNetwork', e.currentTarget.checked); }}
          />
          Web tools
        </label>
        <HelpHint label="Web tools" help={HELP.webTools} />
        </div>
      </div>

      <details open={advanced() || modelBlocked()} onToggle={(e) => setAdvanced(e.currentTarget.open)} class="space-y-3">
        <summary class={SUMMARY_CLASS} style={{ color: 'var(--color-text-primary)' }}>Advanced for this run</summary>
        <div class="mt-3 space-y-3">
          <Select
            label="Agent"
            value={props.form.harnessKind}
            onInput={(e) => props.setForm('harnessKind', e.currentTarget.value)}
            options={props.harnessOptions().map((h) => {
              const blocked = props.levelNeedsDocket && h.value !== 'docket';
              return { value: h.value, label: blocked ? `${h.label} — ${LEVEL_NEEDS_DOCKET}` : h.label, disabled: blocked };
            })}
            hint={props.levelNeedsDocket ? LEVEL_NEEDS_DOCKET : undefined}
          />
          <ModelField
            agentLabel={props.harnessLabel()}
            reports={props.harnessReports()}
            value={props.form.model}
            onChange={(model) => props.setForm('model', model)}
            onType={(model) => props.setForm('model', model)}
          />
          <Show when={props.form.model.trim() === '' ? props.projectDefaultLabel() : null}>
            {(label) => (
              <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>Project default — {label()}</p>
            )}
          </Show>
          <ModelGateNote gate={props.gate()} />
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
            placeholder={props.profileTools().length > 0 ? describeTools(props.profileTools()) : undefined}
            hint={`${TOOLS_HELP[props.form.harnessKind] ?? 'Comma-separated.'} ${props.profileTools().length > 0
              ? `Leave blank to use the profile's: ${describeTools(props.profileTools())}.`
              : 'The profile gives this agent none.'}`}
          />
          <Field
            label="Token limit"
            type="number"
            min="1"
            value={props.form.tokenLimit}
            placeholder="No limit"
            onInput={(e) => props.setForm({ tokenLimit: e.currentTarget.value, tokenLimitEdited: true })}
            hint={props.profileLimit() !== null && !props.form.tokenLimitEdited
              ? `From the profile ${profileName()}. Clear it for no limit.`
              : 'Leave empty for no limit.'}
          />
          <Show when={props.limitSentence?.()}>
            {(sentence) => (
              <p data-testid="run-dialog-limit" class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                {sentence()}
              </p>
            )}
          </Show>
          <Field
            label="Timeout (seconds)"
            type="number"
            min="1"
            value={props.form.timeoutSeconds}
            onInput={(e) => props.setForm({ timeoutSeconds: Number(e.currentTarget.value), timeoutEdited: true })}
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
            <Show
              when={modelBlocked()}
              fallback={
                <Button type="submit" loading={props.submitting()} disabled={!props.canSubmit()}>
                  Run
                </Button>
              }
            >
              <Prerequisite state="missing" label={modelReason()} />
            </Show>
          }
        >
          {(b) => (
            <A href={b().href} class="text-sm font-semibold hover:underline" style={LINK}>{b().text}</A>
          )}
        </Show>
      </div>
    </>
  );
};

/** Profiles as pills, each with the line saying what it delivers beneath it. */
const ProfileGroup: Component<{
  title?: string;
  profiles: AgentProfileSummary[];
  selected: string;
  onPick: (profile: AgentProfileSummary) => void;
}> = (props) => (
  <Show when={props.profiles.length > 0}>
    <div class="space-y-1.5">
      <Show when={props.title}>
        <p class="text-xs font-semibold" style={{ color: 'var(--color-text-secondary)' }}>{props.title}</p>
      </Show>
      <div class="flex flex-wrap gap-x-3 gap-y-2">
        <For each={props.profiles}>
          {(p) => (
            <div class="flex max-w-56 flex-col items-start gap-0.5">
              <button
                type="button"
                title={p.summary}
                aria-pressed={props.selected === p.agent_profile_id}
                aria-describedby={p.delivers ? `delivers-${p.agent_profile_id}` : undefined}
                class="rounded-full px-4 py-1.5 text-sm font-semibold"
                style={props.selected === p.agent_profile_id
                  ? { 'background-color': 'var(--color-accent-soft)', color: 'var(--color-accent-ink)' }
                  : PILL}
                onClick={() => props.onPick(p)}
              >
                {p.name}
              </button>
              <Show when={p.delivers}>
                <span id={`delivers-${p.agent_profile_id}`} class="px-2 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>{p.delivers}</span>
              </Show>
            </div>
          )}
        </For>
      </div>
    </div>
  </Show>
);

const ModelGateNote: Component<{ gate: ReturnType<typeof gateHarnessModelSelection> }> = (props) => (
  <div class="space-y-1.5">
    <p class="flex items-start gap-2 text-xs" style={{ color: props.gate.allowed ? 'var(--color-success-700)' : 'var(--color-danger-600)' }}>
      <Show when={!props.gate.allowed}><Badge tone="danger">Unsupported</Badge></Show>
      <Show when={props.gate.allowed}><Badge tone="success">Supported</Badge></Show>
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
