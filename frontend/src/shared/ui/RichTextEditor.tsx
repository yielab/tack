import { type Component, createSignal, onMount } from 'solid-js';
import { FiBold, FiItalic, FiList, FiCode } from 'solid-icons/fi';

export interface RichTextEditorProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
}

const RichTextEditor: Component<RichTextEditorProps> = (props) => {
  let editorRef: HTMLDivElement | undefined;
  const [isFocused, setIsFocused] = createSignal(false);

  onMount(() => {
    if (editorRef && props.value) {
      editorRef.innerHTML = props.value;
    }
  });

  const execCommand = (command: string, value?: string) => {
    document.execCommand(command, false, value);
    editorRef?.focus();
    updateContent();
  };

  const updateContent = () => {
    if (editorRef) {
      props.onChange(editorRef.innerHTML);
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
    <div class="border border-gray-300 dark:border-gray-600 rounded-lg overflow-hidden bg-white dark:bg-gray-800">
      {/* Toolbar */}
      <div class="flex items-center gap-1 p-2 border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-900/50">
        <button
          type="button"
          onClick={() => execCommand('bold')}
          class="p-2 rounded hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300 transition-colors"
          title="Bold (Ctrl+B)"
          disabled={props.disabled}
        >
          <FiBold size={16} />
        </button>
        <button
          type="button"
          onClick={() => execCommand('italic')}
          class="p-2 rounded hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300 transition-colors"
          title="Italic (Ctrl+I)"
          disabled={props.disabled}
        >
          <FiItalic size={16} />
        </button>
        <div class="w-px h-6 bg-gray-300 dark:bg-gray-600 mx-1" />
        <button
          type="button"
          onClick={() => execCommand('insertUnorderedList')}
          class="p-2 rounded hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300 transition-colors"
          title="Bullet List"
          disabled={props.disabled}
        >
          <FiList size={16} />
        </button>
        <button
          type="button"
          onClick={() => execCommand('insertOrderedList')}
          class="p-2 rounded hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300 transition-colors"
          title="Numbered List"
          disabled={props.disabled}
        >
          <FiCode size={16} />
        </button>
        <div class="w-px h-6 bg-gray-300 dark:bg-gray-600 mx-1" />
        <select
          onChange={(e) => execCommand('formatBlock', e.currentTarget.value)}
          class="text-sm px-2 py-1 rounded border-0 bg-transparent text-gray-700 dark:text-gray-300 focus:ring-2 focus:ring-purple-500"
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
          text-gray-900 dark:text-white
          focus:outline-none
          prose prose-sm dark:prose-invert max-w-none
          ${isFocused() ? 'ring-2 ring-purple-500 ring-inset' : ''}
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
