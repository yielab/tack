import { BOOK, BOOK_BASE } from './routes';
import type { FieldHelp } from '../ui/Field';

/** One sentence per field, shown as the `(?)` title and popover. */
export const HELP = {
  folder: { text: 'The folder on this computer where the code lives. The agent works here or on a branch of it.', href: BOOK.workspace },
  howTheAgentWorks: { text: 'On a new branch: a separate copy, your folder stays as it is. In the folder: directly on your files.', href: BOOK.workspace },
  pushTheBranch: { text: 'After a run, push the branch to your remote with your own git credentials. Off keeps everything local.', href: BOOK.automation },
  mergeOnDone: { text: 'When you move a finished task to Done, Tack merges its pull request for you, once CI has passed.', href: BOOK.automation },
  autonomyLevel: { text: 'How much Tack may merge without you. At L3 you press Merge yourself. L4 and L5 let Tack merge when a check you trust has passed.', href: BOOK.automation },
  verifyRequire: { text: 'The checks prill must have run before Tack merges on its own. Leave empty to ask for none.', href: BOOK.automation },
  loopCap: { text: 'How many times a rejected change goes back to the agent to be fixed. 0 sends it back never.', href: BOOK.automation },
  budgets: { text: 'Limits for work Tack does by itself, per day. Empty means no limit.', href: BOOK.automation },
  model: { text: "The agent's default is what it uses when you run it yourself. Pick a specific one only if you need to.", href: BOOK.automation },
  definitionOfDone: { text: 'Appended to what the agent reads on every run of this project', href: BOOK.automation },
  profile: { text: 'What the agent is allowed to do and how it is told to work. Implementer changes code; Reviewer and Researcher only read; Planner proposes subtasks.', href: BOOK.automation },
  askBeforeEachAction: { text: 'The agent pauses before every action and waits for your answer under Questions from the agent.', href: BOOK.runDialog },
  webTools: { text: "Lets the agent fetch pages and search the web. Off turns those tools off; the agent's shell can still reach the network.", href: BOOK.runDialog },
  sandbox: { text: "Where this computer can sandbox an agent, it only reaches its workspace. Without a sandbox an agent has your user's access, so it must be able to ask before it acts; one that can't ask doesn't run.", href: `${BOOK_BASE}user-guide/what-the-agent-can-reach.html` },
  acceptanceCriteria: { text: 'One sentence per thing that must be true when this is done. The agent reads them; a person checks them.', href: BOOK.items },
  ladderTeams: { text: 'Each role works with an agent profile, and the profile names the team that runs it. A task takes the profile of its role; one with no role takes the project default.', href: BOOK.automation },
  ladderStart: { text: 'When a task reaches the status you choose, Tack starts a run by itself, up to the number you set. It needs a profile to run with.', href: BOOK.automation },
  ladderCeiling: { text: 'The most Tack can merge by itself here. A team that needs a person during a run keeps its tasks at L3 until it is fixed.', href: BOOK.automation },
  startedByItself: { text: 'Tack started this run on its own, because the task reached the status the project chose.', href: BOOK.automation },
  teamBadge: { text: 'The team that ran this task, named by the profile the run used.', href: BOOK.automation },
  mergedByItself: { text: 'Tack merged this change on its own, under the verifier\'s signed verdict.', href: BOOK.automation },
  automationRow: { text: 'What happens to this task without you: whether it starts by itself, which team takes it, and how far Tack may merge it.', href: BOOK.automation },
  forTheAgent: { text: 'Checks a machine can run, limits on what it may touch, and the risk you see. Optional.', href: BOOK.items },
} satisfies Record<string, FieldHelp>;

/** How the runner's sandbox is put in words, wherever a run's sandbox is shown. */
export const SANDBOX_TEXT = {
  bwrap: 'Sandboxed with bwrap',
  none: "Not sandboxed: the agent has your user's access to this machine",
} as const;

/** The part of a runner's harness report that says whether the agent may run here. */
export interface AgentSafetyReport {
  sandbox?: 'bwrap' | 'none';
  decisions?: { support: string };
}

/** Whether an agent runs sandboxed, runs unsandboxed and asks first, or does not run at all —
 *  the runner refuses an agent with no sandbox that cannot ask (ADR 0077). Across several
 *  runners the least safe report decides; `null` when no runner says. */
export function agentSafety(reports: readonly AgentSafetyReport[]): { text: string; tone: 'ok' | 'asks' | 'blocked' } | null {
  const known = reports.filter((r) => r.sandbox === 'bwrap' || r.sandbox === 'none');
  if (known.length === 0) return null;
  const unconfined = known.filter((r) => r.sandbox === 'none');
  if (unconfined.some((r) => r.decisions?.support !== 'supported')) return { text: SAFETY_TEXT.blocked, tone: 'blocked' };
  if (unconfined.length > 0) return { text: SAFETY_TEXT.asks, tone: 'asks' };
  return { text: SAFETY_TEXT.ok, tone: 'ok' };
}

export const SAFETY_TEXT = {
  ok: 'Sandboxed: the agent only reaches its workspace',
  asks: "Not sandboxed on this computer, so the agent asks before each action unless you turn that off",
  blocked: "Doesn't run on this computer: it isn't sandboxed and can't ask before it acts. Install bubblewrap (Linux), or update the agent to a version that can ask.",
} as const;
