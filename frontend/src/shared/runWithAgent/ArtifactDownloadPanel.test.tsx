import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { ArtifactRecord } from '../execution';
import ArtifactDownloadPanel from './ArtifactDownloadPanel';
import { flush } from '../../test/flush';
const disposers: Array<() => void> = [];

const VERIFIED: ArtifactRecord = {
  artifact_id: 'art_1',
  kind: 'diff',
  name: 'patch.diff',
  media_type: 'text/plain',
  size_bytes: 1234,
  content_verified: true,
  created_at: '2026-08-06T12:00:00Z',
};

const UNVERIFIED: ArtifactRecord = {
  ...VERIFIED,
  artifact_id: 'art_2',
  name: 'log.txt',
  content_verified: false,
};

/** The files the panel is given: the screen around it lists them, so `mockFetch` only keeps them for `mount`. */
let listed: ArtifactRecord[] = [];

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <ArtifactDownloadPanel requestId="exec_1" attemptNumber={2} artifacts={listed} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

function jsonError(status: number, code: string, message: string) {
  return new Response(JSON.stringify({ error: { status, message, code } }), { status });
}

/** Answers a per-artifact content download (`.../artifacts/{id}/content`), the one GET this panel makes. */
function mockFetch(list: ArtifactRecord[], download: () => Response) {
  listed = list;
  return vi.spyOn(globalThis, 'fetch').mockImplementation(() => Promise.resolve(download()));
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

// The actions are icon buttons: their names are their labels.
const named = (c: HTMLElement, label: string) =>
  [...c.querySelectorAll('button')].find((b) => b.getAttribute('aria-label') === label) as HTMLButtonElement;
const downloadButton = (c: HTMLElement) => named(c, 'Download');

describe('ArtifactDownloadPanel — discovers artifacts, never asks for an id', () => {
  it('renders every listed artifact as one row of a table with View and Download, no id field anywhere', async () => {
    mockFetch([VERIFIED, UNVERIFIED], () => new Response('bytes'));
    const c = mount();
    await flush();
    await flush();

    expect(c.querySelector('input')).toBeNull();
    expect(c.textContent).toContain('patch.diff');
    expect(c.textContent).toContain('log.txt');
    expect([...c.querySelectorAll('button')].map((b) => b.getAttribute('aria-label'))).toEqual(['View', 'Download', 'View', 'Download']);
    // One compact table, not a card per file: the name heads its row, then kind and size.
    const rows = [...c.querySelectorAll('[data-testid="artifact-table"] tbody tr[data-testid="artifact-row"]')];
    expect(rows).toHaveLength(2);
    expect(rows[0].querySelector('th')?.textContent).toBe('patch.diff');
    expect(rows[0].textContent).toContain('diff');
    expect(rows[0].textContent).toContain('1.2 KB');
  });

  it('an unverified artifact is visibly marked distinct from a verified one', async () => {
    mockFetch([VERIFIED, UNVERIFIED], () => new Response('bytes'));
    const c = mount();
    await flush();
    await flush();

    expect(c.textContent).toContain('Not verified yet');
    const rows = [...c.querySelectorAll('tr')];
    const unverifiedRow = rows.find((tr) => tr.textContent?.includes('log.txt'));
    const verifiedRow = rows.find((tr) => tr.textContent?.includes('patch.diff'));
    expect(unverifiedRow?.textContent).toContain('Not verified yet');
    expect(verifiedRow?.textContent).not.toContain('Not verified yet');
  });

  it('an empty list renders a real empty state, not a silently blank panel', async () => {
    mockFetch([], () => new Response('bytes'));
    const c = mount();
    await flush();
    await flush();

    expect(c.textContent).toContain('No artifacts yet');
  });
});

describe('ArtifactDownloadPanel — every download outcome is a distinct, visible state (artifact failure stays visible)', () => {
  it('clicking Download calls GET .../artifacts/{artifact_id}/content for that exact row and says where it went', async () => {
    const fetchMock = mockFetch([VERIFIED], () => new Response('artifact bytes', { status: 200 }));
    const c = mount();
    await flush();
    await flush();

    downloadButton(c).click();
    await flush();
    await flush();

    const downloadCall = fetchMock.mock.calls.find((call) => String(call[0]).includes('/content'));
    expect(String(downloadCall?.[0])).toBe('/api/executions/exec_1/attempts/2/artifacts/art_1/content');
    expect(c.textContent).toContain("Sent to your browser's downloads as patch.diff.");
  });

  it('View shows the content in place, without downloading', async () => {
    mockFetch([VERIFIED], () => new Response('diff --git a/x b/x', { status: 200 }));
    const c = mount();
    await flush();
    await flush();
    named(c, 'View').click();
    await flush();
    await flush();
    expect(document.body.textContent).toContain('patch.diff');
    expect(c.textContent).not.toContain('downloads as');
  });

  it('a 404 shows "No artifact with that id exists" — distinct from the 409 message', async () => {
    mockFetch([VERIFIED], () => jsonError(404, 'not_found', 'Artifact not found'));
    const c = mount();
    await flush();
    await flush();

    downloadButton(c).click();
    await flush();
    await flush();

    expect(c.textContent).toContain('No artifact with that id exists for this attempt.');
    expect(c.textContent).not.toContain("hasn't been verified yet");
  });

  it('a 409 shows the "content has not been verified yet" state — distinct from 404, and framed as retryable', async () => {
    mockFetch([UNVERIFIED], () => jsonError(409, 'conflict', 'not verified'));
    const c = mount();
    await flush();
    await flush();

    downloadButton(c).click();
    await flush();
    await flush();

    expect(c.textContent).toContain("hasn't been verified yet");
    expect(c.textContent).not.toContain('No artifact with that id exists');
  });

  it('a generic 500 shows a distinct error message rather than either the 404 or 409 text', async () => {
    mockFetch([VERIFIED], () => jsonError(500, 'internal_error', 'server exploded'));
    const c = mount();
    await flush();
    await flush();

    downloadButton(c).click();
    await flush();
    await flush();

    expect(c.textContent).toContain('server exploded');
    expect(c.textContent).not.toContain('No artifact with that id exists');
    expect(c.textContent).not.toContain("hasn't been verified yet");
  });
});

describe('ArtifactDownloadPanel — keyboard accessibility', () => {
  it('every Download button is a real, natively-focusable form control with no negative tabindex', async () => {
    mockFetch([VERIFIED, UNVERIFIED], () => new Response('bytes'));
    const c = mount();
    await flush();
    await flush();

    const buttons = [...c.querySelectorAll('button')];
    expect(buttons.length).toBeGreaterThan(0);
    for (const btn of buttons) {
      const tabindex = btn.getAttribute('tabindex');
      if (tabindex !== null) expect(Number(tabindex)).toBeGreaterThanOrEqual(0);
    }
    buttons[0].focus();
    expect(document.activeElement).toBe(buttons[0]);
  });
});
