import { api } from '../../shared/api';
import type { IssuedCredential } from '../../shared/api/credentials';
import type { RungLevel } from '../../shared/api/changeSets';
import type { Thread } from '../../shared/api/threads';
import type { CreateTrigger, Trigger } from '../../shared/api/triggers';
import { agentProfilesApi } from '../../shared/execution';

/**
 * What a profile that manages is told. It names no project, team or person: everything specific
 * reaches it through the digest and its tools, so the same text serves any board.
 */
export const MANAGER_INSTRUCTIONS = [
  'You look after a board. Start every round by reading the digest, then ask for detail only where it points.',
  'Find what is stuck: failed runs, questions waiting on a person, change sets nobody has decided, tasks no team picks up.',
  'Say who should do each task and in what order, and report to the person in your thread.',
  'Every change you want goes through a change set; a person applies it unless the project lets you act. Never guess what the digest or your tools do not show.',
].join('\n');

/** Which profile the manager is: one that exists, or a new one that runs `agent`. */
export type ManagerProfile = { kind: 'existing'; id: string } | { kind: 'new'; name: string; agent: string };

export interface AddManagerInput {
  projectId: string;
  profile: ManagerProfile;
  /** The offset of the zone the weekday digest is read in, in minutes east of UTC. */
  utcOffsetMinutes: number;
  /** Where the agent reaches Tack, written into its MCP configuration. */
  apiUrl: string;
}

/** What the preset has made so far; each field appears when its step is done. */
export interface AddManagerResult {
  profileId?: string;
  profileName?: string;
  credential?: IssuedCredential;
  thread?: Thread;
  triggers: Trigger[];
  /** The project's "Agents on the board" rung, which the preset leaves as it found it. */
  rung?: RungLevel;
  mcpConfig?: string;
}

/** A step that failed, with what was already made so the person can finish by hand. */
export class AddManagerError extends Error {
  step: string;
  done: AddManagerResult;
  constructor(step: string, message: string, done: AddManagerResult) {
    super(message);
    this.step = step;
    this.done = done;
  }
}

/** The three triggers every manager gets, as general triggers like any a person adds. */
export function presetTriggers(args: { projectId: string; credentialId: string; profile: string; utcOffsetMinutes: number }): CreateTrigger[] {
  const base = { project_id: args.projectId, credential_id: args.credentialId, profile: args.profile };
  return [
    {
      ...base,
      name: 'Failed runs',
      source: { kind: 'event', event: 'run_failed', filter: {} },
      quiet_seconds: 120,
      daily_cap: 6,
    },
    {
      ...base,
      name: 'Change sets waiting',
      source: { kind: 'event', event: 'change_set_waiting', filter: { after_seconds: 3_600 } },
      quiet_seconds: 60,
      daily_cap: 4,
    },
    {
      ...base,
      name: 'Weekday digest',
      source: { kind: 'schedule', days: [1, 2, 3, 4, 5], minute_of_day: 9 * 60, utc_offset_minutes: args.utcOffsetMinutes },
      quiet_seconds: 0,
      daily_cap: 1,
    },
  ];
}

/** The MCP configuration an agent reads (`tack mcp` under the credential), to paste or to write as `.mcp.json`. */
export function mcpConfig(apiUrl: string, secret: string): string {
  return JSON.stringify(
    { mcpServers: { tack: { command: 'tack', args: ['mcp'], env: { TACK_API_URL: apiUrl, TACK_API_TOKEN: secret } } } },
    null,
    2,
  );
}

/**
 * Adds a manager to a project: the profile (unless one is chosen), a credential that may read, propose and
 * converse on the project, the thread to talk with it, the three triggers, and the MCP configuration. Only
 * existing parts are used and nothing is named after the project. `progress` is told after each step so a
 * failure part-way still shows the secret, which is shown once.
 */
export async function addManager(input: AddManagerInput, progress: (done: AddManagerResult) => void = () => {}): Promise<AddManagerResult> {
  const done: AddManagerResult = { triggers: [] };
  const step = async <T,>(name: string, work: () => Promise<T>): Promise<T> => {
    try {
      return await work();
    } catch (error) {
      throw new AddManagerError(name, error instanceof Error ? error.message : String(error), { ...done, triggers: [...done.triggers] });
    }
  };
  const tell = () => progress({ ...done, triggers: [...done.triggers] });

  if (input.profile.kind === 'existing') {
    const id = input.profile.id;
    done.profileId = id;
    const found = await step('profile', () => agentProfilesApi.list());
    done.profileName = found.data.data.find((p) => p.agent_profile_id === id)?.name ?? id;
  } else {
    const wanted = input.profile;
    const created = await step('profile', () =>
      agentProfilesApi.create({
        name: wanted.name,
        instructions: MANAGER_INSTRUCTIONS,
        tool_policy: { agent: wanted.agent },
        limits: {},
        summary: 'Reads the board and proposes changes through change sets',
      }),
    );
    done.profileId = created.agent_profile_id;
    done.profileName = created.name;
  }
  tell();
  const profile = done.profileId!;

  const credential = await step('credential', () =>
    api.credentials.issue(done.profileName!, [
      { all_projects: false, projects: [input.projectId], capabilities: ['read', 'propose', 'converse'] },
    ]),
  );
  done.credential = credential;
  done.mcpConfig = mcpConfig(input.apiUrl, credential.secret);
  tell();

  done.thread = await step('thread', () =>
    api.threads.create({ project_id: input.projectId, credential_id: credential.credential.id, profile, title: `Talking with ${done.profileName}` }),
  );
  tell();

  for (const trigger of presetTriggers({ projectId: input.projectId, credentialId: credential.credential.id, profile, utcOffsetMinutes: input.utcOffsetMinutes })) {
    done.triggers.push(await step(`trigger "${trigger.name}"`, () => api.triggers.create(trigger)));
    tell();
  }

  // The rung is left as the project has it (Propose unless someone changed it); the screen says which.
  const project = await step('project', () => api.projects.get(input.projectId));
  done.rung = project.agents_on_board?.level;
  tell();
  return done;
}
