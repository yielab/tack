export const ITEM_TYPE_META: Record<string, { emoji: string; color: string }> = {
  epic:        { emoji: '🎯', color: 'purple' },
  feature:     { emoji: '✨', color: 'blue' },
  task:        { emoji: '📝', color: 'green' },
  subtask:     { emoji: '📌', color: 'gray' },
  bug:         { emoji: '🐛', color: 'red' },
  requirement: { emoji: '📋', color: 'yellow' },
};

export const VOCAB_KEYS = [
  'epic', 'feature', 'task', 'subtask', 'bug', 'requirement',
  'sprint', 'backlog', 'board', 'blocker', 'story_points',
  'assignee', 'deliverable', 'phase', 'milestone', 'release',
] as const;

export type VocabKey = (typeof VOCAB_KEYS)[number];

const DEFAULT_LABELS: Record<VocabKey, string> = {
  epic:         'Epic',
  feature:      'Feature',
  task:         'Task',
  subtask:      'Subtask',
  bug:          'Bug',
  requirement:  'Requirement',
  sprint:       'Sprint',
  backlog:      'Backlog',
  board:        'Board',
  blocker:      'Blocker',
  story_points: 'Story Points',
  assignee:     'Assignee',
  deliverable:  'Deliverable',
  phase:        'Phase',
  milestone:    'Milestone',
  release:      'Release',
};

export function resolveLabel(vocab: Record<string, string> | undefined, key: string): string {
  return (vocab && vocab[key]) || DEFAULT_LABELS[key as VocabKey] || key;
}

export const ITEM_TYPE_KEYS = ['epic', 'feature', 'task', 'subtask', 'bug', 'requirement'] as const;

export interface ItemTypeConfig {
  value: string;
  emoji: string;
  label: string;
  color: string;
}

export function getItemTypeList(vocab?: Record<string, string>): ItemTypeConfig[] {
  return ITEM_TYPE_KEYS.map(key => ({
    value: key,
    ...ITEM_TYPE_META[key],
    label: resolveLabel(vocab, key),
  }));
}

export function getItemTypeMap(
  vocab?: Record<string, string>,
): Record<string, { label: string; color: string; emoji: string }> {
  const result: Record<string, { label: string; color: string; emoji: string }> = {};
  for (const key of ITEM_TYPE_KEYS) {
    result[key] = { ...ITEM_TYPE_META[key], label: resolveLabel(vocab, key) };
  }
  return result;
}
