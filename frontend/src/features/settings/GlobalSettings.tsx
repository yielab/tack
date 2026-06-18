import { type Component, createSignal, createResource, For, Show } from 'solid-js';
import { api } from '../../shared/api';
import { toast } from '../../shared/ui/toast';
import { Button } from '../../shared/ui';
import { getStoredTheme, setTheme, type Theme } from '../../shared/state/theme';

const THEMES: { value: Theme; label: string }[] = [
  { value: 'light', label: '☀️ Light' },
  { value: 'dark', label: '🌙 Dark' },
  { value: 'system', label: '💻 System' },
];

function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

const GlobalSettings: Component = () => {
  const [theme, setThemeSig] = createSignal<Theme>(getStoredTheme());
  const [backingUp, setBackingUp] = createSignal(false);
  const [restoring, setRestoring] = createSignal(false);
  const [showSystem, setShowSystem] = createSignal(false);
  let restoreInput: HTMLInputElement | undefined;

  const chooseTheme = (t: Theme) => {
    setTheme(t);
    setThemeSig(t);
  };

  const downloadBackup = async () => {
    setBackingUp(true);
    try {
      const blob = await api.data.backup();
      downloadBlob(blob, 'tack-backup.db');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Backup failed');
    } finally {
      setBackingUp(false);
    }
  };

  const restoreFrom = async (file: File) => {
    if (
      !confirm(
        'Restoring REPLACES the entire database with this file. This cannot be undone. Continue?',
      )
    ) {
      return;
    }
    setRestoring(true);
    try {
      await api.data.restore(file);
      toast.success('Database restored. Reload to see the changes.');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Restore failed');
    } finally {
      setRestoring(false);
    }
  };

  const [health] = createResource(showSystem, () => api.system.health());
  const [dbStats] = createResource(showSystem, () => api.system.dbStats());

  return (
    <div class="max-w-2xl mx-auto px-6 py-8 space-y-10">
      <h1 class="text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
        Settings
      </h1>

      {/* Appearance */}
      <section class="space-y-3">
        <h2 class="text-lg font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          Appearance
        </h2>
        <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          Theme is saved to this browser.
        </p>
        <div class="flex gap-2">
          <For each={THEMES}>
            {(t) => (
              <Button
                variant={theme() === t.value ? 'primary' : 'secondary'}
                onClick={() => chooseTheme(t.value)}
              >
                {t.label}
              </Button>
            )}
          </For>
        </div>
      </section>

      {/* Data & Backup */}
      <section class="space-y-3 border-t pt-6" style={{ 'border-color': 'var(--color-border-light)' }}>
        <h2 class="text-lg font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          Data &amp; Backup
        </h2>
        <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          Download a full database backup, or restore from a previous one.
        </p>
        <div class="flex flex-wrap gap-2">
          <Button onClick={() => void downloadBackup()} loading={backingUp()} disabled={backingUp()}>
            Download backup
          </Button>
          <Button variant="secondary" onClick={() => restoreInput?.click()} disabled={restoring()}>
            Restore from file…
          </Button>
          <input
            ref={restoreInput}
            type="file"
            class="hidden"
            onChange={(e) => {
              const f = e.currentTarget.files?.[0];
              e.currentTarget.value = '';
              if (f) void restoreFrom(f);
            }}
          />
        </div>
        <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
          Restoring replaces the entire database.
        </p>
      </section>

      {/* System (advanced) */}
      <section class="space-y-3 border-t pt-6" style={{ 'border-color': 'var(--color-border-light)' }}>
        <button
          type="button"
          onClick={() => setShowSystem((s) => !s)}
          class="text-lg font-semibold"
          style={{ color: 'var(--color-text-primary)' }}
        >
          System {showSystem() ? '▾' : '▸'}
        </button>
        <Show when={showSystem()}>
          <div class="space-y-3 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            <Show when={health()}>
              {(h) => (
                <div>
                  <div>Status: {h().status}</div>
                  <div>Version: {h().version}</div>
                  <div>Migrations applied: {h().migrations_applied}</div>
                </div>
              )}
            </Show>
            <Show when={dbStats()}>
              {(s) => (
                <div>
                  <p class="font-medium" style={{ color: 'var(--color-text-primary)' }}>
                    Table row counts
                  </p>
                  <ul class="mt-1 grid grid-cols-2 gap-x-6">
                    <For each={Object.entries(s().tables)}>
                      {([table, count]) => (
                        <li class="flex justify-between">
                          <span>{table}</span>
                          <span>{count}</span>
                        </li>
                      )}
                    </For>
                  </ul>
                </div>
              )}
            </Show>
          </div>
        </Show>
      </section>
    </div>
  );
};

export default GlobalSettings;
