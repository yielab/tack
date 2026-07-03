import { type Component, type JSX } from 'solid-js';
import { A, useParams } from '@solidjs/router';
import { Button } from '../../shared/ui';
import { useVocab } from '../vocab/useVocab';

interface Props {
  onAddItem: () => void;
}

const Step: Component<{ n: number; title: string; description: string; action: JSX.Element }> = (
  props,
) => (
  <div class="flex gap-4">
    <div
      class="flex-shrink-0 w-8 h-8 rounded-full flex items-center justify-center text-sm font-bold"
      style={{
        background: 'var(--color-primary-100)',
        color: 'var(--color-primary-700)',
      }}
    >
      {props.n}
    </div>
    <div class="flex-1 min-w-0">
      <p class="text-sm font-semibold mb-0.5" style={{ color: 'var(--color-text-primary)' }}>
        {props.title}
      </p>
      <p class="text-sm mb-3" style={{ color: 'var(--color-text-secondary)' }}>
        {props.description}
      </p>
      {props.action}
    </div>
  </div>
);

const EmptyProjectGuide: Component<Props> = (props) => {
  const params = useParams();
  const { t } = useVocab();
  const pid = () => params.id;

  return (
    <div class="flex flex-col items-center justify-center py-16 px-4">
      <div class="w-full max-w-md">
        <div class="text-center mb-8">
          <div class="text-5xl mb-4" aria-hidden="true">🚀</div>
          <h2 class="text-xl font-bold mb-2" style={{ color: 'var(--color-text-primary)' }}>
            Your project is ready
          </h2>
          <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            Three steps to hit the ground running.
          </p>
        </div>

        <div
          class="rounded-xl border p-6 space-y-6"
          style={{
            'background-color': 'var(--color-bg-elevated)',
            'border-color': 'var(--color-border-light)',
          }}
        >
          <Step
            n={1}
            title="Add your first item"
            description="Create a task, bug, epic — whatever your workflow calls it."
            action={
              <Button onClick={props.onAddItem}>+ Add item</Button>
            }
          />

          <div style={{ 'border-top': '1px solid var(--color-border-light)' }} />

          <Step
            n={2}
            title="Make it yours"
            description={'Rename "Task", "Sprint", and "Epic" to match your domain — software, construction, research, anything.'}
            action={
              <A
                href={`/projects/${pid()}/settings?tab=vocabulary`}
                class="inline-flex items-center gap-1 text-sm font-medium"
                style={{ color: 'var(--color-primary-600)' }}
              >
                Open Vocabulary settings &rarr;
              </A>
            }
          />

          <div style={{ 'border-top': '1px solid var(--color-border-light)' }} />

          <Step
            n={3}
            title={`Plan a ${t('sprint').toLowerCase()}`}
            description="Group items into time-boxed iterations to track progress."
            action={
              <A
                href={`/projects/${pid()}/sprint`}
                class="inline-flex items-center gap-1 text-sm font-medium"
                style={{ color: 'var(--color-text-tertiary)' }}
              >
                Go to {t('sprint')} →
              </A>
            }
          />
        </div>
      </div>
    </div>
  );
};

export default EmptyProjectGuide;
