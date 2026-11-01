import { describe, expect, it } from 'vitest';
import { descriptionToHtml } from './descriptionMarkdown';

describe('descriptionToHtml', () => {
  it('renders Markdown when the value has no HTML tag', () => {
    expect(descriptionToHtml('**bold** and a list:\n\n- one\n- two')).toContain('<strong>bold</strong>');
    expect(descriptionToHtml('- one\n- two')).toContain('<li>one</li>');
  });

  it('leaves the editor HTML untouched', () => {
    const html = 'line 1<div>line 2</div>';
    expect(descriptionToHtml(html)).toBe(html);
  });

  it('leaves an empty value empty', () => {
    expect(descriptionToHtml('')).toBe('');
  });
});
