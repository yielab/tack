import { type Component, createResource, createSignal, createMemo, createEffect, onCleanup, Show } from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import Drawer from '../../shared/ui/Drawer';
import Tabs, { type TabItem } from '../../shared/ui/Tabs';
import Button from '../../shared/ui/Button';
import { api } from '../../shared/api';
import { toast } from '../../shared/ui/toast';
import type { Item, UpdateItem } from '../../shared/types';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';
import { agentActivityApi } from '../../shared/agentActivity/api';
import { createBoardSocket } from '../../shared/realtime/boardSocket';
import { dispatchApi, type DispatchItemResponse } from '../../shared/dispatch/api';
import { notifyDispatchOutcome } from '../../shared/dispatch/notify';
import { dispatchOutcomeDetail } from '../../shared/dispatch/format';
import DispatchOutcomeNote from '../../shared/dispatch/DispatchOutcomeNote';
import ItemHeader from './ItemHeader';
import DetailsTab from './tabs/DetailsTab';
import ActivityTab from './tabs/ActivityTab';
import DependenciesTab from './tabs/DependenciesTab';
import FilesTab from './tabs/FilesTab';
import FieldsTab from './tabs/FieldsTab';
import AgentActivityTab from './tabs/AgentActivityTab';

const BASE_TABS: TabItem[] = [
  { id: 'details', label: 'Details' },
  { id: 'activity', label: 'Activity' },
  { id: 'dependencies', label: 'Dependencies' },
  { id: 'files', label: 'Files' },
  { id: 'fields', label: 'Fields' },
];

/**
 * Item detail drawer. Mounted once at the app root; opens whenever the
 * `?item=<id>` query param is present (deep-linkable / shareable), fetches the
 * item, and exposes inline header editing + a tab bar. Built on the kit Drawer
 * (ESC + focus return).
 */
