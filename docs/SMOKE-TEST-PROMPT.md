# Reusable Smoke Test Prompt

Copy-paste this prompt into any new project to run a full QA smoke test with screenshots and an HTML report.

---

## The Prompt

> **Run a comprehensive QA smoke test of this app, fix any bugs you find, and produce a self-contained HTML report with embedded screenshots.**
>
> Follow this exact workflow:
>
> ### 1. Start the app
> - Read CLAUDE.md (if present) to find the correct start commands and ports.
> - Start the backend/API server in the background, wait for it to be ready (curl the health endpoint or watch for the "listening" log line).
> - Start the frontend dev server in the background, wait for it to be ready.
> - Note the actual ports — dev servers often shift (e.g. 5173 → 5174) if another process is using the default.
>
> ### 2. Set up Playwright
> - Check if Playwright is available: `node -e "require('playwright')"`. If not, try the global path `/home/linuxbrew/.linuxbrew/lib/node_modules/playwright`. If still missing, install it: `npm install -g playwright && npx playwright install chromium`.
> - Use `chromium.launch({ headless: true })` with a `1440×900` viewport for desktop tests.
> - Add a `390×844` viewport pass for mobile.
>
> ### 3. Write the test script (`/tmp/<project>-full-qa.js`)
> Structure the script in labelled sections A–H (or as many as apply):
>
> | Section | What to cover |
> |---------|--------------|
> | A | App shell: page loads, nav bar, sidebar, logo, initial render |
> | B | All primary views/pages (navigate to each, screenshot) |
> | C | Create/edit/delete a record (the main entity in the app) |
> | D | Search and command palette (keyboard shortcuts) |
> | E | Settings pages, theme toggle, preferences |
> | F | Templates or presets (if the app has them) |
> | G | Responsive layout (mobile viewport), 404/not-found page, empty states |
> | H | API smoke tests (call key endpoints via `page.evaluate(() => fetch(...))`, check status codes and response shapes) |
>
> **Script conventions:**
> - Each test case: `{ tc: 'A-01', name: '...', status: 'PASS'|'FAIL'|'WARN'|'SKIP', note: '...', screenshot: { file: 'filename.png', label: '...' } }`
> - Write a `getOrCreateProject()` helper (or equivalent) that checks via API first, creates if empty. Check raw response shape — many APIs return a plain array, not `{ data: [...] }`.
> - Use `page.waitForLoadState('networkidle')` + a short `waitForTimeout(800)` after navigation for SPA hydration.
> - Wrap all selector checks with `.catch(() => false)` so a missing element doesn't crash the test.
> - Save screenshots to `/tmp/<project>-qa/` and write `results.json` there.
>
> **API response shape gotcha:** Before writing API helper functions, manually `fetch` one list endpoint and one detail endpoint to confirm whether the API wraps responses in `{ data: [...] }`, returns plain arrays/objects, or uses a detail envelope like `{ item: {...}, roles: [...] }`. Adjust helpers accordingly.
>
> ### 4. Run the test script
> ```bash
> node /tmp/<project>-full-qa.js 2>&1 | tee /tmp/<project>-qa-run.log
> ```
> Read the log and triage every FAIL and WARN before moving on.
>
> ### 5. Fix real bugs (not test selector issues)
> Distinguish between:
> - **Real app bugs** — wrong API response handling, blank pages, undefined values in the UI, missing routes. **Fix these in the source code.**
> - **Selector/detection issues** — the feature works but the test can't find it by that CSS class or text. Mark as WARN and note it; do not mark as FAIL.
>
> After fixing, run `npm run type-check` (or equivalent) to confirm no regressions.
>
> ### 6. Generate the HTML report (`/tmp/<project>-report-gen.js`)
> The generator should:
> - Read `results.json` and all PNG screenshots from `/tmp/<project>-qa/`
> - Embed images as `data:image/png;base64,...` (self-contained, no external deps)
> - Output to `<project-root>/qa-report.html`
> - Include: gradient header, 5-card scorecard (PASS / FAIL / WARN / SKIP / pass-rate %), section navigation pills, per-section tables with TC badge + test name + status pill + notes + thumbnail, lightbox on click, footer with date and total count
> - Group results by the section letter prefix (A, B, C…)
>
> ### 7. Deliver
> - State the final counts: `X PASS / Y FAIL / Z WARN / W SKIP`
> - List every real bug found and the fix applied (file + line)
> - List every WARN with a one-line explanation of why it's a selector issue, not an app bug
> - The report is at `qa-report.html` (self-contained, open in any browser)

