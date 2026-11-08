import { type Accessor, type Component, For, Show, createResource, createSignal } from 'solid-js';
import { api } from '../../../../shared/api';
import type { CreateTrigger, Trigger, TriggerEvent, TriggerSource, UpdateTrigger } from '../../../../shared/api/triggers';
import type { AgentProfileSummary } from '../../../../shared/execution';
import { relativeTimeFromIso } from '../../../../shared/runWithAgent/shared';
import { toast } from '../../../../shared/ui/toast';
import { Button, Field, LoadError, Select, Switch, createConfirm } from '../../../../shared/ui';
import { credentialState } from '../../CredentialsSettings';
import { Block, Heading, Note, inputStyle } from './parts';
import { DEFERRED_EVENTS, EVENT_CHOICES, WEEKDAYS, minuteOf, scheduleText, timeText, triggerSentence, zoneText } from '../../../../shared/agents/triggerText';

const SCHEDULE = 'schedule';

/** The largest quiet period and daily cap the server accepts. */
const MAX_QUIET_SECONDS = 3_600;
const MAX_DAILY_CAP = 100;

const clamp = (raw: string, min: number, max: number) => Math.min(max, Math.max(min, Math.trunc(Number(raw) || 0)));

/** The browser's own zone as minutes east of UTC, which a new schedule is written in. */
const localOffsetMinutes = () => -new Date().getTimezoneOffset();

/** Why a new trigger cannot be added yet, or undefined when it can. */
export function addReason(args: {
  name: string;
  profile: string;
  kind: string;
  days: number[];
  time: string;
  afterMinutes: number;
  event: string;
}): string | undefined {
  if (!args.profile) return 'Choose the profile it wakes';
  if (!args.name.trim()) return 'Give the trigger a name';
  if (args.kind === SCHEDULE) {
    if (args.days.length === 0) return 'Choose at least one weekday';
    if (minuteOf(args.time) === null) return 'Choose a time of day';
  } else if (args.event === 'change_set_waiting' && args.afterMinutes < 1) {
    return 'Say how many minutes a change set waits before it counts';
  }
  return undefined;
}

/**
 * Triggers: what wakes an agent profile on its own. Each is an event (or a time of day), a profile, a quiet
 * period (a burst of events starts one run that reads them all) and a cap of runs a day. An event the profile's
 * own credential caused never wakes it. Everything saves at once.
 */
