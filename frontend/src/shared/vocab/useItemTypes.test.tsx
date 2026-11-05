import { describe, it, expect, afterEach } from 'vitest';
import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import type { Resource } from 'solid-js';
import { ProjectContext, type ProjectContextValue } from '../state/projectContext';
import type { ItemTypeDef, Project } from '../types';
import { useItemTypes, capitalised } from './useItemTypes';

const disposers: Array<() => void> = [];
afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
});

const BUILDING: ItemTypeDef = { key: 'epic', name: 'Building', level: 'group', color: 'accent', order: 0, default_work: false };
const WORK_ORDER: ItemTypeDef = { key: 'task', name: 'Work Order', level: 'work', color: 'neutral', order: 1, default_work: true };

function renderWith(types: () => ItemTypeDef[] | undefined, children: () => unknown) {
  const value: ProjectContextValue = {
    projectId: () => 'p1',
    project: (() => ({ id: 'p1', item_types: types() }) as unknown as Project) as unknown as Resource<Project>,
    workflow: () => undefined,
    vocabulary: () => ({}),
    refetch: () => {},
  };
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <ProjectContext.Provider value={value}>{children() as never}</ProjectContext.Provider>, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

function Names() {
  const types = useItemTypes();
  return (
    <span data-testid="out">
      {types.name('epic')}|{types.name('spike')}|{types.color('epic')}|{types.color('spike')}|{types.defaultKey()}
    </span>
  );
}

describe('useItemTypes', () => {
  it('names a key as the catalog does and reads a key it lacks as its capitalised self, neutral', () => {
    const c = renderWith(() => [BUILDING, WORK_ORDER], () => <Names />);
    expect(c.querySelector('[data-testid="out"]')!.textContent).toBe('Building|Spike|accent|neutral|task');
  });

  it('follows the project when its catalog changes', () => {
    const [types, setTypes] = createSignal<ItemTypeDef[]>([BUILDING, WORK_ORDER]);
    const c = renderWith(types, () => <Names />);
    setTypes([{ ...BUILDING, name: 'Saga' }, WORK_ORDER]);
    expect(c.querySelector('[data-testid="out"]')!.textContent).toBe('Saga|Spike|accent|neutral|task');
  });

  it('works outside a project: every key reads as its capitalised self', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(() => <Names />, container);
    expect(container.textContent).toBe('Epic|Spike|neutral|neutral|task');
    dispose();
    expect(capitalised('bug')).toBe('Bug');
  });
});
