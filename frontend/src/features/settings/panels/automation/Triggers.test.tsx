import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import Triggers, { addReason } from './Triggers';
import { daysText, durationText, scheduleText, triggerSentence } from '../../../../shared/agents/triggerText';
import type { Trigger } from '../../../../shared/api/triggers';
import { flush } from '../../../../test/flush';

const disposers: Array<() => void> = [];
const settle = async () => {
  for (let i = 0; i < 8; i += 1) await flush();
};

const profile = { agent_profile_id: 'prof1', name: 'Planner', instructions: '', tool_policy: {}, limits: {} };

const failedRuns: Trigger = {
  id: 't1',
  name: 'Failed runs',
  scope: { kind: 'project', project: 'p1' },
  profile: 'prof1',
  source: { kind: 'event', event: 'run_failed', filter: {} },
  quiet_seconds: 60,
  daily_cap: 5,
  enabled: true,
  created_at: '2026-11-01T00:00:00Z',
  last_fired_at: null,
};
const standUp: Trigger = {
  ...failedRuns,
  id: 't2',
  name: 'Stand-up',
  source: { kind: 'schedule', days: [1, 2, 3, 4, 5], minute_of_day: 540, utc_offset_minutes: 60 },
  quiet_seconds: 0,
  daily_cap: 1,
};

function mount(triggers: Trigger[]) {
  const calls: Array<{ url: string; method: string; body?: string }> = [];
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? 'GET';
    calls.push({ url, method, body: init?.body as string | undefined });
    const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } });
    if (url.includes('/triggers') && method === 'GET') return json(triggers);
    if (url.includes('/triggers')) return json(triggers[0]);
    if (url.includes('/agent-credentials')) return json([]);
    return json({});
  }) as typeof fetch);
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <Triggers projectId="p1" profiles={() => [profile]} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return { c: container, calls };
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
});

const q = (c: HTMLElement, id: string) => c.querySelector<HTMLElement>(`[data-testid="${id}"]`);
const byLabel = (c: HTMLElement, label: string) => c.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!;

