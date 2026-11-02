import { type Component, For, Show, createMemo, createResource, createSignal, onCleanup } from 'solid-js';
import { Button, LoadError, Skeleton } from '../../shared/ui';
import { api } from '../../shared/api';
import { toast } from '../../shared/ui/toast';
import Message from './Message';
import TurnState from './TurnState';
import { composerState, entries, sendProblem } from './threadModel';

/** While a turn is pending the thread is read again this often, to show its reply as soon as it is posted. */
const POLL_MS = 3000;

/**
 * One thread: its messages in order with who wrote each, the state of each turn, the change sets of a reply
 * inline, and a composer. The composer is off, with its reason beside it, while a turn is answering and once
 * the thread is closed. Anyone may close an open thread.
 */
const ThreadPanel: Component<{
  threadId: string;
  projectId: string;
  /** The name of the profile that answers, said as the writer of its replies. */
  profileName: string;
  /** Called after a message was sent or the thread was closed, so a list of threads can read again. */
  onChanged?: () => void;
}> = (props) => {
  const [detail, { refetch, mutate }] = createResource(
    () => props.threadId,
    (id) => api.threads.get(id),
  );
  const [draft, setDraft] = createSignal('');
  const [sending, setSending] = createSignal(false);
  const [problem, setProblem] = createSignal<string | null>(null);

  const thread = () => detail()?.thread;
  const list = createMemo(() => entries(detail()?.messages ?? [], detail()?.turns ?? []));
  const composer = createMemo(() => {
    const t = thread();
    return t ? composerState(t, detail()?.turns ?? []) : { disabled: true, reason: null };
  });
  const pending = () => (detail()?.turns ?? []).some((t) => t.state === 'pending');

  const timer = setInterval(() => {
    if (pending() && !detail.loading) void refetch();
  }, POLL_MS);
  onCleanup(() => clearInterval(timer));

  const send = async () => {
    const body = draft().trim();
    if (body === '' || composer().disabled || sending()) return;
    setSending(true);
    setProblem(null);
    try {
      await api.threads.post(props.threadId, body);
      setDraft('');
      await refetch();
      props.onChanged?.();
    } catch (error) {
      setProblem(sendProblem(error));
      // The thread may have closed or taken a turn meanwhile; show it as it is now.
      await refetch();
    } finally {
      setSending(false);
    }
  };

  const close = async () => {
    try {
      const closed = await api.threads.close(props.threadId);
      const current = detail();
      if (current) mutate({ ...current, thread: closed });
      props.onChanged?.();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'The thread could not be closed.');
    }
  };

  return (
    <section data-testid="thread-panel" class="flex min-w-0 flex-col gap-3" aria-label="Thread">
      <Show when={!detail.error} fallback={<LoadError what="the thread" error={detail.error} onRetry={() => void refetch()} />}>
        <Show when={thread()} fallback={<Skeleton />}>
          {(t) => (
            <>
              <div class="flex flex-wrap items-start justify-between gap-3">
                <div class="min-w-0">
                  <h2 class="m-0 text-lg" style={{ color: 'var(--color-text-primary)' }} data-testid="thread-title">
                    {t().title}
                  </h2>
                  <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                    With {props.profileName} · {t().turns === 1 ? '1 turn' : `${t().turns} turns`} ·{' '}
                    <span data-testid="thread-status">{t().status === 'open' ? 'Open' : 'Closed'}</span>
                  </p>
                </div>
                <Button
                  size="sm"
                  variant="secondary"
                  data-testid="thread-close"
                  disabled={t().status === 'closed'}
                  reason={t().status === 'closed' ? 'This thread is already closed.' : undefined}
                  onClick={() => void close()}
                >
                  Close thread
                </Button>
              </div>

              <Show
                when={list().length > 0}
                fallback={
                  <p data-testid="thread-empty" class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                    Nothing has been said yet. Write a message below: the agent answers each message with one turn, and its
                    answer may carry changes for you to apply.
                  </p>
                }
              >
                <ol data-testid="message-list" class="m-0 flex min-w-0 list-none flex-col gap-3 p-0">
                  <For each={list()}>
                    {(entry) => (
                      <>
                        <Message entry={entry} profileName={props.profileName} projectId={props.projectId} />
                        <Show when={entry.turn}>
                          {(turn) => <TurnState turn={turn()} version={detail()} onAnswered={() => void refetch()} />}
                        </Show>
                      </>
                    )}
                  </For>
                </ol>
              </Show>

              <form
                class="flex min-w-0 flex-col gap-2"
                onSubmit={(e) => {
                  e.preventDefault();
                  void send();
                }}
              >
                <label class="text-sm font-semibold" for={`composer-${props.threadId}`} style={{ color: 'var(--color-text-primary)' }}>
                  Your message
                </label>
                <textarea
                  id={`composer-${props.threadId}`}
                  data-testid="composer"
                  rows={3}
                  value={draft()}
                  disabled={composer().disabled || sending()}
                  aria-describedby={composer().reason ? `composer-why-${props.threadId}` : undefined}
                  placeholder="Ask for something, or answer what it asked"
                  onInput={(e) => setDraft(e.currentTarget.value)}
                  class="w-full rounded-item border p-2 text-sm"
                  style={{
                    'background-color': 'var(--color-bg-base)',
                    color: 'var(--color-text-primary)',
                    'border-color': 'var(--color-border-medium)',
                  }}
                />
                <Show when={composer().reason}>
                  <p id={`composer-why-${props.threadId}`} data-testid="composer-reason" class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                    {composer().reason}
                  </p>
                </Show>
                <Show when={problem()}>
                  <p data-testid="composer-problem" role="alert" class="text-sm" style={{ color: 'var(--color-danger-600)' }}>
                    {problem()}
                  </p>
                </Show>
                <div>
                  <Button type="submit" size="sm" data-testid="send" disabled={composer().disabled || sending() || draft().trim() === ''} loading={sending()}>
                    Send
                  </Button>
                </div>
              </form>
            </>
          )}
        </Show>
      </Show>
    </section>
  );
};

export default ThreadPanel;
