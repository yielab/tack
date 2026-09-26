import { splitProps, type Component, type JSX } from 'solid-js';
import clsx from 'clsx';

export type ButtonVariant =
  | 'primary'
  | 'secondary'
  | 'ghost'
  | 'danger'
  | 'success';
export type ButtonSize = 'sm' | 'md' | 'lg';

export interface ButtonProps
  extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
}

const SIZE: Record<ButtonSize, string> = {
  sm: 'px-3 py-[5px] text-xs',
  md: 'px-4 py-2 text-sm',
  lg: 'px-5 py-2.5 text-[15px]',
};

function variantStyle(variant: ButtonVariant): JSX.CSSProperties {
  switch (variant) {
    case 'secondary':
      return {
        'background-color': 'transparent',
        color: 'var(--color-text-primary)',
        border: '1px solid var(--color-border-strong)',
      };
    case 'ghost':
      return {
        'background-color': 'transparent',
        color: 'var(--color-accent-ink)',
      };
    case 'danger':
      return {
        'background-color': 'var(--color-danger-600)',
        color: 'var(--color-text-inverse)',
      };
    case 'success':
      return {
        'background-color': 'var(--color-success-600)',
        color: 'var(--color-text-inverse)',
      };
    case 'primary':
    default:
      // on-accent (not text-inverse): palette-aware, so a future palette
      // pairing a light accent with dark text still gets readable text
      // instead of a single hardcoded white.
      return {
        'background-color': 'var(--color-primary-600)',
        color: 'var(--color-on-accent)',
      };
  }
}

/** Token-driven pill button, set in the display face. Colors come only from
 *  CSS variables; hover/pressed darken the fill via `filter` so every variant
 *  and palette gets a themed state without its own hover token. */
const Button: Component<ButtonProps> = (props) => {
  const [local, rest] = splitProps(props, [
    'variant',
    'size',
    'loading',
    'class',
    'style',
    'disabled',
    'children',
  ]);

  const isDisabled = () => local.disabled || local.loading;

  return (
    <button
      {...rest}
      disabled={isDisabled()}
      aria-busy={local.loading ? 'true' : undefined}
      class={clsx(
        'inline-flex items-center justify-center gap-1.5 rounded-full font-heading whitespace-nowrap transition-[filter,background-color]',
        'hover:brightness-[.94] active:brightness-[.88]',
        'focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2',
        'disabled:opacity-45 disabled:cursor-not-allowed disabled:hover:brightness-100',
        SIZE[local.size ?? 'md'],
        local.class
      )}
      style={{
        ...variantStyle(local.variant ?? 'primary'),
        '--tw-ring-color': 'var(--color-focus-ring)',
        ...(typeof local.style === 'object' ? local.style : {}),
      }}
    >
      {local.loading && (
        <span
          class="inline-block h-4 w-4 animate-spin rounded-full border-2 border-current border-r-transparent"
          aria-hidden="true"
        />
      )}
      {local.children}
    </button>
  );
};

export default Button;
