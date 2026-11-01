import { type Accessor, type Component, type JSX, For, createResource } from 'solid-js';
import { api } from '../../../../shared/api';
import type { AutonomySettings } from '../../../../shared/api/autonomy';
import type { AgentProfileSummary } from '../../../../shared/execution';
import type { Project, UpdateProject } from '../../../../shared/types';
import { toast } from '../../../../shared/ui/toast';
import Teams from './Teams';
import Start from './Start';
import Merge from './Merge';
import Limits from './Limits';
import Rungs from './Rungs';
import AgentsOnBoard from './AgentsOnBoard';
import { Section, inputStyle } from './parts';

interface Props {
  projectId: string;
  project: Accessor<Project | undefined>;
  statuses: Accessor<{ name: string; order: number; category?: string }[]>;
  profiles: Accessor<AgentProfileSummary[]>;
  saveProject: (patch: UpdateProject) => void | Promise<void>;
  /** The default agent, its model and profile. */
  agent: JSX.Element;
  /** Where the code is and how the agent works on it. */
  workspace: JSX.Element;
  /** What happens to the item when a run starts. */
  runStart: JSX.Element;
  /** What happens to the item when a run finishes. */
  finish: JSX.Element;
}

/** The sections in the order a person sets them up, and what the index beside them says. */
const SECTIONS = [
  { id: 'automation-who', label: 'Who' },
  { id: 'automation-when', label: 'When' },
  { id: 'automation-merge', label: 'Merge' },
  { id: 'automation-limits', label: 'Limits' },
  { id: 'automation-agents', label: 'Agents' },
] as const;

/**
 * The ladder graphic, then the five sections in the order a person sets them up: Who, When,
 * Merge, Limits, Agents. It owns what several of them share: the project's roles and its autonomy settings.
 */
const Ladder: Component<Props> = (props) => {
  const [roles, { refetch: refetchRoles }] = createResource(() => props.projectId, (id) => api.roles.list(id));
  const [resource, { mutate, refetch: refetchSettings }] = createResource(() => props.projectId, (id) => api.autonomy.get(id));
  /** Reading a failed resource throws; the rungs read this and are told of the failure beside it. */
  const settings = () => (resource.error ? undefined : resource());
  const saveAutonomy = async (patch: Partial<AutonomySettings>) => {
    const current = settings();
    if (!current) return;
    try {
      mutate(await api.autonomy.put(props.projectId, { ...current, ...patch }));
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to save');
    }
  };
  const roleList = () => (roles.error || !Array.isArray(roles()) ? [] : roles()!);

  return (
    <div class="flex min-w-0 flex-col gap-4" data-testid="ladder">
      <Rungs settings={settings} loadError={() => resource.error} onRetry={() => void refetchSettings()} />
      <div class="flex min-w-0 flex-col gap-4 lg:flex-row lg:items-start">
        <nav aria-label="Sections" data-testid="section-index" class="lg:sticky lg:top-4 lg:w-32 lg:shrink-0">
          <ul class="flex flex-wrap gap-2 lg:flex-col lg:gap-1.5">
            <For each={SECTIONS}>
              {(section) => (
                <li>
                  <a
                    href={`#${section.id}`}
                    class="inline-block rounded-full border px-3 py-1 text-sm"
                    style={inputStyle}
                    onClick={(e) => {
                      e.preventDefault();
                      document.getElementById(section.id)?.scrollIntoView?.();
                    }}
                  >
                    {section.label}
                  </a>
                </li>
              )}
            </For>
          </ul>
        </nav>
        <div class="flex min-w-0 flex-1 flex-col gap-4">
          <Section id="automation-who" title="Who" lead="The agent, model and profile that work on this project, and the team each role goes to.">
            {props.agent}
            <Teams
              roles={roleList}
              profiles={props.profiles}
              settings={settings}
              rolesError={() => roles.error}
              onRetryRoles={() => void refetchRoles()}
              project={props.project}
              onRoleSaved={() => void refetchRoles()}
              onDefaultProfile={(id) => void props.saveProject({ default_profile_id: id })}
            />
            {props.workspace}
          </Section>
          <Section id="automation-when" title="When" lead="When a task starts by itself, and what happens to it when a run starts and when it finishes.">
            <Start project={props.project} roles={roleList} statuses={props.statuses} save={props.saveProject} />
            {props.runStart}
            {props.finish}
          </Section>
          <Section id="automation-merge" title="Merge" lead="How much Tack may merge by itself, and what it needs to do so.">
            <Merge settings={settings} loadError={() => resource.error} onRetry={() => void refetchSettings()} save={saveAutonomy} />
          </Section>
          <Section id="automation-limits" title="Limits" lead="What the project may spend and queue, in tokens.">
            <Limits settings={settings} save={saveAutonomy} />
          </Section>
          <Section id="automation-agents" title="Agents" lead="How far an agent may go on this board.">
            <AgentsOnBoard project={props.project} save={props.saveProject} />
          </Section>
        </div>
      </div>
    </div>
  );
};

export default Ladder;
