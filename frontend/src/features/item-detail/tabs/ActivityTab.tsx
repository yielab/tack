import { type Component, createResource, createSignal, For, Show } from 'solid-js';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { Button, EmptyState } from '../../../shared/ui';
import type { Comment } from '../../../shared/types';

export interface ActivityTabProps {
  itemId: string;
}

function relativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  const diff = Date.now() - then;
  if (Number.isNaN(diff)) return '';
  const sec = Math.round(diff / 1000);
  if (sec < 60) return 'just now';
  const min = Math.round(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.round(hr / 24);
  if (day < 30) return `${day}d ago`;
  return new Date(iso).toLocaleDateString();
}

/** Comments for an item, newest-last, with optimistic posting (T-507). */
const ActivityTab: Component<ActivityTabProps> = (props) => {
  const [comments, { mutate }] = createResource(
    () => props.itemId,
    (id) => api.comments.list(id),
  );
  const [content, setContent] = createSignal('');
  const [posting, setPosting] = createSignal(false);

  const post = async () => {
    const text = content().trim();
    if (!text) return;
    const prev = comments() ?? [];
    const optimistic: Comment = {
      id: `temp-${crypto.randomUUID()}`,
      item_id: props.itemId,
      author: null,
      content: text,
      comment_type: 'comment',
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    };
    mutate([...prev, optimistic]);
    setContent('');
    setPosting(true);
    try {
      const created = await api.comments.create(props.itemId, { content: text });
      mutate([...prev, created]);
    } catch (err) {
      mutate(prev); // roll back the optimistic append
      setContent(text);
      toast.error(err instanceof Error ? err.message : 'Failed to post comment');
    } finally {
      setPosting(false);
    }
  };

  return (
    <div class="space-y-4">
      <Show
        when={(comments() ?? []).length > 0}
        fallback={<EmptyState title="No comments yet" description="Start the discussion below." />}
      >
        <ul class="space-y-3">
          <For each={comments()}>
            {(c) => (
              <li
                class="rounded-lg border p-3"
                style={{
                  'background-color': 'var(--color-bg-base)',
                  'border-color': 'var(--color-border-light)',
                }}
              >
                <div class="mb-1 flex items-center justify-between text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                  <span>{c.author || 'Anonymous'}</span>
                  <span>{relativeTime(c.created_at)}</span>
                </div>
                <p class="whitespace-pre-wrap text-sm" style={{ color: 'var(--color-text-primary)' }}>
                  {c.content}
                </p>
              </li>
            )}
          </For>
        </ul>
      </Show>

      <form
        class="space-y-2"
        onSubmit={(e) => {
          e.preventDefault();
          void post();
        }}
      >
        <textarea
          value={content()}
          onInput={(e) => setContent(e.currentTarget.value)}
          placeholder="Write a comment…"
          rows={3}
          disabled={posting()}
          class="w-full resize-none rounded-lg border px-3 py-2 text-sm transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1"
          style={{
            'background-color': 'var(--color-bg-base)',
            color: 'var(--color-text-primary)',
            'border-color': 'var(--color-border-medium)',
            '--tw-ring-color': 'var(--color-focus-ring)',
          }}
        />
        <div class="flex justify-end">
          <Button type="submit" size="sm" loading={posting()} disabled={posting() || !content().trim()}>
            Comment
          </Button>
        </div>
      </form>
    </div>
  );
};

export default ActivityTab;
