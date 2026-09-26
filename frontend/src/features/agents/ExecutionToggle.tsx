import { type Component, Show, createResource, createSignal } from 'solid-js';
import { Badge, Button, Skeleton } from '../../shared/ui';
import { toast } from '../../shared/ui/toast';
import { isLocalRunnerUnavailable, localRunnerApi } from './api';

/**
 * One switch — *Agent execution on this machine* (ADR 0061 decision 6). No
 * flag, no restart: `PUT /api/local-runner` starts or stops the embedded
 * runner immediately, and the choice survives the next `tack serve` because
 * the server persists it (`app_meta`) rather than only holding it in
 * memory.
 *
 * The route is a genuine 404 — not a gate that refuses the request — on any
 * non-loopback bind, or when this Tack process never wired an embedded
 * runner in at all (`isLocalRunnerUnavailable`, `./api.ts`'s header
 * comment). That case renders the equivalent console command instead of an
 * error: turning this on is unavailable here by design, not broken.
 */
const ExecutionToggle: Component = () => {
  const [status, { refetch }] = createResource(() => localRunnerApi.get());
  const [toggling, setToggling] = createSignal(false);

  const unavailable = () => isLocalRunnerUnavailable(status.error);
  const loadFailed = () => status.error !== undefined && !unavailable();

  const setEnabled = async (next: boolean) => {
    if (toggling()) return;
    setToggling(true);
    try {
      await localRunnerApi.update(next);
      await refetch();
      toast.success(next ? 'Agent execution turned on.' : 'Agent execution turned off.');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to update the setting');
    } finally {
      setToggling(false);
    }
  };

  return (
    <section class="flex flex-col gap-3 rounded-[28px] bg-panel px-[22px] py-5">
      <div class="flex items-center gap-2.5">
        <h2 class="text-[20px] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
          Agent execution on this machine
        </h2>
        <Show when={!status.loading && !unavailable() && !loadFailed()}>
          <Badge class="ml-auto" tone={status()!.state === 'running' ? 'success' : 'neutral'}>
            {status()!.state === 'running' ? 'Running' : 'Stopped'}
          </Badge>
        </Show>
      </div>

      <Show when={status.loading}>
        <Skeleton height="60px" class="rounded-[22px]" />
      </Show>

      <Show when={unavailable()}>
        <p class="text-[13px]" style={{ color: 'var(--color-text-secondary)' }}>
          Not available from this screen — this is a remote-runner deployment (this server
          isn't bound to its own loopback address, or this build has no embedded runner
          wired in), so it can never run one here for security reasons. Enroll a runner on
          a separate machine that has your code and credentials instead — see{' '}
          <strong>Advanced</strong> below.
        </p>
      </Show>

      <Show when={loadFailed()}>
        <div class="text-[13px]" style={{ color: 'var(--color-danger-600)' }}>
          Couldn't load the current state.{' '}
          <button type="button" class="font-bold underline" onClick={() => void refetch()}>
            Retry
          </button>
        </div>
      </Show>

      <Show when={!status.loading && !unavailable() && !loadFailed()}>
        <div class="flex flex-wrap items-center gap-3">
          <Button
            variant={status()!.enabled ? 'secondary' : 'primary'}
            loading={toggling()}
            disabled={toggling()}
            onClick={() => void setEnabled(!status()!.enabled)}
          >
            {status()!.enabled ? 'Turn off' : 'Turn on'}
          </Button>
          <span class="text-[13px]" style={{ color: 'var(--color-text-secondary)' }}>
            <Show
              when={status()!.since}
              fallback={status()!.enabled ? 'Starting…' : 'Off since this server started.'}
            >
              {status()!.state === 'running' ? 'Running since ' : 'Stopped — last ran since '}
              {new Date(status()!.since!).toLocaleString()}
            </Show>
          </span>
        </div>
      </Show>
    </section>
  );
};

export default ExecutionToggle;
