import { type Accessor, type Component, type JSX, For, Show, createResource, createSignal } from 'solid-js';
import type { SetStoreFunction } from 'solid-js/store';
import { A } from '@solidjs/router';
import { Button, Field, FieldShell, HelpHint, Select, Badge, Switch } from '../ui';
import { IconBranch, IconChecks, IconFolder, IconGlobe, IconPackage, IconShield, IconWho } from './runDialogIcons';
import type { HarnessCapability } from '../execution/types';
import ModelField from './ModelField';
import { summarizeModels } from './models';
import { HELP } from '../help/texts';
import { teamOf, type AgentProfileSummary } from '../execution';
import type { Project } from '../types';
import { api } from '../api';
import { installLine } from './install';
import { gateHarnessModelSelection } from './shared';
import { spaced } from './runLimit';
import type { RunForm } from './RunWithAgentModal';
import Prerequisite, { type PrerequisiteState } from './Prerequisite';
import ProfilePicker, { type AgentChoice } from './ProfilePicker';
import ParityRules from './ParityRules';

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
  /** A click on a profile, as opposed to the dialog's own choice. */
  onProfilePicked: (profile: AgentProfileSummary) => void;
  /** The agents the picker offers, a disabled one with its reason. */
  agentChoices: Accessor<AgentChoice[]>;
  /** A click on an agent. */
  onAgent: (kind: string) => void;
  projectId: string;
  /** The one line standing for the machine field when there is nothing to choose; null when there is. */
  fixedTarget: Accessor<string | null>;
  /** Why nothing can be chosen when computers are on but none answers; null otherwise. */
  silentTarget: Accessor<string | null>;
  runnersLoading: Accessor<boolean>;
  targetOptions: Accessor<ReadonlyArray<{ value: string; label: string; disabled?: boolean }>>;
  target: Accessor<string>;
  onTarget: (value: string) => void;
  rows: { runner: Accessor<PrerequisiteState>; harness: Accessor<PrerequisiteState>; profile: Accessor<PrerequisiteState>; model: Accessor<PrerequisiteState> };
  /** Whether the card's brief has an acceptance check; undefined while it loads. Never blocks the run. */
  checkedCard?: Accessor<boolean | undefined>;
  /** How many checks the card has: lines of its own Done when list, and checks the agent brings; undefined while it loads or when it cannot be read. */
  acceptanceCount?: Accessor<{ doneWhen: number; agent: number } | undefined>;
  profilesLoading: Accessor<boolean>;
  profiles: Accessor<AgentProfileSummary[]>;
  /** What the agents of the machines this run may use reported, to say what each can do. */
  agentReports: Accessor<HarnessCapability[]>;
  creatingProfile: Accessor<boolean>;
  createDefaultProfile: () => void;
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
  /** Why a profile may not run this item, by profile id (`GET /items/{id}/run-options`). */
  refused?: Accessor<ReadonlyMap<string, string>>;
  /** The item holds work and is planned, not run: the primary button says so. */
  planning?: Accessor<boolean>;
  /** The ancestor the shown profile came from, while it is still the server's choice. */
  profileFrom?: Accessor<string | null | undefined>;
  /** The tools the selected profile gives the selected agent; a blank tools field uses these. */
  profileTools: Accessor<string[]>;
  harnessLabel: Accessor<string>;
  /** The model as the summary line names it. */
  modelLabel: Accessor<string>;
  decisionsAttested: Accessor<boolean>;
  /** What the chosen agent's sandbox is, in words; null when the runner does not say. */
  sandboxLine: Accessor<{ text: string; unsandboxed: boolean } | null>;
  onApprovals: () => void;
  /** The person set the Web tools box, so a profile change no longer fills it. */
  onNetwork: () => void;
  /** Why the run starts asking and without web tools, for a card that came from outside; null for the person's own. */
  untrustedReason?: Accessor<string | null>;
  /** Why the Web tools box is on and cannot be changed, or null when it can. */
  networkLocked: Accessor<string | null>;
  /** The selected runner's own config has a verifier / pushes branches. */
  verifyConfigured: Accessor<boolean>;
  pushConfigured: Accessor<boolean>;
  errors: Accessor<string[]>;
  submitting: Accessor<boolean>;
  canSubmit: Accessor<boolean>;
  /** Why the profile's team would refuse this run, in a sentence; null when it takes it. */
  misfit?: Accessor<string | null>;
  /** What the team lacks without refusing the run, in a sentence; null when nothing. */
  degraded?: Accessor<string | null>;
  onClose: () => void;
}

