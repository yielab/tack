import { type Component, Show, createSignal, type JSX } from 'solid-js';
import Modal from './Modal';
import Button from './Button';

export interface ConfirmOptions {
  title: string;
  /** Plain words: what happens, and whether it can be undone. */
  message: string;
  confirmLabel?: string;
  /** Marks the confirm button as destructive. */
  danger?: boolean;
}

export interface ConfirmDialogProps extends ConfirmOptions {
  onConfirm: () => void;
  onCancel: () => void;
}

/** A small dialog that asks before something is removed or replaced. Replaces `window.confirm`,
 *  which cannot be styled, cannot be tested in the kit, and blocks the page. */
const ConfirmDialog: Component<ConfirmDialogProps> = (props) => (
  <Modal isOpen onClose={props.onCancel} title={props.title} size="sm">
    <div class="space-y-5">
      <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>{props.message}</p>
      <div class="flex justify-end gap-2">
        <Button variant="secondary" onClick={props.onCancel}>Cancel</Button>
        <Button variant={props.danger ? 'danger' : 'primary'} onClick={props.onConfirm}>
          {props.confirmLabel ?? 'Continue'}
        </Button>
      </div>
    </div>
  </Modal>
);

/**
 * A question a handler can await: `const [ask, ConfirmHost] = createConfirm();`, render `<ConfirmHost />`
 * once in the component, then `if (!(await ask({ ... }))) return;`. A new question settles the
 * previous one as "no". Must be called inside a component.
 */
export function createConfirm(): [
  ask: (options: ConfirmOptions) => Promise<boolean>,
  ConfirmHost: Component,
] {
  const [pending, setPending] = createSignal<{ options: ConfirmOptions; resolve: (ok: boolean) => void } | null>(null);

  const settle = (ok: boolean) => {
    const current = pending();
    setPending(null);
    current?.resolve(ok);
  };

  const ask = (options: ConfirmOptions) =>
    new Promise<boolean>((resolve) => {
      pending()?.resolve(false);
      setPending({ options, resolve });
    });

  const ConfirmHost: Component = () => (
    <Show when={pending()}>
      {(p): JSX.Element => (
        <ConfirmDialog
          {...p().options}
          onConfirm={() => settle(true)}
          onCancel={() => settle(false)}
        />
      )}
    </Show>
  );

  return [ask, ConfirmHost];
}

export default ConfirmDialog;
