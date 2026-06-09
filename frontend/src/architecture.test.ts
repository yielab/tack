import { describe, it, expect } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, dirname, resolve, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

// Enforces the hard architectural rule from T-503:
//   features/* may import from shared/* — never from another features/*.
// Implemented as a plain filesystem scan so it needs no extra tooling
// (no dependency-cruiser / eslint-plugin-boundaries dependency) and runs in CI
// via `npm test`.

const srcDir = resolve(dirname(fileURLToPath(import.meta.url)));
const featuresDir = join(srcDir, 'features');

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else if (/\.(ts|tsx)$/.test(entry) && !/\.test\.(ts|tsx)$/.test(entry)) {
      out.push(full);
    }
  }
  return out;
}

/** The top-level feature folder a path belongs to, or null if outside features/. */
function featureOf(absPath: string): string | null {
  const rel = relative(featuresDir, absPath);
  if (rel.startsWith('..')) return null;
  return rel.split(sep)[0];
}

const IMPORT_RE = /(?:import|export)[^'"]*from\s*['"]([^'"]+)['"]|import\(\s*['"]([^'"]+)['"]\s*\)/g;

describe('architecture boundaries', () => {
  it('no features/* file imports from another features/*', () => {
    const violations: string[] = [];

    for (const file of walk(featuresDir)) {
      const ownFeature = featureOf(file);
      const code = readFileSync(file, 'utf8');
      for (const match of code.matchAll(IMPORT_RE)) {
        const spec = match[1] ?? match[2];
        if (!spec || !spec.startsWith('.')) continue; // only relative imports can cross folders
        const targetFeature = featureOf(resolve(dirname(file), spec));
        if (targetFeature && targetFeature !== ownFeature) {
          violations.push(
            `${relative(srcDir, file)} imports '${spec}' (feature '${targetFeature}')`
          );
        }
      }
    }

    expect(violations, `cross-feature imports found:\n${violations.join('\n')}`).toEqual(
      []
    );
  });
});