const PILL = { 'background-color': 'var(--color-bg-app)', color: 'var(--color-text-primary)' };
const LINK = { color: 'var(--color-accent-ink)' };
const SUMMARY_CLASS = 'cursor-pointer font-heading text-lg';
/** The dialog's sentence for the server's `profile_source`. */
export function describeProfileSource(source: string | null | undefined, from?: string | null): string | null {
  const said = sourceWords(source);
  return said && from ? `${said}, from ${from}` : said;
}

function sourceWords(source: string | null | undefined): string | null {
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

/** One line of "What will happen": an icon, the word for it for a screen reader, then the fact. */
const SummaryLine: Component<{ icon: JSX.Element; label: string; children: JSX.Element }> = (props) => (
  <li class="flex min-w-0 items-start gap-2">
    <span class="mt-0.5 flex-none" style={{ color: 'var(--color-text-tertiary)' }}>{props.icon}</span>
    <span class="sr-only">{props.label}: </span>
    <span class="min-w-0" style={{ 'overflow-wrap': 'anywhere' }}>{props.children}</span>
  </li>
);

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
  // The board knows each team's `works_in` from the project's autonomy settings;
  // without them every team is a repository team.
  const [autonomyLines] = createResource(() => props.projectId, (id) => api.autonomy.get(id).catch(() => null));
  const worksInFolder = () => {
    const team = teamOf(props.profiles().find((p) => p.agent_profile_id === props.form.agentProfileId));
    return !!team && autonomyLines()?.teams?.find((t) => t.name === team)?.works_in === 'folder';
  };
  const tools = () => props.form.toolsText.split(',').map((t) => t.trim()).filter(Boolean);
  const toggleTool = (name: string, on: boolean) =>
    props.setForm('toolsText', (on ? [...tools(), name] : tools().filter((t) => t !== name)).join(', '));

  const automation = () => `/projects/${props.projectId}/settings?tab=automation`;
  const folder = () => tilde(props.project()?.repository ?? '');
  const where = () => {
    if (worksInFolder()) return "in the team's own folder on this computer, not in this repository";
    const mode = props.project()?.workspace_mode;
    return mode === 'in_place' ? `in ${folder()}`
      : mode === 'clone' ? `in a fresh clone of ${folder()}`
        : `on a new branch of ${folder()}`;
  };
  const selectedProfile = () => props.profiles().find((p) => p.agent_profile_id === props.form.agentProfileId);
  const profileName = () => props.profiles().find((p) => p.agent_profile_id === props.form.agentProfileId)?.name ?? 'Choose a profile';
  const blocker = (): { text: string; href: string } | undefined => {
    if (props.project()?.code_origin === 'none') return { text: "Choose where this project's code is →", href: automation() };
    if (props.rows.runner() === 'missing') return { text: 'Turn on agent execution →', href: '/agents' };
  };
  const checksText = () => {
    const n = props.acceptanceCount?.();
    const verifier = !props.verifyConfigured() ? 'no verifier on this runner' : props.form.verify ? 'verifier on' : 'verifier off';
    // Each source is counted apart: the item's own Done when list, and the checks the agent brings.
    const counted = n === undefined ? [] : [n.doneWhen === 0 ? 'No Done when' : `${n.doneWhen} Done when`, ...(n.agent > 0 ? [`${n.agent} agent ${n.agent === 1 ? 'check' : 'checks'}`] : [])];
    return [...counted, verifier].join(' · ');
  };
  const limitText = (): string | null => {
    const text = props.form.tokenLimit.trim();
    if (text === '') return 'No token limit';
    const n = Number(text);
    return Number.isInteger(n) && n > 0 ? `Token limit ${spaced(n)}` : null;
  };
  const models = () => summarizeModels(props.harnessReports());
  // Only an agent that must be told a model blocks the run here; the dialog says so in place of Run.
  const modelBlocked = () => models().requiresModel && !props.gate().allowed;
  const modelReason = () => models().requiresModelReason ?? props.gate().reason;

  return (
    <>
      <Show when={props.silentTarget()}>
        {(text) => (
          <div data-testid="run-target-silent" role="alert" class="rounded-item px-4 py-3 text-sm" style={{ 'background-color': 'var(--color-warning-100)', color: 'var(--color-warning-700)' }}>
            <p class="font-semibold">Agents offline</p>
            <p>{text()}</p>
            <A href="/agents" class="font-semibold underline">Turn on agent execution →</A>
          </div>
        )}
      </Show>
      <Show
        when={props.fixedTarget() || props.silentTarget()}
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
        <Show when={props.fixedTarget()}>
          {(line) => <p data-testid="run-target-fixed" class="px-4 text-sm" style={{ color: 'var(--color-text-secondary)' }}>{line()}</p>}
        </Show>
      </Show>

      <Show when={!props.profilesLoading() && props.form.agentProfileId && props.form.toolsText.trim() === '' && props.profileTools().length === 0}>
        <p data-testid="no-tools" class="rounded-item px-4 py-3 text-sm" style={{ 'background-color': 'var(--color-warning-100)', color: 'var(--color-warning-700)' }}>
          {profileName()} gives {props.harnessLabel()} no tools, so it can only answer in text: it cannot read or change files.{' '}
          <A href="/agents" class="font-semibold underline">Give the profile tools</A>, or name some under Advanced.
        </p>
      </Show>

      <Show
        when={props.profilesLoading() || props.profiles().length > 0}
        fallback={
          <div class="flex flex-wrap items-center justify-between gap-3 rounded-item px-4 py-3" style={PILL}>
            <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>No agent profile exists yet.</p>
            <Button size="sm" variant="secondary" loading={props.creatingProfile()} onClick={props.createDefaultProfile}>
              Create default profile
            </Button>
          </div>
        }
      >
        <FieldShell label="Who does it" help={HELP.profile}>
          <ProfilePicker
            label="Who does it"
            withNobody={false}
            value={props.form.agentProfileId || null}
            profiles={props.profiles()}
            reports={props.agentReports()}
            agents={props.agentChoices()}
            agent={props.form.harnessKind}
            onAgent={props.onAgent}
            choiceNote={describeProfileSource(props.profileSource?.(), props.profileFrom?.())}
            refused={props.refused?.()}
            onChange={(p) => { if (!p) return; props.setForm('agentProfileId', p.agent_profile_id); props.onProfilePicked(p); }}
          />
        </FieldShell>
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
      <section data-testid="run-will-happen" class="space-y-2 rounded-item px-4 py-3" style={PILL}>
        <h3 class="font-heading text-lg">What will happen</h3>
        <ul data-testid="run-summary" class="space-y-1.5 text-sm">
          <SummaryLine icon={<IconWho />} label="Runs">
            <A href={automation()} class="font-semibold hover:underline" style={LINK}>{profileName()}</A>
            {' · '}
            <A href={automation()} class="hover:underline" style={LINK}>{props.modelLabel()}</A>
          </SummaryLine>
          <Show when={props.project()?.repository || worksInFolder()}>
            <SummaryLine icon={worksInFolder() ? <IconFolder /> : <IconBranch />} label="Works">
              <A href={automation()} class="hover:underline" style={LINK}>{where()}</A>
            </SummaryLine>
          </Show>
          <Show when={props.project()}>
            {(project) => {
              const line = () => installLine(project().prepare_command, folderCheck(), selectedProfile()?.runs_commands, worksInFolder());
              return (
                <SummaryLine icon={<IconPackage />} label="Prepares">
                  <Show when={line()} fallback={<span data-testid="run-dialog-nothing-installed">Nothing is installed</span>}>
                    {(text) => <span data-testid="run-dialog-install">{text()}</span>}
                  </Show>
                </SummaryLine>
              );
            }}
          </Show>
          <SummaryLine icon={<IconChecks />} label="Checks">
            <span data-testid="run-dialog-checks">{checksText()}</span>
          </SummaryLine>
          <SummaryLine icon={<IconShield />} label="Sandbox">
            <Show when={props.sandboxLine()} fallback={<span>Not reported by the agent</span>}>
              {(line) => (
                <span class="inline-flex flex-wrap items-center gap-x-2" data-testid="sandbox-line">
                  <span>{line().text}</span>
                  <HelpHint label="Sandbox" help={HELP.sandbox} />
                </span>
              )}
            </Show>
            <ParityRules reports={props.harnessReports()} />
          </SummaryLine>
          <SummaryLine icon={<IconGlobe />} label="Network">
            <span data-testid="run-dialog-network">{props.form.allowNetwork ? 'Web tools on: the agent may reach the network' : 'Web tools off: the agent has no network'}</span>
          </SummaryLine>
        </ul>
      </section>

      <div class="space-y-2">
        <div class="flex items-center gap-2">
        <Switch
          name="approvals"
          class="flex-1 cursor-pointer rounded-full px-4 py-2.5"
          style={PILL}
          checked={props.form.approvals === 'ask'}
          disabled={!props.decisionsAttested()}
          onChange={(checked) => { props.onApprovals(); props.setForm('approvals', checked ? 'ask' : 'auto'); }}
        >
          Ask before each action
        </Switch>
        <HelpHint label="Ask before each action" help={HELP.askBeforeEachAction} />
        </div>
        <Show when={!props.decisionsAttested()}>
          <p class="pl-4 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>This agent can't pause to ask — it always decides on its own.</p>
        </Show>
        <Show when={props.form.approvals === 'ask' ? props.untrustedReason?.() : null}>
          {(reason) => <p class="pl-4 text-xs" data-testid="untrusted-reason" style={{ color: 'var(--color-text-tertiary)' }}>{reason()}</p>}
        </Show>
        <div class="flex items-center gap-2">
        <Switch
          class="flex-1 cursor-pointer rounded-full px-4 py-2.5"
          style={PILL}
          checked={props.form.allowNetwork}
          disabled={props.networkLocked() !== null}
          onChange={(checked) => { props.onNetwork(); props.setForm('allowNetwork', checked); }}
        >
          Web tools
        </Switch>
        <HelpHint label="Web tools" help={HELP.webTools} />
        </div>
        <Show when={props.networkLocked()}>
          {(reason) => <p class="pl-4 text-xs" data-testid="web-tools-locked" style={{ color: 'var(--color-text-tertiary)' }}>Web tools stay on: {reason()}.</p>}
        </Show>
      </div>

      <details>
        <summary class={SUMMARY_CLASS} style={{ color: 'var(--color-text-primary)' }}>What the agent will read</summary>
        <pre class="mt-2 max-h-64 overflow-auto whitespace-pre-wrap rounded-item px-4 py-3 text-xs" style={PILL}>{props.agentContext() ?? 'Not available.'}</pre>
      </details>

      <details open={advanced() || modelBlocked()} onToggle={(e) => setAdvanced(e.currentTarget.open)} class="space-y-3">
        <summary class={SUMMARY_CLASS} style={{ color: 'var(--color-text-primary)' }}>Advanced for this run</summary>
        <div class="mt-3 space-y-3">
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
          class="space-y-1 rounded-item px-4 py-3 text-xs font-medium"
          style={{ 'background-color': 'var(--color-danger-100)', color: 'var(--color-danger-600)' }}
        >
          <For each={props.errors()}>{(msg) => <li>{msg}</li>}</For>
        </ul>
      </Show>

      {/* The dialog's scroll area has 1.5rem of padding under its content, and a sticky element stops at
          the padding edge: the offset and the negative margin below put the footer on the dialog's own
          lower edge, so no field shows beneath it. */}
      <div
        data-testid="run-footer"
        class="sticky -bottom-6 -mx-6 -mb-6 flex flex-wrap items-center justify-end gap-x-3 gap-y-2 px-6 py-3"
        style={{
          'background-color': 'var(--color-bg-elevated)',
          'border-top': '1px solid var(--color-border-light)',
          'border-radius': '0 0 32px 32px',
          'z-index': 1,
        }}
      >
        <Show when={props.limitSentence?.() ?? limitText()}>
          {(text) => (
            <span
              data-testid="run-footer-limit"
              class="mr-auto min-w-0 text-small"
              style={{ color: 'var(--color-text-secondary)', 'overflow-wrap': 'anywhere' }}
              title={props.limitSentence?.()}
            >
              {text()}
            </span>
          )}
        </Show>
        <Show when={props.misfit?.()}>
          {(text) => (
            <p
              id="run-misfit"
              data-testid="run-misfit"
              role="alert"
              class="min-w-0 basis-full rounded-item px-3 py-2 text-small"
              style={{ 'background-color': 'var(--color-warning-100)', color: 'var(--color-warning-700)', 'overflow-wrap': 'anywhere' }}
            >
              {text()}
            </p>
          )}
        </Show>
        <Show when={!props.misfit?.() && props.degraded?.()}>
          {(text) => (
            <p
              data-testid="run-degraded"
              class="min-w-0 basis-full rounded-item px-3 py-2 text-small"
              style={{ 'background-color': 'var(--color-warning-100)', color: 'var(--color-warning-700)', 'overflow-wrap': 'anywhere' }}
            >
              {text()}
            </p>
          )}
        </Show>
        <Button type="button" variant="secondary" onClick={props.onClose} disabled={props.submitting()}>
          Cancel
        </Button>
        <Show
          when={blocker()}
          fallback={
            <Show
              when={modelBlocked()}
              fallback={
                <Button type="submit" data-testid="run-start" loading={props.submitting()} disabled={!props.canSubmit()} aria-describedby={props.misfit?.() ? 'run-misfit' : undefined}>
                  {props.planning?.() ? 'Plan with agent' : 'Start run'}
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
