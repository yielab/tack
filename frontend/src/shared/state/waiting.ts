import { createSignal, getOwner, onCleanup } from 'solid-js';
import { request } from '../api/client';
import { attemptsApi, executionsApi, type ExecutionSummary } from '../execution';
import type { ItemDetail } from '../types';
import type { RunStateName } from '../ui/RunState';

// Everything that waits on a person, from the runs and items the app already loads: one read of the
// runs every few seconds, then one read of an item for each run that could be a row, only when that
// run changed. The sidebar's list and the top bar's count both read this one place.

export type WaitingGroup = 'ask' | 'review' | 'failed';

/** The groups in the order they are listed, each with the run state its rows show and its primary action. */
export const WAITING_GROUPS: ReadonlyArray<{ id: WaitingGroup; label: string; state: RunStateName; action: string }> = [
  { id: 'ask', label: 'Questions to answer', state: 'needs', action: 'Answer the question' },
  { id: 'review', label: 'Ready for review', state: 'review', action: 'Review and merge' },
  { id: 'failed', label: 'Failed since you last looked', state: 'failed', action: 'See why it failed' },
];

export interface WaitingRow {
  group: WaitingGroup;
  requestId: string;
  itemId: string;
  /** Null until the item has been read. */
  title: string | null;
  projectId: string | null;
  /** When the thing happened: the run's end for a finished run, its start for a question. */
  at: string;
}

/** What is known of an item, and the run it was read for. */
export interface ItemInfo {
  title: string;
  projectId: string;
  needsReview: boolean;
  /** `request_id:state` of the run this was read for; a run that moved on is read again. */
  forRun: string;
  readAt: number;
}

const POLL_MS = 4000;
/** An item whose run waits for a review is read again this often, to notice that it was reviewed. */
const REVIEW_RECHECK_MS = 30_000;
/** Only the newest finished runs are looked at: the list holds every run the install has ever made. */
const FINISHED_WINDOW = 25;
const LOOKED_KEY = 'tack_waiting_last_looked';

/** The newest run of each item. */
function newestPerItem(runs: readonly ExecutionSummary[]): ExecutionSummary[] {
  const newest = new Map<string, ExecutionSummary>();
  for (const run of runs) {
    const seen = newest.get(run.item_id);
    if (!seen || run.created_at > seen.created_at) newest.set(run.item_id, run);
  }
  return [...newest.values()];
}

const isFailed = (run: ExecutionSummary) => run.state === 'failed' || run.state === 'lost';

/**
 * The rows of the inbox. A question is any newest run waiting for an answer; a run is ready for
 * review when it succeeded and its item says nobody has reviewed it; a run failed since `since`
 * when it ended after that time. Newest first within a group.
 */
export function waitingRows(
  runs: readonly ExecutionSummary[],
  items: Readonly<Record<string, ItemInfo>>,
  endedAt: Readonly<Record<string, string>>,
  since: number,
): WaitingRow[] {
  const rows: WaitingRow[] = [];
  const row = (group: WaitingGroup, run: ExecutionSummary, at: string): WaitingRow => ({
    group,
    requestId: run.request_id,
    itemId: run.item_id,
    title: items[run.item_id]?.title ?? null,
    projectId: items[run.item_id]?.projectId ?? null,
    at,
  });
  for (const run of newestPerItem(runs)) {
    const ended = endedAt[run.request_id] ?? run.created_at;
    if (run.state === 'waiting_decision') rows.push(row('ask', run, run.created_at));
    else if (run.state === 'succeeded' && items[run.item_id]?.needsReview) rows.push(row('review', run, ended));
    else if (isFailed(run) && Date.parse(ended) > since) rows.push(row('failed', run, ended));
  }
  return rows.sort((a, b) => b.at.localeCompare(a.at));
}

function readLooked(): number | null {
  try {
    const value = Number(localStorage.getItem(LOOKED_KEY));
    return Number.isFinite(value) && value > 0 ? value : null;
  } catch {
    return null;
  }
}

function writeLooked(at: number): void {
  try {
    localStorage.setItem(LOOKED_KEY, String(at));
  } catch {
    /* ignore — a convenience: without it every page load starts from "now" */
  }
}

const [runs, setRuns] = createSignal<ExecutionSummary[]>([]);
const [items, setItems] = createSignal<Record<string, ItemInfo>>({});
const [endedAt, setEndedAt] = createSignal<Record<string, string>>({});
const [lookedAt, setLookedAt] = createSignal(Date.now());
// While the list is open, "since you last looked" stays at the time before it was opened, so
// the failures it came to show do not vanish the moment they are seen.
const [viewingSince, setViewingSince] = createSignal<number | null>(null);
const [open, setOpen] = createSignal(false);
const [sidebarAsks, setSidebarAsks] = createSignal(0);

const since = () => viewingSince() ?? lookedAt();

/** Every row, in the order the groups list them. */
export function rows(): WaitingRow[] {
  const all = waitingRows(runs(), items(), endedAt(), since());
  return WAITING_GROUPS.flatMap((g) => all.filter((r) => r.group === g.id));
}

export const waitingCount = () => rows().length;
export const waitingOpen = open;
/** Counts up each time something outside the sidebar asks it to show itself (the narrow layout hides it). */
export const sidebarRequests = sidebarAsks;

