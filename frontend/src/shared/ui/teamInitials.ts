import { initialsOf } from './Avatar';

const tokens = (name: string) => name.split(/[-_.\s]+/).filter(Boolean);

/** The tokens every name starts with, when there are at least two names and each has something left after them. */
function commonPrefix(all: string[][]): number {
  if (all.length < 2) return 0;
  const shortest = Math.min(...all.map((t) => t.length));
  let n = 0;
  while (n < shortest - 1 && all.every((t) => t[n].toLowerCase() === all[0][n].toLowerCase())) n += 1;
  return n;
}

/**
 * The initials for each of `names`, taken from the part of the name that tells teams apart:
 * teams that share a prefix (saas-code, saas-content, saas-lights) are read after it (CO, CN, LI),
 * a one-word rest by its first two letters, a longer one by the first letters of its first two
 * words. Two names that would read the same take the next letter of the later one, so a set of
 * teams never shows the same pair twice. Without a shared prefix the name reads as it always did.
 * One helper for every view that draws a team, called with the same list of names.
 */
export function teamInitialsOf(names: string[]): Map<string, string> {
  const unique = [...new Set(names.filter(Boolean))];
  const parts = unique.map(tokens);
  const strip = commonPrefix(parts);
  const result = new Map<string, string>();
  const taken = new Set<string>();
  unique.forEach((name, i) => {
    const rest = parts[i].slice(strip);
    let initials: string;
    if (strip === 0 || rest.length === 0) {
      initials = initialsOf(name);
    } else {
      const word = rest.join('').toUpperCase();
      const candidates = rest.length >= 2 ? [rest[0][0] + rest[1][0]] : [];
      for (let k = 1; k < word.length; k += 1) candidates.push(word[0] + word[k]);
      const fresh = candidates.map((c) => c.toUpperCase()).find((c) => !taken.has(c));
      initials = fresh ?? candidates[0]?.toUpperCase() ?? word.slice(0, 2);
    }
    taken.add(initials);
    result.set(name, initials);
  });
  return result;
}

/** The initials of one team among the teams on show. */
export function teamInitials(name: string, among: string[]): string {
  return teamInitialsOf(among.includes(name) ? among : [...among, name]).get(name) ?? initialsOf(name);
}
