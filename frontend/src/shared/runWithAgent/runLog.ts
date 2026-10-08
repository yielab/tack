/** What an agent did, read from its run log. Only claude-code's `stream-json` transcript
 *  names its steps; for any other harness the log yields none and the card shows only what
 *  the agent said at the end. */

export interface RunStep {
  /** The tool the agent called, e.g. `Read`, `Bash`, `Write`. */
  tool: string;
  /** The file, command or pattern it was called on, when the call names one. */
  target: string | null;
  /** The tool reported an error. */
  failed: boolean;
}

export interface RunLog {
  steps: RunStep[];
  /** The tools the agent was given (claude's `init` line), or `null` when the log doesn't say. */
  toolsOffered: string[] | null;
}

const TARGET_KEYS = ['file_path', 'notebook_path', 'command', 'pattern', 'path', 'url', 'query', 'description'];

function targetOf(input: unknown): string | null {
  if (!input || typeof input !== 'object') return null;
  const obj = input as Record<string, unknown>;
  for (const key of TARGET_KEYS) {
    const value = obj[key];
    if (typeof value === 'string' && value.trim() !== '') return value.trim();
  }
  return null;
}

export function parseRunLog(text: string): RunLog {
  const steps: RunStep[] = [];
  const byId = new Map<string, RunStep>();
  let toolsOffered: string[] | null = null;
  for (const line of text.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed.startsWith('{')) continue;
    let event: Record<string, unknown>;
    try {
      event = JSON.parse(trimmed) as Record<string, unknown>;
    } catch {
      continue;
    }
    if (event.type === 'system' && event.subtype === 'init' && Array.isArray(event.tools)) {
      toolsOffered = event.tools.filter((t): t is string => typeof t === 'string');
      continue;
    }
    const content = (event.message as { content?: unknown } | undefined)?.content;
    if (!Array.isArray(content)) continue;
    for (const block of content) {
      if (!block || typeof block !== 'object') continue;
      const b = block as Record<string, unknown>;
      if (event.type === 'assistant' && b.type === 'tool_use' && typeof b.name === 'string') {
        const step: RunStep = { tool: b.name, target: targetOf(b.input), failed: false };
        steps.push(step);
        if (typeof b.id === 'string') byId.set(b.id, step);
      } else if (event.type === 'user' && b.type === 'tool_result' && b.is_error === true && typeof b.tool_use_id === 'string') {
        const step = byId.get(b.tool_use_id);
        if (step) step.failed = true;
      }
    }
  }
  return { steps, toolsOffered };
}

/** A step as a short sentence: "Read README.md", "Ran ls -la". */
export function describeStep(step: RunStep): string {
  const verb: Record<string, string> = {
    Read: 'Read', Write: 'Wrote', Edit: 'Edited', MultiEdit: 'Edited', NotebookEdit: 'Edited',
    Bash: 'Ran', Glob: 'Looked for', Grep: 'Searched for', WebFetch: 'Fetched', WebSearch: 'Searched the web for',
    Agent: 'Delegated', Task: 'Delegated',
  };
  const target = step.target ? (step.target.length > 120 ? `${step.target.slice(0, 117)}…` : step.target) : null;
  const action = verb[step.tool] ?? `Used ${step.tool}`;
  return target ? `${action} ${target}` : action;
}
