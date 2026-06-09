import { type Component } from 'solid-js';
import RichTextEditor from '../../../shared/ui/RichTextEditor';
import type { Item } from '../../../shared/types';

export interface DetailsTabProps {
  item: Item;
  /** Persist a description change (debounced by the caller is optional). */
  onDescriptionChange: (html: string) => void;
}

/** Item description, edited with the existing rich-text editor. */
const DetailsTab: Component<DetailsTabProps> = (props) => {
  return (
    <div class="space-y-3">
      <h3 class="text-sm font-semibold" style={{ color: 'var(--color-text-secondary)' }}>
        Description
      </h3>
      <RichTextEditor
        value={props.item.description ?? ''}
        onChange={props.onDescriptionChange}
        placeholder="Add details, acceptance criteria, or notes…"
      />
    </div>
  );
};

export default DetailsTab;
