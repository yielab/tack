import { type Accessor, type Component, createResource } from 'solid-js';
import { api } from '../../../../shared/api';
import type { AutonomySettings } from '../../../../shared/api/autonomy';
import type { AgentProfileSummary, RunnerSummary } from '../../../../shared/execution';
import type { Project, UpdateProject } from '../../../../shared/types';
import { toast } from '../../../../shared/ui/toast';
import Teams from './Teams';
import Start from './Start';
import Merge from './Merge';
import Ceiling from './Ceiling';

interface Props {
  projectId: string;
  project: Accessor<Project | undefined>;
  statuses: Accessor<{ name: string; order: number; category?: string }[]>;
  profiles: Accessor<AgentProfileSummary[]>;
  runners: Accessor<RunnerSummary[]>;
  saveProject: (patch: UpdateProject) => void | Promise<void>;
}

/**
 * The four rungs in the order a person climbs them: Teams, Start, Merge, Ceiling.
 * The ladder owns what two rungs share: the project's roles and its autonomy settings.
 */
const Ladder: Component<Props> = (props) => {
  const [roles, { refetch: refetchRoles }] = createResource(
    () => props.projectId,
    (id) => api.roles.list(id).catch(() => []),
  );
  const [settings, { mutate }] = createResource(
    () => props.projectId,
    (id) => api.autonomy.get(id).catch(() => null),
  );
  const saveAutonomy = async (patch: Partial<AutonomySettings>) => {
    const current = settings();
    if (!current) return;
    try {
      mutate(await api.autonomy.put(props.projectId, { ...current, ...patch }));
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to save');
    }
  };
  const roleList = () => (Array.isArray(roles()) ? roles()! : []);

  return (
    <div class="flex flex-col gap-4" data-testid="ladder">
      <Teams
        roles={roleList}
        profiles={props.profiles}
        runners={props.runners}
        project={props.project}
        onRoleSaved={() => void refetchRoles()}
        onDefaultProfile={(id) => void props.saveProject({ default_profile_id: id })}
      />
      <Start project={props.project} roles={roleList} statuses={props.statuses} save={props.saveProject} />
      <Merge settings={settings} save={saveAutonomy} />
      <Ceiling settings={settings} />
    </div>
  );
};

export default Ladder;
