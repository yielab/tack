import { type Component } from 'solid-js';
import EmptyState from '../../../shared/ui/EmptyState';

// Placeholder tab bodies. Filled in by later tasks:
//   Fields → T-510

const Stub = (label: string, hint: string): Component => () =>
  <EmptyState title={label} description={hint} />;

export const FieldsTab = Stub('Fields', 'Custom fields & roles arrive in T-510.');
