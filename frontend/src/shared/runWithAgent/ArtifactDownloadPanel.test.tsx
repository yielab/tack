import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import ArtifactDownloadPanel from './ArtifactDownloadPanel';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <ArtifactDownloadPanel requestId="exec_1" attemptNumber={2} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

function jsonError(status: number, code: string, message: string) {
  return new Response(JSON.stringify({ error: { status, message, code } }), { status });
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

function fillArtifactId(c: HTMLElement, id: string) {
  const input = c.querySelector('input') as HTMLInputElement;
  input.value = id;
  input.dispatchEvent(new Event('input', { bubbles: true }));
}

describe('ArtifactDownloadPanel — disabled control names its reason', () => {
  it('the Download button starts disabled with a visible reason until an artifact id is entered', () => {
    const c = mount();
    const btn = c.querySelector('button') as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
    expect(c.textContent).toContain('Enter an artifact id to enable download.');

    fillArtifactId(c, 'art_1');
    expect(btn.disabled).toBe(false);
  });
});

describe('ArtifactDownloadPanel — every outcome is a distinct, visible state (acceptance bar: "artifact failure visible")', () => {
  it('calls GET /executions/{id}/attempts/{n}/artifacts/{artifact_id}/content and shows "Downloaded." on success', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response('artifact bytes', { status: 200, headers: { 'Content-Type': 'application/octet-stream' } }),
    );
    const c = mount();
    fillArtifactId(c, 'art_1');
    const btn = c.querySelector('button') as HTMLButtonElement;
    btn.click();
    await flush();
    await flush();

    expect(String(fetchMock.mock.calls[0][0])).toBe('/api/executions/exec_1/attempts/2/artifacts/art_1/content');
    expect(c.textContent).toContain('Downloaded.');
  });

  it('a 404 shows "No artifact with that id exists" — distinct from the 409 message', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonError(404, 'not_found', 'Artifact not found'));
    const c = mount();
    fillArtifactId(c, 'missing');
    (c.querySelector('button') as HTMLButtonElement).click();
    await flush();
    await flush();

    expect(c.textContent).toContain('No artifact with that id exists for this attempt.');
    expect(c.textContent).not.toContain("hasn't been verified yet");
  });

  it('a 409 shows the "content has not been verified yet" state — distinct from 404, and framed as retryable', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonError(409, 'conflict', 'not verified'));
    const c = mount();
    fillArtifactId(c, 'art_1');
    (c.querySelector('button') as HTMLButtonElement).click();
    await flush();
    await flush();

    expect(c.textContent).toContain("hasn't been verified yet");
    expect(c.textContent).not.toContain('No artifact with that id exists');
  });

  it('a generic 500 shows a distinct error message rather than either the 404 or 409 text', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonError(500, 'internal_error', 'server exploded'));
    const c = mount();
    fillArtifactId(c, 'art_1');
    (c.querySelector('button') as HTMLButtonElement).click();
    await flush();
    await flush();

    expect(c.textContent).toContain('server exploded');
    expect(c.textContent).not.toContain('No artifact with that id exists');
    expect(c.textContent).not.toContain("hasn't been verified yet");
  });

  it('editing the artifact id after a failure clears the stale status', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonError(404, 'not_found', 'gone'));
    const c = mount();
    fillArtifactId(c, 'missing');
    (c.querySelector('button') as HTMLButtonElement).click();
    await flush();
    await flush();
    expect(c.textContent).toContain('No artifact with that id exists');

    fillArtifactId(c, 'something-else');
    expect(c.textContent).not.toContain('No artifact with that id exists');
  });
});

describe('ArtifactDownloadPanel — keyboard accessibility', () => {
  it('the artifact-id field and Download button are real, natively-focusable form controls with no negative tabindex', () => {
    const c = mount();
    const input = c.querySelector('input') as HTMLInputElement;
    const button = c.querySelector('button') as HTMLButtonElement;
    for (const el of [input, button]) {
      const tabindex = el.getAttribute('tabindex');
      if (tabindex !== null) expect(Number(tabindex)).toBeGreaterThanOrEqual(0);
    }
    input.focus();
    expect(document.activeElement).toBe(input);
  });
});
