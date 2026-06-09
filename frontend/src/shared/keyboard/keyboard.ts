import { onCleanup, createEffect } from 'solid-js';

export type KeyboardShortcut = {
  key: string;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  meta?: boolean;
  description: string;
  action: () => void;
};

export type ShortcutContext = 'global' | 'board' | 'modal';

/**
 * Keyboard shortcuts manager
 */
export class KeyboardManager {
  private shortcuts: Map<string, Map<string, KeyboardShortcut>> = new Map();
  private listener: ((e: KeyboardEvent) => void) | null = null;

  constructor() {
    // Initialize context maps
    this.shortcuts.set('global', new Map());
    this.shortcuts.set('board', new Map());
    this.shortcuts.set('modal', new Map());
  }

  /**
   * Register a keyboard shortcut
   */
  register(context: ShortcutContext, shortcut: KeyboardShortcut) {
    const key = this.createKey(shortcut);
    const contextMap = this.shortcuts.get(context);
    if (contextMap) {
      contextMap.set(key, shortcut);
    }
  }

  /**
   * Unregister a keyboard shortcut
   */
  unregister(context: ShortcutContext, shortcut: Omit<KeyboardShortcut, 'description' | 'action'>) {
    const key = this.createKey(shortcut);
    const contextMap = this.shortcuts.get(context);
    if (contextMap) {
      contextMap.delete(key);
    }
  }

  /**
   * Create a unique key for a shortcut
   */
  private createKey(shortcut: Pick<KeyboardShortcut, 'key' | 'ctrl' | 'shift' | 'alt' | 'meta'>): string {
    const parts: string[] = [];
    if (shortcut.ctrl) parts.push('ctrl');
    if (shortcut.shift) parts.push('shift');
    if (shortcut.alt) parts.push('alt');
    if (shortcut.meta) parts.push('meta');
    parts.push(shortcut.key.toLowerCase());
    return parts.join('+');
  }

  /**
   * Check if an event matches a shortcut
   */
  private matchesShortcut(event: KeyboardEvent, shortcut: KeyboardShortcut): boolean {
    const keyMatches = event.key.toLowerCase() === shortcut.key.toLowerCase();
    const ctrlMatches = !!shortcut.ctrl === (event.ctrlKey || event.metaKey); // meta for Mac
    const shiftMatches = !!shortcut.shift === event.shiftKey;
    const altMatches = !!shortcut.alt === event.altKey;

    return keyMatches && ctrlMatches && shiftMatches && altMatches;
  }

  /**
   * Start listening for keyboard events
   */
  start(activeContext: () => ShortcutContext = () => 'global') {
    this.listener = (event: KeyboardEvent) => {
      // Don't trigger shortcuts when typing in inputs
      const target = event.target as HTMLElement;
      const isInput = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable;

      // Allow Ctrl+K and Escape even in inputs
      const isAllowedInInput = (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k';
      const isEscape = event.key === 'Escape';

      if (isInput && !isAllowedInInput && !isEscape) {
        return;
      }

      // Get current context
      const context = activeContext();
      const contextMap = this.shortcuts.get(context);
      const globalMap = this.shortcuts.get('global');

      // Try context-specific shortcuts first
      if (contextMap) {
        for (const shortcut of contextMap.values()) {
          if (this.matchesShortcut(event, shortcut)) {
            event.preventDefault();
            shortcut.action();
            return;
          }
        }
      }

      // Fall back to global shortcuts
      if (globalMap && context !== 'global') {
        for (const shortcut of globalMap.values()) {
          if (this.matchesShortcut(event, shortcut)) {
            event.preventDefault();
            shortcut.action();
            return;
          }
        }
      }
    };

    window.addEventListener('keydown', this.listener);
  }

  /**
   * Stop listening for keyboard events
   */
  stop() {
    if (this.listener) {
      window.removeEventListener('keydown', this.listener);
      this.listener = null;
    }
  }

  /**
   * Get all shortcuts for a context
   */
  getShortcuts(context: ShortcutContext): KeyboardShortcut[] {
    const contextMap = this.shortcuts.get(context);
    return contextMap ? Array.from(contextMap.values()) : [];
  }

  /**
   * Get all shortcuts across all contexts
   */
  getAllShortcuts(): Record<ShortcutContext, KeyboardShortcut[]> {
    return {
      global: this.getShortcuts('global'),
      board: this.getShortcuts('board'),
      modal: this.getShortcuts('modal'),
    };
  }
}

/**
 * Global keyboard manager instance
 */
export const keyboardManager = new KeyboardManager();

/**
 * SolidJS hook for keyboard shortcuts
 */
export function useKeyboard(activeContext: () => ShortcutContext = () => 'global') {
  createEffect(() => {
    keyboardManager.start(activeContext);
  });

  onCleanup(() => {
    keyboardManager.stop();
  });

  return keyboardManager;
}

/**
 * Format a shortcut for display
 */
export function formatShortcut(shortcut: KeyboardShortcut): string {
  const parts: string[] = [];

  // Detect Mac vs Windows/Linux
  const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;

  if (shortcut.ctrl) {
    parts.push(isMac ? '⌘' : 'Ctrl');
  }
  if (shortcut.shift) {
    parts.push(isMac ? '⇧' : 'Shift');
  }
  if (shortcut.alt) {
    parts.push(isMac ? '⌥' : 'Alt');
  }

  // Format key nicely
  const key = shortcut.key.toUpperCase();
  parts.push(key === 'ESCAPE' ? 'Esc' : key);

  return parts.join(isMac ? '' : '+');
}