/** Open the list: this is looking, so the next visit starts from now. */
export function openWaiting(): void {
  if (open()) return;
  setViewingSince(lookedAt());
  const now = Date.now();
  setLookedAt(now);
  writeLooked(now);
  setOpen(true);
}

export function closeWaiting(): void {
  if (!open()) return;
  const now = Date.now();
  setLookedAt(now);
  writeLooked(now);
  setViewingSince(null);
  setOpen(false);
}

/** Open the list and ask the sidebar to come into view. */
export function revealWaiting(): void {
  openWaiting();
  setSidebarAsks((n) => n + 1);
}

async function readItem(run: ExecutionSummary): Promise<[string, ItemInfo] | null> {
  try {
    const detail = await request<ItemDetail>(`/items/${encodeURIComponent(run.item_id)}`);
    const item = detail?.item;
    if (!item) return null;
    return [
      run.item_id,
      {
        title: item.title,
        projectId: item.project_id,
        needsReview: item.needs_review === true,
        forRun: `${run.request_id}:${run.state}`,
        readAt: Date.now(),
      },
    ];
  } catch {
    return null;
  }
}

async function readEnd(run: ExecutionSummary): Promise<[string, string] | null> {
  try {
    const { data } = await attemptsApi.list(run.request_id);
    const ends = (data.data ?? []).map((a) => a.ended_at ?? a.updated_at).filter((t): t is string => !!t);
    return ends.length > 0 ? [run.request_id, ends.sort()[ends.length - 1]] : null;
  } catch {
    return null;
  }
}

/** Read what the runs that could be rows still lack: their item, and for a finished run when it ended. */
async function enrich(live: () => boolean): Promise<void> {
  const newest = newestPerItem(runs());
  const finished = newest
    .filter((r) => r.state === 'succeeded' || isFailed(r))
    .sort((a, b) => b.created_at.localeCompare(a.created_at))
    .slice(0, FINISHED_WINDOW);
  const candidates = [...newest.filter((r) => r.state === 'waiting_decision'), ...finished];
  const now = Date.now();
  const stale = (run: ExecutionSummary) => {
    const known = items()[run.item_id];
    if (!known || known.forRun !== `${run.request_id}:${run.state}`) return true;
    return known.needsReview && now - known.readAt > REVIEW_RECHECK_MS;
  };
  const reads = (await Promise.all(candidates.filter(stale).map(readItem))).filter((r) => r !== null);
  if (!live()) return;
  if (reads.length > 0) setItems((prev) => ({ ...prev, ...Object.fromEntries(reads) }));

  const runOf = (row: WaitingRow) => candidates.find((c) => c.request_id === row.requestId);
  const needEnd = rows()
    .filter((r) => r.group !== 'ask' && !(r.requestId in endedAt()))
    .map(runOf);
  // A failed run is a row only once its end is known to be after the last look, so those are read first.
  const failedUnknown = finished.filter((r) => isFailed(r) && !(r.request_id in endedAt()));
  const toRead = [...new Set([...needEnd, ...failedUnknown])].filter((r): r is ExecutionSummary => r !== undefined);
  const ends = (await Promise.all(toRead.map(readEnd))).filter((r) => r !== null);
  if (!live()) return;
  if (ends.length > 0) setEndedAt((prev) => ({ ...prev, ...Object.fromEntries(ends) }));

  // The items of failed runs that turned out to be new failures (a title and a project to show).
  const lacking = rows().filter((r) => r.title === null).map(runOf).filter((r): r is ExecutionSummary => r !== undefined);
  const more = (await Promise.all(lacking.map(readItem))).filter((r) => r !== null);
  if (!live()) return;
  if (more.length > 0) setItems((prev) => ({ ...prev, ...Object.fromEntries(more) }));
}

let watchers = 0;
let timer: ReturnType<typeof setInterval> | undefined;
// Each time the reading starts afresh it gets a new object, so a read still in flight from the
// last time cannot write into the new one.
let reading: object = {};
let ticking = false;

async function tick(): Promise<void> {
  if (ticking) return;
  ticking = true;
  const mine = reading;
  const live = () => mine === reading;
  try {
    const res = await executionsApi.list();
    if (!live()) return;
    setRuns(res.data.data ?? []);
    await enrich(live);
  } catch {
    /* keep what is on screen: the next tick tries again */
  } finally {
    if (live()) ticking = false;
  }
}

/**
 * Keep the inbox current for as long as the calling component lives; the first one starts the
 * reading and the last one stops it. A browser that never looked before starts from now, so it is
 * not greeted by every failure the install has ever had.
 */
export function useWaiting(): void {
  if (watchers === 0) {
    reading = {};
    ticking = false;
    const stored = readLooked();
    setLookedAt(stored ?? Date.now());
    if (stored === null) writeLooked(lookedAt());
    setViewingSince(null);
    setOpen(false);
    void tick();
    timer = setInterval(() => void tick(), POLL_MS);
  }
  watchers += 1;
  if (getOwner()) {
    onCleanup(() => {
      watchers -= 1;
      if (watchers === 0) {
        clearInterval(timer);
        timer = undefined;
        setRuns([]);
        setItems({});
        setEndedAt({});
      }
    });
  }
}
