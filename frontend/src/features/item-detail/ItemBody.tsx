import { type Component, type JSX, createMemo, Show } from 'solid-js';
import Tabs, { type TabItem } from '../../shared/ui/Tabs';
import type { Item } from '../../shared/types';
import RunWithAgentButton from '../../shared/runWithAgent/RunWithAgentButton';
import ExecutionTimeline from '../../shared/runWithAgent/ExecutionTimeline';
import LatestRunSummary from '../../shared/runWithAgent/LatestRunSummary';
import ItemHeader from './ItemHeader';
import ItemProperties from './ItemProperties';
import type { ItemEditor } from './useItemEditor';
import DetailsTab from './tabs/DetailsTab';
import ActivityTab from './tabs/ActivityTab';
import DependenciesTab, { classify } from './tabs/DependenciesTab';
import FilesTab from './tabs/FilesTab';
import FieldsTab from './tabs/FieldsTab';

const BASE_TABS: TabItem[] = [
  { id: 'details', label: 'Details' },
  { id: 'activity', label: 'Activity' },
  // "Execution" — the neutral execution domain (`ExecutionRequest`/
  // `ExecutionAttempt` via `tack-runner`). Always present — an item with
  // zero execution requests is still a real, honest state worth a visible
  // empty tab (`ExecutionTimeline`'s own `EmptyState`), not a hidden one.
  { id: 'execution', label: 'Execution' },
  { id: 'dependencies', label: 'Dependencies' },
  { id: 'files', label: 'Files' },
  { id: 'fields', label: 'Fields' },
];

/** Every tab id an item screen can land on directly. */
const DEEP_LINKABLE_TAB_IDS = new Set(BASE_TABS.map((t) => t.id));

/** The tab a `?tab=` value names; anything else is Details. */
export function tabFromSearchParam(value: string | string[] | undefined): string {
  // The old Brief tab is part of Details now.
  if (value === 'brief') return 'details';
  return typeof value === 'string' && DEEP_LINKABLE_TAB_IDS.has(value) ? value : 'details';
}

export interface ItemBodyProps {
  item: Item;
  editor: ItemEditor;
  /** `drawer`: one column, properties as a grid under the header. `page`: a side column. */
  layout: 'drawer' | 'page';
  tab: string;
  onTabChange: (id: string) => void;
  /** Buttons at the right of the header row (Copy link on the page). */
  headerActions?: JSX.Element;
}

/**
 * The item's whole screen: header, run with agent, properties and tabs. The
 * drawer and the item page show this one component, laid out two ways.
 */
const ItemBody: Component<ItemBodyProps> = (props) => {
  const page = () => props.layout === 'page';

  // The dependency edges are loaded for "Blocks" anyway, so the tab counts them for free.
  // A failed load shows no number rather than breaking the screen.
  const edges = () => {
    const { dependencies } = props.editor;
    return dependencies.state === 'ready' || dependencies.state === 'refreshing' ? dependencies() : undefined;
  };
  const blocks = () => {
    const all = edges();
    return all ? all.filter((d) => classify(d, props.item.id).dir === 'blocks').length : undefined;
  };
  const tabs = createMemo<TabItem[]>(() =>
    BASE_TABS.map((t) => (t.id === 'dependencies' ? { ...t, count: edges()?.length } : t)),
  );

  const properties = (
    <ItemProperties
      item={props.item}
      onPatch={props.editor.patch}
      layout={page() ? 'list' : 'grid'}
      blocks={blocks()}
      onOpenBlocks={() => props.onTabChange('dependencies')}
    />
  );

  return (
    <div class="space-y-6">
      <ItemHeader
        item={props.item}
        onPatch={props.editor.patch}
        actions={props.headerActions}
        focusTitle={page()}
      />

      <Show when={!page()}>{properties}</Show>

      <div class={page() ? 'grid items-start gap-6 lg:grid-cols-[minmax(0,1fr)_20rem]' : 'space-y-6'}>
        <div class="min-w-0 space-y-6">
          {/* Run with agent — the neutral execution surface
              (`ExecutionRequest`/`ExecutionAttempt` via `tack-runner`). A
              successful run switches straight to the "Execution" tab so
              the request that just appeared is immediately visible,
              without a page navigation. Once the item has a run, its
              latest one is summed up here, so coming back to the task
              shows what the agent did. */}
          <div
            class="flex flex-wrap items-start gap-3 rounded-[28px] px-5 py-4"
            style={{ 'background-color': 'var(--color-bg-panel)' }}
          >
            <LatestRunSummary
              itemId={props.item.id}
              itemTitle={props.item.title}
              projectId={props.item.project_id}
              onCreated={() => props.onTabChange('execution')}
              onOpen={() => props.onTabChange('execution')}
            />
            <RunWithAgentButton
              itemId={props.item.id}
              itemTitle={props.item.title}
              projectId={props.item.project_id}
              onCreated={() => props.onTabChange('execution')}
            />
          </div>

          <Tabs tabs={tabs()} active={props.tab} onChange={props.onTabChange}>
            <Show when={props.tab === 'details'}>
              <DetailsTab item={props.item} onDescriptionChange={props.editor.onDescriptionChange} />
            </Show>
            <Show when={props.tab === 'activity'}>
              <ActivityTab itemId={props.item.id} />
            </Show>
            <Show when={props.tab === 'execution'}>
              <ExecutionTimeline itemId={props.item.id} itemTitle={props.item.title} projectId={props.item.project_id} />
            </Show>
            <Show when={props.tab === 'dependencies'}>
              <DependenciesTab
                item={props.item}
                dependencies={props.editor.dependencies}
                refetch={props.editor.refetchDependencies}
              />
            </Show>
            <Show when={props.tab === 'files'}>
              <FilesTab itemId={props.item.id} />
            </Show>
            <Show when={props.tab === 'fields'}>
              <FieldsTab item={props.item} />
            </Show>
          </Tabs>
        </div>

        <Show when={page()}>
          <aside
            class="rounded-[28px] p-5"
            style={{ 'background-color': 'var(--color-bg-panel)' }}
          >
            {properties}
          </aside>
        </Show>
      </div>
    </div>
  );
};

export default ItemBody;
