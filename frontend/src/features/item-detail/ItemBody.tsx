import { type Component, type JSX, createMemo, createSignal, Show } from 'solid-js';
import Tabs, { type TabItem } from '../../shared/ui/Tabs';
import { RunState, type DocketRunWord, type RunStateName, priorityLabel } from '../../shared/ui';
import type { Item } from '../../shared/types';
import ProposeFromFile from './ProposeFromFile';
import AutomationRow, { WhoWouldDoIt, useAutomation } from './AutomationRow';
import DetailsFold from './DetailsFold';
import ItemMenu from './ItemMenu';
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
  { id: 'details', label: 'Overview' },
  // "Execution" — the neutral execution domain (`ExecutionRequest`/
  // `ExecutionAttempt` via `tack-runner`). Always present — an item with
  // zero execution requests is still a real, honest state worth a visible
  // empty tab (`ExecutionTimeline`'s own `EmptyState`), not a hidden one.
  { id: 'execution', label: 'Execution' },
  { id: 'activity', label: 'Activity' },
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
  /** `drawer`: one column, properties in a Details fold under the tabs. `page`: a side column. */
  layout: 'drawer' | 'page';
  tab: string;
  onTabChange: (id: string) => void;
  /** The run the Execution tab shows (its request id); the latest when absent. */
  run?: string;
  onRunChange?: (requestId: string | undefined) => void;
  /** Buttons at the right of the header row (Copy link on the page). */
  headerActions?: JSX.Element;
}

/**
 * The item's whole screen: header, the Now panel (what to do next), the tabs and the
 * properties. The drawer and the item page show this one component, laid out two ways.
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
  // The latest run's state, told by the Now panel, marks the Execution tab with the same icon.
  const [runState, setRunState] = createSignal<{ state: RunStateName; docket?: DocketRunWord } | undefined>(undefined);
  const tabs = createMemo<TabItem[]>(() =>
    BASE_TABS.map((t) => {
      if (t.id === 'dependencies') return { ...t, count: edges()?.length };
      const state = runState();
      if (t.id === 'execution' && state) return { ...t, adornment: <RunState size="dot" {...state} /> };
      return t;
    }),
  );

  const automation = useAutomation(() => props.item);
  const [proposing, setProposing] = createSignal(false);

  const properties = () => (
    <ItemProperties
      item={props.item}
      onPatch={props.editor.patch}
      layout={page() ? 'list' : 'grid'}
      blocks={blocks()}
      onOpenBlocks={() => props.onTabChange('dependencies')}
    />
  );
  // What the closed Details fold still says: the values people look for first.
  const glance = () =>
    [priorityLabel(props.item.priority), props.item.assignee, props.item.due_date ? `due ${props.item.due_date.split('T')[0]}` : null]
      .filter(Boolean)
      .join(' · ');

  // Run with agent — the neutral execution surface (`ExecutionRequest`/`ExecutionAttempt`
  // via `tack-runner`). A successful run switches straight to the "Execution" tab so the
  // request that just appeared is immediately visible. The Now panel is the one place that
  // says what to do with the item: its latest run, the one button for what comes next, and,
  // at its foot, what happens to the item without the person.
  const now = (
    <LatestRunSummary
      itemId={props.item.id}
      itemTitle={props.item.title}
      projectId={props.item.project_id}
      onCreated={() => props.onTabChange('execution')}
      onOpen={() => props.onTabChange('execution')}
      reviewInTab={props.tab === 'execution'}
      onState={setRunState}
      idle={<WhoWouldDoIt automation={automation} />}
      footer={<AutomationRow item={props.item} automation={automation} />}
    />
  );
  const propose = (
    <Show when={proposing()}>
      <ProposeFromFile itemId={props.item.id} onClose={() => setProposing(false)} />
    </Show>
  );

  return (
    <div class="space-y-6">
      <ItemHeader
        item={props.item}
        onPatch={props.editor.patch}
        actions={
          <>
            {props.headerActions}
            <ItemMenu entries={[{ label: 'Propose items from a file', onSelect: () => {
                    setProposing(true);
                    props.onTabChange('execution');
                  },
                }]} />
          </>
        }
        focusTitle={page()}
      />

      <div class={page() ? 'grid items-start gap-6 lg:grid-cols-[minmax(0,1fr)_20rem]' : 'space-y-6'}>
        <div class="min-w-0 space-y-6">
          {now}
          <Tabs tabs={tabs()} active={props.tab} onChange={props.onTabChange}>
            <Show when={props.tab === 'details'}>
              <DetailsTab item={props.item} onDescriptionChange={props.editor.onDescriptionChange} />
            </Show>
            <Show when={props.tab === 'activity'}>
              <ActivityTab itemId={props.item.id} />
            </Show>
            <Show when={props.tab === 'execution'}>
              {propose}
              <ExecutionTimeline itemId={props.item.id} itemTitle={props.item.title} projectId={props.item.project_id} layout={props.layout} runId={props.run} onRunChange={props.onRunChange} nowShown />
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
          <Show when={!page()}>
            <DetailsFold glance={glance()}>{properties()}</DetailsFold>
          </Show>
        </div>

        <Show when={page()}>
          <aside
            class="space-y-5 rounded-card p-5"
            style={{ 'background-color': 'var(--color-bg-panel)' }}
          >
            {properties()}
          </aside>
        </Show>
      </div>
    </div>
  );
};

export default ItemBody;
