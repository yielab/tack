import { initialsOf } from './Avatar';

/** A team's name is read in words: hyphens, underscores and dots part them as spaces do (saas-code is two). */
const words = (name: string) => name.split(/[-_.\s]+/).filter(Boolean);

/**
 * The initials of a team, taken from its name alone: one word reads as its first two letters (docs → DO),
 * several as the first letters of the first and the last word (saas-code → SC, acme-content-gen → AG).
 * Nothing but the name goes in, so a team reads the same on the board, in Flow and anywhere else it is
 * drawn, however many other teams there are beside it. Two teams that read alike (saas-code and
 * saas-content) are told apart by the avatar's colour, which also follows the name, and by its tooltip.
 */
export function teamInitials(name: string): string {
  const parts = words(name);
  if (parts.length < 2) return initialsOf(parts[0] ?? '');
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}
