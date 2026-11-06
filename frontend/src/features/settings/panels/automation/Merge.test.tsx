import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { Resource } from 'solid-js';
import { ProjectContext, type ProjectContextValue } from '../../../../shared/state/projectContext';
import type { Project } from '../../../../shared/types';
import type { AutonomySettings } from '../../../../shared/api/autonomy';
import Merge from './Merge';

const TYPES = [
  { key: 'epic', name: 'Building', level: 'group', color: 'accent', order: 0, default_work: false },
  { key: 'task', name: 'Work Order', level: 'work', color: 'neutral', order: 1, default_work: true },
  { key: 'defect', name: 'Defect', level: 'work', color: 'danger', order: 2, default_work: false },
];

const settings = (types: string[]): AutonomySettings =>
  ({
    level: 'L4',
    domains: [{ name: 'docs', path_globs: ['docs/**'], item_types: types, level: 'L4', merged: 0, undone: 0 }],
    verify_require: [],
    attestation_public_key: null,
    verify_loop_max_attempts: 2,
    merge_on_done: false,
    automatic_merge_blocked_by: [],
  }) as unknown as AutonomySettings;

afterEach(() => {
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

function mount(current: AutonomySettings) {
  const save = vi.fn(async () => {});
  const ctx: ProjectContextValue = {
    projectId: () => 'p1',
    project: (() => ({ id: 'p1', item_types: TYPES })) as unknown as Resource<Project>,
    workflow: () => undefined,
    vocabulary: () => ({}),
    refetch: () => {},
  };
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(
    () => (
      <ProjectContext.Provider value={ctx}>
        <Merge settings={() => current} loadError={() => undefined} onRetry={() => {}} save={save} />
      </ProjectContext.Provider>
    ),
    container,
  );
  return { container, save };
}

const boxes = (c: ParentNode) => [...c.querySelectorAll<HTMLInputElement>('[data-testid="domain-types"] input[type="checkbox"]')];

describe('Merge: the domains picker', () => {
  it('lists_the_projects_own_types_with_the_chosen_ones_checked', () => {
    const { container } = mount(settings(['task']));
    const picker = container.querySelector('[data-testid="domain-types"]')!;
    expect([...picker.querySelectorAll('[data-testid="type-tile"]')].map((t) => t.textContent)).toEqual(['Building', 'Work Order', 'Defect']);
    expect(boxes(container).map((b) => b.checked)).toEqual([false, true, false]);
    expect(picker.textContent).toContain('Only the types checked');
  });

  it('saves_the_keys_of_the_types_checked_and_unchecked', () => {
    const { container, save } = mount(settings(['task']));
    const defect = boxes(container)[2];
    defect.checked = true;
    defect.dispatchEvent(new Event('change', { bubbles: true }));
    expect(save).toHaveBeenLastCalledWith({ domains: [expect.objectContaining({ item_types: ['task', 'defect'] })] });
    const task = boxes(container)[1];
    task.checked = false;
    task.dispatchEvent(new Event('change', { bubbles: true }));
    expect(save).toHaveBeenLastCalledWith({ domains: [expect.objectContaining({ item_types: [] })] });
  });

  it('says_any_type_when_none_is_chosen_and_keeps_a_removed_type_listed_so_it_can_be_unchecked', () => {
    const none = mount(settings([]));
    expect(none.container.querySelector('[data-testid="domain-types"]')!.textContent).toContain('Any type');
    document.body.innerHTML = '';
    const stale = mount(settings(['retired']));
    const picker = stale.container.querySelector('[data-testid="domain-types"]')!;
    expect(picker.textContent).toContain('retired (no longer a type)');
    const gone = boxes(stale.container).at(-1)!;
    gone.checked = false;
    gone.dispatchEvent(new Event('change', { bubbles: true }));
    expect(stale.save).toHaveBeenLastCalledWith({ domains: [expect.objectContaining({ item_types: [] })] });
  });
});