---

## Reusable script skeletons

### Test script skeleton (`/tmp/<project>-full-qa.js`)

```js
const { chromium } = require('/home/linuxbrew/.linuxbrew/lib/node_modules/playwright');
// fallback: require('playwright')
const fs   = require('fs');
const path = require('path');

const BASE_URL = 'http://localhost:PORT';  // update per project
const OUT      = '/tmp/<project>-qa';
fs.mkdirSync(OUT, { recursive: true });

const results = [];

function tc(id, name, status, note, ssFile, ssLabel) {
  results.push({ tc: id, name, status, note: note || null,
    screenshot: ssFile ? { file: ssFile, label: ssLabel || ssFile } : null });
  const icon = { PASS: '✅', FAIL: '❌', WARN: '⚠️ ', SKIP: '⏭ ' }[status] || '?';
  console.log(`${icon} ${id.padEnd(6)} ${name}${note ? ' — ' + note : ''}`);
}

async function shot(page, filename, label) {
  const file = path.join(OUT, filename);
  await page.screenshot({ path: file, fullPage: true });
  return filename;
}

async function waitApp(page) {
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(800);
}

// Helper: ensure at least one record exists via API, return its id
async function getOrCreate(page) {
  const list = await page.evaluate(async () => {
    const r = await fetch('/api/RESOURCE');
    const j = await r.json();
    return Array.isArray(j) ? j : (j.data ?? []);
  });
  if (list.length) return list[0].id;
  const created = await page.evaluate(async () => {
    const r = await fetch('/api/RESOURCE', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ /* minimal payload */ }),
    });
    return r.ok ? await r.json() : null;
  });
  return created?.id ?? null;
}

(async () => {
  const browser = await chromium.launch({ headless: true });
  const ctx     = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page    = await ctx.newPage();

  // ── A: App shell ──────────────────────────────────────────────────────────
  await page.goto(BASE_URL, { waitUntil: 'networkidle' });
  await waitApp(page);
  const ss = await shot(page, 'A-01-home.png', 'Home page');
  tc('A-01', 'Home page loads', 'PASS', null, ss, 'Home page');

  // ... add more test cases following A-01 pattern ...

  // ── G: Responsive + 404 ───────────────────────────────────────────────────
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(BASE_URL, { waitUntil: 'networkidle' });
  await waitApp(page);
  const mss = await shot(page, 'G-01-mobile.png', 'Mobile 390×844');
  tc('G-01', 'Mobile viewport renders', 'PASS', null, mss, 'Mobile view');
  await page.setViewportSize({ width: 1440, height: 900 });

  await page.goto(`${BASE_URL}/this-path-does-not-exist`, { waitUntil: 'networkidle' });
  await waitApp(page);
  const nfss = await shot(page, 'G-02-404.png', '404 page');
  const has404 = await page.locator('text=/not found/i').first().isVisible().catch(() => false);
  tc('G-02', '404 page shows for unknown routes', has404 ? 'PASS' : 'FAIL', null, nfss, '404 page');

  // ── H: API smoke ──────────────────────────────────────────────────────────
  const health = await page.evaluate(async () => {
    const r = await fetch('/api/health');
    return { status: r.status, body: await r.json().catch(() => ({})) };
  });
  tc('H-01', 'GET /api/health → 200', health.status === 200 ? 'PASS' : 'FAIL',
    JSON.stringify(health.body), null, null);

  await browser.close();

  // Write results.json
  fs.writeFileSync(path.join(OUT, 'results.json'), JSON.stringify(results, null, 2));

  // Print summary
  const counts = results.reduce((a, r) => { a[r.status] = (a[r.status]||0)+1; return a; }, {});
  console.log(`\n── ${counts.PASS||0} PASS / ${counts.FAIL||0} FAIL / ${counts.WARN||0} WARN / ${counts.SKIP||0} SKIP ──`);
})();
```

