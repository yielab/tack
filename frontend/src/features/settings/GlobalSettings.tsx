import {
  type Component,
  createSignal,
  createResource,
  createEffect,
  For,
  Show,
} from 'solid-js';
import { FiUploadCloud, FiDownloadCloud, FiRefreshCw, FiCheckCircle } from 'solid-icons/fi';
import { api } from '../../shared/api';
import type { CloudBackupConfigInput } from '../../shared/api/data';
import { toast } from '../../shared/ui/toast';
import { Button, Field, Badge } from '../../shared/ui';
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

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

const GlobalSettings: Component = () => {
  const [theme, setThemeSig] = createSignal<Theme>(getStoredTheme());
  const [backingUp, setBackingUp] = createSignal(false);
  const [restoring, setRestoring] = createSignal(false);
  const [showSystem, setShowSystem] = createSignal(false);
  let restoreInput: HTMLInputElement | undefined;

  const [health] = createResource(() => api.system.health());

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

  // ── Cloud backup ───────────────────────────────────────────────────────────
  const [cloudConfig, { refetch: refetchConfig }] = createResource(() =>
    api.data.getCloudConfig(),
  );

  const [endpoint, setEndpoint] = createSignal('');
  const [bucket, setBucket] = createSignal('');
  const [region, setRegion] = createSignal('auto');
  const [accessKey, setAccessKey] = createSignal('');
  const [secretKey, setSecretKey] = createSignal('');
  const [prefix, setPrefix] = createSignal('tack');
  const [retention, setRetention] = createSignal(10);

  const [savingCloud, setSavingCloud] = createSignal(false);
  const [cloudBackingUp, setCloudBackingUp] = createSignal(false);
  const [cloudRestoring, setCloudRestoring] = createSignal(false);
  const [cloudVerifying, setCloudVerifying] = createSignal(false);

  // Populate the form whenever the saved config (re)loads.
  createEffect(() => {
    const c = cloudConfig();
    if (!c) return;
    setEndpoint(c.endpoint ?? '');
    setBucket(c.bucket ?? '');
    setRegion(c.region ?? 'auto');
    setAccessKey(c.access_key ?? '');
    setPrefix(c.prefix ?? 'tack');
    setRetention(c.retention ?? 10);
    setSecretKey(''); // never prefilled; blank = keep stored secret
  });

  const configured = () => cloudConfig()?.configured ?? false;

  const [cloudBackups, { refetch: refetchBackups }] = createResource(
    configured,
    (isConfigured) => (isConfigured ? api.data.cloudBackups() : Promise.resolve([])),
  );

  const saveCloud = async () => {
    setSavingCloud(true);
    try {
      const payload: CloudBackupConfigInput = {
        endpoint: endpoint(),
        bucket: bucket(),
        region: region(),
        access_key: accessKey(),
        prefix: prefix(),
        retention: retention(),
      };
      // Only send the secret if the user typed a new one.
      if (secretKey().trim()) payload.secret_key = secretKey();
      await api.data.saveCloudConfig(payload);
      await refetchConfig();
      toast.success('Cloud backup settings saved.');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Could not save settings');
    } finally {
      setSavingCloud(false);
    }
  };

  const backupNow = async () => {
    setCloudBackingUp(true);
    try {
      await api.data.cloudBackupNow();
      toast.success('Backed up to cloud storage.');
      void refetchBackups();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Cloud backup failed');
    } finally {
      setCloudBackingUp(false);
    }
  };

  const restoreFromCloud = async (key?: string) => {
    if (
      !confirm(
        'Restoring from the cloud REPLACES the entire database on the next restart. Continue?',
      )
    ) {
      return;
    }
    setCloudRestoring(true);
    try {
      const res = await api.data.cloudRestore(key);
      toast.success(res.message ?? 'Restore staged. Restart to apply.');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Cloud restore failed');
    } finally {
      setCloudRestoring(false);
    }
  };

  const verifyCloud = async (key?: string) => {
    setCloudVerifying(true);
    try {
      const res = await api.data.cloudVerify(key);
      if (res.ok) {
        toast.success(
          `Backup verified: ${res.manifest.item_count} items, checksum and schema OK.`,
        );
      } else {
        toast.error(`Backup FAILED verification: ${res.reason ?? 'unknown reason'}`);
      }
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Cloud verify failed');
    } finally {
      setCloudVerifying(false);
    }
  };

  const [dbStats] = createResource(showSystem, () => api.system.dbStats());

  return (
    <div class="max-w-2xl mx-auto px-6 py-8 space-y-10">
      <div class="flex items-baseline justify-between gap-3">
        <h1 class="text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
          Settings
        </h1>
        <Show when={health()}>
          {(h) => (
            <span class="text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
              Tack v{h().version}
            </span>
          )}
        </Show>
      </div>

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

      {/* Data & Backup (local) */}
      <section class="space-y-3 border-t pt-6" style={{ 'border-color': 'var(--color-border-light)' }}>
        <h2 class="text-lg font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          Local Backup
        </h2>
        <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          Download a full database backup file, or restore from a previous one.
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

      {/* Cloud / external backup */}
      <section class="space-y-4 border-t pt-6" style={{ 'border-color': 'var(--color-border-light)' }}>
        <div class="flex items-center gap-3">
          <h2 class="text-lg font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            Cloud Backup
          </h2>
          <Show
            when={configured()}
            fallback={<Badge tone="neutral">Not configured</Badge>}
          >
            <Badge tone="success">Connected</Badge>
          </Show>
        </div>
        <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          Sync your database to an external S3-compatible store (Cloudflare R2,
          Backblaze B2, AWS S3, MinIO). Enter the destination once, then back up
          or restore with one click.
        </p>

        <div class="grid gap-3 sm:grid-cols-2">
          <Field
            label="Endpoint URL"
            placeholder="https://<account>.r2.cloudflarestorage.com"
            hint="Leave blank for AWS S3."
            value={endpoint()}
            onInput={(e) => setEndpoint(e.currentTarget.value)}
          />
          <Field
            label="Bucket"
            placeholder="my-tack-backups"
            value={bucket()}
            onInput={(e) => setBucket(e.currentTarget.value)}
          />
          <Field
            label="Region"
            placeholder="auto"
            hint="Cloudflare R2 uses “auto”; AWS needs the real region."
            value={region()}
            onInput={(e) => setRegion(e.currentTarget.value)}
          />
          <Field
            label="Object prefix"
            placeholder="tack"
            value={prefix()}
            onInput={(e) => setPrefix(e.currentTarget.value)}
          />
          <Field
            label="Access key ID"
            value={accessKey()}
            onInput={(e) => setAccessKey(e.currentTarget.value)}
          />
          <Field
            label="Secret access key"
            type="password"
            placeholder={
              cloudConfig()?.secret_key_set ? '•••••••• (unchanged)' : 'secret access key'
            }
            hint="Stored locally; never shown again. Leave blank to keep the current one."
            value={secretKey()}
            onInput={(e) => setSecretKey(e.currentTarget.value)}
          />
          <Field
            label="Keep last N backups"
            type="number"
            min="1"
            value={String(retention())}
            onInput={(e) => setRetention(Number(e.currentTarget.value) || 1)}
          />
        </div>

        <div class="flex flex-wrap items-center gap-2">
          <Button onClick={() => void saveCloud()} loading={savingCloud()} disabled={savingCloud()}>
            Save settings
          </Button>
          <Button
            variant="secondary"
            onClick={() => void backupNow()}
            loading={cloudBackingUp()}
            disabled={!configured() || cloudBackingUp()}
            title={configured() ? 'Sync the database to cloud storage now' : 'Configure cloud storage first'}
          >
            <FiUploadCloud size={16} class="mr-1.5" /> Back up now
          </Button>
          <Button
            variant="secondary"
            onClick={() => void restoreFromCloud()}
            loading={cloudRestoring()}
            disabled={!configured() || cloudRestoring()}
            title="Restore the latest cloud backup (applied on next restart)"
          >
            <FiDownloadCloud size={16} class="mr-1.5" /> Restore latest
          </Button>
          <Button
            variant="secondary"
            onClick={() => void verifyCloud()}
            loading={cloudVerifying()}
            disabled={!configured() || cloudVerifying()}
            title="Download and validate the latest cloud backup without restoring it"
          >
            <FiCheckCircle size={16} class="mr-1.5" /> Verify latest
          </Button>
          <Show when={configured()}>
            <Button
              variant="ghost"
              onClick={() => void refetchBackups()}
              title="Refresh the list of cloud backups"
            >
              <FiRefreshCw size={16} />
            </Button>
          </Show>
        </div>

        {/* Existing cloud backups */}
        <Show when={configured()}>
          <div class="rounded-lg border" style={{ 'border-color': 'var(--color-border-light)' }}>
            <p
              class="px-3 py-2 text-xs font-semibold uppercase tracking-wide border-b"
              style={{ color: 'var(--color-text-tertiary)', 'border-color': 'var(--color-border-light)' }}
            >
              Cloud backups
            </p>
            <Show
              when={(cloudBackups() ?? []).length > 0}
              fallback={
                <p class="px-3 py-3 text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
                  {cloudBackups.loading ? 'Loading…' : 'No cloud backups yet.'}
                </p>
              }
            >
              <ul>
                <For each={cloudBackups()}>
                  {(b) => (
                    <li
                      class="flex items-center justify-between gap-3 px-3 py-2 text-sm border-b last:border-b-0"
                      style={{ 'border-color': 'var(--color-border-light)', color: 'var(--color-text-secondary)' }}
                    >
                      <div class="min-w-0">
                        <div style={{ color: 'var(--color-text-primary)' }}>
                          {new Date(b.created_at).toLocaleString()}
                        </div>
                        <div class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                          {b.item_count} items · {formatBytes(b.bundle_size_bytes)}
                        </div>
                      </div>
                      <div class="flex items-center gap-1">
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={cloudVerifying()}
                          onClick={() => void verifyCloud(b.object_key)}
                        >
                          Verify
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={cloudRestoring()}
                          onClick={() => void restoreFromCloud(b.object_key)}
                        >
                          Restore
                        </Button>
                      </div>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
          </div>
        </Show>

        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          Restoring is staged and applied on the next server restart. Schema newer
          than your running version is rejected.
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
