import { For, Show, createEffect, createSignal, createUniqueId, onCleanup, onMount, type Component, type JSX } from 'solid-js';

export interface TabItem {
  id: string;
  label: string;
  /** Shown beside the label when the number is already loaded. */
  count?: number;
  /** A small graphic after the label (a run's state dot). It carries its own accessible label. */
  adornment?: JSX.Element;
}

export interface TabsProps {
  tabs: TabItem[];
  active: string;
  onChange: (id: string) => void;
  /** Panel content for the active tab (caller switches on `active`). */
  children?: JSX.Element;
  class?: string;
}

/** Accessible tab bar (role=tablist) + panel slot. Arrow keys move between
 * tabs; the selected tab carries `aria-selected`. Token-driven. */
const Tabs: Component<TabsProps> = (props) => {
  const base = createUniqueId();
  const tabId = (id: string) => `${base}-tab-${id}`;
  const panelId = (id: string) => `${base}-panel-${id}`;
  const move = (dir: 1 | -1) => {
    const idx = props.tabs.findIndex((t) => t.id === props.active);
    if (idx < 0) return;
    const next = (idx + dir + props.tabs.length) % props.tabs.length;
    props.onChange(props.tabs[next].id);
  };

  // When the tabs do not all fit, the list fades out at the edge that has more, so it reads as
  // scrollable. It is a mask on the list: nothing is added to the tab order or to the accessibility tree.
  let list: HTMLDivElement | undefined;
  const [more, setMore] = createSignal<{ start: boolean; end: boolean }>({ start: false, end: false });
  const measure = () => {
    if (!list) return;
    const start = list.scrollLeft > 1;
    const end = list.scrollLeft + list.clientWidth < list.scrollWidth - 1;
    setMore((m) => (m.start === start && m.end === end ? m : { start, end }));
  };
  onMount(() => {
    measure();
    if (typeof ResizeObserver === 'undefined' || !list) return;
    const observer = new ResizeObserver(measure);
    observer.observe(list);
    onCleanup(() => observer.disconnect());
  });
  // The selected tab stays in view, so the arrow keys never move the selection out of sight.
  createEffect(() => {
    const id = props.active;
    list?.querySelector<HTMLElement>(`[id="${tabId(id)}"]`)?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
  });
  const fade = () => {
    const { start, end } = more();
    if (!start && !end) return undefined;
    return `linear-gradient(to right, ${start ? 'transparent 0, black 28px' : 'black 0'}, ${end ? 'black calc(100% - 28px), transparent 100%' : 'black 100%'})`;
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'ArrowRight') {
      e.preventDefault();
      move(1);
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      move(-1);
    }
  };

  return (
    <div class={props.class}>
      <div
        role="tablist"
        ref={list}
        data-fade={more().start ? (more().end ? 'both' : 'start') : more().end ? 'end' : undefined}
        class="flex overflow-x-auto border-b"
        style={{ 'border-color': 'var(--color-border-light)', '-webkit-mask-image': fade(), 'mask-image': fade() }}
        onScroll={measure}
        onKeyDown={onKey}
      >
        <For each={props.tabs}>
          {(tab) => {
            const selected = () => tab.id === props.active;
            return (
              <button
                role="tab"
                type="button"
                id={tabId(tab.id)}
                aria-controls={panelId(tab.id)}
                aria-selected={selected() ? 'true' : 'false'}
                tabindex={selected() ? 0 : -1}
                onClick={() => props.onChange(tab.id)}
                class="-mb-px shrink-0 whitespace-nowrap border-b-[3px] px-1 pb-2.5 pt-1 mr-4 text-sm transition-colors focus:outline-none focus-visible:ring-2"
                style={{
                  'border-color': selected()
                    ? 'var(--color-primary-600)'
                    : 'transparent',
                  color: selected()
                    ? 'var(--color-accent-ink)'
                    : 'var(--color-text-secondary)',
                  'font-weight': selected() ? 700 : 500,
                  '--tw-ring-color': 'var(--color-focus-ring)',
                }}
              >
                {tab.label}
                <Show when={tab.adornment}>
                  <span class="ml-1.5 inline-flex align-middle">{tab.adornment}</span>
                </Show>
                <Show when={tab.count !== undefined}>
                  <span
                    class="ml-1.5 rounded-full px-1.5 py-px text-micro font-bold"
                    style={{ 'background-color': 'var(--color-bg-subtle)', color: 'var(--color-text-secondary)' }}
                  >
                    {tab.count}
                  </span>
                </Show>
              </button>
            );
          }}
        </For>
      </div>
      <div role="tabpanel" id={panelId(props.active)} aria-labelledby={tabId(props.active)} class="pt-4">
        {props.children}
      </div>
    </div>
  );
};

export default Tabs;
