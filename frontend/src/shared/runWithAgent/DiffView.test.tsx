import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import DiffView from './DiffView';

let dispose: (() => void) | undefined;
afterEach(() => dispose?.());

describe('DiffView', () => {
  it('splits per file and marks added, removed and hunk lines', () => {
    const c = document.createElement('div');
    dispose = render(
      () => <DiffView patch={'diff --git a/x b/dir/x\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/y b/y\n+c\n'} />,
      c,
    );
    expect(Array.from(c.querySelectorAll('[data-testid="diff-file"]')).map((h) => h.textContent)).toEqual(['dir/x', 'y']);
    expect(c.querySelectorAll('[data-diff="add"]').length).toBe(2);
    expect(c.querySelectorAll('[data-diff="del"]').length).toBe(1);
    expect(c.querySelectorAll('[data-diff="hunk"]').length).toBe(1);
  });
});
