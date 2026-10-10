import { describe, expect, it, vi } from 'vitest';
import { render } from 'solid-js/web';
import RichTextEditor, { sanitizeRichText } from './RichTextEditor';

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

  it('keeps the lines the browser wraps in a div on Enter, as paragraphs', () => {
    expect(sanitizeRichText('línea 1<div>línea 2</div><div><br></div><div>línea 3</div>')).toBe(
      'línea 1<p>línea 2</p><p><br></p><p>línea 3</p>'
    );
  });
});

describe('RichTextEditor', () => {
  it('emits the sanitised value without rewriting what is being typed', () => {
    const onChange = vi.fn();
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(() => <RichTextEditor value="" onChange={onChange} />, container);
    const editor = container.querySelector('[contenteditable]') as HTMLDivElement;
    const typed = '<div>línea 1</div><div>línea 2</div><div>línea 3</div>';
    editor.innerHTML = typed;
    const caretLine = editor.lastChild;

    editor.dispatchEvent(new InputEvent('input', { bubbles: true }));

    expect(onChange).toHaveBeenLastCalledWith('<p>línea 1</p><p>línea 2</p><p>línea 3</p>');
    expect(editor.innerHTML).toBe(typed);
    expect(editor.lastChild).toBe(caretLine);
    dispose();
    container.remove();
  });
});