### Report generator skeleton (`/tmp/<project>-report-gen.js`)

```js
const fs   = require('fs');
const path = require('path');

const OUT     = '/tmp/<project>-qa';
const DEST    = '/path/to/project/qa-report.html';  // update per project
const results = JSON.parse(fs.readFileSync(path.join(OUT, 'results.json'), 'utf8'));

const counts   = results.reduce((a, r) => { a[r.status]=(a[r.status]||0)+1; return a; }, {});
const total    = results.length;
const passRate = Math.round((counts.PASS / (total - (counts.SKIP||0))) * 100);
const runDate  = new Date().toLocaleString('en-US', { dateStyle: 'long', timeStyle: 'short' });

const C = { PASS:'#22c55e', FAIL:'#ef4444', WARN:'#f59e0b', SKIP:'#94a3b8' };
const BG= { PASS:'#f0fdf4', FAIL:'#fef2f2', WARN:'#fffbeb', SKIP:'#f8fafc' };
const IC= { PASS:'✅', FAIL:'❌', WARN:'⚠️', SKIP:'⏭' };

function b64(filename) {
  const p = path.join(OUT, filename);
  if (!filename || !fs.existsSync(p)) return null;
  return `data:image/png;base64,${fs.readFileSync(p).toString('base64')}`;
}

// Group by section letter
const sectionNames = {
  A:'App Shell & Navigation', B:'Primary Views', C:'Create / Edit / Delete',
  D:'Search & Command Palette', E:'Settings & Themes', F:'Templates',
  G:'Responsive & Edge Cases', H:'API Smoke Tests',
};
const sections = {};
for (const r of results) {
  const k = r.tc.match(/^([A-Z])/)?.[1] || 'X';
  (sections[k] = sections[k]||[]).push(r);
}

function renderSection(k, items) {
  const rows = items.map(r => {
    const img = r.screenshot ? b64(r.screenshot.file) : null;
    const thumb = img
      ? `<div><img src="${img}" style="width:200px;height:112px;object-fit:cover;object-position:top left;border-radius:6px;border:1px solid #e2e8f0;cursor:zoom-in" onclick="lb(this.src,'${(r.screenshot.label||'').replace(/'/g,"\\'")}')"><div style="font-size:.68rem;color:#94a3b8;max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${r.screenshot.label||''}</div></div>`
      : `<span style="color:#cbd5e1;font-size:.75rem">—</span>`;
    return `<tr>
      <td style="padding:.75rem 1rem"><span style="background:${BG[r.status]};color:${C[r.status]};border:1px solid ${C[r.status]}33;padding:.2rem .55rem;border-radius:6px;font-family:monospace;font-size:.72rem;font-weight:700">${r.tc}</span></td>
      <td style="padding:.75rem 1rem;font-weight:500;color:#334155">${r.name}</td>
      <td style="padding:.75rem 1rem"><span style="background:${BG[r.status]};color:${C[r.status]};padding:.25rem .65rem;border-radius:20px;font-size:.72rem;font-weight:700">${IC[r.status]} ${r.status}</span></td>
      <td style="padding:.75rem 1rem;font-size:.75rem;color:#64748b;font-family:monospace;word-break:break-all">${r.note||''}</td>
      <td style="padding:.75rem 1rem">${thumb}</td>
    </tr>`;
  }).join('');

  const p=items.filter(r=>r.status==='PASS').length, f=items.filter(r=>r.status==='FAIL').length,
        w=items.filter(r=>r.status==='WARN').length, s=items.filter(r=>r.status==='SKIP').length;

  return `<section id="s-${k}" style="background:#fff;border-radius:12px;box-shadow:0 1px 3px rgba(0,0,0,.08);margin-bottom:1.5rem;overflow:hidden">
    <div style="display:flex;align-items:center;justify-content:space-between;padding:1rem 1.25rem;border-bottom:1px solid #f1f5f9;background:#fafafa">
      <div style="display:flex;align-items:center;gap:.6rem">
        <span style="display:inline-flex;align-items:center;justify-content:center;width:2rem;height:2rem;background:#4f46e5;color:#fff;border-radius:6px;font-size:.8rem;font-weight:700">${k}</span>
        <span style="font-size:.95rem;font-weight:600">${sectionNames[k]||k}</span>
      </div>
      <div style="display:flex;gap:.4rem">
        ${p?`<span style="background:#f0fdf4;color:#16a34a;padding:.2rem .6rem;border-radius:20px;font-size:.7rem;font-weight:700">${p} PASS</span>`:''}
        ${f?`<span style="background:#fef2f2;color:#dc2626;padding:.2rem .6rem;border-radius:20px;font-size:.7rem;font-weight:700">${f} FAIL</span>`:''}
        ${w?`<span style="background:#fffbeb;color:#d97706;padding:.2rem .6rem;border-radius:20px;font-size:.7rem;font-weight:700">${w} WARN</span>`:''}
        ${s?`<span style="background:#f8fafc;color:#64748b;padding:.2rem .6rem;border-radius:20px;font-size:.7rem;font-weight:700">${s} SKIP</span>`:''}
      </div>
    </div>
    <table style="width:100%;border-collapse:collapse;font-size:.85rem">
      <thead><tr style="background:#fafafa">
        <th style="padding:.6rem 1rem;text-align:left;font-size:.7rem;font-weight:700;letter-spacing:.06em;text-transform:uppercase;color:#94a3b8;border-bottom:1px solid #f1f5f9;width:80px">TC</th>
        <th style="padding:.6rem 1rem;text-align:left;font-size:.7rem;font-weight:700;letter-spacing:.06em;text-transform:uppercase;color:#94a3b8;border-bottom:1px solid #f1f5f9">Test Case</th>
        <th style="padding:.6rem 1rem;text-align:left;font-size:.7rem;font-weight:700;letter-spacing:.06em;text-transform:uppercase;color:#94a3b8;border-bottom:1px solid #f1f5f9;width:110px">Result</th>
        <th style="padding:.6rem 1rem;text-align:left;font-size:.7rem;font-weight:700;letter-spacing:.06em;text-transform:uppercase;color:#94a3b8;border-bottom:1px solid #f1f5f9;width:280px">Notes</th>
        <th style="padding:.6rem 1rem;text-align:left;font-size:.7rem;font-weight:700;letter-spacing:.06em;text-transform:uppercase;color:#94a3b8;border-bottom:1px solid #f1f5f9;width:220px">Screenshot</th>
      </tr></thead>
      <tbody>${rows}</tbody>
    </table>
  </section>`;
}

