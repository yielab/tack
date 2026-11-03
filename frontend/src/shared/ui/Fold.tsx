import { type Component, type JSX, Show, createEffect, createSignal, createUniqueId, on } from 'solid-js';
import { IconChevronDown } from './icons';

const KEY_PREFIX = 'tack_fold:';

/** Whether the person last left this kind of section open (true) or shut (false); undefined when they never chose. */
export function readFoldOpen(kind: string): boolean | undefined {
  try {
    const saved = localStorage.getItem(KEY_PREFIX + kind);
    return saved === '1' ? true : saved === '0' ? false : undefined;
  } catch {
    return undefined;
  }
}

function saveFoldOpen(kind: string, open: boolean): void {
  try {
    localStorage.setItem(KEY_PREFIX + kind, open ? '1' : '0');
  } catch {
    /* a convenience, not state: the page works without it */
  }
}

export interface FoldProps {
  /** What kind of section this is: the browser remembers open or shut once per kind. */
  kind: string;
  title: JSX.Element;
  /** The section in one line, beside the title while it is shut ("Medium · no GitHub issue"). */
  summary?: JSX.Element;
  /** What in the section waits for a person ("1 needs you"). It is written in the header while the section is
   *  shut, and the section opens by itself when it appears. */
  needs?: string | null;
  /** Open or shut when the person never chose; shut when absent. */
  defaultOpen?: boolean;
  /** A number the owner raises to open the section on purpose (a button elsewhere that points into it). */
  openRequest?: number;
  /** Keep the content out of the page until the first time it opens. */
  lazy?: boolean;
  /** `sub` is a fold inside a section: a smaller heading. */
  size?: 'section' | 'sub';
  testId?: string;
  /** Extra attributes on the section, for a test or a deep link to find it by. */
  attrs?: Record<string, string>;
  /** Told each time the person opens or shuts it, with the new state. */
  onToggle?: (open: boolean) => void;
  class?: string;
  style?: JSX.CSSProperties;
  children: JSX.Element;
}

/**
 * The one way a part of the item screen folds: a header that is a real button (`aria-expanded`,
 * `aria-controls`, a chevron), the section's one-line summary beside it while it is shut, and the
 * content one press away. What the person chose is remembered per kind of section, in the browser;
 * without storage the defaults apply. A section whose content needs a person says so in its header
 * and opens by itself when that appears.
 */
const Fold: Component<FoldProps> = (props) => {
  const panelId = createUniqueId();
  const [open, setOpen] = createSignal(!!props.needs || (readFoldOpen(props.kind) ?? props.defaultOpen ?? false));
  const [everOpened, setEverOpened] = createSignal(open());
  const set = (next: boolean) => {
    setOpen(next);
    if (next) setEverOpened(true);
  };
  createEffect(on(() => !!props.needs, (needs) => needs && set(true), { defer: true }));
  createEffect(on(() => props.openRequest, () => set(true), { defer: true }));
  const toggle = () => {
    const next = !open();
    set(next);
    saveFoldOpen(props.kind, next);
    props.onToggle?.(next);
  };
  const sub = () => props.size === 'sub';
  return (
    <section data-testid={props.testId} data-fold={props.kind} {...props.attrs} class={`min-w-0 ${props.class ?? ''}`} style={props.style}>
      <button
        type="button"
        aria-expanded={open()}
        aria-controls={panelId}
        onClick={toggle}
        class={`flex w-full min-w-0 items-center gap-2 rounded-item py-1 text-left focus:outline-none focus-visible:ring-2 ${sub() ? 'text-xs' : 'text-sm'}`}
        style={{ color: 'var(--color-text-primary)', '--tw-ring-color': 'var(--color-focus-ring)' }}
      >
        <span aria-hidden="true" class="inline-flex flex-none" style={{ transform: open() ? 'rotate(0deg)' : 'rotate(-90deg)' }}>
          <IconChevronDown size={14} />
        </span>
        <span class="flex-none font-semibold">{props.title}</span>
        <Show when={!open() && props.needs}>
          <span data-testid="fold-needs" class="flex-none text-xs font-semibold" style={{ color: 'var(--run-needs-ink)' }}>· {props.needs}</span>
        </Show>
        <Show when={!open() && props.summary}>
          <span data-testid="fold-summary" class="min-w-0 truncate text-xs font-normal" style={{ color: 'var(--color-text-secondary)' }}>{props.summary}</span>
        </Show>
      </button>
      <div id={panelId} hidden={!open()} class="pt-2">
        <Show when={!props.lazy || everOpened()}>{props.children}</Show>
      </div>
    </section>
  );
};

export default Fold;
