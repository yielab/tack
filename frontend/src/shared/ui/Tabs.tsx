import { For, type Component, type JSX } from 'solid-js';

export interface TabItem {
  id: string;
  label: string;
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
  const move = (dir: 1 | -1) => {
    const idx = props.tabs.findIndex((t) => t.id === props.active);
    if (idx < 0) return;
    const next = (idx + dir + props.tabs.length) % props.tabs.length;
    props.onChange(props.tabs[next].id);
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
        class="flex gap-1 border-b"
        style={{ 'border-color': 'var(--color-border-light)' }}
        onKeyDown={onKey}
      >
        <For each={props.tabs}>
          {(tab) => {
            const selected = () => tab.id === props.active;
            return (
              <button
                role="tab"
                type="button"
                aria-selected={selected() ? 'true' : 'false'}
                tabindex={selected() ? 0 : -1}
                onClick={() => props.onChange(tab.id)}
                class="-mb-px border-b-2 px-4 py-2 text-sm font-medium transition-colors focus:outline-none focus-visible:ring-2"
                style={{
                  'border-color': selected()
                    ? 'var(--color-primary-600)'
                    : 'transparent',
                  color: selected()
                    ? 'var(--color-primary-700)'
                    : 'var(--color-text-secondary)',
                  '--tw-ring-color': 'var(--color-focus-ring)',
                }}
              >
                {tab.label}
              </button>
            );
          }}
        </For>
      </div>
      <div role="tabpanel" class="pt-4">
        {props.children}
      </div>
    </div>
  );
};

export default Tabs;
