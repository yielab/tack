import { For, Show } from 'solid-js';
import { useParams, useNavigate, useSearchParams } from '@solidjs/router';
import { Button, EmptyState } from '../../shared/ui';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import { IconTimeline } from '../../shared/ui/icons';
import Card from '../../shared/ui/Card';
import TimelineBars from './TimelineBars';
import TimelineMonth from './TimelineMonth';

const LAYOUTS = [
  { layout: 'bars', label: 'Bars', hint: 'Drag bars to shift dates. Drag the left or right edge to resize start or due date.' },
  { layout: 'month', label: 'Month', hint: 'Drag items between days to reschedule. Drop on "Unscheduled" to clear a due date.' },
] as const;

/**
 * The Timeline: when work is due and how long it spans, as Bars or as a Month of days. The layout is
 * held in the address (`?layout=month`), so a link opens the same one; Bars is the default.
 */
export default function Timeline() {
  const params = useParams();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const { items, loading } = useProjectItems();

  const current = () => LAYOUTS.find((l) => l.layout === searchParams.layout) ?? LAYOUTS[0];
  const choose = (layout: (typeof LAYOUTS)[number]['layout']) =>
    setSearchParams({ layout: layout === 'bars' ? undefined : layout });

  return (
    <div class="flex flex-col gap-6">
      <div class="flex flex-col gap-2">
        <div class="flex flex-wrap items-center gap-x-6 gap-y-2">
          <h1 class="m-0 text-title text-content">Timeline</h1>
          <div role="group" aria-label="Layout" class="flex gap-1 rounded-md bg-panel p-1 text-small">
            <For each={LAYOUTS}>
              {(l) => (
                <button
                  type="button"
                  aria-pressed={current().layout === l.layout}
                  onClick={() => choose(l.layout)}
                  class="rounded-sm px-3 py-1 transition-colors focus:outline-none focus-visible:ring-2"
                  style={{
                    'background-color': current().layout === l.layout ? 'var(--color-accent-soft)' : 'transparent',
                    color: current().layout === l.layout ? 'var(--color-accent-ink)' : 'var(--color-text-secondary)',
                    'font-weight': current().layout === l.layout ? 700 : 400,
                    '--tw-ring-color': 'var(--color-focus-ring)',
                  }}
                >
                  {l.label}
                </button>
              )}
            </For>
          </div>
        </div>
        <p class="m-0 text-small text-content-muted">{current().hint}</p>
      </div>

      <Show
        when={(items() ?? []).length > 0}
        fallback={
          <Show when={!loading()}>
            <Card>
              <EmptyState
                icon={<IconTimeline size={28} />}
                title="No items to show on the timeline"
                description="Add items in Board or List — they'll appear here once created."
                action={
                  <Button variant="secondary" size="sm" onClick={() => navigate(`/projects/${params.id}/board`)}>
                    Go to Board
                  </Button>
                }
              />
            </Card>
          </Show>
        }
      >
        <Show when={current().layout === 'month'} fallback={<TimelineBars />}>
          <TimelineMonth />
        </Show>
      </Show>
    </div>
  );
}