const Triggers: Component<{ projectId: string; profiles: Accessor<AgentProfileSummary[]> }> = (props) => {
  const [list, { refetch }] = createResource(
    () => props.projectId,
    (id) => api.triggers.list(id),
  );
  const [credentials] = createResource(() => api.credentials.list().catch(() => []));
  const [ask, ConfirmHost] = createConfirm();
  const rows = (): Trigger[] => (list.error || !Array.isArray(list()) ? [] : list()!);
  const profileName = (id: string) => props.profiles().find((p) => p.agent_profile_id === id)?.name ?? id;

  const [busy, setBusy] = createSignal<string | null>(null);
  const patch = async (t: Trigger, change: UpdateTrigger) => {
    setBusy(t.id);
    try {
      await api.triggers.update(t.id, change);
      await refetch();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Could not save the trigger');
      await refetch();
    } finally {
      setBusy(null);
    }
  };
  const remove = async (t: Trigger) => {
    if (!(await ask({ title: `Delete ${t.name}?`, message: 'It stops waking the profile. Runs it already started go on.', confirmLabel: 'Delete', danger: true }))) return;
    try {
      await api.triggers.remove(t.id);
      await refetch();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Could not delete the trigger');
    }
  };

  // ── the form that adds one
  const [adding, setAdding] = createSignal(false);
  const [name, setName] = createSignal('');
  const [kind, setKind] = createSignal<'event' | 'schedule'>('event');
  const [event, setEvent] = createSignal<TriggerEvent>('run_failed');
  const [statusFilter, setStatusFilter] = createSignal('');
  const [afterMinutes, setAfterMinutes] = createSignal(60);
  const [days, setDays] = createSignal<number[]>([1, 2, 3, 4, 5]);
  const [time, setTime] = createSignal('09:00');
  const [profile, setProfile] = createSignal('');
  const [credential, setCredential] = createSignal('');
  const [quiet, setQuiet] = createSignal(60);
  const [cap, setCap] = createSignal(5);
  const [saving, setSaving] = createSignal(false);
  const chosenProfile = () => profile() || props.profiles()[0]?.agent_profile_id || '';
  const reason = () =>
    addReason({ name: name(), profile: chosenProfile(), kind: kind(), days: days(), time: time(), afterMinutes: afterMinutes(), event: event() });
  const itemEvent = () => event().startsWith('item_') && event() !== 'item_deleted';

  const source = (): TriggerSource =>
    kind() === SCHEDULE
      ? { kind: 'schedule', days: days(), minute_of_day: minuteOf(time()) ?? 0, utc_offset_minutes: localOffsetMinutes() }
      : {
          kind: 'event',
          event: event(),
          filter: {
            ...(itemEvent() && statusFilter().trim() ? { status: statusFilter().trim() } : {}),
            ...(event() === 'change_set_waiting' ? { after_seconds: afterMinutes() * 60 } : {}),
          },
        };

  const add = async () => {
    if (reason() || saving()) return;
    setSaving(true);
    try {
      const input: CreateTrigger = {
        project_id: props.projectId,
        profile: chosenProfile(),
        name: name().trim(),
        source: source(),
        quiet_seconds: kind() === SCHEDULE ? 0 : quiet(),
        daily_cap: cap(),
        ...(credential() ? { credential_id: credential() } : {}),
      };
      await api.triggers.create(input);
      setName('');
      setAdding(false);
      await refetch();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Could not add the trigger');
    } finally {
      setSaving(false);
    }
  };

  const activeCredentials = () => (credentials() ?? []).filter((c) => credentialState(c) === 'active');
  const toggleDay = (day: number, on: boolean) => setDays(on ? [...days().filter((d) => d !== day), day].sort() : days().filter((d) => d !== day));

  return (
    <Block id="automation-triggers" testid="triggers">
      <ConfirmHost />
      <Heading>Triggers</Heading>
      <Note>
        A trigger wakes an agent profile on its own: when something happens on the board, or at a time of day. Events that
        arrive close together start one run that reads all of them. What the profile&apos;s own credential did never wakes
        it, and a run still going is not started twice.
      </Note>

      <Show when={!list.error} fallback={<LoadError what="the triggers" error={list.error} onRetry={() => void refetch()} />}>
        <Show
          when={rows().length > 0}
          fallback={
            <Note testid="triggers-empty">{list.loading ? 'Loading…' : 'No triggers in this project yet.'}</Note>
          }
        >
          <ul class="m-0 flex list-none flex-col gap-3 p-0" data-testid="trigger-list">
            <For each={rows()}>
              {(t) => (
                <li class="flex min-w-0 flex-col gap-2 rounded-item p-3" data-testid="trigger" style={{ border: '1px solid var(--color-border-light)' }}>
                  <div class="flex flex-wrap items-center gap-x-3 gap-y-1">
                    <span class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>{t.name}</span>
                    <span class="text-caption" style={{ color: 'var(--color-text-secondary)' }}>wakes {profileName(t.profile)}</span>
                    <Switch
                      class="ml-auto"
                      checked={t.enabled}
                      disabled={busy() === t.id}
                      ariaLabel={`${t.name} is on`}
                      onChange={(on) => void patch(t, { enabled: on })}
                    >
                      On
                    </Switch>
                    <Button size="sm" variant="ghost" onClick={() => void remove(t)}>
                      Delete
                    </Button>
                  </div>
                  <Note>{triggerSentence(t)}</Note>
                  <div class="flex flex-wrap items-center gap-3 text-sm" style={{ color: 'var(--color-text-primary)' }}>
                    <Show when={t.source.kind === 'event'}>
                      <label class="flex items-center gap-2 whitespace-nowrap">
                        Quiet seconds
                        <input
                          type="number"
                          min="0"
                          max={MAX_QUIET_SECONDS}
                          aria-label={`Quiet seconds for ${t.name}`}
                          class="w-24 rounded-full border px-3.5 py-2 text-sm"
                          style={inputStyle}
                          disabled={busy() === t.id}
                          value={t.quiet_seconds}
                          onChange={(e) => void patch(t, { quiet_seconds: clamp(e.currentTarget.value, 0, MAX_QUIET_SECONDS) })}
                        />
                      </label>
                    </Show>
                    <label class="flex items-center gap-2 whitespace-nowrap">
                      Runs a day, at most
                      <input
                        type="number"
                        min="1"
                        max={MAX_DAILY_CAP}
                        aria-label={`Daily cap for ${t.name}`}
                        class="w-20 rounded-full border px-3.5 py-2 text-sm"
                        style={inputStyle}
                        disabled={busy() === t.id}
                        value={t.daily_cap}
                        onChange={(e) => void patch(t, { daily_cap: clamp(e.currentTarget.value, 1, MAX_DAILY_CAP) })}
                      />
                    </label>
                  </div>
                  <Show when={t.source.kind === 'schedule' ? t.source : undefined}>
                    {(schedule) => (
                      <div class="flex flex-wrap items-center gap-2 text-sm" data-testid="trigger-schedule">
                        <For each={WEEKDAYS}>
                          {(w) => (
                            <label class="flex items-center gap-1">
                              <input
                                type="checkbox"
                                aria-label={`${w.short} for ${t.name}`}
                                checked={schedule().days.includes(w.day)}
                                disabled={busy() === t.id}
                                onChange={(e) => {
                                  const next = e.currentTarget.checked
                                    ? [...schedule().days.filter((d) => d !== w.day), w.day]
                                    : schedule().days.filter((d) => d !== w.day);
                                  if (next.length === 0) {
                                    e.currentTarget.checked = true;
                                    toast.error('A schedule needs at least one weekday');
                                    return;
                                  }
                                  void patch(t, { source: { ...schedule(), days: next.sort() } });
                                }}
                              />
                              {w.short}
                            </label>
                          )}
                        </For>
                        <label class="flex items-center gap-2 whitespace-nowrap">
                          At
                          <input
                            type="time"
                            aria-label={`Time for ${t.name}`}
                            class="rounded-full border px-3.5 py-2 text-sm"
                            style={inputStyle}
                            disabled={busy() === t.id}
                            value={timeText(schedule().minute_of_day)}
                            onChange={(e) => {
                              const minute = minuteOf(e.currentTarget.value);
                              if (minute !== null) void patch(t, { source: { ...schedule(), minute_of_day: minute } });
                            }}
                          />
                          <span class="text-caption" style={{ color: 'var(--color-text-secondary)' }}>{zoneText(schedule().utc_offset_minutes ?? 0)}</span>
                        </label>
                      </div>
                    )}
                  </Show>
                  <Note>{t.last_fired_at ? `Last ran ${relativeTimeFromIso(t.last_fired_at)}.` : 'Has not run yet.'}</Note>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </Show>

      <Show
        when={adding()}
        fallback={
          <div>
            <Button size="sm" data-testid="trigger-add-open" onClick={() => setAdding(true)}>
              Add a trigger
            </Button>
          </div>
        }
      >
        <form
          data-testid="trigger-form"
          class="flex min-w-0 flex-col gap-3 rounded-item p-3"
          style={{ border: '1px solid var(--color-border-light)' }}
          onSubmit={(e) => {
            e.preventDefault();
            void add();
          }}
        >
          <Field label="Name" value={name()} onInput={(e) => setName(e.currentTarget.value)} placeholder="Failed runs" hint="Only to tell your triggers apart." />
          <Select
            label="It wakes"
            data-testid="trigger-profile"
            value={chosenProfile()}
            disabled={props.profiles().length === 0}
            options={props.profiles().map((p) => ({ value: p.agent_profile_id, label: p.name }))}
            onChange={(e) => setProfile(e.currentTarget.value)}
            hint={props.profiles().length === 0 ? 'There is no agent profile yet. Make one on the Agents page.' : undefined}
          />
          <Select
            label="When"
            data-testid="trigger-kind"
            value={kind() === SCHEDULE ? SCHEDULE : event()}
            onChange={(e) => {
              const v = e.currentTarget.value;
              if (v === SCHEDULE) setKind('schedule');
              else {
                setKind('event');
                setEvent(v as TriggerEvent);
              }
            }}
          >
            <optgroup label="Something happens">
              <For each={EVENT_CHOICES}>{(c) => <option value={c.value}>{c.label}</option>}</For>
              <For each={DEFERRED_EVENTS}>{(d) => <option disabled>{`${d.label} (${d.reason})`}</option>}</For>
            </optgroup>
            <optgroup label="At a time">
              <option value={SCHEDULE}>On chosen weekdays at a time of day</option>
            </optgroup>
          </Select>
          <Show when={kind() === 'event' && event() === 'change_set_waiting'}>
            <label class="flex items-center gap-2 text-sm">
              It counts after
              <input
                type="number"
                min="1"
                aria-label="Minutes a change set waits"
                class="w-24 rounded-full border px-3.5 py-2 text-sm"
                style={inputStyle}
                value={afterMinutes()}
                onInput={(e) => setAfterMinutes(clamp(e.currentTarget.value, 0, 100_000))}
              />
              minutes without an answer
            </label>
          </Show>
          <Show when={kind() === 'event' && itemEvent()}>
            <Field
              label="Only in this status (optional)"
              value={statusFilter()}
              onInput={(e) => setStatusFilter(e.currentTarget.value)}
              placeholder="In Review"
            />
          </Show>
          <Show when={kind() === SCHEDULE}>
            <div class="flex flex-wrap items-center gap-2 text-sm">
              <For each={WEEKDAYS}>
                {(w) => (
                  <label class="flex items-center gap-1">
                    <input type="checkbox" aria-label={w.short} checked={days().includes(w.day)} onChange={(e) => toggleDay(w.day, e.currentTarget.checked)} />
                    {w.short}
                  </label>
                )}
              </For>
              <label class="flex items-center gap-2">
                At
                <input
                  type="time"
                  aria-label="Time of day"
                  class="rounded-full border px-3.5 py-2 text-sm"
                  style={inputStyle}
                  value={time()}
                  onInput={(e) => setTime(e.currentTarget.value)}
                />
              </label>
              <span class="text-caption" style={{ color: 'var(--color-text-secondary)' }}>
                {scheduleText({ days: days(), minute_of_day: minuteOf(time()) ?? 0, utc_offset_minutes: localOffsetMinutes() })}
              </span>
            </div>
          </Show>
          <Show when={kind() === 'event'}>
            <label class="flex items-center gap-2 text-sm">
              Wait until things stay quiet for
              <input
                type="number"
                min="0"
                max={MAX_QUIET_SECONDS}
                aria-label="Quiet seconds"
                class="w-24 rounded-full border px-3.5 py-2 text-sm"
                style={inputStyle}
                value={quiet()}
                onInput={(e) => setQuiet(clamp(e.currentTarget.value, 0, MAX_QUIET_SECONDS))}
              />
              seconds
            </label>
          </Show>
          <label class="flex items-center gap-2 text-sm">
            Runs a day, at most
            <input
              type="number"
              min="1"
              max={MAX_DAILY_CAP}
              aria-label="Daily cap"
              class="w-20 rounded-full border px-3.5 py-2 text-sm"
              style={inputStyle}
              value={cap()}
              onInput={(e) => setCap(clamp(e.currentTarget.value, 1, MAX_DAILY_CAP))}
            />
          </label>
          <Select
            label="Its credential (optional)"
            data-testid="trigger-credential"
            value={credential()}
            onChange={(e) => setCredential(e.currentTarget.value)}
            options={[{ value: '', label: 'None' }, ...activeCredentials().map((c) => ({ value: c.id, label: c.name }))]}
            hint="Name the credential the profile works with, and what it changes never wakes it; with it, events from every project it may read count."
          />
          <div class="flex gap-2">
            <Button type="submit" size="sm" loading={saving()} disabled={!!reason() || saving()} reason={reason()}>
              Add trigger
            </Button>
            <Button type="button" size="sm" variant="secondary" onClick={() => setAdding(false)}>
              Cancel
            </Button>
          </div>
        </form>
      </Show>
    </Block>
  );
};

export default Triggers;
