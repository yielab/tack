// k6 load test — establishes the tack-api performance baseline.
//
//   k6 run tests/load/smoke.js
//   BASE_URL=http://localhost:3210 TOKEN=secret k6 run tests/load/smoke.js
//
// See tests/load/README.md for install + interpretation.

import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Trend } from 'k6/metrics';

const BASE = __ENV.BASE_URL || 'http://localhost:3210';
const TOKEN = __ENV.TOKEN || '';
const headers = TOKEN ? { Authorization: `Bearer ${TOKEN}` } : {};

const writes = new Trend('writes', true);

export const options = {
  scenarios: {
    reads: {
      executor: 'ramping-vus',
      startVUs: 0,
      stages: [
        { duration: '15s', target: 50 }, // ramp up
        { duration: '30s', target: 50 }, // sustain
        { duration: '10s', target: 0 }, // ramp down
      ],
    },
  },
  thresholds: {
    http_req_failed: ['rate<0.01'], // < 1% errors
    http_req_duration: ['p(95)<500'], // reads stay interactive
    writes: ['p(95)<1000'], // SQLite single-writer ceiling
  },
};

// Resolve a project id once per VU and reuse it.
function pickProject() {
  const res = http.get(`${BASE}/api/projects`, { headers });
  if (res.status !== 200) return null;
  const list = res.json();
  return Array.isArray(list) && list.length ? list[0].id : null;
}

export default function () {
  const projectId = pickProject();

  group('read hot path', () => {
    const projects = http.get(`${BASE}/api/projects`, { headers });
    check(projects, { 'projects 200': (r) => r.status === 200 });

    if (projectId) {
      const items = http.get(`${BASE}/api/projects/${projectId}/items`, { headers });
      check(items, { 'items 200': (r) => r.status === 200 });
    }
  });

  group('write path', () => {
    if (!projectId) return;
    const items = http.get(`${BASE}/api/projects/${projectId}/items`, { headers }).json();
    if (Array.isArray(items) && items.length) {
      const id = items[0].id;
      const res = http.patch(
        `${BASE}/api/items/${id}`,
        JSON.stringify({ description: `load ${Date.now()}` }),
        { headers: { ...headers, 'Content-Type': 'application/json' } },
      );
      writes.add(res.timings.duration);
      check(res, { 'patch ok': (r) => r.status === 200 });
    }
  });

  sleep(1);
}
