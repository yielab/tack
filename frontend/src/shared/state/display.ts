// Display preferences: text size, interface zoom, a high-legibility typeface
// and reduced motion. Like theme.ts and palette.ts they live in this browser's
// localStorage and are applied to <html>, where index.css reads them.
//
// Text size sets the root font size as a percentage of the browser's own
// default, so a user who already raised that default keeps it, and every
// rem-based size in the app follows. Zoom scales the whole interface; a
// browser tab already has its own (Ctrl + / Ctrl −), so the setting drives
// only the desktop app's webview.

import { createSignal } from 'solid-js';

export const TEXT_SIZES = [0.875, 1, 1.125, 1.25, 1.5, 1.75, 2] as const;
export const ZOOM_LEVELS = [0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2] as const;

export type Motion = 'system' | 'reduce';

export interface Display {
  textSize: number;
  zoom: number;
  legibleFont: boolean;
  motion: Motion;
}

export const DEFAULT_DISPLAY: Display = { textSize: 1, zoom: 1, legibleFont: false, motion: 'system' };

const KEY = 'tack_display';

const inDesktop = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/** Whether the zoom setting can act here (the desktop app), or the browser's own zoom applies. */
export const zoomSupported = inDesktop;

function oneOf(v: unknown, allowed: readonly number[], fallback: number): number {
  return typeof v === 'number' && allowed.includes(v) ? v : fallback;
}

export function getStoredDisplay(): Display {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) {
      const v = JSON.parse(raw) as Partial<Display>;
      return {
        textSize: oneOf(v.textSize, TEXT_SIZES, DEFAULT_DISPLAY.textSize),
        zoom: oneOf(v.zoom, ZOOM_LEVELS, DEFAULT_DISPLAY.zoom),
        legibleFont: v.legibleFont === true,
        motion: v.motion === 'reduce' ? 'reduce' : 'system',
      };
    }
  } catch {
    /* localStorage unavailable or the value is not JSON */
  }
  return { ...DEFAULT_DISPLAY };
}

const [display, setDisplaySignal] = createSignal<Display>(getStoredDisplay());

/** Reactive accessor — drives Settings → Display & accessibility. */
export const currentDisplay = display;

async function applyZoom(zoom: number): Promise<void> {
  if (!inDesktop()) return;
  try {
    const { getCurrentWebview } = await import('@tauri-apps/api/webview');
    await getCurrentWebview().setZoom(zoom);
  } catch {
    /* an older desktop build without the zoom permission: the hotkeys still work */
  }
}

export function applyDisplay(d: Display): void {
  const el = document.documentElement;
  el.style.fontSize = d.textSize === 1 ? '' : `${d.textSize * 100}%`;
  if (d.legibleFont) el.setAttribute('data-font', 'legible');
  else el.removeAttribute('data-font');
  if (d.motion === 'reduce') el.setAttribute('data-motion', 'reduce');
  else el.removeAttribute('data-motion');
  void applyZoom(d.zoom);
}

export function setDisplay(patch: Partial<Display>): void {
  const next = { ...display(), ...patch };
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    /* ignore */
  }
  applyDisplay(next);
  setDisplaySignal(next);
}

export function resetDisplay(): void {
  setDisplay({ ...DEFAULT_DISPLAY });
}

/** Apply the persisted display preferences on startup. */
export function initDisplay(): void {
  applyDisplay(getStoredDisplay());
}
