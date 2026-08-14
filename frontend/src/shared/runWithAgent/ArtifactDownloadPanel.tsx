import { type Component, Show, createSignal } from 'solid-js';
import { Button, Field } from '../ui';
import { artifactsApi, isArtifactContentNotVerified, isArtifactNotFound } from '../execution';

export interface ArtifactDownloadPanelProps {
  requestId: string;
  attemptNumber: number;
}

type DownloadStatusKind = 'idle' | 'downloading' | 'done' | 'not_found' | 'not_verified' | 'error';

/**
 * Verified artifact download for one attempt (TODO.md III-F4: "verified
 * artifact download", "one generated-artifact integration"). Calls the
 * real, mounted `GET .../artifacts/{artifact_id}/content` (III-F2, wired by
 * III-F6/F6a) via `fetch` + `Blob` rather than a plain `<a href download>`
 * (the pattern `FilesTab.tsx` uses for ordinary attachments) — an anchor
 * tag cannot attach the `Authorization` bearer header this operator route
 * requires, and more importantly cannot report *why* a download failed, and
 * this card's acceptance bar is explicit: "artifact failure visible".
 *
 * Two failure states are kept visually AND semantically distinct, matching
 * `artifact_download.rs`'s own documented distinction: a 404 (no manifest
 * exists under this id) is a different fact from a 409 (the manifest
 * exists, but its content has not been verified/streamed in yet — worth
 * retrying, not gone). See `shared/execution/artifacts.ts`'s header comment
 * for why `artifactId` is a manually-entered value: no artifact-discovery
 * endpoint exists anywhere in this codebase yet.
 */
const ArtifactDownloadPanel: Component<ArtifactDownloadPanelProps> = (props) => {
  const [artifactId, setArtifactId] = createSignal('');
  const [status, setStatus] = createSignal<DownloadStatusKind>('idle');
  const [errorMessage, setErrorMessage] = createSignal<string | undefined>(undefined);

  const canSubmit = () => artifactId().trim().length > 0 && status() !== 'downloading';

  const download = async (e: Event) => {
    e.preventDefault();
    if (!canSubmit()) return;
    setStatus('downloading');
    setErrorMessage(undefined);
    try {
      const blob = await artifactsApi.download(props.requestId, props.attemptNumber, artifactId().trim());
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = artifactId().trim();
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
    <form
      class="space-y-2 rounded-lg border border-dashed p-3"
      style={{ 'border-color': 'var(--color-border-light)' }}
      onSubmit={download}
    >
      <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
        Download a verified artifact by id — no artifact-discovery endpoint exists on this
        deployment yet (see docs/agent-handoffs/part-iii/III-F4.md), so enter an id you already
        know.
      </p>
      <Field
        label="Artifact id"
        value={artifactId()}
        onInput={(e) => {
          setArtifactId(e.currentTarget.value);
          setStatus('idle');
          setErrorMessage(undefined);
        }}
        required
      />
      <div class="flex items-center gap-2">
        <Button type="submit" size="sm" variant="secondary" disabled={!canSubmit()} loading={status() === 'downloading'}>
          Download artifact
        </Button>
        <Show when={!artifactId().trim()}>
          <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
            Enter an artifact id to enable download.
          </span>
        </Show>
      </div>

      {/* Every outcome — success and each distinct failure — is a visible,
          named state (acceptance bar: "artifact failure visible"). */}
      <Show when={status() === 'done'}>
        <p class="text-xs" style={{ color: 'var(--color-success-700)' }}>
          Downloaded.
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
    </form>
  );
};

export default ArtifactDownloadPanel;
