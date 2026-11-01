import { type Component, type JSX, Show, createSignal, createUniqueId } from 'solid-js';
import { Icons } from '../../shared/ui';

const OPEN_KEY = 'tack_item_details_open';

/** Whether the person left the Details fold open last time; closed when nothing was saved. */
export function readDetailsOpen(): boolean {
  try {
    return localStorage.getItem(OPEN_KEY) === '1';
  } catch {
    return false;
  }
}

function saveDetailsOpen(open: boolean): void {
  try {
    localStorage.setItem(OPEN_KEY, open ? '1' : '0');
  } catch {
    /* ignore — a convenience, not state */
  }
}

/**
 * The item's properties in the drawer, behind one heading. The fold stays as the person left it
 * (per browser), and its heading says the few values worth a glance while it is shut.
 */
const DetailsFold: Component<{ glance?: string; children: JSX.Element }> = (props) => {
  const [open, setOpen] = createSignal(readDetailsOpen());
  const panelId = createUniqueId();
  const toggle = () => {
    const next = !open();
    setOpen(next);
    saveDetailsOpen(next);
  };
  return (
    <section data-testid="details-fold" class="min-w-0">
      <button
        type="button"
        aria-expanded={open()}
        aria-controls={panelId}
        onClick={toggle}
        class="flex w-full min-w-0 items-center gap-2 rounded-item py-1 text-left text-sm focus:outline-none focus-visible:ring-2"
        style={{ color: 'var(--color-text-primary)', '--tw-ring-color': 'var(--color-focus-ring)' }}
      >
        <span aria-hidden="true" class="inline-flex flex-none" style={{ transform: open() ? 'rotate(0deg)' : 'rotate(-90deg)' }}>
          <Icons.IconChevronDown size={14} />
        </span>
        <span class="font-semibold">Details</span>
        <Show when={!open() && props.glance}>
          <span class="min-w-0 truncate text-xs" style={{ color: 'var(--color-text-secondary)' }}>{props.glance}</span>
        </Show>
      </button>
      <div id={panelId} hidden={!open()} class="pt-2">
        <Show when={open()}>{props.children}</Show>
      </div>
    </section>
  );
};

export default DetailsFold;
