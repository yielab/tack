import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import MergeButton from './MergeButton';
import MergeNoteBadge from './MergeNoteBadge';
import { toast } from '../ui/toast';
import { flush } from '../../test/flush';
const disposers: Array<() => void> = [];
const mountIn = (ui: () => unknown) => {
  const c = document.createElement('div');
  document.body.appendChild(c);
  const d = render(ui as () => never, c);
  disposers.push(() => { d(); c.remove(); });
  return c;
};
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
});

describe('MergeButton', () => {
  it('the_merge_button_is_enabled_and_posts_the_merge', async () => {
    const spy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(json({ merged: true, merge_sha: 'abc' }));
    const done = vi.fn();
    const el = mountIn(() => <MergeButton attemptId="att_1" merge={{ enabled: true }} onMerged={done} />);
    const b = el.querySelector('button')!;
    expect(b.textContent).toBe('Merge');
    expect(b.disabled).toBe(false);
    b.click();
    await flush();
    expect(String(spy.mock.calls[0][0])).toBe('/api/attempts/att_1/merge');
    expect((spy.mock.calls[0][1] as RequestInit).method).toBe('POST');
    expect(done).toHaveBeenCalled();
  });

  it('the_merge_button_is_disabled_with_its_reason', async () => {
    const spy = vi.spyOn(globalThis, 'fetch');
    const el = mountIn(() => <MergeButton attemptId="att_1" merge={{ enabled: false, reason: 'No branch was pushed' }} />);
    const b = el.querySelector('button')!;
    expect(b.disabled).toBe(true);
    expect(el.textContent).toContain('No branch was pushed');
    b.click();
    await flush();
    expect(spy).not.toHaveBeenCalled();
  });

  it('a_refused_merge_shows_the_409_reason', async () => {
    const err = vi.spyOn(toast, 'error').mockImplementation(() => undefined as never);
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(json({ error: { status: 409, message: 'CI failed on the head commit' } }, 409));
    const el = mountIn(() => <MergeButton attemptId="att_1" merge={{ enabled: true }} />);
    el.querySelector('button')!.click();
    await flush();
    expect(el.textContent).toContain('CI failed on the head commit');
    expect(err).toHaveBeenCalled();
  });
});

describe('MergeNoteBadge', () => {
  it('the_card_badge_names_the_held_merge_and_opens_the_reason_and_what_to_do', async () => {
    const el = mountIn(() => <MergeNoteBadge note="The GitHub token can't merge" />);
    const b = el.querySelector('button')!;
    expect(b.textContent).toBe("The GitHub token can't merge");
    expect(el.querySelector('[data-testid="merge-note-detail"]')).toBeNull();
    b.click();
    await flush();
    const detail = el.querySelector('[data-testid="merge-note-detail"]')!;
    expect(detail.textContent).toContain("The GitHub token can't merge");
    expect(detail.textContent).toContain('What to do');
  });

  it('other_notes_read_not_merged_and_nothing_renders_without_a_note', () => {
    const el = mountIn(() => <MergeNoteBadge note="CI failed on the head commit" />);
    expect(el.querySelector('button')!.textContent).toBe('Not merged: CI failed on the head commit');
    const waiting = mountIn(() => <MergeNoteBadge note="Waiting for CI on the head commit" />);
    expect(waiting.querySelector('button')!.textContent).toBe('Waiting for CI to merge');
    const none = mountIn(() => <MergeNoteBadge note={null} />);
    expect(none.querySelector('button')).toBeNull();
  });
});
