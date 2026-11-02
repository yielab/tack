import type { RungLevel } from '../api/changeSets';

/** The levels in the order they widen, each with what an agent may do there. */
export const AGENT_LEVELS: ReadonlyArray<{ value: RungLevel; name: string; sentence: string }> = [
  { value: 'read', name: 'Read', sentence: 'Agents read the board and talk; they change nothing.' },
  { value: 'propose', name: 'Propose', sentence: 'Changes wait for you to apply them.' },
  { value: 'act', name: 'Act', sentence: 'Everything but new items applies by itself: who does it, priority, move, dependencies, comments and due dates. New items wait for you.' },
  { value: 'create', name: 'Create', sentence: 'New items apply by themselves too, up to the two limits below. Off until you choose it.' },
];
