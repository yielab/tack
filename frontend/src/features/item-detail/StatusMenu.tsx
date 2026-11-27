import type { Component } from 'solid-js';
import { Icons } from '../../shared/ui';
import ItemMenu from '../../shared/ui/ItemMenu';
import { statusDotColor } from '../../shared/ui/statusDot';
import { useProject } from '../../shared/state/projectContext';
import type { Item } from '../../shared/types';

const StatusDot: Component<{ category: string | undefined }> = (props) => (
  <span aria-hidden="true" class="flex-none rounded-full" style={{ width: '8px', height: '8px', background: statusDotColor(props.category) }} />
);

/** The item's status as one control: the current status on the button, the workflow's others in the menu it opens. */
const StatusMenu: Component<{ item: Item; onChange: (status: string) => void }> = (props) => {
  const { workflow } = useProject();
  const statuses = () => workflow()?.statuses ?? [];
  const categoryOf = (status: string) => statuses().find((s) => s.name === status)?.category;
  return (
    <ItemMenu
      buttonLabel="Change status"
      menuLabel="Status"
      align="left"
      label={
        <>
          <StatusDot category={categoryOf(props.item.status)} />
          <span data-testid="item-status">{props.item.status}</span>
        </>
      }
      icon={<Icons.IconChevronDown size={14} />}
      entries={statuses().map((s) => ({
        label: s.name,
        current: s.name === props.item.status,
        leading: <StatusDot category={s.category} />,
        onSelect: () => {
          if (s.name !== props.item.status) props.onChange(s.name);
        },
      }))}
    />
  );
};

export default StatusMenu;
