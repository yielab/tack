import { type Component, Show, createResource, onCleanup } from 'solid-js';
import { A } from '@solidjs/router';
import { runnersApi } from '../execution';
import { isActiveRunnerState, isAnsweringRunner, silentRunnersText } from '../runWithAgent/shared';

// The sidebar's warning while computers are set up but none answers: no run can start until one does.
// A workspace with no computer at all is the first-run banner's case, not this one. It reads
// `GET /api/runners` every few seconds and says nothing while it cannot read them.

const POLL_MS = 15_000;

const AgentsOffline: Component<{ onNavigate?: () => void }> = (props) => {
  const [runners, { refetch }] = createResource(() => runnersApi.list().then((r) => r.data.data ?? [], () => null));
  const timer = setInterval(() => void refetch(), POLL_MS);
  onCleanup(() => clearInterval(timer));

  /** Why no run can start; null while one can, or while the list cannot be read. */
  const silent = (): string | null => {
    // `latest` keeps the last answer during a refetch, so the warning does not blink.
    const rows = runners.latest;
    if (!rows) return null;
    const active = rows.filter((r) => isActiveRunnerState(r.state));
    if (active.length === 0 || active.some(isAnsweringRunner)) return null;
    return silentRunnersText(active);
  };

  return (
    <Show when={silent()}>
      {(text) => (
        <div style={{ padding: '0 14px 10px' }}>
          <A
            href="/agents"
            onClick={() => props.onNavigate?.()}
            role="status"
            data-testid="agents-offline"
            title={text()}
            style={{
              display: 'flex', 'flex-direction': 'column', gap: '2px',
              padding: '8px 14px', 'border-radius': '18px',
              background: 'var(--color-warning-100)', color: 'var(--color-warning-700)',
            }}
          >
            <span style={{ display: 'flex', 'align-items': 'center', gap: '8px', 'font-size': '0.8125rem', 'font-weight': 700 }}>
              <span aria-hidden="true" style={{ width: '8px', height: '8px', 'border-radius': '99px', background: 'var(--color-warning-700)', 'flex-shrink': 0 }} />
              Agents offline
            </span>
            <span style={{ 'font-size': '0.75rem', 'overflow-wrap': 'anywhere' }}>{text()}</span>
            <span style={{ 'font-size': '0.75rem', 'font-weight': 600, 'text-decoration': 'underline' }}>Turn on agent execution →</span>
          </A>
        </div>
      )}
    </Show>
  );
};

export default AgentsOffline;
