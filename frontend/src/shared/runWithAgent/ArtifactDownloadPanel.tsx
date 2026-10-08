import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { Badge, Button, EmptyState, Modal } from '../ui';
import { artifactsApi, isArtifactContentNotVerified, isArtifactNotFound, type ArtifactRecord } from '../execution';
import DiffView from './DiffView';
import { inDesktop } from './desktop';

export interface ArtifactDownloadPanelProps {
  requestId: string;
  attemptNumber: number;
}

type DownloadStatusKind = 'idle' | 'downloading' | 'done' | 'not_found' | 'not_verified' | 'error';

/** An artifact's content, read in place: a patch as a diff, JSON indented, anything else as text. */
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
            <Show when={isPatch()} fallback={
              <pre data-testid="artifact-text" class="max-h-[65vh] overflow-auto whitespace-pre-wrap break-words rounded-[20px] px-4 py-3 text-xs"
                style={{ 'background-color': 'var(--color-bg-app)', color: 'var(--color-text-primary)', 'font-family': 'var(--font-mono)' }}>{shown()}</pre>
            }>
              <DiffView patch={shown()} />
            </Show>
          </Show>
        </Show>
      </Show>
    </Modal>
  );
};

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

  return (
    <li class="space-y-2 rounded-[20px] px-4 py-3" style={{ 'background-color': 'var(--color-bg-panel)' }}>
      <div class="flex flex-wrap items-center gap-2">
        <span class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          {props.artifact.name}
        </span>
        <Badge tone="neutral">{props.artifact.kind}</Badge>
        <Show when={!props.artifact.content_verified}>
          <Badge tone="warning">Not verified yet</Badge>
        </Show>
        <span class="text-[11px]" style={{ 'font-family': 'var(--font-mono)', color: 'var(--color-text-tertiary)' }}>
          {formatSize(props.artifact.size_bytes)}
        </span>
        <div class="ml-auto flex gap-2">
          <Button size="sm" variant="secondary" onClick={() => setViewing(true)}>View</Button>
          <Button size="sm" variant="secondary" onClick={download} disabled={status() === 'downloading'} loading={status() === 'downloading'}>
            Download
          </Button>
        </div>
      </div>
      <ArtifactView requestId={props.requestId} attemptNumber={props.attemptNumber}
        artifact={viewing() ? props.artifact : undefined} onClose={() => setViewing(false)} />

      {/* Every outcome — success and each distinct failure — is a visible,
          named state: artifact failure stays visible. */}
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
    </li>
  );
};

/**
 * Every artifact manifested for one attempt, reading real data from
 * `GET /executions/{request_id}/attempts/{attempt_number}/artifacts`
 * — no artifact id is ever typed by an operator.
 */
const ArtifactDownloadPanel: Component<ArtifactDownloadPanelProps> = (props) => {
  const [artifacts] = createResource(
    () => `${props.requestId}:${props.attemptNumber}`,
    () => artifactsApi.list(props.requestId, props.attemptNumber),
  );

  return (
    <div>
      <Show when={artifacts.loading}>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          Loading artifacts…
        </p>
      </Show>
      <Show when={artifacts.error}>
        <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
          Couldn't load artifacts: {artifacts.error instanceof Error ? artifacts.error.message : 'unknown error'}
        </p>
      </Show>
      <Show when={!artifacts.loading && !artifacts.error && (artifacts() ?? []).length === 0}>
        <EmptyState title="No artifacts yet" />
      </Show>
      <Show when={!artifacts.loading && !artifacts.error && (artifacts() ?? []).length > 0}>
        <ul class="space-y-2">
          <For each={artifacts()}>
            {(artifact) => (
              <ArtifactRow requestId={props.requestId} attemptNumber={props.attemptNumber} artifact={artifact} />
            )}
          </For>
        </ul>
      </Show>
    </div>
  );
};

export default ArtifactDownloadPanel;
