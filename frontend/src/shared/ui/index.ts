// Design-system kit. Every component consumes design tokens only —
// no hardcoded colors — so a single `.dark` class flip restyles the lot.

export { default as Button } from './Button';
export type { ButtonProps, ButtonVariant, ButtonSize } from './Button';

export { default as CopyFix } from './CopyFix';

export { default as Badge } from './Badge';
export type { BadgeProps, BadgeTone } from './Badge';

export { default as LoadError } from './LoadError';
export type { LoadErrorProps } from './LoadError';

export { default as RouteBoundary } from './RouteBoundary';

export { default as Skeleton } from './Skeleton';
export type { SkeletonProps } from './Skeleton';

export { default as EmptyState } from './EmptyState';
export type { EmptyStateProps } from './EmptyState';

export { default as Field, FieldShell, HelpHint, HelpNote } from './Field';
export type { FieldProps, FieldShellProps, FieldHelp } from './Field';

export { default as Select } from './Select';
export type { SelectProps, SelectOption } from './Select';

export { default as Modal } from './Modal';
export type { ModalProps } from './Modal';

export type { CardProps } from './Card';

export { default as Switch } from './Switch';
export type { SwitchProps } from './Switch';

export { createConfirm } from './ConfirmDialog';
export type { ConfirmOptions, ConfirmDialogProps } from './ConfirmDialog';

export type { DrawerProps } from './Drawer';

export { default as Fold } from './Fold';
export type { FoldProps } from './Fold';

export { default as Tabs } from './Tabs';
export type { TabsProps, TabItem } from './Tabs';

// Redesign primitives (Tack.dc.html) — token-styled atoms shared across the
// shell, board, and drawer.
export { default as Avatar, AvatarStack } from './Avatar';
export { teamInitials } from './teamInitials';
export type { AvatarProps, AvatarStackProps, AvatarSize } from './Avatar';

export { default as TypeTile } from './TypeTile';
export type { TypeTileProps } from './TypeTile';

export { default as TypeList } from './TypeList';

// GroupHeader, ItemPath and ParentPicker are imported from their own files: the header opens the run dialog, which imports this kit.
export type { ProgressBarProps } from './ProgressBar';

export { default as PriorityDot, priorityLabel } from './PriorityDot';
export type { PriorityDotProps } from './PriorityDot';

export { default as WipChip } from './WipChip';
export type { WipChipProps } from './WipChip';

export type { KbdHintProps } from './KbdHint';

export { default as RunState } from './RunState';
export type { RunStateProps, RunStateName, RunStateSize, DocketRunWord } from './RunState';

export * as Icons from './icons';
export { BrandMark } from './icons';
export type { IconProps } from './icons';
