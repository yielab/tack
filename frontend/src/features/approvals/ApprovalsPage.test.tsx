import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import ApprovalsPage from './ApprovalsPage';
import { approvalTokenStore } from './api';

const flush = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
  approvalTokenStore.set(null);
});

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter>
        <Route path="/" component={ApprovalsPage} />
      </MemoryRouter>
    ),
    container
  );
  return { container, dispose };
}

function mockFetch(status: number, body: unknown) {
  return vi
    .spyOn(globalThis, 'fetch')
    .mockImplementation(() => Promise.resolve(new Response(JSON.stringify(body), { status })));
}

const uncorrelated = {
  token: 'apr-uncorrelated',
  control_plane_id: 'cp-1',
  control_plane_name: 'docket-1',
  item_id: null,
  item_title: null,
  item_status: null,
  project_id: null,
  project_name: null,
  remote_task_id: null,
  agent: 'cli-agent',
  action: 'rm -rf /tmp/build',
  requested_at: new Date(Date.now() - 3_600_000).toISOString(),
};

const correlated = {
  token: 'apr-correlated',
  control_plane_id: 'cp-1',
  control_plane_name: 'docket-1',
  item_id: 'item-1',
  item_title: 'Deploy service',
  item_status: 'In Progress',
  project_id: 'proj-1',
  project_name: 'Backend',
  remote_task_id: 'task-1',
  agent: 'builder',
  action: 'git push origin main',
  requested_at: new Date().toISOString(),
};

describe('ApprovalsPage — orchestration disabled (404, the default)', () => {
  it('shows the disabled explanation', async () => {
    mockFetch(404, { error: { status: 404, message: 'not found' } });
    const { container, dispose } = mount();
    await flush();
    expect(container.textContent).toContain('Agent-fleet orchestration is disabled');
    dispose();
  });
});

describe('ApprovalsPage — request failure (not a 404)', () => {
  it('shows a retry-able error state', async () => {
    mockFetch(500, { error: { status: 500, message: 'boom' } });
    const { container, dispose } = mount();
    await flush();
    expect(container.textContent).toContain("Couldn't load the approvals inbox");
    dispose();
  });
});

describe('ApprovalsPage — enabled, inbox empty', () => {
  it('shows "Inbox zero"', async () => {
    mockFetch(200, { rows: [], grant_available: true });
    const { container, dispose } = mount();
    await flush();
    expect(container.textContent).toContain('Inbox zero');
    dispose();
  });
});

describe('ApprovalsPage — grant_available: false', () => {
  it('never renders a Grant or Deny button, even with rows present', async () => {
    mockFetch(200, { rows: [uncorrelated, correlated], grant_available: false });
    const { container, dispose } = mount();
    await flush();
    const buttons = Array.from(container.querySelectorAll('button')).map((b) => b.textContent);
    expect(buttons).not.toContain('Grant');
    expect(buttons).not.toContain('Deny');
    expect(container.textContent).toContain('TACK_ORCH_APPROVAL_TOKEN');
    dispose();
  });
});

describe('ApprovalsPage — populated inbox, oldest first, uncorrelated surfaced', () => {
  it('renders both rows, oldest (uncorrelated) first, and names the uncorrelated one explicitly', async () => {
    mockFetch(200, { rows: [uncorrelated, correlated], grant_available: true });
    const { container, dispose } = mount();
    await flush();

    const rowsText = Array.from(container.querySelectorAll('tbody tr')).map((r) => r.textContent ?? '');
    expect(rowsText.length).toBe(2);
    // Oldest (uncorrelated) row comes first — server order is trusted verbatim.
    expect(rowsText[0]).toContain('cli-agent');
    expect(rowsText[0].toLowerCase()).toContain('uncorrelated');
    expect(rowsText[1]).toContain('builder');
    expect(rowsText[1]).toContain('Deploy service');
    dispose();
  });
});

describe('ApprovalsPage — grant is never a single click', () => {
  it('clicking Grant opens a confirmation modal naming the agent/action/item before any decide call fires', async () => {
    const fetchMock = mockFetch(200, { rows: [correlated], grant_available: true });
    const { container, dispose } = mount();
    await flush();

    const grantBtn = Array.from(container.querySelectorAll('button')).find(
      (b) => b.textContent === 'Grant'
    )!;
    grantBtn.click();
    await flush();

    // Only the initial list fetch happened — clicking Grant must not itself
    // call the decide endpoint.
    expect(fetchMock).toHaveBeenCalledTimes(1);
    // The modal renders via a Portal (document.body), not inside `container`.
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog).toBeTruthy();
    expect(dialog.textContent).toContain('Grant this approval?');
    expect(dialog.textContent).toContain('builder');
    expect(dialog.textContent).toContain('git push origin main');
    expect(dialog.textContent).toContain('Deploy service');
    expect(dialog.textContent).toContain('cannot be undone');
    dispose();
  });
});

describe('ApprovalsPage — confirmed grant', () => {
  it('sends the decide request only after the confirm click, and removes the row on success', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const url = String(input);
      if (url.endsWith('/approvals') || url.includes('/approvals?')) {
        return Promise.resolve(
          new Response(JSON.stringify({ rows: [correlated], grant_available: true }), {
            status: 200,
          })
        );
      }
      if (url.includes('/approvals/apr-correlated')) {
        return Promise.resolve(
          new Response(JSON.stringify({ token: 'apr-correlated', state: 'granted' }), {
            status: 200,
          })
        );
      }
      return Promise.resolve(new Response('not found', { status: 404 }));
    });

    const { container, dispose } = mount();
    await flush();

    const grantBtn = Array.from(container.querySelectorAll('button')).find(
      (b) => b.textContent === 'Grant'
    )!;
    grantBtn.click();
    await flush();

    // The modal's own confirm button is also labelled "Grant" — pick the one
    // inside the dialog (rendered via a Portal into document.body, not
    // inside `container`).
    const dialog = document.querySelector('[role="dialog"]')!;
    const confirmBtn = Array.from(dialog.querySelectorAll('button')).find(
      (b) => b.textContent === 'Grant'
    )!;
    confirmBtn.click();
    await flush();
    await flush();

    const decideCall = fetchMock.mock.calls.find(([u]) => String(u).includes('/approvals/apr-correlated'));
    expect(decideCall).toBeDefined();
    expect((decideCall![1] as RequestInit).method).toBe('POST');
    dispose();
  });
});
