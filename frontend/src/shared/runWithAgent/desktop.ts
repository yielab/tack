/** The desktop app's window, as opposed to a browser tab on the same server. */
export const inDesktop = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

