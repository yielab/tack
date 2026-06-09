import { type Component, createSignal, Show, Switch, Match } from 'solid-js';
import Tabs, { type TabItem } from '../../shared/ui/Tabs';
import { useProject } from '../../shared/state/projectContext';
import GeneralPanel from './panels/GeneralPanel';
import WorkflowPanel from './panels/WorkflowPanel';
import VocabularyPanel from './panels/VocabularyPanel';
import BoardsPanel from './panels/BoardsPanel';
import FieldsPanel from './panels/FieldsPanel';
import RolesPanel from './panels/RolesPanel';
import DataPanel from './panels/DataPanel';

const TABS: TabItem[] = [
  { id: 'general', label: 'General' },
  { id: 'workflow', label: 'Workflow' },
  { id: 'vocabulary', label: 'Vocabulary' },
  { id: 'boards', label: 'Boards' },
  { id: 'fields', label: 'Fields' },
  { id: 'roles', label: 'Roles' },
  { id: 'data', label: 'Data' },
];

/** One tabbed surface for every project setting (T-511). */
const ProjectSettings: Component = () => {
  const { project } = useProject();
  const [active, setActive] = createSignal('general');

  return (
    <div class="mx-auto max-w-4xl px-6 py-8">
      <h1 class="mb-1 text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
        Project Settings
      </h1>
      <Show when={project()}>
        <p class="mb-6 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          {project()!.name}
        </p>
      </Show>

      <Tabs tabs={TABS} active={active()} onChange={setActive}>
        <Switch>
          <Match when={active() === 'general'}>
            <GeneralPanel />
          </Match>
          <Match when={active() === 'workflow'}>
            <WorkflowPanel />
          </Match>
          <Match when={active() === 'vocabulary'}>
            <VocabularyPanel />
          </Match>
          <Match when={active() === 'boards'}>
            <BoardsPanel />
          </Match>
          <Match when={active() === 'fields'}>
            <FieldsPanel />
          </Match>
          <Match when={active() === 'roles'}>
            <RolesPanel />
          </Match>
          <Match when={active() === 'data'}>
            <DataPanel />
          </Match>
        </Switch>
      </Tabs>
    </div>
  );
};

export default ProjectSettings;
