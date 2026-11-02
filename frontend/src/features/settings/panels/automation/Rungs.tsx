import { type Accessor, type Component, For, Show, createUniqueId } from 'solid-js';
import { HelpHint, LoadError } from '../../../../shared/ui';
import { HELP } from '../../../../shared/help/texts';
import { BOOK, BOOK_BASE } from '../../../../shared/help/routes';
import type { AutonomySettings } from '../../../../shared/api/autonomy';
import { Heading, LEVELS, Note, PUBLIC_KEY_ID, SECTION } from './parts';

type TeamLine = NonNullable<AutonomySettings['teams']>[number];

/** The blockers the team lines already say: "The team X runs attended: …". */
const isTeamBlocker = (text: string, names: string[]) => names.some((n) => text.startsWith(`The team ${n} runs attended`));

const ORDER = LEVELS.map((l) => l.value as string);
const rank = (level: string | undefined) => ORDER.indexOf(level ?? '');

/** Where to go to clear a blocker. `field` is an id on this page; without it the link opens the book. */
interface Fix {
  label: string;
  href: string;
  field?: string;
}

const GITHUB_POLLING = `${BOOK_BASE}admin-guide/runners.html#starting-tasks-by-itself`;

/** The fix for one of the board's blockers, matched on the sentence the server writes; none for a sentence it does not know. */
const fixForBlocker = (text: string): Fix | null => {
  if (/GitHub polling/.test(text)) return { label: 'Turn on GitHub polling', href: GITHUB_POLLING };
  if (/public key/.test(text)) return { label: 'Add the public key', href: `#${PUBLIC_KEY_ID}`, field: PUBLIC_KEY_ID };
  if (/signed pack cannot be checked/.test(text)) return { label: 'Install the verifier', href: BOOK.verifier };
  return null;
};

const fixForTeam = (team: TeamLine): Fix =>
  team.profile === 'unknown'
    ? { label: 'See the team on the Agents page', href: '/agents' }
    : { label: `Run ${team.name} lights-out`, href: BOOK.teamsAdmin };

/** A fix the server writes starts lower-case ("run it lights-out"); after a full stop it is a sentence. */
const sentence = (text: string | null | undefined) => (text ? text.charAt(0).toUpperCase() + text.slice(1) : '');

const FIX_CLASS = 'text-sm underline';
const FIX_STYLE = { color: 'var(--color-text-primary)' } as const;

const FixLink = (props: { fix: Fix }) => (
  <Show
    when={props.fix.field}
    fallback={
      <a href={props.fix.href} target="_blank" rel="noreferrer" class={FIX_CLASS} style={FIX_STYLE}>
        {props.fix.label}
      </a>
    }
  >
    {(field) => (
      <a
        href={props.fix.href}
        class={FIX_CLASS}
        style={FIX_STYLE}
        onClick={(e) => {
          e.preventDefault();
          const target = document.getElementById(field());
          target?.scrollIntoView?.();
          target?.focus?.();
        }}
      >
        {props.fix.label}
      </a>
    )}
  </Show>
);

/**
 * The ladder: L3 to L5 side by side, the level the project is set to filled, the most it can reach
 * marked as the ceiling, and under it what holds the rungs above off, each with its fix. The lines
 * under the rungs are the ladder's description for a screen reader. Each rung is also a radio:
 * pressing one sets the level; L4 and L5 are off while the board has a blocker.
 */
