import type { Trigger, TriggerEvent, TriggerSchedule, TriggerSource } from '../api/triggers';

/** The events a trigger can wait for, in the words a person reads. */
export const EVENT_CHOICES: ReadonlyArray<{ value: TriggerEvent; label: string }> = [
  { value: 'run_failed', label: 'A run fails' },
  { value: 'run_waiting', label: 'A run waits for you' },
  { value: 'run_finished', label: 'A run finishes' },
  { value: 'run_started', label: 'A run starts' },
  { value: 'decision_opened', label: 'A question is asked of you' },
  { value: 'decision_resolved', label: 'A question is answered' },
  { value: 'change_set_waiting', label: 'A change set waits for you too long' },
  { value: 'item_created', label: 'A task is created' },
  { value: 'item_moved', label: 'A task moves to another status' },
  { value: 'item_updated', label: 'A task is edited' },
  { value: 'item_deleted', label: 'A task is deleted' },
];

/** What a trigger could wait for but cannot yet: shown disabled, never hidden, each with its reason. */
export const DEFERRED_EVENTS: ReadonlyArray<{ label: string; reason: string }> = [
  { label: 'A message in a thread', reason: 'Writing in a thread already starts its profile, so no trigger is needed' },
  { label: "A team's activity", reason: "Tack does not announce a team's activity as an event yet" },
];

export const WEEKDAYS: ReadonlyArray<{ day: number; short: string }> = [
  { day: 1, short: 'Mon' },
  { day: 2, short: 'Tue' },
  { day: 3, short: 'Wed' },
  { day: 4, short: 'Thu' },
  { day: 5, short: 'Fri' },
  { day: 6, short: 'Sat' },
  { day: 7, short: 'Sun' },
];

const pad = (n: number) => String(n).padStart(2, '0');

/** "09:00" for a minute of the day. */
export function timeText(minuteOfDay: number): string {
  return `${pad(Math.floor(minuteOfDay / 60))}:${pad(minuteOfDay % 60)}`;
}

/** The minute of the day an `<input type="time">` value names, or null for an empty or odd one. */
export function minuteOf(time: string): number | null {
  const m = /^(\d{1,2}):(\d{2})$/.exec(time);
  if (!m) return null;
  const minute = Number(m[1]) * 60 + Number(m[2]);
  return Number(m[1]) < 24 && Number(m[2]) < 60 ? minute : null;
}

/** "UTC+01:00" for an offset in minutes. */
export function zoneText(offsetMinutes: number): string {
  const sign = offsetMinutes < 0 ? '-' : '+';
  const abs = Math.abs(offsetMinutes);
  return `UTC${sign}${pad(Math.floor(abs / 60))}:${pad(abs % 60)}`;
}

/** Days as a short phrase: "every weekday", "on Mon, Wed". */
export function daysText(days: readonly number[]): string {
  const set = [...new Set(days)].sort((a, b) => a - b);
  if (set.join() === '1,2,3,4,5') return 'every weekday';
  if (set.length === 7) return 'every day';
  return `on ${set.map((d) => WEEKDAYS.find((w) => w.day === d)?.short ?? String(d)).join(', ')}`;
}

export function scheduleText(s: TriggerSchedule): string {
  return `${daysText(s.days)} at ${timeText(s.minute_of_day)} (${zoneText(s.utc_offset_minutes ?? 0)})`;
}

function eventText(source: Extract<TriggerSource, { kind: 'event' }>): string {
  const base = EVENT_CHOICES.find((e) => e.value === source.event)?.label ?? source.event;
  const filter = source.filter;
  if (source.event === 'change_set_waiting' && filter?.after_seconds) {
    return `A change set has waited for you more than ${durationText(filter.after_seconds)}`;
  }
  if (filter?.status) return `${base} (in ${filter.status})`;
  return base;
}

/** A length of time in the largest whole unit: "90 seconds", "1 hour", "2 days". */
export function durationText(seconds: number): string {
  const unit = (n: number, name: string) => `${n} ${name}${n === 1 ? '' : 's'}`;
  if (seconds % 86_400 === 0 && seconds >= 86_400) return unit(seconds / 86_400, 'day');
  if (seconds % 3_600 === 0 && seconds >= 3_600) return unit(seconds / 3_600, 'hour');
  if (seconds % 60 === 0 && seconds >= 60) return unit(seconds / 60, 'minute');
  return unit(seconds, 'second');
}

const capitalize = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

/** One sentence for what wakes the profile and how often. */
export function triggerSentence(t: Trigger): string {
  const when = t.source.kind === 'schedule' ? capitalize(scheduleText(t.source)) : eventText(t.source);
  const quiet =
    t.source.kind === 'event' && t.quiet_seconds > 0 ? `, once things stay quiet for ${durationText(t.quiet_seconds)}` : '';
  return `${when}${quiet}; at most ${t.daily_cap} run${t.daily_cap === 1 ? '' : 's'} a day.`;
}
