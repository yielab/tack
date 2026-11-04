import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import ImportFromFile from './ImportFromFile';
import { flush } from '../../test/flush';

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

const preview = (missing: unknown[]) => ({
  dry_run: true, created: 1, skipped: 0, dependencies: 0, warnings: [], items: [{ title: 'Answer customers', status: 'To Do' }],
  sections: [{ section: 'profiles', created: 1, updated: 0, not_mentioned: 0, warnings: [] }], project: null, missing,
});

async function choose(result: unknown) {
  vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify(result), { status: 200 }));
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(
    () => (
      <MemoryRouter>
        <Route path="/" component={() => <ImportFromFile target={{ kind: 'new' }}>From a file</ImportFromFile>} />
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
});
