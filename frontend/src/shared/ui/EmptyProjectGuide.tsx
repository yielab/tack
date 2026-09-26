import { type Component, type JSX } from 'solid-js';
import { A, useParams } from '@solidjs/router';
import { Button } from '../../shared/ui';
import { IconBoard } from './icons';
import { useVocab } from '../vocab/useVocab';

interface Props {
  onAddItem: () => void;
}

const Step: Component<{ n: number; title: string; description: string; action: JSX.Element }> = (
  props,
) => (
  <div
    class="grid items-center gap-3.5 p-3.5 grid-cols-[44px_1fr] sm:grid-cols-[44px_1fr_auto]"
    style={{ background: 'var(--color-bg-app)', 'border-radius': '24px' }}
  >
    <div
      class="font-heading w-10 h-10 rounded-full grid place-items-center text-lg"
      style={{
        background: 'var(--color-primary-600)',
        color: 'var(--color-on-accent)',
      }}
    >
      {props.n}
    </div>
    <div class="min-w-0">
      <p class="text-[15px] font-bold" style={{ color: 'var(--color-text-primary)' }}>
        {props.title}
      </p>
      <p class="text-[13px]" style={{ color: 'var(--color-text-secondary)' }}>
        {props.description}
      </p>
    </div>
    <div class="col-start-2 sm:col-start-auto whitespace-nowrap">{props.action}</div>
  </div>
);

const linkClass = 'inline-flex items-center gap-1 text-[13px] font-bold hover:underline';

const EmptyProjectGuide: Component<Props> = (props) => {
  const params = useParams();
  const { t } = useVocab();
  const pid = () => params.id;

  return (
    <div class="w-full max-w-[900px] flex flex-col gap-4 py-4">
      <div class="flex items-center gap-4">
        <div
          class="flex-shrink-0 w-[84px] h-[84px] rounded-full grid place-items-center"
          style={{ background: 'var(--color-accent2-soft)', color: 'var(--color-accent2-ink)' }}
          aria-hidden="true"
        >
          <IconBoard size={34} />
        </div>
        <div class="min-w-0">
          <h2 class="text-2xl" style={{ color: 'var(--color-text-primary)' }}>
            Your project is ready
          </h2>
          <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            Three steps to hit the ground running.
          </p>
        </div>
      </div>

      <div
        class="flex flex-col gap-1.5 p-2.5"
        style={{ background: 'var(--color-bg-panel)', 'border-radius': '32px' }}
      >
        <Step
          n={1}
          title="Add your first item"
          description="Create a task, bug, epic — whatever your workflow calls it."
          action={
            <Button onClick={props.onAddItem}>+ Add item</Button>
          }
        />

        <Step
          n={2}
          title="Make it yours"
          description={'Rename "Task", "Sprint", and "Epic" to match your domain — software, construction, research, anything.'}
          action={
            <A
              href={`/projects/${pid()}/settings?tab=vocabulary`}
              class={linkClass}
              style={{ color: 'var(--color-accent-ink)' }}
            >
              Open Vocabulary settings &rarr;
            </A>
          }
        />

        <Step
          n={3}
          title={`Plan a ${t('sprint').toLowerCase()}`}
          description="Group items into time-boxed iterations to track progress."
          action={
            <A
              href={`/projects/${pid()}/sprint`}
              class={linkClass}
              style={{ color: 'var(--color-accent-ink)' }}
            >
              Go to {t('sprint')} →
            </A>
          }
        />
      </div>
    </div>
  );
};

export default EmptyProjectGuide;
