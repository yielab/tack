import type { HarnessCapability, HarnessDocs, HarnessModelsSource } from '../execution/types';

/** The part of a runner's report of one agent that says what it does about models. */
export type AgentModelsReport = Partial<
  Pick<HarnessCapability, 'models' | 'default_model' | 'docs' | 'requires_model' | 'requires_model_reason'>
>;

/** What the runners that report one agent say about its models, merged. */
export interface AgentModels {
  source: HarnessModelsSource;
  ids: string[];
  /** The model the agent uses when a run names none. */
  defaultModel: string | null;
  /** The agent's own documentation pages. */
  docs: HarnessDocs | null;
  /** Every runner that reports the agent says it needs a model named. */
  requiresModel: boolean;
  requiresModelReason: string | null;
}

const SOURCE_RANK: Record<HarnessModelsSource, number> = { listed: 2, suggested: 1, none: 0 };

/** Merges several runners' reports of one agent: the ids are the union, `listed` beats
 *  `suggested` beats `none`, and the default and the docs are the first a runner gives.
 *  A report with no `models` (an older runner) reads as `none`. */
export function summarizeModels(reports: readonly AgentModelsReport[]): AgentModels {
  let source: HarnessModelsSource = 'none';
  const ids = new Set<string>();
  for (const r of reports) {
    const s = r.models?.source ?? 'none';
    if (SOURCE_RANK[s] > SOURCE_RANK[source]) source = s;
    for (const id of r.models?.ids ?? []) ids.add(id);
  }
  return {
    source,
    ids: [...ids],
    defaultModel: reports.find((r) => r.default_model)?.default_model ?? null,
    docs: reports.find((r) => r.docs)?.docs ?? null,
    requiresModel: reports.length > 0 && reports.every((r) => r.requires_model === true),
    requiresModelReason: reports.find((r) => r.requires_model_reason)?.requires_model_reason ?? null,
  };
}

/** Tool names measured per harness (docs/plans/measurements/tools-<harness>.md). Only claude-code
 *  has a measured list; the others offer "All tools" and free text only. */
export const KNOWN_TOOLS: Record<string, readonly string[]> = {
  'claude-code': ['Read', 'Edit', 'Write', 'Bash', 'Agent', 'Grep', 'Glob', 'WebFetch', 'WebSearch'],
  codex: [],
  opencode: [],
  docket: [],
};
