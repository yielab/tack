import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { ExecutionStoreProvider } from '../state/executionContext';
import RunWithAgentButton from './RunWithAgentButton';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

function mockFetch(): typeof fetch {
  return (async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.includes('/runner-fleets')) return new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 });
    if (url.includes('/agent-profiles')) return new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 });
    if (url.includes('/model-profiles')) return new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 });
    if (url.includes('/executions')) return new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 });
    return new Response(JSON.stringify({}), { status: 200 });
  }) as typeof fetch;
}

function mount(props: { compact?: boolean } = {}) {
  vi.spyOn(globalThis, 'fetch').mockImplementation(mockFetch());
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ExecutionStoreProvider>
        <RunWithAgentButton itemId="item-1" itemTitle="Fix login bug" {...props} />
      </ExecutionStoreProvider>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('RunWithAgentButton', () => {
  it('compact mode renders an icon-only trigger with an item-specific accessible name — distinct from DispatchCardMenu\'s "⋮" kebab', () => {
    const c = mount({ compact: true });
    const trigger = c.querySelector('button[aria-label="Run with agent: Fix login bug"]');
    expect(trigger).toBeTruthy();
    expect(trigger?.textContent).toBe('▶');
    expect(c.querySelector('[aria-haspopup="menu"]')).toBeNull();
  });

  it('labeled mode renders a "Run with agent" button', () => {
    const c = mount({ compact: false });
    expect(c.textContent).toContain('Run with agent');
  });

  it('clicking the trigger opens the shared modal, titled with the item', async () => {
    const c = mount({ compact: true });
    (c.querySelector('button') as HTMLButtonElement).click();
    await flush();
    const dialog = document.querySelector('[role="dialog"]');
    expect(dialog).toBeTruthy();
    expect(dialog?.textContent).toContain('Run with agent: Fix login bug');
  });

  it('Cancel closes the modal', async () => {
    const c = mount({ compact: true });
    (c.querySelector('button') as HTMLButtonElement).click();
    await flush();
    expect(document.querySelector('[role="dialog"]')).toBeTruthy();
    const cancelBtn = [...document.querySelectorAll('button')].find((b) => b.textContent === 'Cancel');
    cancelBtn!.click();
    await flush();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it('a click on the trigger does not bubble to a parent click handler (Board/Sprint card click-to-open)', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(mockFetch());
    const parentClick = vi.fn();
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(
      () => (
        <ExecutionStoreProvider>
          <div onClick={parentClick}>
            <RunWithAgentButton itemId="item-1" itemTitle="Fix login bug" compact />
          </div>
        </ExecutionStoreProvider>
      ),
      container,
    );
    disposers.push(() => {
      dispose();
      container.remove();
    });
    (container.querySelector('button') as HTMLButtonElement).click();
    await flush();
    expect(parentClick).not.toHaveBeenCalled();
  });
});
