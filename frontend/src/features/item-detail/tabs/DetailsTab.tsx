import { type Component, createResource, createSignal, Show } from 'solid-js';
import RichTextEditor from '../../../shared/ui/RichTextEditor';
import { api } from '../../../shared/api';
import { Button, Field } from '../../../shared/ui';
import { toast } from '../../../shared/ui/toast';
import type { Item } from '../../../shared/types';

export interface DetailsTabProps {
  item: Item;
  /** Persist a description change (debounced by the caller is optional). */
  onDescriptionChange: (html: string) => void;
}

/** Item description, edited with the existing rich-text editor, plus the
 * item's manual GitHub issue link (link, unlink; shows the current link). */
const DetailsTab: Component<DetailsTabProps> = (props) => {
  const [link, { refetch: refetchLink }] = createResource(
    () => props.item.id,
    (id) => api.items.getGithubLink(id),
  );
  const [draft, setDraft] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  const linkIssue = async () => {
    // One field, split on the last `#`: "owner/repo#42".
    const raw = draft().trim();
    const hashIndex = raw.lastIndexOf('#');
    const repo = hashIndex > 0 ? raw.slice(0, hashIndex) : '';
    const issueNumber = hashIndex > 0 ? Number(raw.slice(hashIndex + 1)) : NaN;
    if (!repo || !Number.isInteger(issueNumber) || issueNumber < 1) {
      toast.error('Enter the link as owner/repo#issue-number');
      return;
    }
    setBusy(true);
    try {
      await api.items.setGithubLink(props.item.id, repo, issueNumber);
      setDraft('');
      await refetchLink();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to link the GitHub issue');
    } finally {
      setBusy(false);
    }
  };

  const unlinkIssue = async () => {
    setBusy(true);
    try {
      await api.items.removeGithubLink(props.item.id);
      await refetchLink();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to unlink the GitHub issue');
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="space-y-4">
      <section class="space-y-3 rounded-[28px] p-5" style={{ 'background-color': 'var(--color-bg-panel)' }}>
      <h3 class="text-lg" style={{ color: 'var(--color-text-primary)' }}>
        Description
      </h3>
      <RichTextEditor
        value={props.item.description ?? ''}
        onChange={props.onDescriptionChange}
        placeholder="Add details, acceptance criteria, or notes…"
      />
      </section>

      <section class="space-y-3 rounded-[28px] p-5" style={{ 'background-color': 'var(--color-bg-panel)' }}>
      <h3 class="text-lg" style={{ color: 'var(--color-text-primary)' }}>
        Link GitHub issue
      </h3>
      <Show
        when={link()}
        fallback={
          <div class="flex items-end gap-2">
            <Field
              class="flex-1"
              label="Issue"
              placeholder="owner/repo#42"
              value={draft()}
              disabled={busy()}
              onInput={(e) => setDraft(e.currentTarget.value)}
            />
            <Button size="sm" onClick={() => void linkIssue()} loading={busy()}>
              Link
            </Button>
          </div>
        }
      >
        <div class="flex items-center justify-between gap-2 rounded-[20px] py-2 pl-4 pr-2" style={{ 'background-color': 'var(--color-bg-app)', 'box-shadow': 'var(--shadow-sm)' }}>
          <span class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)', 'font-family': 'var(--font-mono)' }}>
            {link()!.repo}#{link()!.issue_number}
          </span>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void unlinkIssue()}
            loading={busy()}
          >
            Unlink
          </Button>
        </div>
      </Show>
      </section>
    </div>
  );
};

export default DetailsTab;