const ItemDetailDrawer: Component = () => {
  const [searchParams, setSearchParams] = useSearchParams();
  const itemId = () => (searchParams.item as string | undefined) || undefined;

  const [item, { mutate, refetch }] = createResource(
    itemId,
    (id) => (id ? api.items.get(id) : null),
  );

  const [activeTab, setActiveTab] = createSignal('details');

  // Agent activity is fetched once here — not inside `AgentActivityTab`, unlike
  // every other tab — because the drawer needs to know whether the item HAS
  // any agent activity before deciding whether to show the tab at all
  // (TODO.md card B5: "an item with no agent activity shows no chip and no
  // empty tab"). A 404 (`TACK_ORCH_ENABLE` unset — the default install state,
  // TODO.md §0 rule 8) or any other fetch failure is treated the same as "no
  // activity": the tab quietly doesn't appear rather than surfacing an error
  // for a feature most installs haven't turned on.
  const [agentActivity, { refetch: refetchAgentActivity }] = createResource(itemId, (id) =>
    id ? agentActivityApi.getForItem(id) : null,
  );

  // Card B4 (Wave 2, realtime broadcast, task 34.5): a mirrored agent run or
  // approval change for the item currently open in this drawer should update
  // the "Agent Activity" tab without a manual reopen. The socket needs the
  // item's *project*, not just its id, and that's only known once `item()`
  // has loaded — so this waits on the item resource rather than opening a
  // socket the instant the drawer does.
  createEffect(() => {
    const projectId = item()?.project_id;
    const id = itemId();
    if (!projectId || !id) return;
    const s = createBoardSocket(projectId);
    const off = s.onEvent((event) => {
      if (
        (event.type === 'agent_run_updated' || event.type === 'approval_pending') &&
        event.item_id === id
      ) {
        void refetchAgentActivity();
      }
    });
    onCleanup(() => { off(); s.close(); });
  });

  // Dispatch-to-agents control (TODO.md Wave 3, card C4, task 35.8). Reuses
  // the per-item agent-activity fetch above as the "is orchestration even
  // enabled here" signal rather than adding a second probe: `orchAvailable`
  // is true only once that fetch has resolved WITHOUT error — a 404
  // (`TACK_ORCH_ENABLE` unset, the default install state, TODO.md §0 rule 8)
  // or any other failure both mean "don't show a privileged control that's
  // about to fail," the same conservative posture `useAgentActivityMap`'s
  // own `orchAvailable` applies for the Board/List/Table badges.
  const orchAvailable = () => !agentActivity.loading && agentActivity.error === undefined;
  const [dispatching, setDispatching] = createSignal(false);
  const [lastDispatch, setLastDispatch] = createSignal<DispatchItemResponse | null>(null);
  // Clear any previous result when a different item opens, so a stale
  // "dispatched"/"blocked" note never appears to belong to the newly opened item.
  createEffect(() => { itemId(); setLastDispatch(null); });

  const dispatchToAgents = async () => {
    const current = item();
    if (!current) return;
    setDispatching(true);
    try {
      const res = await dispatchApi.dispatchItem(current.id);
      setLastDispatch(res);
      notifyDispatchOutcome(res);
      void refetchAgentActivity();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to dispatch item');
    } finally {
      setDispatching(false);
    }
  };

  // A Solid resource accessor THROWS once it has errored (calling
  // `agentActivity()` directly, as opposed to reading `.loading`/`.error`) —
  // discovered while building this card's dispatch control: calling the
  // errored accessor from inside a memo that runs in the same reactive batch
  // that just set the error aborts that batch, silently wedging sibling
  // computations (found the hard way via `DispatchSprintModal`'s equivalent
  // resource — see its `dryRunData` for the fuller explanation). Every read
  // of the resource's *value* goes through this safe accessor instead —
  // `undefined` once errored, exactly like `agentActivity()` would return if
  // it simply hadn't thrown. This predates this card (card B5's original
  // resource), surfaced only now because no test previously drove it into an
  // error state through the full drawer.
  const agentActivityData = () => (agentActivity.error !== undefined ? undefined : agentActivity());

  const hasAgentActivity = () => {
    const a = agentActivityData();
    return !!a && ((a.attempts?.length ?? 0) > 0 || (a.approvals?.length ?? 0) > 0);
  };
  const tabs = createMemo((): TabItem[] => {
    if (!hasAgentActivity()) return BASE_TABS;
    // Placed right after "Activity" — agent activity is a variant of the
    // item's activity history, not an unrelated concern.
    const idx = BASE_TABS.findIndex((t) => t.id === 'activity');
    return [
      ...BASE_TABS.slice(0, idx + 1),
      { id: 'agent', label: 'Agent Activity' },
      ...BASE_TABS.slice(idx + 1),
    ];
  });

  const close = () => setSearchParams({ item: undefined });

  // Optimistic PATCH: apply locally, persist, then notify the host (or roll back).
  const patch = async (p: UpdateItem) => {
    const current = item();
    if (!current) return;
    mutate({ ...current, ...p } as Item);
    try {
      const updated = await api.items.update(current.id, p);
      mutate(updated);
      window.dispatchEvent(new CustomEvent(ITEM_UPDATED_EVENT, { detail: updated }));
    } catch (err) {
      void refetch(); // reconcile to the server value
      toast.error(err instanceof Error ? err.message : 'Failed to update item');
    }
  };

  // Debounce description edits so we don't PATCH on every keystroke.
  let descTimer: ReturnType<typeof setTimeout> | undefined;
  const onDescriptionChange = (html: string) => {
    if (descTimer) clearTimeout(descTimer);
    descTimer = setTimeout(() => void patch({ description: html }), 600);
  };

  return (
    <Drawer isOpen={!!itemId()} onClose={close} title="Item details" width="md">
      <Show
        when={item()}
        fallback={
          <p class="py-8 text-center text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
            {item.loading ? 'Loading…' : 'Item not found.'}
          </p>
        }
      >
        {(it) => (
          <div class="space-y-6">
            <ItemHeader item={it()} onPatch={patch} />

            {/* Dispatch to agents (TODO.md Wave 3, card C4, task 35.8) — a
                privileged, outward-facing action, so it's a distinct,
                explicit control rather than folded into an existing button
                row. Only rendered once `orchAvailable()` positively confirms
                the feature is on for this install (§0 rule 8: off by
                default). The three outcomes this whole card exists to keep
                distinct (queued/dispatched, policy-blocked, waiting-approval)
                render via the same `DispatchOutcomeNote` the sprint dispatch
                modal uses, so they read identically everywhere. */}
            <Show when={orchAvailable()}>
              <div
                class="flex flex-wrap items-center gap-3 rounded-lg border p-3"
                style={{ 'background-color': 'var(--color-bg-subtle)', 'border-color': 'var(--color-border-light)' }}
              >
                <Button size="sm" onClick={dispatchToAgents} loading={dispatching()} disabled={dispatching()}>
                  Dispatch to agents
                </Button>
                <Show when={lastDispatch()}>
                  {(res) => <DispatchOutcomeNote decision={res().outcome} detail={dispatchOutcomeDetail(res())} />}
                </Show>
              </div>
            </Show>

            <Tabs tabs={tabs()} active={activeTab()} onChange={setActiveTab}>
              <Show when={activeTab() === 'details'}>
                <DetailsTab item={it()} onDescriptionChange={onDescriptionChange} />
              </Show>
              <Show when={activeTab() === 'activity'}>
                <ActivityTab itemId={it().id} />
              </Show>
              <Show when={activeTab() === 'agent'}>
                <AgentActivityTab activity={agentActivityData()} loading={agentActivity.loading} />
              </Show>
              <Show when={activeTab() === 'dependencies'}>
                <DependenciesTab item={it()} />
              </Show>
              <Show when={activeTab() === 'files'}>
                <FilesTab itemId={it().id} />
              </Show>
              <Show when={activeTab() === 'fields'}>
                <FieldsTab item={it()} />
              </Show>
            </Tabs>
          </div>
        )}
      </Show>
    </Drawer>
  );
};

export default ItemDetailDrawer;

/** Helper for host views: the query patch that opens the drawer for an item. */
export const openItemParam = (itemId: string) => ({ item: itemId });
