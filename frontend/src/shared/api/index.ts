// Aggregated, typed API surface. Import `api` and call e.g.
// `api.projects.list()`, `api.items.update(id, patch)`.
//
// Covers the full backend route inventory (T-501 + T-502). The realtime board
// socket lives in `shared/realtime/boardSocket.ts`.

import { projects } from './projects';
import { items } from './items';
import { boards } from './boards';
import { sprints } from './sprints';
import { search } from './search';
import { templates } from './templates';
import { customFields } from './customFields';
import { comments } from './comments';
import { dependencies } from './dependencies';
import { attachments } from './attachments';
import { roles } from './roles';
import { data } from './data';
import { system } from './system';

export const api = {
  projects,
  items,
  boards,
  sprints,
  search,
  templates,
  customFields,
  comments,
  dependencies,
  attachments,
  roles,
  data,
  system,
};

export { ApiError, tokenStore, request, requestBlob, requestForm, apiUrl } from './client';
