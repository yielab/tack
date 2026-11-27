import { type Component, type JSX, createEffect, createSignal, on, Show } from 'solid-js';
import { Fold, Icons, RunState, SectionHeading, priorityLabel, type DocketRunWord, type RunStateName } from '../../shared/ui';
import type { Item } from '../../shared/types';
import ItemMenu from '../../shared/ui/ItemMenu';
import MoveUnderDialog from '../../shared/ui/MoveUnderDialog';
import { useItemTypes } from '../../shared/vocab/useItemTypes';
import ExecutionTimeline from '../../shared/runWithAgent/ExecutionTimeline';
import LatestRunSummary from '../../shared/runWithAgent/LatestRunSummary';
import AutomationRow, { WhoWouldDoIt, useAutomation } from './AutomationRow';
import DescriptionSection from './DescriptionSection';
import DoneWhenSection from './DoneWhenSection';
import ForTheAgentSection from './ForTheAgentSection';
import GroupProgress from './GroupProgress';
import ItemCrumbs from './ItemCrumbs';
import ItemProperties from './ItemProperties';
import ItemTitle from './ItemTitle';
import ProposeFromFile from './ProposeFromFile';
import RelatedPanel from './RelatedPanel';
import ActivitySection from './sections/ActivitySection';
import type { ItemEditor } from './useItemEditor';
import { useBrief } from './useBrief';
import { useFolderMarks } from './useFolderMarks';
import { useProjectItemList } from './useItemList';

/** The screen's position in its address: `execution` opens the run history (`?run=` names the run in it); anything else is the item as it reads. */
export function tabFromSearchParam(value: string | string[] | undefined): string {
  return value === 'execution' ? 'execution' : 'details';
}

export interface ItemBodyProps {
  item: Item;
  editor: ItemEditor;
  /** `drawer`: one column, the properties in a fold at the end. `page`: a main surface and a column of properties on the right. */
  layout: 'drawer' | 'page';
  /** `execution` when the run history is open. */
  tab: string;
  onTabChange: (id: string) => void;
  /** The run the history shows (its request id); the latest when absent. */
  run?: string;
  onRunChange?: (requestId: string | undefined) => void;
  /** Buttons at the start of the actions at the top right (Copy link on the page). */
  headerActions?: JSX.Element;
}

const surface = {
  'background-color': 'var(--color-bg-base)',
  'border-color': 'var(--color-border-light)',
  'box-shadow': 'var(--shadow-sm)',
} as const;

/**
 * The item's whole screen, read top down: what it is, the next step (or what needs you), the description,
 * Done when, For the agent, the runs and the activity, with the properties and what it is related to apart.
 * The drawer and the item page show this one component, laid out two ways.
 */
