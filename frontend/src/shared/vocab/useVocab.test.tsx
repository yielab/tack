import { describe, it, expect, afterEach } from 'vitest';
import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { ProjectContext, type ProjectContextValue } from '../state/projectContext';
import { useVocab } from './useVocab';
import type { Resource } from 'solid-js';

const disposers: Array<() => void> = [];
afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
});

/** Render `children` under a ProjectContext whose vocabulary is driven by `vocab`. */
function renderWithVocab(
  vocab: () => Record<string, string> | undefined,
  children: () => unknown,
) {
  const value: ProjectContextValue = {
    projectId: () => 'p1',
    project: (() => null) as unknown as Resource<null>,
    workflow: () => undefined,
    vocabulary: vocab,
    refetch: () => {},
  };
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ProjectContext.Provider value={value}>
        {children() as never}
      </ProjectContext.Provider>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

function Label(props: { vkey: string }) {
  const { t } = useVocab();
  return <span data-testid="label">{t(props.vkey)}</span>;
}

describe('useVocab', () => {
  it('maps a custom label and falls back to the default', () => {
    const c = renderWithVocab(
      () => ({ task: 'Work Order' }),
      () => (
        <>
          <Label vkey="task" />
          <span data-testid="default">{useVocab().t('sprint')}</span>
        </>
      ),
    );
    expect(c.querySelector('[data-testid="label"]')!.textContent).toBe('Work Order');
    // 'sprint' not overridden → default label
    expect(c.querySelector('[data-testid="default"]')!.textContent).toBe('Sprint');
  });

  it('updates a consuming component when the context vocabulary changes', () => {
    const [vocab, setVocab] = createSignal<Record<string, string>>({});
    const c = renderWithVocab(vocab, () => <Label vkey="sprint" />);

    expect(c.querySelector('[data-testid="label"]')!.textContent).toBe('Sprint');

    setVocab({ sprint: 'Phase' });
    expect(c.querySelector('[data-testid="label"]')!.textContent).toBe('Phase');

    setVocab({});
    expect(c.querySelector('[data-testid="label"]')!.textContent).toBe('Sprint');
  });

  it('exposes reactive item-type configs with current labels', () => {
    const [vocab, setVocab] = createSignal<Record<string, string>>({});
    function Types() {
      const { types } = useVocab();
      return <span data-testid="types">{types().map((t) => t.label).join(',')}</span>;
    }
    const c = renderWithVocab(vocab, () => <Types />);
    expect(c.querySelector('[data-testid="types"]')!.textContent).toContain('Task');

    setVocab({ task: 'Ticket' });
    expect(c.querySelector('[data-testid="types"]')!.textContent).toContain('Ticket');
  });
});
