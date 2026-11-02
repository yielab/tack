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

  it('lists the work views Board, Orchestration, List, Calendar, Timeline and Sprint, and no Table', () => {
    const ids = PROJECT_DESTINATIONS.map((d) => d.id);
    expect(ids.slice(0, 6)).toEqual(['board', 'orchestration', 'list', 'calendar', 'timeline', 'sprint']);
    expect(ids).not.toContain('table');
    expect(PROJECT_DESTINATIONS.map((d) => d.label((k) => k))).not.toContain('Table');
    expect(PROJECT_DESTINATIONS.map((d) => d.path('p1'))).not.toContain('/projects/p1/table');
  });

  it('names the sprint view from the project vocabulary', () => {
    expect(label(PROJECT_DESTINATIONS, 'sprint', (k) => (k === 'sprint' ? 'Cycle' : k))).toBe('Cycle');
  });

  it('names the Orchestration view from the vocabulary, Orchestration by default', () => {
    expect(label(PROJECT_DESTINATIONS, 'orchestration', (k) => (k === 'orchestration' ? 'Fleet' : k))).toBe('Fleet');
    expect(PROJECT_DESTINATIONS.find((d) => d.id === 'orchestration')!.path('p1')).toBe('/projects/p1/orchestration');
  });

  it('lists Threads among the project views, with its route', () => {
    const threads = PROJECT_DESTINATIONS.find((d) => d.id === 'threads');
    expect(threads?.label(() => '')).toBe('Threads');
    expect(threads?.path('p1')).toBe('/projects/p1/threads');
  });

  it('keeps the all-projects page active only on an exact match', () => {
    expect(WORKSPACE_DESTINATIONS.find((d) => d.id === 'all-projects')!.end).toBe(true);
  });
});