const ItemBody: Component<ItemBodyProps> = (props) => {
  const page = () => props.layout === 'page';
  const marks = useFolderMarks(() => props.item);
  const { byId } = useProjectItemList(() => props.item);
  const brief = useBrief(() => props.item.id);
  const notReady = () => byId().get(props.item.id)?.brief_summary?.not_ready ?? null;

  // The latest run's state, told by the next-step panel, marks the Runs section with the same icon.
  const [runState, setRunState] = createSignal<{ state: RunStateName; docket?: DocketRunWord } | undefined>(undefined);

  // The run history opens when the address, a created run or a "Propose items" asks for it, whatever the person last left.
  const [openRuns, setOpenRuns] = createSignal(0);
  createEffect(on(() => props.tab, (tab) => { if (tab === 'execution') queueMicrotask(() => setOpenRuns((n) => n + 1)); }));

  const automation = useAutomation(() => props.item);
  const [proposing, setProposing] = createSignal(false);
  const [moving, setMoving] = createSignal(false);

  // The server says whether the item holds work (`node`); the catalog says whether its type does, which
  // tells a group from work that a plan split.
  const itemTypes = useItemTypes();
  const isGroup = () => props.item.node === 'group';
  const typeHoldsWork = () => itemTypes.get(props.item.item_type)?.level === 'group';
  const splitInto = () => (isGroup() && !typeHoldsWork() ? (props.item.progress?.total ?? 0) : 0);

  const glance = () =>
    [priorityLabel(props.item.priority), props.item.assignee, props.item.due_date ? `due ${props.item.due_date.split('T')[0]}` : null]
      .filter(Boolean)
      .join(' · ');

  const properties = () => (
    <ItemProperties item={props.item} onPatch={props.editor.patch} marks={marks} showStatus={page()} />
  );
  const related = () => (
    <RelatedPanel
      item={props.item}
      byId={byId()}
      dependencies={props.editor.dependencies}
      refetchDependencies={props.editor.refetchDependencies}
      marks={marks}
    />
  );

  // Run with agent: the neutral execution surface. The next-step panel is the one place that says what to do
  // with the item: its latest run, the one button for what comes next, and, at its foot, what happens to the
  // item without the person.
  const nextStep = (
    <LatestRunSummary
      itemId={props.item.id}
      itemTitle={props.item.title}
      projectId={props.item.project_id}
      group={isGroup()}
      onCreated={() => props.onTabChange('execution')}
      onOpen={() => props.onTabChange('execution')}
      reviewInTab={props.tab === 'execution'}
      onState={setRunState}
      idle={
        <>
          <Show when={notReady()}>
            {(why) => (
              <p data-testid="not-ready" class="text-small font-semibold" style={{ color: 'var(--run-needs-ink)' }}>
                {why()}
              </p>
            )}
          </Show>
          <WhoWouldDoIt automation={automation} />
        </>
      }
      footer={<AutomationRow item={props.item} automation={automation} whoShownAbove={() => !runState()} />}
    />
  );

  const runs = (
    <Fold
      kind="runs"
      title={
        <span class="inline-flex items-center gap-2 text-section">
          Runs
          <Show when={runState()}>{(state) => <RunState size="dot" {...state()} />}</Show>
        </span>
      }
      lazy
      openRequest={openRuns()}
      defaultOpen={props.tab === 'execution'}
      onToggle={(open) => props.onTabChange(open ? 'execution' : 'details')}
    >
      <Show when={proposing()}>
        <ProposeFromFile itemId={props.item.id} onClose={() => setProposing(false)} />
      </Show>
      <ExecutionTimeline
        itemId={props.item.id}
        itemTitle={props.item.title}
        projectId={props.item.project_id}
        layout={props.layout}
        runId={props.run}
        onRunChange={props.onRunChange}
        nowShown
      />
    </Fold>
  );

  const activity = (
    <section aria-labelledby="item-activity" class="space-y-4">
      <SectionHeading id="item-activity" icon={<Icons.IconClock size={17} />} tone="people">
        Activity
      </SectionHeading>
      <ActivitySection itemId={props.item.id} />
    </section>
  );

  const actions = (
    <>
      {props.headerActions}
      <ItemMenu entries={[
        { label: 'Move under…', onSelect: () => setMoving(true) },
        { label: 'Propose items from a file', onSelect: () => {
          setProposing(true);
          props.onTabChange('execution');
        } },
      ]} />
    </>
  );

  const story = (
    <>
      <ItemTitle item={props.item} onPatch={props.editor.patch} marks={marks} layout={props.layout} focusTitle={page()} />
      <Show when={splitInto() > 0}>
        <p data-testid="item-split" class="text-lead font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          Split into {splitInto()} {splitInto() === 1 ? 'item' : 'items'}
        </p>
      </Show>
      <Show when={isGroup() && typeHoldsWork()}>
        <GroupProgress item={props.item} onChanged={() => void props.editor.refetchItem()} />
      </Show>
      {nextStep}
      <DescriptionSection item={props.item} onDescriptionChange={props.editor.onDescriptionChange} marks={marks} />
      <DoneWhenSection brief={brief} marks={marks} />
      <ForTheAgentSection itemId={props.item.id} brief={brief} marks={marks} layout={props.layout} />
      {runs}
      {activity}
    </>
  );

  return (
    <div class="min-w-0 max-w-full space-y-4">
      <ItemCrumbs item={props.item} byId={byId()} actions={actions} />
      <MoveUnderDialog
        projectId={props.item.project_id}
        item={moving() ? props.item : null}
        onClose={() => setMoving(false)}
        onPick={(parent) => {
          setMoving(false);
          void props.editor.patch({ parent_id: parent?.id ?? null });
        }}
      />

      <Show
        when={page()}
        fallback={
          <div class="space-y-8">
            {story}
            <Fold kind="details" title={<span class="text-section">Properties</span>} summary={glance()} lazy>
              <div class="space-y-6 pt-2">
                {properties()}
                {related()}
              </div>
            </Fold>
          </div>
        }
      >
        <div class="grid items-start gap-8 lg:grid-cols-[minmax(0,1fr)_20rem]">
          <main class="min-w-0 space-y-12 rounded-lg border p-10 xl:p-12" style={surface}>{story}</main>
          <aside class="space-y-4">
            <section class="space-y-3 rounded-lg border p-5" style={surface}>
              <h2 class="text-lead font-semibold" style={{ color: 'var(--color-text-primary)' }}>Properties</h2>
              {properties()}
            </section>
            <div class="rounded-lg border p-5" style={surface}>{related()}</div>
          </aside>
        </div>
      </Show>
    </div>
  );
};

export default ItemBody;
