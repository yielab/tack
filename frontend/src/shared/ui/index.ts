// Design-system kit (T-504). Every component consumes design tokens only —
// no hardcoded colors — so a single `.dark` class flip restyles the lot.

export { default as Button } from './Button';
export type { ButtonProps, ButtonVariant, ButtonSize } from './Button';

export { default as Badge } from './Badge';
export type { BadgeProps, BadgeTone } from './Badge';

export { default as Skeleton } from './Skeleton';
export type { SkeletonProps } from './Skeleton';

export { default as EmptyState } from './EmptyState';
export type { EmptyStateProps } from './EmptyState';

export { default as Field, FieldShell } from './Field';
export type { FieldProps, FieldShellProps } from './Field';

export { default as Select } from './Select';
export type { SelectProps, SelectOption } from './Select';

export { default as Modal } from './Modal';
export type { ModalProps } from './Modal';

export { default as Drawer } from './Drawer';
export type { DrawerProps } from './Drawer';

export { default as Tabs } from './Tabs';
export type { TabsProps, TabItem } from './Tabs';

// Redesign primitives (Tack.dc.html) — token-styled atoms shared across the
// shell, board, and drawer.
export { default as Avatar, AvatarStack, hueFromName, initialsOf } from './Avatar';
export type { AvatarProps, AvatarStackProps, AvatarSize } from './Avatar';

export { default as TypeBadge, typeKey, typeBadgeTone } from './TypeBadge';
export type { TypeBadgeProps } from './TypeBadge';

export { default as PriorityDot, priorityColor, priorityLabel } from './PriorityDot';
export type { PriorityDotProps } from './PriorityDot';

export { default as WipChip, wipChipStyle } from './WipChip';
export type { WipChipProps } from './WipChip';

export { default as KbdHint } from './KbdHint';
export type { KbdHintProps } from './KbdHint';

export * as Icons from './icons';
export { BrandMark } from './icons';
export type { IconProps } from './icons';
