import { describe, expect, it } from 'vitest';
import { sanitizeRichText } from './RichTextEditor';

describe('sanitizeRichText', () => {
  it('removes persisted scripts and event handlers before they can reach innerHTML', () => {
    const value = sanitizeRichText(
      '<h1 onclick="globalThis.pwned = true">Heading</h1><script>globalThis.pwned = true</script><p>Safe</p>'
    );

    expect(value).toBe('<h1>Heading</h1><p>Safe</p>');
    expect(value).not.toContain('onclick');
    expect(value).not.toContain('script');
  });

  it('keeps only safe links and strips embedded or javascript content', () => {
    const value = sanitizeRichText(
      '<a href="javascript:alert(1)" onmouseover="alert(1)">bad</a><a href="https://example.test/docs">good</a><img src=x onerror="alert(1)">'
    );

    expect(value).toContain('<a>bad</a>');
    expect(value).toContain('href="https://example.test/docs"');
    expect(value).toContain('rel="noopener noreferrer"');
    expect(value).not.toContain('javascript:');
    expect(value).not.toContain('<img');
  });
});
