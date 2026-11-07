import { BOOK, BOOK_BASE } from './routes';
import type { FieldHelp } from '../ui/Field';

/** One sentence per field, shown as the `(?)` title and popover. */
export const HELP = {
  folder: { text: 'The folder on this computer where the code lives. The agent works here or on a branch of it.', href: BOOK.workspace },
  howTheAgentWorks: { text: 'On a new branch: a separate copy, your folder stays as it is. In the folder: directly on your files.', href: BOOK.workspace },
  pushTheBranch: { text: 'After a run, push the branch to your remote with your own git credentials. Off keeps everything local.', href: BOOK.automation },
  mergeOnDone: { text: 'When you move a finished task to Done, Tack merges its pull request for you, once CI has passed.', href: BOOK.automation },
  autonomyLevel: { text: 'How much Tack may merge without you. At L3 you press Merge yourself. L4 and L5 let Tack merge when a check you trust has passed.', href: BOOK.automation },
  verifyRequire: { text: 'The checks the verifier must have run before Tack merges on its own. Leave empty to ask for none.', href: BOOK.automation },
  loopCap: { text: 'How many times a rejected change goes back to the agent to be fixed. 0 sends it back never.', href: BOOK.automation },
  budgets: { text: 'Limits for work Tack does by itself, per day. Empty means no limit.', href: BOOK.automation },
  model: { text: "The agent's default is what it uses when you run it yourself. Pick a specific one only if you need to.", href: BOOK.automation },
  definitionOfDone: { text: 'Appended to what the agent reads on every run of this project', href: BOOK.automation },
  profile: { text: 'What the agent is allowed to do and how it is told to work. Implementer changes code; Reviewer and Researcher only read; Planner proposes subtasks.', href: BOOK.automation },
  askBeforeEachAction: { text: 'The agent pauses before every action and waits for your answer under Questions from the agent. A card that was imported starts with this on.', href: BOOK.runDialog },
  webTools: { text: "Lets the agent fetch pages and search the web. Off turns those tools off; the agent's shell can still reach the network. A card that was imported starts with this off.", href: BOOK.runDialog },
  sandbox: { text: "Where this computer can sandbox an agent, it only reaches its workspace. Without a sandbox an agent has your user's access, so it must be able to ask before it acts; one that can't ask doesn't run.", href: `${BOOK_BASE}admin-guide/what-the-agent-can-reach.html` },
  acceptanceCriteria: { text: 'One sentence per thing that must be true when this is done. The agent reads them; a person checks them.', href: BOOK.items },
  ladderTeams: { text: 'Each role works with an agent profile, and the profile names the team that runs it. A task takes the profile of its role; one with no role takes the project default.', href: BOOK.automation },
  ladderStart: { text: 'When a task reaches the status you choose, Tack starts a run by itself, up to the number you set. It needs a profile to run with.', href: BOOK.automation },
  ladderAgents: { text: 'How far an agent may go on this board: read only, propose changes for you to apply, apply some by itself, or create items too. The stricter of this rung and the agent\'s own credential wins.', href: BOOK.automation },
  ladderCeiling: { text: 'The most Tack can merge by itself here. A team that needs a person during a run keeps its tasks at L3 until it is fixed.', href: BOOK.automation },
  startedByItself: { text: 'Tack started this run on its own, because the task reached the status the project chose.', href: BOOK.automation },
  teamBadge: { text: 'The team that ran this task, named by the profile the run used.', href: BOOK.automation },
  mergedByItself: { text: 'Tack merged this change on its own, under the verifier\'s signed verdict.', href: BOOK.automation },
  automationRow: { text: 'What happens to this task without you: whether it starts by itself, which team takes it, and how far Tack may merge it.', href: BOOK.automation },
  agentCapabilities: { text: 'What each agent told Tack it can do. A line it did not report, or that it cannot do, is greyed out with its reason. Each task chooses its own agent.', href: BOOK.agentTeams },
  teamsPanel: { text: 'Each card shows only what the agent that runs the team reported. A line it left out says so, so you can tell "none" from "not said".', href: BOOK.agentTeams },
  builtInProfiles: { text: 'Implementer, Reviewer, Researcher and Planner come with Tack. You can edit them, not delete them. A profile that names no agent runs with the agent you pick for the task.', href: BOOK.agentTeams },
  decisionToken: { text: 'A second secret, apart from signing in, that your administrator sets up. Needed to answer the agent; never sent when you only look.', href: BOOK.decisionToken },
  promptFingerprint: { text: "Identifies the fixed wording Tack put around the card's text in this prompt. Two runs with the same fingerprint were given the same wording; a different one means it changed between them.", href: BOOK.runDialog },
  qualitySuccess: { text: 'Runs that succeeded out of the runs that ended after starting. A run the computer or the team turned away is not counted: it did no work.', href: BOOK.workQuality },
  qualityFirstPass: { text: "Tasks whose first run you accepted out of the tasks whose first run you reviewed. It says how often an agent gets it right the first time.", href: BOOK.workQuality },
  qualityRework: { text: 'Runs after a task\'s first, per task that has run. 0.5 means one extra round for every two tasks.', href: BOOK.workQuality },
  qualityRevert: { text: 'Pull requests that were reverted out of the ones that were merged or reverted. It says how often merged work did harm.', href: BOOK.workQuality },
  qualityAgreement: { text: "How often the verifier's word matched what became of the work. Bad work is work you rejected or whose pull request was reverted; good work is work you accepted and kept.", href: BOOK.workQuality },
  qualityPassK: { text: 'For a task that repeats, the chance that any three of its runs all succeed. It says whether the agent does it every time, not just once.', href: BOOK.workQuality },
  qualityQuestions: { text: 'How many times an agent asked you something per hundred steps it took, and how often you let it go on. A kind of question you always allow is noise.', href: BOOK.workQuality },
  qualityStops: { text: 'Why runs ended without succeeding, by the code the run gave. "Not started" runs were turned away before the agent did anything.', href: BOOK.workQuality },
  qualityCuts: { text: 'The same rates, cut by who did the work: the profile, the agent, the model, the team and the fingerprint of the prompt wording.', href: BOOK.workQuality },
  forTheAgent: { text: 'Checks a machine can run, limits on what it may touch, and the risk you see. Optional.', href: BOOK.items },
} satisfies Record<string, FieldHelp>;

