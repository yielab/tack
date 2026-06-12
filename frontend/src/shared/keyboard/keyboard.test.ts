import { describe, it, expect, vi, beforeEach } from 'vitest';
import { KeyboardManager, formatShortcut } from './keyboard';
import type { KeyboardShortcut } from './keyboard';

function shortcut(overrides: Partial<KeyboardShortcut> = {}): KeyboardShortcut {
  return {
    key: 'k',
    description: 'Test shortcut',
    action: vi.fn(),
    ...overrides,
  };
}

describe('KeyboardManager — register / getShortcuts', () => {
  let km: KeyboardManager;

  beforeEach(() => {
    km = new KeyboardManager();
  });

  it('starts with empty shortcut lists for all contexts', () => {
    expect(km.getShortcuts('global')).toHaveLength(0);
    expect(km.getShortcuts('board')).toHaveLength(0);
    expect(km.getShortcuts('modal')).toHaveLength(0);
  });

  it('registered shortcut appears in the correct context', () => {
    km.register('global', shortcut({ key: 'k', ctrl: true, description: 'Open palette' }));
    const shortcuts = km.getShortcuts('global');
    expect(shortcuts).toHaveLength(1);
    expect(shortcuts[0].key).toBe('k');
    expect(shortcuts[0].ctrl).toBe(true);
  });

  it('registering in one context does not pollute another', () => {
    km.register('board', shortcut({ key: 'n' }));
    expect(km.getShortcuts('board')).toHaveLength(1);
    expect(km.getShortcuts('global')).toHaveLength(0);
    expect(km.getShortcuts('modal')).toHaveLength(0);
  });

  it('re-registering the same key combination overwrites the previous entry', () => {
    const first = vi.fn();
    const second = vi.fn();
    km.register('global', shortcut({ key: 'x', action: first }));
    km.register('global', shortcut({ key: 'x', action: second }));
    expect(km.getShortcuts('global')).toHaveLength(1);
    expect(km.getShortcuts('global')[0].action).toBe(second);
  });
});

describe('KeyboardManager — unregister', () => {
  let km: KeyboardManager;

  beforeEach(() => {
    km = new KeyboardManager();
  });

  it('removes a shortcut that was registered', () => {
    km.register('global', shortcut({ key: 's', ctrl: true }));
    km.unregister('global', { key: 's', ctrl: true });
    expect(km.getShortcuts('global')).toHaveLength(0);
  });

  it('is a no-op for a shortcut that was never registered', () => {
    expect(() => km.unregister('global', { key: 'z' })).not.toThrow();
  });
});

describe('KeyboardManager — getAllShortcuts', () => {
  it('returns an object keyed by context', () => {
    const km = new KeyboardManager();
    km.register('global', shortcut({ key: 'g' }));
    km.register('board', shortcut({ key: 'b' }));
    const all = km.getAllShortcuts();
    expect(all.global).toHaveLength(1);
    expect(all.board).toHaveLength(1);
    expect(all.modal).toHaveLength(0);
  });
});

describe('formatShortcut', () => {
  it('formats a Ctrl+K shortcut on a non-Mac platform', () => {
    vi.spyOn(navigator, 'platform', 'get').mockReturnValue('Win32');
    const sc = shortcut({ key: 'k', ctrl: true });
    expect(formatShortcut(sc)).toBe('Ctrl+K');
  });

  it('formats a plain letter shortcut with no modifiers', () => {
    vi.spyOn(navigator, 'platform', 'get').mockReturnValue('Win32');
    expect(formatShortcut(shortcut({ key: 'n' }))).toBe('N');
  });

  it('formats Escape as "Esc"', () => {
    vi.spyOn(navigator, 'platform', 'get').mockReturnValue('Win32');
    expect(formatShortcut(shortcut({ key: 'Escape' }))).toBe('Esc');
  });

  it('formats Ctrl+Shift+S on Windows', () => {
    vi.spyOn(navigator, 'platform', 'get').mockReturnValue('Win32');
    const sc = shortcut({ key: 's', ctrl: true, shift: true });
    expect(formatShortcut(sc)).toBe('Ctrl+Shift+S');
  });
});
