import { createSignal } from 'solid-js';

// The one place that tells a person something outside the page. Today that is the browser's own
// notification, sent only while the tab is hidden and only when the person turned it on.

const STORAGE_KEY = 'tack_notify_when_hidden';

const supported = (): boolean => typeof Notification !== 'undefined';

function readStored(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === '1';
  } catch {
    return false;
  }
}

function writeStored(on: boolean): void {
  try {
    if (on) localStorage.setItem(STORAGE_KEY, '1');
    else localStorage.removeItem(STORAGE_KEY);
  } catch {
    /* ignore — the choice then lasts until the page is closed */
  }
}

const [enabled, setEnabled] = createSignal(readStored());
const [permission, setPermission] = createSignal<NotificationPermission>(supported() ? Notification.permission : 'denied');

/** Why the switch cannot be turned on, or `null` when it can. */
export function notifyUnavailableReason(): string | null {
  if (!supported()) return 'This browser has no notifications.';
  if (permission() === 'denied') return 'The browser blocked notifications for this site. Allow them in its site settings.';
  return null;
}

/** Whether the person asked to be notified while the tab is hidden, and the browser lets us. */
export const notifyWhenHidden = (): boolean => enabled() && supported() && permission() === 'granted';

/** Turn it on: asks the browser for permission, and stays off when the person declines. Resolves to whether it is on. */
export async function enableNotifyWhenHidden(): Promise<boolean> {
  if (notifyUnavailableReason()) return false;
  const answer = permission() === 'granted' ? 'granted' : await Notification.requestPermission();
  setPermission(answer);
  const on = answer === 'granted';
  writeStored(on);
  setEnabled(on);
  return on;
}

export function disableNotifyWhenHidden(): void {
  writeStored(false);
  setEnabled(false);
}

/** Tell the person something needs them: a browser notification when the tab is hidden and they turned it on. */
export function notifyNeedsYou(title: string): void {
  if (!document.hidden || !notifyWhenHidden()) return;
  try {
    new Notification(title);
  } catch {
    /* some browsers refuse to construct one outside a service worker; the toast and the title still tell */
  }
}
