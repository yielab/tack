import { type Component, Show, createSignal } from 'solid-js';
import { Badge } from '../ui';

/** A short label for why a merge did not happen, and what to do about it. */
export function describeMergeNote(note: string): { label: string; todo: string } {
  const n = note.toLowerCase();
  if (n.includes('waiting for ci') || n.includes('still running') || n.includes('pending')) {
    return { label: 'Waiting for CI to merge', todo: 'Nothing yet. Tack tries again when CI finishes; you can also press Merge on the run.' };
  }
  if (n.includes("token can't merge") || n.includes('token')) {
    return {
      label: note.toLowerCase().includes("can't merge") ? "The GitHub token can't merge" : `Not merged: ${note}`,
      todo: 'Give the GitHub token permission to merge pull requests, then press Merge on the run.',
    };
  }
  if (/\bci\b/.test(n)) {
    return { label: `Not merged: ${note}`, todo: 'Fix what CI reports, or open the run and merge it yourself.' };
  }
  return { label: `Not merged: ${note}`, todo: 'Open the run to see the pack and press Merge when you are ready.' };
}

/** The card's badge for `item.merge_note`: opens the reason and what to do. Renders nothing without a note. */
const MergeNoteBadge: Component<{ note?: string | null }> = (props) => {
  const [open, setOpen] = createSignal(false);
  return (
    <Show when={props.note}>
      {(note) => {
        const info = () => describeMergeNote(note());
        return (
          <span class="inline-flex flex-col items-start gap-1" data-testid="merge-note">
            <button
              type="button"
              aria-expanded={open()}
              onClick={(e) => {
                e.stopPropagation();
                setOpen((v) => !v);
              }}
              class="cursor-pointer"
            >
              <Badge tone="warning">{info().label}</Badge>
            </button>
            <Show when={open()}>
              <span
                data-testid="merge-note-detail"
                class="block rounded-[12px] px-2 py-1.5 text-xs"
                style={{ 'background-color': 'var(--color-bg-panel)', color: 'var(--color-text-secondary)' }}
                onClick={(e) => e.stopPropagation()}
              >
                <span class="block">{note()}</span>
                <span class="block font-semibold" style={{ color: 'var(--color-text-primary)' }}>What to do</span>
                <span class="block">{info().todo}</span>
              </span>
            </Show>
          </span>
        );
      }}
    </Show>
  );
};

export default MergeNoteBadge;
