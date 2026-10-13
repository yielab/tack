import { type Component, Show, createMemo, createSignal } from 'solid-js';
import { Field, Select } from '../ui';
import { HELP } from '../help/texts';
import { type AgentModelsReport, summarizeModels } from './models';

/** The Select value that opens the free-text field. */
const OTHER = '__other__';

export interface ModelFieldProps {
  /** The agent's name as the person knows it, e.g. "Codex". */
  agentLabel: string;
  /** Every runner's report of that agent. */
  reports: readonly AgentModelsReport[];
  /** The model named; an empty string names none, so the agent uses its own. */
  value: string;
  /** A choice from the list, or a typed name once the field is left. */
  onChange: (value: string) => void;
  /** Each keystroke in the typed name. */
  onType?: (value: string) => void;
  label?: string;
}

/**
 * The one control that names a model. It offers what the agent reports, the agent's
 * own default when the agent does not need one named, and a field to type any other
 * name. A typed name is never checked by Tack: the agent is the one that knows.
 */
const ModelField: Component<ModelFieldProps> = (props) => {
  const models = createMemo(() => summarizeModels(props.reports));
  const [other, setOther] = createSignal(false);
  const showOther = () => other() || (props.value !== '' && !models().ids.includes(props.value));

  const options = createMemo(() => [
    models().requiresModel
      ? { value: '', label: 'Choose a model' }
      : { value: '', label: models().defaultModel ? `Agent's default (${models().defaultModel})` : "Agent's default" },
    ...models().ids.map((id) => ({ value: id, label: id })),
    { value: OTHER, label: 'Another model…' },
  ]);
  const line = () => {
    const agent = props.agentLabel;
    switch (models().source) {
      case 'listed':
        return `From ${agent} on this computer.`;
      case 'suggested':
        return `${agent} can't list its models here. These are the names its documentation gives; you can type another.`;
      default:
        return `${agent} can't list its models here. Type the model's name as ${agent} spells it.`;
    }
  };

  return (
    <div class="flex flex-col gap-2" data-testid="model-field">
      {/* A new list is a new control: a select does not keep its value across its options being replaced. */}
      <Show when={options()} keyed>
        {(opts) => (
          <Select
            label={props.label ?? 'Model'}
            aria-label={props.label ?? 'Model'}
            help={HELP.model}
            value={showOther() ? OTHER : props.value}
            options={opts}
            onChange={(e) => {
              const v = e.currentTarget.value;
              if (v === OTHER) {
                setOther(true);
                props.onChange('');
              } else {
                setOther(false);
                props.onChange(v);
              }
            }}
          />
        )}
      </Show>
      <Show when={showOther()}>
        <Field
          label="Model name"
          aria-label="Model name"
          value={props.value}
          onInput={(e) => props.onType?.(e.currentTarget.value)}
          onChange={(e) => props.onChange(e.currentTarget.value)}
        />
      </Show>
      <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }} data-testid="model-line">
        {line()}
        <Show when={models().docs}>
          {(docs) => (
            <>
              {' '}
              <a href={docs().models} target="_blank" rel="noopener noreferrer" class="underline" style={{ color: 'var(--color-accent-ink)' }}>
                How {props.agentLabel} names models
              </a>
            </>
          )}
        </Show>
      </p>
    </div>
  );
};

export default ModelField;
