import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import ImportFromFile, { type ImportTarget } from './ImportFromFile';
import { flush } from '../../test/flush';

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

const preview = (missing: unknown[]) => ({
  dry_run: true, created: 1, skipped: 0, dependencies: 0, warnings: [], items: [{ title: 'Answer customers', status: 'To Do' }],
  sections: [{ section: 'profiles', created: 1, updated: 0, not_mentioned: 0, warnings: [] }], project: null, missing,
});

async function choose(result: unknown, target: ImportTarget = { kind: 'new' }, answer?: (url: string, body: unknown) => Response | undefined) {
  vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
    const url = String(input);
    const answered = answer?.(url, url.endsWith('/teams/apply') ? JSON.parse(String(init?.body)) : undefined);
    return answered ?? new Response(JSON.stringify(result), { status: 200 });
  });
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(
    () => (
      <MemoryRouter>
        <Route path="/" component={() => <ImportFromFile target={target}>From a file</ImportFromFile>} />
      </MemoryRouter>
    ),
    container,
  );
  await flush();
  const input = container.querySelector<HTMLInputElement>('input[type="file"]')!;
  Object.defineProperty(input, 'files', { value: [new File(['name: x'], 'bundle.yaml')], configurable: true });
  input.dispatchEvent(new Event('change', { bubbles: true }));
  await vi.waitFor(() => expect(document.body.textContent).toContain('will be added'));
}

describe('ImportFromFile preview', () => {
  it('lists what the file expects that no computer reports, each with a fix to copy, under the sections', async () => {
    const written: string[] = [];
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: (t: string) => { written.push(t); return Promise.resolve(); } }, configurable: true });
    await choose(preview([
      { kind: 'team', name: 'fab-research', shape: 'respond', fix: 'docket init fab-research --blueprint respond --recipe research-review' },
      { kind: 'harness', name: 'codex', fix: 'No computer reports codex. Install it on a computer that runs a runner, then run `tack runner doctor` there to check it.' },
    ]));
    const list = document.querySelector('[data-testid="import-missing"]')!;
    expect(list.textContent).toContain('Nothing is created from it');
    const entries = Array.from(list.querySelectorAll('[data-testid="import-missing-entry"]'));
    expect(entries.map((e) => e.getAttribute('data-kind'))).toEqual(['team', 'harness']);
    expect(entries[0].textContent).toContain('Team fab-research · respond');
    expect(entries[0].querySelector('[data-testid="copy-fix-text"]')!.textContent).toBe('docket init fab-research --blueprint respond --recipe research-review');
    entries[0].querySelector<HTMLButtonElement>('button')!.click();
    await flush();
    expect(written).toEqual(['docket init fab-research --blueprint respond --recipe research-review']);
    // The list stands between the sections and the items.
    const sections = document.body.textContent!.indexOf('Profiles: 1 new');
    expect(sections).toBeLessThan(document.body.textContent!.indexOf('Nothing is created from it'));
    expect(document.body.textContent!.indexOf('Nothing is created from it')).toBeLessThan(document.body.textContent!.indexOf('1 item will be added'));
  });

  it('says nothing about computers when the file expects nothing the computers lack', async () => {
    await choose(preview([]));
    expect(document.querySelector('[data-testid="import-missing"]')).toBeNull();
  });

  it('offers Create beside a missing team: disabled before the project exists, and sending the file\'s shape in an existing one', async () => {
    const missing = [{ kind: 'team', name: 'fab-ops', shape: 'operate', fix: 'docket init fab-ops --blueprint operate' }];
    await choose(preview(missing));
    const before = document.querySelector<HTMLButtonElement>('button[aria-label="Create the team fab-ops"]')!;
    expect(before.disabled).toBe(true);
    expect(document.body.textContent).toContain('create the project from this file first');
    document.body.innerHTML = '';

    vi.restoreAllMocks();
    const sent: unknown[] = [];
    const runners = { protocol_version: 1, data: [{
      runner_id: 'r1', name: 'This computer', state: 'active', online: true, labels: null, labels_raw: '{}', total_capacity: 1, available_capacity: 1,
      capability_snapshot: { harnesses: [{ harness_kind: 'docket', installed_version: '', api_version: '6', probe_error: null, probed_at: '', api_features: ['podShape'] }], concurrency: { max_concurrent: 1 }, limits: {} },
      capability_snapshot_raw: '', protocol_version: 1, runner_version: '1', last_heartbeat_at: null, revoked_at: null, fleet_ids: [], created_at: '', updated_at: '',
    }] };
    await choose(preview(missing), { kind: 'project', id: '22222222-2222-2222-2222-222222222222' }, (url, body) => {
      if (url.endsWith('/api/runners')) return new Response(JSON.stringify(runners), { status: 200 });
      if (url.endsWith('/teams/apply')) {
        sent.push(body);
        return new Response(JSON.stringify([{ team: 'fab-ops', state: 'applied' }]), { status: 200 });
      }
      return undefined;
    });
    await flush();
    await flush();
    const after = document.querySelector<HTMLButtonElement>('button[aria-label="Create the team fab-ops"]')!;
    expect(after.disabled).toBe(false);
    expect(document.querySelector('select[aria-label="Shape of fab-ops"]')).toBeNull();
    after.click();
    await flush();
    await flush();
    expect(sent).toEqual([{ teams: [{ name: 'fab-ops', shape: 'operate' }] }]);
    expect(document.querySelector('[data-testid="team-apply-notes"]')!.textContent).toContain('fab-ops is made');
  });
});
