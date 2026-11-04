import { createSignal } from 'solid-js';
import { inDesktop } from './runWithAgent/desktop';

// The one place that tells a person something outside the page: the browser's own notification, or
// the system's through the desktop app, sent only while the tab is hidden and only when the person
// turned it on.

const STORAGE_KEY = 'tack_notify_when_hidden';

const supported = (): boolean => inDesktop() || typeof Notification !== 'undefined';

// Loaded on demand: the browser build never needs the plugin.
const desktopPlugin = () => import('@tauri-apps/plugin-notification');

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
const [permission, setPermission] = createSignal<NotificationPermission>(
  inDesktop() ? 'default' : supported() ? Notification.permission : 'denied',
);

// The system answers asynchronously, so a choice made in an earlier session waits for it.
const systemAnswered: Promise<void> = inDesktop()
  ? desktopPlugin()
      .then(({ isPermissionGranted }) => isPermissionGranted())
      .then((granted) => void setPermission(granted ? 'granted' : 'default'))
      .catch(() => void setPermission('denied'))
  : Promise.resolve();

/** Why the switch cannot be turned on, or `null` when it can. */
export function notifyUnavailableReason(): string | null {
  if (!supported()) return 'This browser has no notifications.';
  if (permission() === 'denied') {
    return inDesktop()
      ? "The system blocked notifications for Tack. Allow them in the system's notification settings."
      : 'The browser blocked notifications for this site. Allow them in its site settings.';
  }
  return null;
}

/** Whether the person asked to be notified while the tab is hidden, and the browser lets us. */
export const notifyWhenHidden = (): boolean => enabled() && supported() && permission() === 'granted';

/** Turn it on: asks the browser for permission, and stays off when the person declines. Resolves to whether it is on. */
export async function enableNotifyWhenHidden(): Promise<boolean> {
  await systemAnswered;
  if (notifyUnavailableReason()) return false;
  const answer = permission() === 'granted' ? 'granted' : await requestPermission();
  setPermission(answer);
  const on = answer === 'granted';
  writeStored(on);
  setEnabled(on);
  return on;
}

async function requestPermission(): Promise<NotificationPermission> {
  if (!inDesktop()) return Notification.requestPermission();
  try {
    return (await (await desktopPlugin()).requestPermission()) as NotificationPermission;
  } catch {
    return 'denied';
  }
}

export function disableNotifyWhenHidden(): void {
  writeStored(false);
  setEnabled(false);
}

/** Tell the person something needs them: a notification when the tab is hidden and they turned it on. */
export function notifyNeedsYou(title: string): void {
  if (!document.hidden || !notifyWhenHidden()) return;
  if (inDesktop()) {
    desktopPlugin()
      .then(({ sendNotification }) => sendNotification({ title }))
      .catch(() => {
        /* the toast and the title still tell */
      });
    return;
  }
  try {
    new Notification(title);
  } catch {
    /* some browsers refuse to construct one outside a service worker; the toast and the title still tell */
  }
}
