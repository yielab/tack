import { type Component, For, type JSX, Show, createResource, createSignal } from 'solid-js';
import { Badge, Modal } from '../ui';
import { artifactsApi, isArtifactContentNotVerified, isArtifactNotFound, type ArtifactRecord } from '../execution';
import DiffView from './DiffView';
import { inDesktop } from './desktop';
import { truncateMiddle } from '../ui/truncateMiddle';
import Markdown from '../ui/Markdown';

export interface ArtifactDownloadPanelProps {
  requestId: string;
  attemptNumber: number;
  /** The files the attempt manifested, listed by the screen that also needs them for its other sections. */
  artifacts: ArtifactRecord[];
}

type DownloadStatusKind = 'idle' | 'downloading' | 'done' | 'not_found' | 'not_verified' | 'error';

/** A file that is Markdown: a report or a reply, read as a page and not as marks. */
export const isMarkdownArtifact = (a: Pick<ArtifactRecord, 'kind' | 'name' | 'media_type'> | undefined): boolean =>
  !!a && (a.kind === 'report' || a.kind === 'reply' || a.media_type === 'text/markdown' || (a.name ?? '').endsWith('.md'));

/** Hands text to the browser (or the desktop app) as a file. */
export function saveTextAs(name: string, text: string, mediaType = 'text/markdown'): void {
  const url = URL.createObjectURL(new Blob([text], { type: `${mediaType};charset=utf-8` }));
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}

/** An artifact's content, read in place: a patch as a diff, JSON indented, Markdown as a page,
 *  anything else as text. */
export const ArtifactView: Component<{
  requestId: string; attemptNumber: number; artifact: ArtifactRecord | undefined; onClose: () => void;
}> = (props) => {
  const [text] = createResource(
    () => props.artifact,
    async (a) => (await artifactsApi.download(props.requestId, props.attemptNumber, a.artifact_id)).text(),
  );
  const shown = () => {
    const t = text() ?? '';
    const a = props.artifact;
    if (a && (a.media_type?.includes('json') || a.name.endsWith('.json'))) {
      try { return JSON.stringify(JSON.parse(t), null, 2); } catch { return t; }
    }
    return t;
  };
  const isPatch = () => props.artifact?.kind === 'patch';
  return (
    <Modal isOpen={!!props.artifact} onClose={props.onClose} title={props.artifact?.name ?? ''} size="xl">
      <Show when={!text.loading} fallback={<p class="text-sm" style={{ color: 'var(--color-text-tertiary)' }}>Loading…</p>}>
        <Show when={!text.error} fallback={<p class="text-sm" style={{ color: 'var(--color-danger-600)' }}>Couldn't read this artifact.</p>}>
          <Show when={shown() !== ''} fallback={<p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>This file is empty.</p>}>
            <Show when={isMarkdownArtifact(props.artifact)}>
              <div class="mb-3 flex justify-end">
                <button type="button" data-testid="artifact-save" class="text-xs font-semibold hover:underline" style={{ color: 'var(--color-accent-ink)' }}
                  onClick={() => saveTextAs(props.artifact?.name || 'report.md', shown())}>
                  Download {props.artifact?.name || 'report.md'}
                </button>
              </div>
              <div class="overflow-auto pr-1" style={{ "max-height": "70vh" }}><Markdown testId="artifact-markdown" text={shown()} /></div>
            </Show>
            <Show when={!isMarkdownArtifact(props.artifact) && isPatch()}>
              <DiffView patch={shown()} />
            </Show>
            <Show when={!isMarkdownArtifact(props.artifact) && !isPatch()}>
              <pre data-testid="artifact-text" class="max-h-[65vh] overflow-auto whitespace-pre-wrap break-words rounded-item px-4 py-3 text-xs"
                style={{ 'background-color': 'var(--color-bg-app)', color: 'var(--color-text-primary)', 'font-family': 'var(--font-mono)' }}>{shown()}</pre>
            </Show>
          </Show>
        </Show>
      </Show>
    </Modal>
  );
};

const glyph = (body: JSX.Element) => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    {body}
  </svg>
);
const IconEye: Component = () =>
  glyph(
    <>
      <path d="M1.8 8s2.2-4 6.2-4 6.2 4 6.2 4-2.2 4-6.2 4-6.2-4-6.2-4z" />
      <circle cx="8" cy="8" r="1.9" />
    </>,
  );
const IconDownload: Component = () => glyph(<path d="M8 2.5v7.5M4.8 7.3 8 10.5l3.2-3.2M3 13h10" />);

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * One manifested artifact, with its own real download action. Calls the
 * real, mounted `GET .../artifacts/{artifact_id}/content` via `fetch` +
 * `Blob` rather than a plain `<a href download>` (the pattern
 * `FilesTab.tsx` uses for ordinary attachments) — an anchor tag cannot
 * attach the `Authorization` bearer header this operator route requires,
 * and more importantly cannot report *why* a download failed: artifact
 * failure must stay visible.
 *
 * Two failure states are kept visually AND semantically distinct, matching
 * `artifact_download.rs`'s own documented distinction: a 404 (no manifest
 * exists under this id) is a different fact from a 409 (the manifest
 * exists, but its content has not been verified/streamed in yet — worth
 * retrying, not gone). `content_verified` on the manifest row itself
 * already answers this before a download is even attempted, but the button
 * stays enabled either way — the field can be stale by the time the click
 * lands, so the real 409 is still the authority, never overridden by a
 * pre-emptive guess.
 */