describe('Triggers', () => {
  it('says there are none and offers to add one', async () => {
    const { c } = mount([]);
    await settle();
    expect(q(c, 'triggers-empty')?.textContent).toContain('No triggers');
    expect(q(c, 'trigger-add-open')).not.toBeNull();
  });

  it('lists each trigger in plain words, with the profile it wakes', async () => {
    const { c } = mount([failedRuns, standUp]);
    await settle();
    const items = c.querySelectorAll('[data-testid="trigger"]');
    expect(items).toHaveLength(2);
    expect(items[0].textContent).toContain('wakes Planner');
    expect(items[0].textContent).toContain('A run fails, once things stay quiet for 1 minute; at most 5 runs a day.');
    expect(items[1].textContent).toContain('Every weekday at 09:00 (UTC+01:00); at most 1 run a day.');
    expect(items[1].textContent).not.toContain('quiet');
    expect(items[0].textContent).toContain('Has not run yet');
  });

  it('turns a trigger off with one request', async () => {
    const { c, calls } = mount([failedRuns]);
    await settle();
    const on = c.querySelector<HTMLInputElement>('input[role="switch"]')!;
    on.checked = false;
    on.dispatchEvent(new Event('change', { bubbles: true }));
    await settle();
    const patch = calls.find((x) => x.method === 'PATCH')!;
    expect(patch.url).toContain('/triggers/t1');
    expect(JSON.parse(patch.body!)).toEqual({ enabled: false });
  });

  it('saves the quiet period and the daily cap as typed, within the limits', async () => {
    const { c, calls } = mount([failedRuns]);
    await settle();
    const quiet = byLabel(c, 'Quiet seconds for Failed runs');
    quiet.value = '9999';
    quiet.dispatchEvent(new Event('change', { bubbles: true }));
    await settle();
    const cap = byLabel(c, 'Daily cap for Failed runs');
    cap.value = '0';
    cap.dispatchEvent(new Event('change', { bubbles: true }));
    await settle();
    const bodies = calls.filter((x) => x.method === 'PATCH').map((x) => JSON.parse(x.body!));
    expect(bodies).toEqual([{ quiet_seconds: 3600 }, { daily_cap: 1 }]);
  });

  it('edits the weekdays and the time of a schedule, and keeps at least one weekday', async () => {
    const { c, calls } = mount([standUp]);
    await settle();
    expect(q(c, 'trigger-schedule')).not.toBeNull();
    const friday = byLabel(c, 'Fri for Stand-up');
    friday.checked = false;
    friday.dispatchEvent(new Event('change', { bubbles: true }));
    await settle();
    const time = byLabel(c, 'Time for Stand-up');
    time.value = '10:30';
    time.dispatchEvent(new Event('change', { bubbles: true }));
    await settle();
    const bodies = calls.filter((x) => x.method === 'PATCH').map((x) => JSON.parse(x.body!));
    expect(bodies[0].source.days).toEqual([1, 2, 3, 4]);
    expect(bodies[1].source.minute_of_day).toBe(630);
    expect(bodies[1].source.utc_offset_minutes).toBe(60);
  });

  it('adds a trigger from the form, and shows deferred events disabled with their reason', async () => {
    const { c, calls } = mount([]);
    await settle();
    q(c, 'trigger-add-open')!.click();
    await settle();
    const deferred = [...c.querySelectorAll<HTMLOptionElement>('option[disabled]')].map((o) => o.textContent);
    expect(deferred.some((t) => t?.includes('A message in a thread') && t.includes('already starts'))).toBe(true);
    expect(deferred.some((t) => t?.includes("A team's activity"))).toBe(true);

    const submit = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Add trigger')!;
    expect(submit.disabled).toBe(true);
    const name = c.querySelector<HTMLInputElement>('input[placeholder="Failed runs"]')!;
    name.value = 'Failures';
    name.dispatchEvent(new Event('input', { bubbles: true }));
    await settle();
    expect(submit.disabled).toBe(false);
    submit.click();
    await settle();
    const post = calls.find((x) => x.method === 'POST')!;
    expect(JSON.parse(post.body!)).toEqual({
      project_id: 'p1',
      profile: 'prof1',
      name: 'Failures',
      source: { kind: 'event', event: 'run_failed', filter: {} },
      quiet_seconds: 60,
      daily_cap: 5,
    });
  });
});

describe('trigger words and rules', () => {
  it('names why a trigger cannot be added yet', () => {
    const ok = { name: 'x', profile: 'p', kind: 'event', days: [1], time: '09:00', afterMinutes: 60, event: 'run_failed' };
    expect(addReason(ok)).toBeUndefined();
    expect(addReason({ ...ok, profile: '' })).toContain('profile');
    expect(addReason({ ...ok, name: ' ' })).toContain('name');
    expect(addReason({ ...ok, kind: 'schedule', days: [] })).toContain('weekday');
    expect(addReason({ ...ok, kind: 'schedule', time: '' })).toContain('time');
    expect(addReason({ ...ok, event: 'change_set_waiting', afterMinutes: 0 })).toContain('minutes');
  });

  it('writes durations and days the way a person says them', () => {
    expect(durationText(90)).toBe('90 seconds');
    expect(durationText(3600)).toBe('1 hour');
    expect(durationText(7200)).toBe('2 hours');
    expect(daysText([1, 2, 3, 4, 5])).toBe('every weekday');
    expect(daysText([3, 1])).toBe('on Mon, Wed');
    expect(scheduleText({ days: [1], minute_of_day: 65, utc_offset_minutes: -300 })).toBe('on Mon at 01:05 (UTC-05:00)');
    expect(
      triggerSentence({ ...failedRuns, source: { kind: 'event', event: 'change_set_waiting', filter: { after_seconds: 3600 } }, quiet_seconds: 0 }),
    ).toBe('A change set has waited for you more than 1 hour; at most 5 runs a day.');
  });
});
