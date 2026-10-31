import { type Component, type JSX, splitProps } from 'solid-js';
import clsx from 'clsx';

/** The panel that holds a group of content: the card radius on the panel surface. Any other
 *  attribute (role, data-testid, id, handlers) goes to the element; `class` is added to the base. */
export type CardProps = JSX.HTMLAttributes<HTMLDivElement>;

const Card: Component<CardProps> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <div {...rest} class={clsx('rounded-card bg-panel', local.class)}>
      {local.children}
    </div>
  );
};

export default Card;
