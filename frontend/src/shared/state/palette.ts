// Palette persistence + application (Tack redesign).
//
// The second theme axis alongside theme.ts (light/dark mode). A palette swaps
// the accent + surface token *values* via a `data-palette` attribute on <html>;
// the token CSS in index.css supplies the per-palette overrides. "harbor"
// (harbor blue + coral) is the default. "teal" is the base token set and
// carries NO attribute.

import { createSignal } from 'solid-js';

export type Palette = 'harbor' | 'teal' | 'clay' | 'graphite';

export const PALETTES: Palette[] = ['harbor', 'teal', 'clay', 'graphite'];

const KEY = 'tack_palette';

export function getStoredPalette(): Palette {
  try {
    const v = localStorage.getItem(KEY);
    if (v === 'harbor' || v === 'teal' || v === 'clay' || v === 'graphite') return v;
  } catch {
    /* localStorage unavailable */
  }
  return 'harbor';
}

const [palette, setPaletteSignal] = createSignal<Palette>(getStoredPalette());

/** Reactive accessor — drives the sidebar swatch control. */
export const currentPalette = palette;

export function applyPalette(p: Palette): void {
  const el = document.documentElement;
  if (p === 'teal') el.removeAttribute('data-palette');
  else el.setAttribute('data-palette', p);
}

export function setPalette(p: Palette): void {
  try {
    localStorage.setItem(KEY, p);
  } catch {
    /* ignore */
  }
  applyPalette(p);
  setPaletteSignal(p);
}

/** Apply the persisted palette on startup. */
export function initPalette(): void {
  applyPalette(getStoredPalette());
}
