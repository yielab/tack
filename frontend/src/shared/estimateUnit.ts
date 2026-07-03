import type { EstimateUnit } from './types';

// The backend `EstimateUnit::Custom(String)` serializes as `{ custom: "<label>" }`
// (externally-tagged serde), while the built-in units are bare strings. These
// helpers resolve either shape to display text — see the generated schema.

/** Human-readable label, e.g. `story_points` → "story points", custom → its label. */
export function estimateUnitLabel(unit: EstimateUnit): string {
  return typeof unit === 'object' ? unit.custom : unit.replace('_', ' ');
}

const SUFFIX: Record<string, string> = { story_points: 'pts', hours: 'h', days: 'd' };

/** Short suffix for compact display, e.g. `3 pts`. Custom units use their label. */
export function estimateUnitSuffix(unit: EstimateUnit): string {
  return typeof unit === 'object' ? unit.custom : (SUFFIX[unit] ?? '');
}
