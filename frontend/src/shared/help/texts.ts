import { BOOK } from './routes';
import type { FieldHelp } from '../ui/Field';

/** One sentence per field, shown as the `(?)` title and popover. */
export const HELP = {
  folder: { text: 'The folder on this computer where the code lives. The agent works here or on a branch of it.', href: BOOK.workspace },
  howTheAgentWorks: { text: 'On a new branch: a separate copy, your folder stays as it is. In the folder: directly on your files.', href: BOOK.workspace },
  pushTheBranch: { text: 'After a run, push the branch to your remote with your own git credentials. Off keeps everything local.', href: BOOK.automation },
  model: { text: "The agent's default is what it uses when you run it yourself. Pick a specific one only if you need to.", href: BOOK.automation },
  profile: { text: 'What the agent is allowed to do and how it is told to work. Implementer changes code; Reviewer and Researcher only read; Planner proposes subtasks.', href: BOOK.automation },
  askBeforeEachAction: { text: 'The agent pauses before every action and waits for your answer under Questions from the agent.', href: BOOK.runDialog },
  allowNetwork: { text: 'Lets the agent fetch pages and search. Off is safer; some tasks need it.', href: BOOK.runDialog },
  acceptanceCriteria: { text: 'One sentence per thing that must be true when this is done. The agent reads them; a person checks them.', href: BOOK.items },
  forTheAgent: { text: 'Checks a machine can run, limits on what it may touch, and the risk you see. Optional.', href: BOOK.items },
} satisfies Record<string, FieldHelp>;
