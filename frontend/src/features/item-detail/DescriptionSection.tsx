import { type Component, createEffect, createSignal, on, Show } from 'solid-js';
import RichTextEditor, { sanitizeRichText } from '../../shared/ui/RichTextEditor';
import { descriptionToHtml } from '../../shared/ui/descriptionMarkdown';
import { Button, SectionHeading } from '../../shared/ui';
import type { Item } from '../../shared/types';
import { FolderMark } from './FolderMark';
import { IconDescription } from './sectionIcons';
import type { FolderMarks } from './useFolderMarks';

export interface DescriptionSectionProps {
  item: Item;
  /** Persist a description change (debounced by the caller). */
  onDescriptionChange: (html: string) => void;
  marks: FolderMarks;
}

/** The description: the task told to a person, in reading size. It reads as text and becomes the editor on a click. */
const DescriptionSection: Component<DescriptionSectionProps> = (props) => {
  // The text shown follows what was typed, so leaving the editor never flashes the
  // value from before the debounced save lands.
  const [editing, setEditing] = createSignal(false);
  const [html, setHtml] = createSignal(props.item.description ?? '');
  let editorBox: HTMLDivElement | undefined;
  createEffect(
    on(
      () => props.item.description,
      (d) => {
        if (!editing()) setHtml(d ?? '');
      },
      { defer: true },
    ),
  );
  const edit = () => {
    // A Markdown description becomes the editor's HTML on its first edit.
    setHtml(descriptionToHtml(html()));
    setEditing(true);
    queueMicrotask(() => editorBox?.querySelector<HTMLElement>('[role="textbox"]')?.focus());
  };
  const typed = (value: string) => {
    setHtml(value);
    props.onDescriptionChange(value);
  };

  return (
    <section aria-labelledby="item-description" class="space-y-4">
      <SectionHeading
        id="item-description"
        icon={<IconDescription />}
        actions={
          <>
            <Show when={!editing() && html().trim() !== ''}>
              <Button variant="ghost" size="sm" type="button" aria-label="Edit description" onClick={edit}>
                Edit
              </Button>
            </Show>
            <Show when={editing()}>
              <Button variant="ghost" size="sm" type="button" onClick={() => setEditing(false)}>
                Done
              </Button>
            </Show>
          </>
        }
      >
        Description
      </SectionHeading>
      <FolderMark marks={props.marks} fields={['description']} class="block" />
      <div style={{ 'max-width': '68ch' }}>
        <Show
          when={editing()}
          fallback={
            <Show
              when={html().trim() !== ''}
              fallback={
                <button
                  type="button"
                  class="w-full rounded-md border border-dashed px-4 py-6 text-left text-body"
                  style={{ 'border-color': 'var(--color-border-medium)', color: 'var(--color-text-secondary)' }}
                  onClick={edit}
                >
                  Click to edit
                </button>
              }
            >
              <div
                class="prose dark:prose-invert text-reading max-w-none cursor-text"
                data-testid="description"
                innerHTML={sanitizeRichText(descriptionToHtml(html()))}
                onClick={(e) => {
                  if (!(e.target as HTMLElement).closest('a')) edit();
                }}
              />
            </Show>
          }
        >
          <div ref={editorBox}>
            <RichTextEditor
              label="Description"
              value={html()}
              onChange={typed}
              placeholder="Add details or notes…"
            />
          </div>
        </Show>
      </div>
    </section>
  );
};

export default DescriptionSection;
