import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { A, useParams, useSearchParams } from '@solidjs/router';
import { Button, LoadError, Skeleton } from '../../shared/ui';
import { api } from '../../shared/api';
import type { Thread } from '../../shared/api/threads';
import { agentProfilesApi } from '../../shared/execution';
import { relativeTimeFromIso } from '../../shared/runWithAgent/shared';
import NewThread from './NewThread';
import ThreadPanel from './ThreadPanel';

/**
 * A project's threads: the list on one side, the open thread on the other, and a form to start one. A
 * thread is a conversation with an agent profile; each message you write is answered by one run of it.
 */
const ThreadsPage: Component = () => {
  const params = useParams();
  const [search, setSearch] = useSearchParams();
  const projectId = () => params.id as string;
  const opened = () => (typeof search.open === 'string' && search.open !== '' ? search.open : undefined);
  const [starting, setStarting] = createSignal(false);
  const [list, { refetch }] = createResource(projectId, (id) => api.threads.list(id));
  const [profiles] = createResource(() => agentProfilesApi.list().then((r) => r.data.data).catch(() => []));
  const threads = (): Thread[] => (list.error || !Array.isArray(list()) ? [] : list()!);
  const profileName = (id: string) => profiles()?.find((p) => p.agent_profile_id === id)?.name ?? 'The agent';
  const current = () => threads().find((t) => t.id === opened());

  return (
    <div class="flex min-w-0 flex-col gap-4 px-6 py-7 sm:px-12" data-testid="threads-page">
      <div class="flex flex-wrap items-end justify-between gap-3">
        <div class="min-w-0">
          <h1 class="m-0 text-content leading-tight" style={{ 'font-size': '2.125rem', color: 'var(--color-text-primary)' }}>
            Threads
          </h1>
          <p class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
            Talk with an agent profile. Each message you write is answered by one run of it, and its answer may carry changes
            for you to apply.
          </p>
        </div>
        <Button size="sm" data-testid="new-thread-open" onClick={() => setStarting(true)} disabled={starting()}>
          New thread
        </Button>
      </div>

      <Show when={starting()}>
        <NewThread
          projectId={projectId()}
          profiles={profiles() ?? []}
          onCancel={() => setStarting(false)}
          onStarted={(thread) => {
            setStarting(false);
            void refetch();
            setSearch({ open: thread.id });
          }}
        />
      </Show>

      <Show when={!list.error} fallback={<LoadError what="the threads" error={list.error} onRetry={() => void refetch()} />}>
        <Show when={!list.loading || list()} fallback={<Skeleton />}>
          <div class="grid min-w-0 gap-6 lg:grid-cols-3">
            <Show
              when={threads().length > 0}
              fallback={
                <p data-testid="threads-empty" class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                  There are no threads in this project yet. Start one to talk with an agent profile.
                </p>
              }
            >
              <ul data-testid="thread-list" class="m-0 flex min-w-0 list-none flex-col gap-2 p-0">
                <For each={threads()}>
                  {(t) => (
                    <li data-testid="thread-item">
                      <A
                        href={`/projects/${projectId()}/threads?open=${t.id}`}
                        class="flex min-w-0 flex-col gap-0.5 rounded-item p-3"
                        aria-current={t.id === opened() ? 'true' : undefined}
                        style={{
                          border: `1px solid ${t.id === opened() ? 'var(--color-primary-600)' : 'var(--color-border-light)'}`,
                          'background-color': t.id === opened() ? 'var(--color-accent-soft)' : 'transparent',
                          color: 'var(--color-text-primary)',
                        }}
                      >
                        <span class="truncate text-sm font-semibold">{t.title}</span>
                        <span class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                          {profileName(t.profile)} · {t.status === 'open' ? 'Open' : 'Closed'} · {relativeTimeFromIso(t.last_message_at)}
                        </span>
                      </A>
                    </li>
                  )}
                </For>
              </ul>
            </Show>

            <div class="min-w-0 lg:col-span-2">
              <Show
                when={opened()}
                fallback={
                  <Show when={threads().length > 0}>
                    <p data-testid="thread-none-open" class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                      Pick a thread to read it.
                    </p>
                  </Show>
                }
              >
                {(id) => (
                  <ThreadPanel
                    threadId={id()}
                    projectId={projectId()}
                    profileName={profileName(current()?.profile ?? '')}
                    onChanged={() => void refetch()}
                  />
                )}
              </Show>
            </div>
          </div>
        </Show>
      </Show>
    </div>
  );
};

export default ThreadsPage;
