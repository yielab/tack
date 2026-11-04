import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

type Perm = 'default' | 'granted' | 'denied';

// The module keeps its choice in module state, so each test loads it afresh.
async function load(permission: Perm, request: Perm = permission) {
  const made: string[] = [];
  class FakeNotification {
    static permission: Perm = permission;
    static requestPermission = vi.fn(async () => {
      FakeNotification.permission = request;
      return request;
    });
    constructor(title: string) {
      made.push(title);
    }
  }
  vi.stubGlobal('Notification', FakeNotification);
  vi.resetModules();
  const mod = await import('./notify');
  return { mod, made, FakeNotification };
}

const hidden = (value: boolean) => Object.defineProperty(document, 'hidden', { value, configurable: true });

beforeEach(() => localStorage.clear());
afterEach(() => {
  hidden(false);
  vi.unstubAllGlobals();
});

describe('notify', () => {
  it('turning it on asks the browser, remembers the choice, and a needs-you in a hidden tab is sent', async () => {
    const { mod, made, FakeNotification } = await load('default', 'granted');
    expect(mod.notifyWhenHidden()).toBe(false);
    expect(await mod.enableNotifyWhenHidden()).toBe(true);
    expect(FakeNotification.requestPermission).toHaveBeenCalledOnce();
    expect(mod.notifyWhenHidden()).toBe(true);
    hidden(true);
    mod.notifyNeedsYou('A run failed');
    expect(made).toEqual(['A run failed']);
  });

  it('sends nothing while the tab is visible, or when it is off', async () => {
    const { mod, made } = await load('granted');
    hidden(true);
    mod.notifyNeedsYou('off');
    await mod.enableNotifyWhenHidden();
    hidden(false);
    mod.notifyNeedsYou('visible');
    expect(made).toEqual([]);
  });

  it('stays off when the browser says no, and a blocked site cannot be turned on at all', async () => {
    const { mod } = await load('default', 'denied');
    expect(await mod.enableNotifyWhenHidden()).toBe(false);
    expect(mod.notifyWhenHidden()).toBe(false);
    expect(mod.notifyUnavailableReason()).toContain('blocked');
    expect(await mod.enableNotifyWhenHidden()).toBe(false);
  });

  it('turning it off sends nothing more', async () => {
    const { mod, made } = await load('granted');
    await mod.enableNotifyWhenHidden();
    mod.disableNotifyWhenHidden();
    hidden(true);
    mod.notifyNeedsYou('x');
    expect(made).toEqual([]);
  });

  it('a browser without notifications says so', async () => {
    vi.stubGlobal('Notification', undefined);
    vi.resetModules();
    const mod = await import('./notify');
    expect(mod.notifyUnavailableReason()).toBe('This browser has no notifications.');
    expect(() => mod.notifyNeedsYou('x')).not.toThrow();
  });
});

describe('notify in the desktop app', () => {
  afterEach(() => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    vi.doUnmock('@tauri-apps/plugin-notification');
  });

  async function loadDesktop(granted: boolean, request: string = 'granted') {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    const plugin = {
      isPermissionGranted: vi.fn(async () => granted),
      requestPermission: vi.fn(async () => request),
      sendNotification: vi.fn(),
    };
    vi.doMock('@tauri-apps/plugin-notification', () => plugin);
    const browserMade: string[] = [];
    class FakeNotification {
      static permission: Perm = 'default';
      static requestPermission = vi.fn(async () => 'denied' as Perm);
      constructor(title: string) {
        browserMade.push(title);
      }
    }
    vi.stubGlobal('Notification', FakeNotification);
    vi.resetModules();
    const mod = await import('./notify');
    return { mod, plugin, browserMade };
  }

  it('asks the system through the plugin and sends through it, never the browser path', async () => {
    const { mod, plugin, browserMade } = await loadDesktop(false);
    expect(await mod.enableNotifyWhenHidden()).toBe(true);
    expect(plugin.requestPermission).toHaveBeenCalledOnce();
    hidden(true);
    mod.notifyNeedsYou('A run failed');
    await vi.waitFor(() => expect(plugin.sendNotification).toHaveBeenCalledWith({ title: 'A run failed' }));
    expect(browserMade).toEqual([]);
  });

  it('remembers a permission the system already gave', async () => {
    const { mod, plugin } = await loadDesktop(true);
    await mod.enableNotifyWhenHidden();
    await vi.waitFor(() => expect(mod.notifyWhenHidden()).toBe(true));
    expect(plugin.requestPermission).not.toHaveBeenCalled();
  });

  it('stays off when the system says no', async () => {
    const { mod, plugin } = await loadDesktop(false, 'denied');
    expect(await mod.enableNotifyWhenHidden()).toBe(false);
    hidden(true);
    mod.notifyNeedsYou('x');
    await Promise.resolve();
    expect(plugin.sendNotification).not.toHaveBeenCalled();
    expect(mod.notifyUnavailableReason()).toContain('system');
  });
});
