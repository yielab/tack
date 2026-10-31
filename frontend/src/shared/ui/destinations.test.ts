import { describe, it, expect } from 'vitest';
import { PROJECT_DESTINATIONS, WORKSPACE_DESTINATIONS } from './destinations';

const label = (list: typeof PROJECT_DESTINATIONS, id: string, t = (k: string) => k) =>
  list.find((d) => d.id === id)!.label(t);

describe('destinations shared by the sidebar and the command palette', () => {
  it('lists Agents among the workspace places, with its route', () => {
    const agents = WORKSPACE_DESTINATIONS.find((d) => d.id === 'agents');
    expect(agents?.label(() => '')).toBe('Agents');
    expect(agents?.path('any')).toBe('/agents');
  });

  it('builds a project route from the project id', () => {
    expect(PROJECT_DESTINATIONS.find((d) => d.id === 'board')!.path('p1')).toBe('/projects/p1/board');
    expect(PROJECT_DESTINATIONS.find((d) => d.id === 'project-settings')!.path('p1')).toBe('/projects/p1/settings');
  });

  it('names the sprint view from the project vocabulary', () => {
    expect(label(PROJECT_DESTINATIONS, 'sprint', (k) => (k === 'sprint' ? 'Cycle' : k))).toBe('Cycle');
  });

  it('keeps the all-projects page active only on an exact match', () => {
    expect(WORKSPACE_DESTINATIONS.find((d) => d.id === 'all-projects')!.end).toBe(true);
  });
});
