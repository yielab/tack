import { type Component, createResource, createSignal, Show } from 'solid-js';
import { api } from '../../../shared/api';
import { Button, Field, Fold } from '../../../shared/ui';
import { toast } from '../../../shared/ui/toast';
import type { Item } from '../../../shared/types';

/** The item's manual GitHub issue link: shows the current link, links one ("owner/repo#42") or unlinks it. */
const GithubIssueSection: Component<{ item: Item }> = (props) => {
  const [link, { refetch }] = createResource(
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
      await refetch();
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
      await refetch();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to unlink the GitHub issue');
    } finally {
      setBusy(false);
    }
  };

  return (
    <Fold kind="github" title="GitHub issue" summary={link() ? `${link()!.repo}#${link()!.issue_number}` : 'no GitHub issue linked'}>
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
            <Button variant="secondary" size="sm" onClick={() => void linkIssue()} loading={busy()}>
              Link
            </Button>
          </div>
        }
      >
        <div class="flex items-center justify-between gap-2 py-1">
          <span class="font-mono text-small font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            {link()!.repo}#{link()!.issue_number}
          </span>
          <Button variant="secondary" size="sm" onClick={() => void unlinkIssue()} loading={busy()}>
            Unlink
          </Button>
        </div>
      </Show>
    </Fold>
  );
};

export default GithubIssueSection;
