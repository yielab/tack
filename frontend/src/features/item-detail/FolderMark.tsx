import { type Component, Show } from 'solid-js';
import type { FolderMarks, PlanField } from './useFolderMarks';

/** A plan field's small note: that the project folder holds it, or that the board changed it since the folder's last revision. */
export const FolderMark: Component<{ marks: FolderMarks; fields: PlanField[]; class?: string }> = (props) => (
  <Show when={props.marks.say(...props.fields)}>
    {(said) => (
      <span
        data-testid="folder-mark"
        data-state={said().kind}
        class={`text-caption ${props.class ?? ''}`}
        style={{ color: 'var(--color-text-secondary)' }}
      >
        {said().kind === 'changed'
          ? `Changed on the board since revision #${(said() as { revision: number }).revision}: the next export writes it`
          : 'In the project folder'}
      </span>
    )}
  </Show>
);

/** A field the board owns, in a project that has a folder. */
export const BoardMark: Component<{ marks: FolderMarks }> = (props) => (
  <Show when={props.marks.revision() !== null}>
    <span data-testid="board-mark" class="text-caption" style={{ color: 'var(--color-text-secondary)' }}>
      The board's
    </span>
  </Show>
);
