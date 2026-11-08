// The four agents Tack can run, named plainly, and what each one reports it can do. A line is
// built only from what the agent reported: a feature it left out is "Not reported by <agent>",
// never guessed, and a feature it cannot do keeps the agent's reason. Shared by the Agents page and
// by every picker that names who does a task.

import type { CapabilityValue, HarnessCapability } from './types';

/** The name a person reads for each agent kind. */
const AGENT_NAMES: Readonly<Record<string, string>> = {
  'claude-code': 'Claude Code',
  codex: 'Codex',
  opencode: 'OpenCode',
  docket: 'Docket',
};

/** The agents in the order a page lists them. */
export const AGENT_KINDS: readonly string[] = ['claude-code', 'codex', 'opencode', 'docket'];

export const agentName = (kind: string): string => AGENT_NAMES[kind] ?? kind;

export type AgentCapabilityId = 'cancel' | 'resume' | 'decisions' | 'proposals' | 'usage';

const CAPABILITIES: ReadonlyArray<{ id: AgentCapabilityId; can: string }> = [
  { id: 'cancel', can: 'stop a run' },
  { id: 'resume', can: 'pick a run back up after a restart' },
  { id: 'decisions', can: 'answer questions' },
  { id: 'proposals', can: 'propose items' },
  { id: 'usage', can: 'report usage' },
];

export interface AgentCapabilityLine {
  id: AgentCapabilityId;
  state: 'can' | 'cannot' | 'not_reported';
  /** What the agent said about it; null when it said nothing. */
  reason: string | null;
  /** The verb phrase: "stop a run". */
  can: string;
  /** The whole line: "Can stop a run", "Cannot stop a run", "Not reported by Codex". */
  text: string;
}

function reported(harness: HarnessCapability | undefined, id: AgentCapabilityId): CapabilityValue | undefined {
  if (!harness) return undefined;
  if (id === 'decisions') return harness.decisions ?? harness.features?.decisions;
  return harness.features?.[id];
}

/** One line per capability, in a fixed order, for the agent `kind` as `harness` reported it. */
export function agentCapabilityLines(kind: string, harness: HarnessCapability | undefined): AgentCapabilityLine[] {
  return CAPABILITIES.map(({ id, can }) => {
    const value = reported(harness, id);
    if (!value) return { id, state: 'not_reported', reason: null, can, text: `Not reported by ${agentName(kind)}` };
    if (value.support === 'unsupported') return { id, state: 'cannot', reason: value.reason, can, text: `Cannot ${can}` };
    return { id, state: 'can', reason: value.reason, can, text: `Can ${can}` };
  });
}

/** "Can stop a run, answer questions and report usage." or what to do when it reported nothing it can. */
export function agentCanSentence(kind: string, lines: readonly AgentCapabilityLine[]): string {
  const can = lines.filter((l) => l.state === 'can').map((l) => l.can);
  if (can.length === 0) return `${agentName(kind)} has not reported anything it can do.`;
  const list = can.length === 1 ? can[0] : `${can.slice(0, -1).join(', ')} and ${can[can.length - 1]}`;
  return `Can ${list}.`;
}

/** The agent's report to read for `kind`: one that probed clean first, else any. */
export function pickHarness(reports: readonly HarnessCapability[], kind: string): HarnessCapability | undefined {
  const mine = reports.filter((h) => h.harness_kind === kind);
  return mine.find((h) => h.probe_error === null) ?? mine[0];
}

/** Who runs a profile, in words: its agent, or "any agent you choose for the task". */
export function profileAgentText(profile: { tool_policy?: unknown }): string {
  const agent = (profile.tool_policy as { agent?: unknown } | null | undefined)?.agent;
  return typeof agent === 'string' && agent ? agentName(agent) : 'Any agent, chosen for each task';
}

export const SINGLE_AGENT_TEXT = 'A single agent, no team';
export const PER_TASK_AGENT_TEXT =
  'Each task can use a different agent: you choose it for the task, and the profile you give it can name one.';
