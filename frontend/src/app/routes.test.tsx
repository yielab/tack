import { describe, it, expect } from 'vitest';
import { routes } from './routes';

const paths = () => routes.map((r) => r.path);

describe('routes', () => {
  it('has_the_five_work_views_under_one_layout', () => {
    const workRoutes = routes.find((r) => r.path === '/projects/:id')!.children as { path: string }[];
    expect(workRoutes.map((r) => r.path)).toEqual(['/board', '/orchestration', '/list', '/timeline', '/sprint']);
  });

  it('has_Needs_you_as_a_project_destination', () => {
    expect(paths()).toContain('/projects/:id/needs-you');
  });
});
