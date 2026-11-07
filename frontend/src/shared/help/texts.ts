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
  egressMode: { text: "How far the agent's traffic may go, when this computer holds it to that. Closed: only the model's own address. Listed: the model and the hosts you list for the profile. Open: any public site. In every mode, this computer's own address and private networks are refused. A site the mode allows can still receive what the agent sends it, because the page is not seen.", href: BOOK.webAccess },
  sandbox: { text: "Every agent runs contained: by this computer (bubblewrap), by its own sandbox, or by docket; Claude Code and opencode only by bubblewrap. Where nothing contains an agent it doesn't run, unless the administrator turned the requirement off, and then it asks before each action.", href: `${BOOK_BASE}admin-guide/what-the-agent-can-reach.html` },
  prepare: { text: "The project's dependencies are installed before the agent starts, inside the same sandbox as the agent, with no access to your home folder or to this computer's other secrets. A command the board replaces runs only if the administrator allows it.", href: `${BOOK_BASE}admin-guide/what-the-agent-can-reach.html` },
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

/** The three web accesses a profile can have, in the order the profile's select lists them. */
export const EGRESS_MODES = [
  { value: 'closed', label: 'Closed: only the model', line: 'Web: only the model' },
  { value: 'listed', label: 'Listed: the model and the profile\'s hosts', line: "Web: the model and the profile's hosts" },
  { value: 'open', label: 'Open: any public site', line: 'Web: any public site, no private networks' },
] as const;

/** What a profile starts with by what it does, as the server decides it. */
export function defaultEgressMode(kind: string | undefined): (typeof EGRESS_MODES)[number]['value'] {
  if (kind === 'reviewer' || kind === 'planner') return 'closed';
  return kind === 'researcher' ? 'open' : 'listed';
}

/** Whether the computers a run could land on hold the agent to its profile's web access: every one
 *  that says so, else false. `undefined` when none reports it. */
export function egressEnforced(reports: ReadonlyArray<{ egress?: { enforced?: boolean } }>): boolean | undefined {
  const said = reports.map((r) => r.egress?.enforced).filter((e): e is boolean => typeof e === 'boolean');
  return said.length === 0 ? undefined : said.every(Boolean);
}

/** How the runner's sandbox is put in words, wherever a run's sandbox is shown. */
export const SANDBOX_TEXT = {
  bwrap: 'Runs in an isolated sandbox',
  none: "Not sandboxed: the agent has your user's access to this machine",
  harness: 'Runs in an isolated sandbox',
} as const;

/** The part of a runner's harness report that says who contains the agent and whether it may run here. */
export interface AgentSafetyReport {
  harness_kind?: string;
  sandbox?: 'bwrap' | 'none' | 'harness';
  /** Whether the runner's settings require a sandbox; absent on an older runner. */
  sandbox_required?: boolean;
  decisions?: { support: string };
}

function containedBy(report: AgentSafetyReport): string {
  if (report.harness_kind === 'docket') return SAFETY_TEXT.byDocket;
  if (report.sandbox === 'harness') return SAFETY_TEXT.byAgent;
  return report.harness_kind === 'codex' ? SAFETY_TEXT.byComputerAndAgent : SAFETY_TEXT.byComputer;
}

/** Who contains an agent, or that it runs asking first, or that it does not run at all. Across
 *  several runners the least safe report decides; `null` when no runner says. */
export function agentSafety(reports: readonly AgentSafetyReport[]): { text: string; tone: 'ok' | 'asks' | 'blocked' } | null {
  const known = reports.filter((r) => r.sandbox === 'bwrap' || r.sandbox === 'none' || r.sandbox === 'harness');
  if (known.length === 0) return null;
  const unconfined = known.filter((r) => r.sandbox === 'none');
  // docket decides whether to run; the board only asks it about its jail and never refuses on the answer.
  const blocked = unconfined.find((r) => r.harness_kind !== 'docket' && (r.sandbox_required === true || r.decisions?.support !== 'supported'));
  if (blocked) return { text: blocked.sandbox_required === true ? SAFETY_TEXT.needsBubblewrap : SAFETY_TEXT.blocked, tone: 'blocked' };
  const asking = unconfined[0];
  if (asking) {
    const text = asking.sandbox_required === false ? SAFETY_TEXT.optedOut : asking.harness_kind === 'docket' ? SAFETY_TEXT.docketSilent : SAFETY_TEXT.asks;
    return { text, tone: 'asks' };
  }
  return { text: containedBy(known[0]), tone: 'ok' };
}

export const SAFETY_TEXT = {
  byComputer: 'Contained by this computer (bwrap)',
  byComputerAndAgent: 'Contained by this computer (bwrap) and by the agent itself',
  byAgent: 'Contained by the agent itself',
  byDocket: 'Contained by docket',
  optedOut: 'Not contained: the administrator turned the requirement off',
  docketSilent: 'Not contained, as far as docket says',
  needsBubblewrap: "Doesn't run here: install bubblewrap to contain it",
  asks: "Not sandboxed on this computer, so the agent asks before each action unless you turn that off",
  blocked: "Doesn't run on this computer: it isn't sandboxed and can't ask before it acts. Install bubblewrap (Linux), or update the agent to a version that can ask.",
} as const;

/** What a level allows, in one sentence; the words the Merging guide uses. */
export const LEVEL_MEANING: Record<'L3' | 'L4' | 'L5', string> = {
  L3: 'L3: you approve each step that needs it, and you merge every change yourself.',
  L4: 'L4: Tack merges by itself inside the domains you name; you approve the rest.',
  L5: 'L5: Tack merges by itself anywhere in the repository, once its checks pass.',
};