/** How the runner's sandbox is put in words, wherever a run's sandbox is shown. */
export const SANDBOX_TEXT = {
  bwrap: 'Runs in an isolated sandbox',
  none: "Not sandboxed: the agent has your user's access to this machine",
  harness: 'Runs in an isolated sandbox',
} as const;

/** The part of a runner's harness report that says whether the agent may run here. */
export interface AgentSafetyReport {
  sandbox?: 'bwrap' | 'none' | 'harness';
  decisions?: { support: string };
}

/** Whether an agent runs sandboxed, runs unsandboxed and asks first, or does not run at all —
 *  the runner refuses an agent with no sandbox that cannot ask (ADR 0018). Across several
 *  runners the least safe report decides; `null` when no runner says. */
export function agentSafety(reports: readonly AgentSafetyReport[]): { text: string; tone: 'ok' | 'asks' | 'blocked' } | null {
  const known = reports.filter((r) => r.sandbox === 'bwrap' || r.sandbox === 'none' || r.sandbox === 'harness');
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

/** What a level allows, in one sentence; the words the Merging guide uses. */
export const LEVEL_MEANING: Record<'L3' | 'L4' | 'L5', string> = {
  L3: 'L3: you approve each step that needs it, and you merge every change yourself.',
  L4: 'L4: Tack merges by itself inside the domains you name; you approve the rest.',
  L5: 'L5: Tack merges by itself anywhere in the repository, once its checks pass.',
};
