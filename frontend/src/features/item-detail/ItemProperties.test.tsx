import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { Resource } from 'solid-js';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import type { Item } from '../../shared/types';
import ItemProperties from './ItemProperties';
import type { FolderMarks } from './useFolderMarks';

const noFolder: FolderMarks = { revision: () => null, say: () => null };
const atRevision = (n: number): FolderMarks => ({ revision: () => n, say: () => ({ kind: 'held' }) });

const projectValue: ProjectContextValue = {
  projectId: () => 'p1',
  project: (() => null) as unknown as Resource<null>,
  workflow: () => ({
    workflow_type: 'kanban',
    statuses: [{ name: 'todo', category: 'todo', order: 0 }],
  }),
  vocabulary: () => ({}),
  refetch: () => {},
};

const item = {
  id: 'item-1',
  project_id: 'p1',
  title: 'Pay rent',
  item_type: 'task',
  status: 'todo',
  priority: 'medium',
  estimate_unit: 'story_points',
  tags: [],
  sort_order: 0,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
} as Item;

beforeEach(() => {
  vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
    Promise.resolve(new Response('[]', { status: 200, headers: { 'Content-Type': 'application/json' } })),
  );
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

function mount(onPatch: () => void, overrides: Partial<Item> = {}, showStatus = false, marks: FolderMarks = noFolder) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ProjectContext.Provider value={projectValue}>
        <ItemProperties item={{ ...item, ...overrides }} onPatch={onPatch} marks={marks} showStatus={showStatus} />
      </ProjectContext.Provider>
    ),
    container,
  );
  return { container, dispose };
}

const labelled = (container: HTMLElement, tag: string, label: string) =>
  [...container.querySelectorAll<HTMLSelectElement | HTMLInputElement>(tag)].find(
    (c) => c.labels?.[0]?.textContent?.includes(label),
  )!;

describe('ItemProperties', () => {
  it('lists every property', () => {
    const { container, dispose } = mount(vi.fn());
    const text = container.textContent ?? '';
    for (const label of ['Priority', 'Due date', 'Assignee', 'Estimate', 'Repeats', 'Last update', 'Tags']) {
      expect(text).toContain(label);
    }
    dispose();
  });

  it('the_assignee_field_patches_assignee_and_clears_it_with_null', () => {
    const onPatch = vi.fn();
    const { container, dispose } = mount(onPatch, { assignee: 'Ana' });
    const input = labelled(container, 'input', 'Assignee') as HTMLInputElement;
    expect(input.value).toBe('Ana');
    input.value = 'Luis';
    input.dispatchEvent(new Event('change', { bubbles: true }));
    expect(onPatch).toHaveBeenLastCalledWith({ assignee: 'Luis' });
    input.value = '';
    input.dispatchEvent(new Event('change', { bubbles: true }));
    expect(onPatch).toHaveBeenLastCalledWith({ assignee: null });
    dispose();
  });

  it('the_repeats_select_patches_recurrence', () => {
    const onPatch = vi.fn();
    const { container, dispose } = mount(onPatch, { recurrence: 'weekly' });
    const select = labelled(container, 'select', 'Repeats') as HTMLSelectElement;
    expect(select.value).toBe('weekly');
    select.value = 'monthly';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    expect(onPatch).toHaveBeenLastCalledWith({ recurrence: 'monthly' });
    select.value = '';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    expect(onPatch).toHaveBeenLastCalledWith({ recurrence: null });
    dispose();
  });

  it('the properties are a list of label and value, each label tied to its control, in the order a person reads them', () => {
    const { container, dispose } = mount(vi.fn(), { assignee: 'Ana' });
    const list = container.querySelector('section[aria-label="Properties"] dl')!;
    expect(list).not.toBeNull();
    const terms = [...list.querySelectorAll('dt')].map((t) => t.textContent?.trim());
    expect(terms).toEqual(['Priority', 'Assignee', 'Sprint', 'Due date', 'Tags', 'Estimate (story points)', 'Repeats', 'Last update']);
    expect(labelled(container, 'input', 'Assignee').closest('dd')).not.toBeNull();
    // The Automation line is the next-step panel's, not a property.
    expect(container.querySelector('[data-testid="automation-row"]')).toBeNull();
    dispose();
  });

  it('the page lists the status first, as the one control that changes it; the drawer shows it under the title instead', () => {
    const onPatch = vi.fn();
    const page = mount(onPatch, {}, true);
    expect([...page.container.querySelectorAll('dt')].map((t) => t.textContent?.trim())[0]).toBe('Status');
    expect(page.container.querySelector('button[aria-label$="Change status"]')).not.toBeNull();
    page.dispose();
    const drawer = mount(onPatch, {}, false);
    expect(drawer.container.querySelector('button[aria-label$="Change status"]')).toBeNull();
    drawer.dispose();
  });

  it("says \"The board's\" under the fields the board owns, and only in a project with a folder", () => {
    const withFolder = mount(vi.fn(), {}, true, atRevision(7));
    const owned = [...withFolder.container.querySelectorAll('dt')]
      .filter((t) => t.querySelector('[data-testid="board-mark"]'))
      .map((t) => t.firstChild?.textContent?.trim());
    expect(owned).toEqual(['Status', 'Priority', 'Assignee', 'Sprint', 'Due date']);
    expect(withFolder.container.querySelector('[data-testid="board-mark"]')!.textContent).toBe("The board's");
    withFolder.dispose();
    const without = mount(vi.fn(), {}, true);
    expect(without.container.querySelector('[data-testid="board-mark"]')).toBeNull();
    without.dispose();
  });
});