const Rungs: Component<{
  settings: Accessor<AutonomySettings | null | undefined>;
  loadError: Accessor<unknown>;
  onRetry: () => void;
  /** Sets the project's level: the ladder is the selector. */
  save: (patch: Partial<AutonomySettings>) => Promise<void>;
}> = (props) => {
  const uid = createUniqueId();
  const teams = () => props.settings()?.teams ?? [];
  const capped = () => teams().filter((t) => t.caps_at);
  const blockers = () => (props.settings()?.automatic_merge_blocked_by ?? []).filter((b) => !isTeamBlocker(b, teams().map((t) => t.name)));
  const ceiling = () => props.settings()?.ceiling;
  const held = (level: string) => rank(level) > rank(ceiling() ?? 'L5') || (level !== 'L3' && capped().length + blockers().length > 0);
  /** A level above L3 cannot be picked while anything blocks automatic merges. */
  const off = (level: string) => level !== 'L3' && (props.settings()?.automatic_merge_blocked_by ?? []).length > 0;
  const holdCount = () => capped().length + blockers().length;
  /** Ids of the lines that make up the description, in the order they are read. */
  const lineIds = () => [
    ...(ceiling() ? [`${uid}-summary`] : []),
    ...teams().map((_, i) => `${uid}-team-${i}`),
    ...blockers().map((_, i) => `${uid}-blocker-${i}`),
  ];

  const status = (level: string) => {
    const current = props.settings()?.level;
    if (level === current) return held(level) ? `Set here, held at ${ceiling()}` : 'Set here';
    if (rank(level) < rank(current)) return '';
    return held(level) ? (holdCount() > 0 ? `Blocked by ${holdCount()} ${holdCount() === 1 ? 'thing' : 'things'}` : 'Blocked') : 'Available';
  };

  return (
    <section
      class={SECTION}
      id="automation-ceiling"
      data-testid="rung-ceiling"
      aria-labelledby={`${uid}-title`}
      aria-describedby={lineIds().join(' ') || undefined}
    >
      <div class="flex items-center gap-2">
        <div id={`${uid}-title`}>
          <Heading>Autonomy ladder</Heading>
        </div>
        <HelpHint label="Ceiling" help={HELP.ladderCeiling} />
      </div>
      <Show
        when={props.settings()}
        fallback={
          <Show when={props.loadError()} fallback={<Note>The ceiling is not available yet.</Note>}>
            {(error) => <LoadError what="the ceiling" error={error()} onRetry={props.onRetry} />}
          </Show>
        }
      >
        {(s) => (
          <>
            <ol class="grid grid-cols-3 gap-2" aria-label="Autonomy levels">
              <For each={LEVELS}>
                {(l) => {
                  const current = () => s().level === l.value;
                  return (
                    <li
                      data-testid="ladder-rung"
                      data-level={l.value}
                      data-state={current() ? 'current' : held(l.value) ? 'held' : 'open'}
                      aria-current={current() ? 'step' : undefined}
                      class={`relative flex min-w-0 flex-col gap-1 rounded-item border p-3 focus-within:ring-2 ${!current() && held(l.value) ? 'border-dashed' : ''}`}
                      style={{
                        'background-color': current() ? 'var(--color-primary-600)' : 'var(--color-bg-base)',
                        'border-color': current() ? 'var(--color-primary-600)' : 'var(--color-border-medium)',
                        color: current() ? 'var(--color-on-accent)' : 'var(--color-text-primary)',
                      }}
                    >
                      <input
                        type="radio"
                        name="autonomy-level"
                        value={l.value}
                        aria-label={`${l.value} — ${l.sentence}`}
                        checked={current()}
                        disabled={off(l.value)}
                        onChange={() => void props.save({ level: l.value })}
                        class="absolute inset-0 h-full w-full cursor-pointer opacity-0 disabled:cursor-not-allowed"
                      />
                      <span class="font-semibold">{l.value}</span>
                      <span class="text-caption" style={{ 'overflow-wrap': 'anywhere' }}>{l.short}</span>
                      <span class="text-caption font-semibold" data-testid="ladder-status">{status(l.value)}</span>
                      <Show when={ceiling() === l.value}>
                        <span class="text-caption font-semibold" data-testid="ladder-ceiling-mark">▲ Ceiling</span>
                      </Show>
                    </li>
                  );
                }}
              </For>
            </ol>
            <Show when={ceiling()} fallback={<Note>The ceiling is not available yet.</Note>}>
              {(ceil) => (
                <p id={`${uid}-summary`} class="text-sm" style={{ color: 'var(--color-text-primary)' }}>
                  This project can reach {ceil()}.
                </p>
              )}
            </Show>
            <ul class="flex flex-col gap-1.5">
              <For each={teams()}>
                {(t, i) => (
                  <li class="flex flex-wrap items-baseline gap-x-3">
                    <p id={`${uid}-team-${i()}`} data-testid="ceiling-team" class="min-w-0 text-sm" style={{ color: 'var(--color-text-primary)', 'overflow-wrap': 'anywhere' }}>
                      {t.caps_at ? `✗ The team ${t.name} keeps its tasks at ${t.caps_at}. ${sentence(t.fix)}` : `✓ The team ${t.name} runs lights-out.`}
                    </p>
                    <Show when={t.caps_at}>
                      <FixLink fix={fixForTeam(t)} />
                    </Show>
                  </li>
                )}
              </For>
              <For each={blockers()}>
                {(b, i) => (
                  <li class="flex flex-wrap items-baseline gap-x-3">
                    <p id={`${uid}-blocker-${i()}`} data-testid="ceiling-blocker" class="min-w-0 text-sm" style={{ color: 'var(--color-text-secondary)', 'overflow-wrap': 'anywhere' }}>
                      ✗ {b}
                    </p>
                    <Show when={fixForBlocker(b)}>{(fix) => <FixLink fix={fix()} />}</Show>
                  </li>
                )}
              </For>
            </ul>
          </>
        )}
      </Show>
    </section>
  );
};

export default Rungs;
