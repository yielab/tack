import { test, expect } from '@playwright/test';
import { API_ORIGIN as ORIGIN } from './helpers';

// API contract smoke tests. These hit the tack-api server directly (bypassing
// the Vite proxy) so they assert the real wire contract documented in
// docs/API-REFERENCE.md. They're browser-independent — run on chromium only.

test.skip(({ browserName }) => browserName !== 'chromium', 'API contract runs once');

test('GET /api/health returns ok + version + migration count', async ({ request }) => {
  const res = await request.get(`${ORIGIN}/api/health`);
  expect(res.status()).toBe(200);
  const body = await res.json();
  expect(body.status).toBe('ok');
  expect(typeof body.version).toBe('string');
  expect(typeof body.migrations_applied).toBe('number');
  expect(body.migrations_applied).toBeGreaterThan(0);
});

test('responses carry the hardening headers', async ({ request }) => {
  const res = await request.get(`${ORIGIN}/api/health`);
  const h = res.headers();
  expect(h['x-content-type-options']).toBe('nosniff');
  expect(h['x-frame-options']).toBe('DENY');
  expect(h['referrer-policy']).toBe('same-origin');
});

test('GET /api/projects returns a bare array (no envelope)', async ({ request }) => {
  const res = await request.get(`${ORIGIN}/api/projects`);
  expect(res.status()).toBe(200);
  expect(Array.isArray(await res.json())).toBe(true);
});

test('GET /api/items/:id returns the { item, roles, dependencies } envelope', async ({
  request,
}) => {
  const projects = await request.get(`${ORIGIN}/api/projects`).then((r) => r.json());
  let projectId = projects[0]?.id;
  if (!projectId) {
    projectId = (
      await request
        .post(`${ORIGIN}/api/projects`, { data: { name: 'API test', project_type: 'software' } })
        .then((r) => r.json())
    ).id;
  }

  let items = await request
    .get(`${ORIGIN}/api/projects/${projectId}/items`)
    .then((r) => r.json());
  if (!items.length) {
    await request.post(`${ORIGIN}/api/projects/${projectId}/items`, {
      data: { title: 'API test item', item_type: 'task' },
    });
    items = await request.get(`${ORIGIN}/api/projects/${projectId}/items`).then((r) => r.json());
  }

  const res = await request.get(`${ORIGIN}/api/items/${items[0].id}`);
  expect(res.status()).toBe(200);
  const body = await res.json();
  expect(body).toHaveProperty('item');
  expect(body).toHaveProperty('roles');
  expect(body).toHaveProperty('dependencies');
  expect(body.item.id).toBe(items[0].id);
});

test('unknown API route returns 404', async ({ request }) => {
  const res = await request.get(`${ORIGIN}/api/does-not-exist`);
  expect(res.status()).toBe(404);
});

test('JSON export endpoint succeeds', async ({ request }) => {
  const projects = await request.get(`${ORIGIN}/api/projects`).then((r) => r.json());
  test.skip(!projects.length, 'no project to export');
  const res = await request.get(`${ORIGIN}/api/projects/${projects[0].id}/export?format=json`);
  expect(res.ok()).toBeTruthy();
});
