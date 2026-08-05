import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { createResource, createEffect, Component, Switch, Match } from 'solid-js';
import { provisioningApi } from './api';
import { Button, EmptyState, Modal } from '../../shared/ui';
import { MemoryRouter, Route, useNavigate } from '@solidjs/router';

const flush = () => new Promise((r) => setTimeout(r, 20));

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

const Probe: Component = () => {
  const navigate = useNavigate();
  void navigate;
  const [res, { refetch }] = createResource(() => provisioningApi.listControlPlanes());
  createEffect(() => {
    console.log('PROBE', { loading: res.loading, error: res.error, state: res.state });
  });
  return (
    <div>
      <Switch fallback={<div>loading</div>}>
        <Match when={res.loading}>
          <div>loading2</div>
        </Match>
        <Match when={res.error !== undefined}>
          <EmptyState
            icon="⚠️"
            title="err title"
            description="err desc"
            action={<Button onClick={() => refetch()}>Retry</Button>}
          />
        </Match>
        <Match when={res.error === undefined}>
          <div>ok</div>
          <Modal isOpen={false} onClose={() => {}} title="x">
            <div>modal content</div>
          </Modal>
        </Match>
      </Switch>
    </div>
  );
};

describe('debug createResource with a 404 + EmptyState/Modal', () => {
  it('settles', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ error: { status: 404, message: 'nf' } }), { status: 404 })
    );
    const container = document.createElement('div');
    document.body.appendChild(container);
    render(() => (<MemoryRouter><Route path="/" component={Probe} /></MemoryRouter>), container);
    await flush();
    await flush();
    await flush();
    console.log('FINAL HTML', container.innerHTML);
    expect(container.textContent).toContain('err title');
  });
});
