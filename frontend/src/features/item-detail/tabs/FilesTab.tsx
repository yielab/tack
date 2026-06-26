import { type Component, createResource, createSignal, For, Show } from 'solid-js';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { Button, EmptyState } from '../../../shared/ui';

export interface FilesTabProps {
  itemId: string;
}

const MAX_BYTES = 50 * 1024 * 1024; // 50 MB (server limit)

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Attachments tab: drag-drop / picker upload, list, download, delete. */
const FilesTab: Component<FilesTabProps> = (props) => {
  const [files, { refetch }] = createResource(
    () => props.itemId,
    (id) => api.attachments.list(id),
  );
  const [dragOver, setDragOver] = createSignal(false);
  const [uploading, setUploading] = createSignal(false);
  let input: HTMLInputElement | undefined;

  const uploadAll = async (list: FileList | File[]) => {
    const arr = Array.from(list);
    for (const file of arr) {
      if (file.size > MAX_BYTES) {
        toast.error(`"${file.name}" is ${formatSize(file.size)} — exceeds the 50 MB limit.`);
        continue;
      }
      setUploading(true);
      try {
        await api.attachments.upload(props.itemId, file);
      } catch (err) {
        toast.error(err instanceof Error ? err.message : `Failed to upload ${file.name}`);
      } finally {
        setUploading(false);
      }
    }
    await refetch();
  };

  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    setDragOver(false);
    if (e.dataTransfer?.files?.length) void uploadAll(e.dataTransfer.files);
  };

  const remove = async (id: string) => {
    try {
      await api.attachments.remove(id);
      await refetch();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to delete attachment');
    }
  };

  return (
    <div class="space-y-4">
      {/* Drop zone */}
      <div
        role="button"
        tabindex={0}
        onClick={() => input?.click()}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') input?.click();
        }}
        onDragOver={(e) => {
          e.preventDefault();
          setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={onDrop}
        class="cursor-pointer rounded-lg border-2 border-dashed p-6 text-center text-sm transition-colors"
        style={{
          'border-color': dragOver() ? 'var(--color-primary-500)' : 'var(--color-border-medium)',
          'background-color': dragOver() ? 'var(--color-bg-active)' : 'transparent',
          color: 'var(--color-text-secondary)',
        }}
      >
        <Show when={uploading()} fallback={<>Drop files here, or click to browse (max 50 MB).</>}>
          Uploading…
        </Show>
        <input
          ref={input}
          type="file"
          multiple
          class="hidden"
          onChange={(e) => {
            if (e.currentTarget.files?.length) void uploadAll(e.currentTarget.files);
            e.currentTarget.value = '';
          }}
        />
      </div>

      <Show
        when={(files() ?? []).length > 0}
        fallback={<EmptyState title="No attachments yet" />}
      >
        <ul class="space-y-1">
          <For each={files()}>
            {(f) => (
              <li
                class="flex items-center justify-between rounded-md border px-3 py-2 text-sm"
                style={{
                  'background-color': 'var(--color-bg-base)',
                  'border-color': 'var(--color-border-light)',
                }}
              >
                <div class="min-w-0 flex-1">
                  <a
                    href={api.attachments.downloadUrl(f.id)}
                    download={f.filename}
                    class="block truncate font-medium hover:underline"
                    style={{ color: 'var(--color-primary-700)' }}
                  >
                    {f.filename}
                  </a>
                  <div class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                    {formatSize(f.size_bytes)} · {f.mime_type}
                  </div>
                </div>
                <Button size="sm" variant="ghost" onClick={() => void remove(f.id)}>
                  Delete
                </Button>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </div>
  );
};

export default FilesTab;
