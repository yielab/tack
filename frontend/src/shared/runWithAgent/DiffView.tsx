import { type Component, For } from 'solid-js';

interface FileDiff {
  path: string;
  lines: string[];
}

function splitDiff(patch: string): FileDiff[] {
  const files: FileDiff[] = [];
  for (const line of patch.split('\n')) {
    if (line.startsWith('diff --git ')) {
      const m = / b\/(.+)$/.exec(line);
      files.push({ path: m ? m[1] : line.slice('diff --git '.length), lines: [] });
    } else if (files.length) {
      files[files.length - 1].lines.push(line);
    }
  }
  return files;
}

function lineKind(line: string): 'add' | 'del' | 'hunk' | 'ctx' {
  if (line.startsWith('@@')) return 'hunk';
  if (line.startsWith('+') && !line.startsWith('+++')) return 'add';
  if (line.startsWith('-') && !line.startsWith('---')) return 'del';
  return 'ctx';
}

const STYLE = {
  add: { color: 'var(--color-success-600)', 'background-color': 'var(--color-success-100)' },
  del: { color: 'var(--color-danger-600)', 'background-color': 'var(--color-danger-100)' },
  hunk: { color: 'var(--color-text-tertiary)' },
  ctx: { color: 'var(--color-text-primary)' },
} as const;

/** A unified diff, one heading per file; added and removed lines coloured. */
const DiffView: Component<{ patch: string }> = (props) => (
  <div class="space-y-4">
    <For each={splitDiff(props.patch)}>
      {(file) => (
        <section>
          <h5 data-testid="diff-file" class="mb-1 text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            {file.path}
          </h5>
          <pre class="overflow-x-auto text-xs" style={{ 'font-family': 'var(--font-mono)', 'white-space': 'pre' }}>
            <For each={file.lines}>
              {(line) => (
                <div data-diff={lineKind(line)} style={STYLE[lineKind(line)]}>
                  {line}
                </div>
              )}
            </For>
          </pre>
        </section>
      )}
    </For>
  </div>
);

export default DiffView;
