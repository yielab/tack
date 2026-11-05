// The words that are not item types: a project's types are its own catalog (`Project.item_types`).
export const VOCAB_KEYS = [
  'sprint', 'backlog', 'board', 'blocker', 'story_points',
  'assignee', 'deliverable', 'phase', 'milestone', 'release',
] as const;

export type VocabKey = (typeof VOCAB_KEYS)[number];

// `orchestration` names a work view, not an item word, so it is a default only: the server's
// vocabulary has no such key to customize.
const DEFAULT_LABELS: Record<VocabKey | 'orchestration', string> = {
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
  orchestration: 'Orchestration',
};

export function resolveLabel(vocab: Record<string, string> | undefined, key: string): string {
  return (vocab && vocab[key]) || DEFAULT_LABELS[key as VocabKey] || key;
}
