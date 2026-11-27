import { type Component, createResource, createSignal, For, Show } from 'solid-js';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { Button } from '../../../shared/ui';
import type { Comment } from '../../../shared/types';

export interface ActivitySectionProps {
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

/** The item's comments, oldest first, and the field that adds one (posted at once, rolled back on a failure). */
const ActivitySection: Component<ActivitySectionProps> = (props) => {
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

  const who = (c: Comment) => c.author || (c.comment_type === 'system' ? 'Tack' : 'Anonymous');

  return (
    <div class="space-y-4">
      <Show
        when={(comments() ?? []).length > 0}
        fallback={
          <p data-testid="no-comments" class="text-body" style={{ color: 'var(--color-text-secondary)' }}>
            No comments yet. Start the discussion below.
          </p>
        }
      >
        <ol class="space-y-3.5">
          <For each={comments()}>
            {(c) => (
              <li class="flex items-start gap-3">
                <span
                  aria-hidden="true"
                  class="mt-0.5 grid h-7 w-7 flex-none place-items-center rounded-full text-micro font-bold"
                  style={{ 'background-color': 'var(--color-bg-subtle)', color: 'var(--color-text-secondary)' }}
                >
                  {who(c).slice(0, 2).toUpperCase()}
                </span>
                <div class="min-w-0 flex-1">
                  <p class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
                    <span class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>{who(c)}</span> · {relativeTime(c.created_at)}
                  </p>
                  <p class="whitespace-pre-wrap text-body" style={{ color: 'var(--color-text-primary)' }}>
                    {c.content}
                  </p>
                </div>
              </li>
            )}
          </For>
        </ol>
      </Show>

      <form
        class="space-y-2"
        onSubmit={(e) => {
          e.preventDefault();
          void post();
        }}
      >
        <textarea
          aria-label="Comment"
          value={content()}
          onInput={(e) => setContent(e.currentTarget.value)}
          placeholder="Leave a comment…"
          rows={3}
          disabled={posting()}
          class="w-full resize-none rounded-md border px-3.5 py-3 text-lead transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1"
          style={{
            'background-color': 'var(--color-bg-base)',
            color: 'var(--color-text-primary)',
            'border-color': 'var(--color-border-medium)',
            '--tw-ring-color': 'var(--color-focus-ring)',
          }}
        />
        <div class="flex justify-end">
          <Button type="submit" variant="secondary" size="sm" loading={posting()} disabled={posting() || !content().trim()}>
            Comment
          </Button>
        </div>
      </form>
    </div>
  );
};

export default ActivitySection;