const ArtifactRow: Component<{ requestId: string; attemptNumber: number; artifact: ArtifactRecord }> = (props) => {
  const [status, setStatus] = createSignal<DownloadStatusKind>('idle');
  const [errorMessage, setErrorMessage] = createSignal<string | undefined>(undefined);
  const [viewing, setViewing] = createSignal(false);
  const fileName = () => props.artifact.name || props.artifact.artifact_id;

  const download = async () => {
    setStatus('downloading');
    setErrorMessage(undefined);
    try {
      const blob = await artifactsApi.download(props.requestId, props.attemptNumber, props.artifact.artifact_id);
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = fileName();
      document.body.appendChild(link);
      link.click();
      link.remove();
      URL.revokeObjectURL(url);
      setStatus('done');
    } catch (err) {
      if (isArtifactNotFound(err)) {
        setStatus('not_found');
      } else if (isArtifactContentNotVerified(err)) {
        setStatus('not_verified');
      } else {
        setStatus('error');
        setErrorMessage(err instanceof Error ? err.message : 'Download failed.');
      }
    }
  };

  const iconButton = (label: string, icon: JSX.Element, onClick: () => void, busy: () => boolean = () => false) => (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={busy()}
      onClick={onClick}
      class="inline-flex h-7 w-7 items-center justify-center rounded-full focus:outline-none focus-visible:ring-2 disabled:cursor-wait disabled:opacity-60"
      style={{ color: 'var(--color-accent-ink)', border: '1px solid var(--color-border-light)' }}
    >
      {icon}
    </button>
  );

  return (
    <>
      <tr data-testid="artifact-row">
        {/* A name breaks at its separators, never inside a word (`overflow-wrap: anywhere` let the
            table squeeze the column until `changes.patch` broke in two); a very long one keeps its
            head and its extension, with the whole name on hover. */}
        <th
          scope="row"
          title={props.artifact.name}
          class="min-w-0 px-3 py-2 text-left text-sm font-semibold"
          style={{ color: 'var(--color-text-primary)', 'overflow-wrap': 'break-word', 'word-break': 'normal' }}
        >
          {truncateMiddle(props.artifact.name ?? '', 32, 12)}
          <Show when={!props.artifact.content_verified}>
            <span class="ml-2 align-middle"><Badge tone="warning">Not verified yet</Badge></span>
          </Show>
        </th>
        <td class="px-3 py-2"><Badge tone="neutral">{props.artifact.kind}</Badge></td>
        <td class="whitespace-nowrap px-3 py-2 text-micro" style={{ 'font-family': 'var(--font-mono)', color: 'var(--color-text-tertiary)' }}>
          {formatSize(props.artifact.size_bytes)}
        </td>
        <td class="px-3 py-2">
          <div class="flex justify-end gap-1.5">
            {iconButton('View', <IconEye />, () => setViewing(true))}
            {iconButton('Download', <IconDownload />, download, () => status() === 'downloading')}
          </div>
          <ArtifactView requestId={props.requestId} attemptNumber={props.attemptNumber}
            artifact={viewing() ? props.artifact : undefined} onClose={() => setViewing(false)} />
        </td>
      </tr>
      {/* Every outcome — success and each distinct failure — is a visible,
          named state: artifact failure stays visible. */}
      <Show when={status() !== 'idle' && status() !== 'downloading'}>
        <tr>
          <td colspan="4" class="px-3 pb-2">
            <Show when={status() === 'done'}>
              <p class="text-xs" style={{ color: 'var(--color-success-700)' }}>
                {inDesktop() ? `Saved to your Downloads folder as ${fileName()}.` : `Sent to your browser's downloads as ${fileName()}.`}
              </p>
            </Show>
            <Show when={status() === 'not_found'}>
              <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
                No artifact with that id exists for this attempt.
              </p>
            </Show>
            <Show when={status() === 'not_verified'}>
              <p class="text-xs" style={{ color: 'var(--color-warning-700)' }}>
                This artifact's manifest exists, but its content hasn't been verified yet — try again
                shortly.
              </p>
            </Show>
            <Show when={status() === 'error'}>
              <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
                {errorMessage() ?? 'Download failed.'}
              </p>
            </Show>
          </td>
        </tr>
      </Show>
    </>
  );
};

/**
 * Every artifact manifested for one attempt as one compact table: its name, kind and size, and
 * View and Download as icon buttons. The list comes from the screen that asked for it
 * (`GET /executions/{request_id}/attempts/{attempt_number}/artifacts`) — no artifact id is ever
 * typed by an operator.
 */
const ArtifactDownloadPanel: Component<ArtifactDownloadPanelProps> = (props) => (
  <Show when={props.artifacts.length > 0} fallback={<p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>No artifacts yet</p>}>
    <div class="max-w-full overflow-x-auto">
      <table data-testid="artifact-table" class="w-full text-left" style={{ 'table-layout': 'auto' }}>
        <thead class="sr-only">
          <tr>
            <th scope="col">Name</th>
            <th scope="col">Kind</th>
            <th scope="col">Size</th>
            <th scope="col">Actions</th>
          </tr>
        </thead>
        <tbody class="divide-y" style={{ 'border-color': 'var(--color-border-light)' }}>
          <For each={props.artifacts}>
            {(artifact) => (
              <ArtifactRow requestId={props.requestId} attemptNumber={props.attemptNumber} artifact={artifact} />
            )}
          </For>
        </tbody>
      </table>
    </div>
  </Show>
);

export default ArtifactDownloadPanel;