const nav = Object.keys(sections).sort()
  .map(k=>`<a href="#s-${k}" style="background:#fff;border:1px solid #e2e8f0;border-radius:20px;padding:.3rem .9rem;font-size:.75rem;font-weight:600;color:#475569;text-decoration:none">${k} — ${sectionNames[k]||k}</a>`)
  .join('');

const body = Object.keys(sections).sort().map(k=>renderSection(k,sections[k])).join('\n');

const html = `<!DOCTYPE html><html lang="en"><head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>QA Smoke Test Report</title>
<style>*,*::before,*::after{box-sizing:border-box;margin:0;padding:0}body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;background:#f1f5f9;color:#1e293b;line-height:1.5}
#lb{display:none;position:fixed;inset:0;background:rgba(0,0,0,.85);z-index:9999;align-items:center;justify-content:center;flex-direction:column;gap:1rem;padding:2rem;cursor:zoom-out}
#lb.on{display:flex}#lb img{max-width:90vw;max-height:80vh;border-radius:8px}#lb p{color:#e2e8f0;font-size:.875rem}
#lbc{position:fixed;top:1rem;right:1rem;background:rgba(255,255,255,.15);color:#fff;border:none;border-radius:50%;width:2.5rem;height:2.5rem;font-size:1.25rem;cursor:pointer;display:flex;align-items:center;justify-content:center}</style>
</head><body>
<div style="background:linear-gradient(135deg,#4f46e5,#7c3aed);color:#fff;padding:2.5rem 2rem 2rem">
  <div style="max-width:1200px;margin:0 auto">
    <h1 style="font-size:1.75rem;font-weight:700;letter-spacing:-.02em">QA Smoke Test Report</h1>
    <p style="margin-top:.5rem;font-size:.875rem;opacity:.85">Generated: ${runDate}</p>
  </div>
</div>
<div style="max-width:1200px;margin:1.5rem auto;padding:0 1.5rem;display:grid;grid-template-columns:repeat(5,1fr);gap:1rem">
  ${['PASS','FAIL','WARN','SKIP'].map(s=>`<div style="background:#fff;border-radius:12px;padding:1.25rem 1rem;text-align:center;box-shadow:0 1px 3px rgba(0,0,0,.08)"><div style="font-size:2rem;font-weight:700;color:${C[s]}">${counts[s]||0}</div><div style="font-size:.75rem;font-weight:600;letter-spacing:.05em;text-transform:uppercase;opacity:.6;margin-top:.25rem">${s}</div></div>`).join('')}
  <div style="background:#fff;border-radius:12px;padding:1.25rem 1rem;text-align:center;box-shadow:0 1px 3px rgba(0,0,0,.08)"><div style="font-size:2rem;font-weight:700;color:#4f46e5">${passRate}%</div><div style="font-size:.75rem;font-weight:600;letter-spacing:.05em;text-transform:uppercase;opacity:.6;margin-top:.25rem">Pass Rate</div></div>
</div>
<div style="max-width:1200px;margin:0 auto 1rem;padding:0 1.5rem;display:flex;gap:.5rem;flex-wrap:wrap">${nav}</div>
<div style="max-width:1200px;margin:0 auto;padding:0 1.5rem 3rem">${body}</div>
<p style="text-align:center;padding:2rem;font-size:.75rem;color:#94a3b8">QA Report &bull; ${runDate} &bull; ${total} test cases</p>
<div id="lb" onclick="cl()"><button id="lbc" onclick="cl()">✕</button><img id="lbi" src="" alt=""><p id="lbp"></p></div>
<script>
function lb(src,cap){document.getElementById('lbi').src=src;document.getElementById('lbp').textContent=cap;document.getElementById('lb').classList.add('on')}
function cl(){document.getElementById('lb').classList.remove('on')}
document.addEventListener('keydown',e=>{if(e.key==='Escape')cl()})
</script></body></html>`;

fs.writeFileSync(DEST, html);
console.log(`Report → ${DEST}`);
```

---

## Key gotchas to tell Claude

Paste this block at the end of the prompt to prevent the most common mistakes:

```
Gotchas to watch for:
1. Dev server port may differ from default (e.g. 5173 → 5174). Check the startup log.
2. API responses may NOT use a {data:[...]} envelope — many return plain arrays or objects. Verify before writing helpers.
3. Detail endpoints (GET /resource/:id) sometimes wrap as {item:{...}, relations:[...]}. Check each endpoint individually.
4. Use .catch(()=>false) on every Playwright visibility check — missing elements must not abort the test.
5. After SPA navigation, always waitForLoadState('networkidle') + waitForTimeout(800) before asserting.
6. Distinguish real bugs (fix them) from selector failures (mark WARN, note reason). Do not mark a feature FAIL because a CSS class changed.
7. Run type-check (or equivalent lint/build) after every source code fix to catch regressions.
8. The report must be self-contained (base64 images). Do not reference /tmp paths in the HTML.
```
