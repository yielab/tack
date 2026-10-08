/** The desktop app's window, as opposed to a browser tab on the same server. */
export const inDesktop = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/** Opens a folder or file with the system's own app (desktop only). Rejects with the
 *  shell's reason, so the caller can say why instead of doing nothing. */
export async function openPath(path: string): Promise<void> {
  const { open } = await import('@tauri-apps/plugin-shell');
  await open(path);
}
