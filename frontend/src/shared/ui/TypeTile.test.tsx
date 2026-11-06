import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { Resource } from 'solid-js';
import { ProjectContext, type ProjectContextValue } from '../state/projectContext';
import type { ItemTypeDef } from '../types';
import TypeTile from './TypeTile';

const CATALOG: ItemTypeDef[] = [
  { key: 'epic', name: 'Epic', color: 'accent2', level: 'group', default_work: false, order: 0 },
  { key: 'task', name: 'Task', color: 'accent', level: 'work', default_work: true, order: 1 },
];

const disposers: Array<() => void> = [];
afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
});

function mount(type: string, label?: string) {
  const ctx = { project: (() => ({ id: 'p1', item_types: CATALOG })) as unknown as Resource<null> } as ProjectContextValue;
  const container = document.createElement('div');
  document.body.appendChild(container);
  disposers.push(render(() => <ProjectContext.Provider value={ctx}><TypeTile type={type} label={label} /></ProjectContext.Provider>, container));
  return container.querySelector<HTMLElement>('[data-testid="type-tile"]')!;
}

describe('TypeTile', () => {
  it('draws_a_type_that_holds_work_stacked_and_work_flat_with_the_catalogs_name_and_colour', () => {
    const epic = mount('epic');
    expect(epic.textContent).toBe('Epic');
    expect(epic.getAttribute('data-holds-work')).toBe('true');
    expect(epic.style.boxShadow).not.toBe('');
    expect(epic.style.color).toContain('accent2-ink');
    const task = mount('task');
    expect(task.textContent).toBe('Task');
    expect(task.getAttribute('data-holds-work')).toBe('false');
    expect(task.style.boxShadow).toBe('');
    expect(task.style.color).toContain('accent-ink');
  });

  it('reads_a_key_the_catalog_lacks_as_its_capitalised_self_flat_and_neutral', () => {
    const tile = mount('story');
    expect(tile.textContent).toBe('Story');
    expect(tile.getAttribute('data-holds-work')).toBe('false');
    expect(tile.style.backgroundColor).toContain('chip');
  });

  it('lets_a_caller_override_the_name', () => {
    expect(mount('epic', 'Big thing').textContent).toBe('Big thing');
  });
});
