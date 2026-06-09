import { type Component } from 'solid-js';
import EmptyState from '../../../shared/ui/EmptyState';

// Placeholder tab bodies. Filled in by later tasks:
//   Files → T-509 · Fields → T-510

const Stub = (label: string, hint: string): Component => () =>
  <EmptyState title={label} description={hint} />;

export const FilesTab = Stub('Files', 'Attachments arrive in T-509.');
export const FieldsTab = Stub('Fields', 'Custom fields & roles arrive in T-510.');
