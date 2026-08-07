import { type Component, createSignal, onMount } from 'solid-js';
import { FiBold, FiItalic, FiList, FiCode } from 'solid-icons/fi';

export interface RichTextEditorProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
}

const ALLOWED_ELEMENTS = new Set([
  'p', 'br', 'strong', 'b', 'em', 'i', 'u', 's', 'ul', 'ol', 'li', 'h1', 'h2', 'h3', 'blockquote', 'pre', 'code', 'a',
]);

function safeLink(value: string): string | null {
  try {
    const url = new URL(value, window.location.href);
    return ['http:', 'https:', 'mailto:'].includes(url.protocol) ? url.href : null;
  } catch {
    return null;
  }
}

/**
 * Sanitise rich content both before rendering saved HTML and before emitting a
 * change. The editor has a deliberately small formatting vocabulary; unknown
 * elements are unwrapped, while executable/embedded elements are removed.
 */
export function sanitizeRichText(value: string): string {
  const documentFragment = new DOMParser().parseFromString(value, 'text/html');
  const output = document.createElement('div');
  const blockedElements = new Set(['script', 'style', 'iframe', 'object', 'embed', 'svg', 'math', 'template']);

  const appendClean = (node: Node, parent: Node) => {
    if (node.nodeType === Node.TEXT_NODE) {
      parent.appendChild(document.createTextNode(node.textContent ?? ''));
      return;
    }
    if (node.nodeType !== Node.ELEMENT_NODE) return;

    const element = node as HTMLElement;
    const tag = element.tagName.toLowerCase();
    if (blockedElements.has(tag)) return;
    if (!ALLOWED_ELEMENTS.has(tag)) {
      for (const child of Array.from(element.childNodes)) appendClean(child, parent);
      return;
    }

    const clean = document.createElement(tag);
    if (tag === 'a') {
      const href = element.getAttribute('href');
      const safeHref = href && safeLink(href);
      if (safeHref) {
        clean.setAttribute('href', safeHref);
        clean.setAttribute('rel', 'noopener noreferrer');
      }
    }
    for (const child of Array.from(element.childNodes)) appendClean(child, clean);
    parent.appendChild(clean);
  };

  for (const child of Array.from(documentFragment.body.childNodes)) appendClean(child, output);
  return output.innerHTML;
}

const RichTextEditor: Component<RichTextEditorProps> = (props) => {
  let editorRef: HTMLDivElement | undefined;
  const [isFocused, setIsFocused] = createSignal(false);

  onMount(() => {
    if (editorRef && props.value) {
      editorRef.innerHTML = sanitizeRichText(props.value);
    }
  });

  const execCommand = (command: string, value?: string) => {
    document.execCommand(command, false, value);
    editorRef?.focus();
    updateContent();
  };

  const updateContent = () => {
    if (editorRef) {
      const sanitized = sanitizeRichText(editorRef.innerHTML);
      if (editorRef.innerHTML !== sanitized) editorRef.innerHTML = sanitized;
      props.onChange(sanitized);
    }
  };

  const handlePaste = (e: ClipboardEvent) => {
    e.preventDefault();
    const text = e.clipboardData?.getData('text/plain');
    if (text) {
      document.execCommand('insertText', false, text);
    }
  };

  return (
    <div class="border border-line-medium rounded-lg overflow-hidden bg-elevated">
      {/* Toolbar */}
      <div class="flex items-center gap-1 p-2 border-b border-line bg-app">
        <button
          type="button"
          onClick={() => execCommand('bold')}
          class="p-2 rounded hover:bg-sunken text-content-muted transition-colors"
          title="Bold (Ctrl+B)"
          disabled={props.disabled}
        >
          <FiBold size={16} />
        </button>
        <button
          type="button"
          onClick={() => execCommand('italic')}
          class="p-2 rounded hover:bg-sunken text-content-muted transition-colors"
          title="Italic (Ctrl+I)"
          disabled={props.disabled}
        >
          <FiItalic size={16} />
        </button>
        <div class="w-px h-6 bg-sunken mx-1" />
        <button
          type="button"
          onClick={() => execCommand('insertUnorderedList')}
          class="p-2 rounded hover:bg-sunken text-content-muted transition-colors"
          title="Bullet List"
          disabled={props.disabled}
        >
          <FiList size={16} />
        </button>
        <button
          type="button"
          onClick={() => execCommand('insertOrderedList')}
          class="p-2 rounded hover:bg-sunken text-content-muted transition-colors"
          title="Numbered List"
          disabled={props.disabled}
        >
          <FiCode size={16} />
        </button>
        <div class="w-px h-6 bg-sunken mx-1" />
        <select
          aria-label="Text style"
          onChange={(e) => execCommand('formatBlock', e.currentTarget.value)}
          class="text-sm px-2 py-1 rounded border-0 bg-transparent text-content-muted focus:ring-2 focus:ring-brand-500"
          disabled={props.disabled}
        >
          <option value="p">Normal</option>
          <option value="h1">Heading 1</option>
          <option value="h2">Heading 2</option>
          <option value="h3">Heading 3</option>
        </select>
      </div>

      {/* Editor */}
      <div
        ref={editorRef}
        contentEditable={!props.disabled}
        onInput={updateContent}
        onFocus={() => setIsFocused(true)}
        onBlur={() => setIsFocused(false)}
        onPaste={handlePaste}
        class={`
          min-h-[120px] max-h-[300px] overflow-y-auto px-3 py-2
          text-content
          focus:outline-none
          prose prose-sm dark:prose-invert max-w-none
          ${isFocused() ? 'ring-2 ring-brand-500 ring-inset' : ''}
          ${props.disabled ? 'opacity-50 cursor-not-allowed' : ''}
        `}
        data-placeholder={props.placeholder}
      />

      <style>
        {`
          [contenteditable][data-placeholder]:empty:before {
            content: attr(data-placeholder);
            color: rgb(156 163 175);
            pointer-events: none;
            position: absolute;
          }
          [contenteditable] ul,
          [contenteditable] ol {
            padding-left: 1.5rem;
            margin: 0.5rem 0;
          }
          [contenteditable] h1 {
            font-size: 1.5rem;
            font-weight: bold;
            margin: 0.5rem 0;
          }
          [contenteditable] h2 {
            font-size: 1.25rem;
            font-weight: bold;
            margin: 0.5rem 0;
          }
          [contenteditable] h3 {
            font-size: 1.1rem;
            font-weight: bold;
            margin: 0.5rem 0;
          }
        `}
      </style>
    </div>
  );
};

export default RichTextEditor;
