// Shared open-state for the global command palette, so any surface (sidebar
// search pill, top-bar search button, Ctrl+K) can open it while Layout owns the
// single CommandPalette instance.

import { createSignal } from 'solid-js';

const [isOpen, setIsOpen] = createSignal(false);

export const paletteOpen = isOpen;
export const openPalette = () => setIsOpen(true);
export const closePalette = () => setIsOpen(false);
