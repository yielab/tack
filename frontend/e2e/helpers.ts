import { type Page, type APIRequestContext, expect } from '@playwright/test';

// Shared helpers for E2E specs. The single source of truth for the app's API
// response shapes lives here so a backend contract change is fixed in one place.
//
// Response-shape notes (verified against crates/tack-api/src/handlers):
//   GET  /api/projects        -> Project[]            (plain array, no envelope)
//   POST /api/projects        -> Project              (the created object)
//   GET  /api/projects/:id/items -> { data: Item[], total, page, per_page }  (paginated envelope)
//   POST /api/projects/:id/items -> { id } | Item     (id is always present)
//   GET  /api/items/:id       -> { item, roles, dependencies }  (detail envelope)

// Test *setup* talks to the API server directly (deterministic), rather than
// through the Vite dev proxy. The browser `page` still uses the relative /api
// path so the proxy/same-origin behaviour is exercised by the real app.
export const API_ORIGIN = process.env.E2E_API_ORIGIN || 'http://127.0.0.1:3210';
export const API = `${API_ORIGIN}/api`;

/**
 * Wait for the SolidJS SPA to be ready after a navigation. We deliberately do
 * NOT use 'networkidle' — the app holds a persistent board WebSocket (with a
 * keepalive Ping), so the network never goes idle. 'domcontentloaded' plus the
 * web-first auto-waiting assertions in each test is the robust primitive.
 */
export async function waitForApp(page: Page): Promise<void> {
  await page.waitForLoadState('domcontentloaded');
}

/**
 * Ensure at least one project exists and return its id. Shape-agnostic: it
 * creates via POST when empty, then re-reads the list so it never depends on the
 * create response body.
 */
export async function getOrCreateProject(request: APIRequestContext): Promise<string> {
  const existing = await request.get(`${API}/projects`).then((r) => r.json());
  if (Array.isArray(existing) && existing.length) return existing[0].id;

  const res = await request.post(`${API}/projects`, {
    data: { name: 'E2E Project', project_type: 'software', description: 'created by e2e' },
  });
  expect(res.ok(), `create project failed: ${res.status()}`).toBeTruthy();

  const list = await request.get(`${API}/projects`).then((r) => r.json());
  expect(Array.isArray(list) && list.length, 'project list empty after create').toBeTruthy();
  return list[0].id;
}

/** Ensure the given project has at least one item and return its id. */
export async function getOrCreateItem(
  request: APIRequestContext,
  projectId: string,
  title = 'E2E Item',
): Promise<string> {
  const existing = await request
    .get(`${API}/projects/${projectId}/items`)
    .then((r) => r.json())
    .then((p) => p.data ?? []);
  if (existing.length) return existing[0].id;

  const res = await request.post(`${API}/projects/${projectId}/items`, {
    data: { title, item_type: 'task' },
  });
  expect(res.ok(), `create item failed: ${res.status()}`).toBeTruthy();
  const body = await res.json();
  if (body?.id) return body.id;

  const list = await request
    .get(`${API}/projects/${projectId}/items`)
    .then((r) => r.json())
    .then((p) => p.data ?? []);
  return list[0].id;
}

/**
 * Always create a fresh item carrying an `assignee`, so callers get a
 * populated `<Avatar>` (Board.tsx only renders one when `item.assignee` is
 * set) — unlike `getOrCreateItem`, this doesn't reuse an existing assignee-less
 * item. Returns the item id.
 */
export async function createItemWithAssignee(
  request: APIRequestContext,
  projectId: string,
  assignee: string,
  title = 'E2E Item (assigned)',
): Promise<string> {
  const res = await request.post(`${API}/projects/${projectId}/items`, {
    data: { title, item_type: 'task', assignee },
  });
  expect(res.ok(), `create item failed: ${res.status()}`).toBeTruthy();
  const body = await res.json();
  if (body?.id) return body.id;

  const list = await request
    .get(`${API}/projects/${projectId}/items`)
    .then((r) => r.json())
    .then((p) => p.data ?? []);
  const match = list.find((it: { assignee?: string }) => it.assignee === assignee);
  return (match ?? list[list.length - 1]).id;
}

/**
 * Create a fresh sprint with one item assigned to it — the minimum the
 * Sprints view's "Run sprint" dispatch control needs to render at all
 * (`Sprints.tsx` only shows the button for a sprint with `itemsForSprint(id).
 * length > 0`, TODO.md Wave 3, card C4). Returns both ids since the caller
 * typically needs the sprint id (to mock its dry-run route) and doesn't
 * otherwise have one.
 */
export async function createSprintWithItem(
  request: APIRequestContext,
  projectId: string,
  sprintName = 'E2E Sprint',
): Promise<{ sprintId: string; itemId: string; sprintName: string }> {
  // The caller's project is shared (`getOrCreateProject` reuses one per spec
  // file) and `e2e.db` survives between runs, so a fixed name accumulates a
  // fresh identically-named sprint on every run. Card F1 gave each button an
  // accessible name including its sprint's name, which disambiguates two
  // *differently* named sprints — but not six sprints that all share one name,
  // which is what repeated runs actually produce. Suffixing here makes the name
  // unique per invocation, so the accessible name is unique too and a
  // `getByRole` locator resolves to exactly one button.
  const uniqueName = `${sprintName} ${Date.now().toString(36)}-${Math.random()
    .toString(36)
    .slice(2, 7)}`;
  const sprintRes = await request.post(`${API}/projects/${projectId}/sprints`, {
    data: { name: uniqueName },
  });
  expect(sprintRes.ok(), `create sprint failed: ${sprintRes.status()}`).toBeTruthy();
  const sprint = await sprintRes.json();

  const itemRes = await request.post(`${API}/projects/${projectId}/items`, {
    data: { title: 'E2E Sprint Item', item_type: 'task' },
  });
  expect(itemRes.ok(), `create item failed: ${itemRes.status()}`).toBeTruthy();
  const itemBody = await itemRes.json();
  const itemId: string =
    itemBody?.id ??
    (await request
      .get(`${API}/projects/${projectId}/items`)
      .then((r) => r.json())
      .then((p) => p.data.at(-1).id));

  const patchRes = await request.patch(`${API}/items/${itemId}`, {
    data: { sprint_id: sprint.id },
  });
  expect(patchRes.ok(), `assign item to sprint failed: ${patchRes.status()}`).toBeTruthy();

  return { sprintId: sprint.id, itemId, sprintName: uniqueName };
}
