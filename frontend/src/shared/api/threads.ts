import { request } from './client';
import type { components } from './schema.gen';

type Schemas = components['schemas'];

/** A conversation between a person and an agent profile; each message of the person is one turn, a run of the profile. */
export type Thread = Schemas['Thread'];
export type ThreadDetail = Schemas['ThreadDetail'];
export type ThreadMessage = Schemas['Message'];
export type Turn = Schemas['Turn'];
export type TurnState = Schemas['TurnState'];
export type MessageRef = Schemas['Ref'];
export type PostedMessage = Schemas['PostedMessage'];
export type CreateThread = Schemas['CreateThread'];

export const threads = {
  /** The threads, the most recently written first; `projectId` limits them to one project's, `open` to the open ones. */
  list: (projectId?: string, open = false) => {
    const query = new URLSearchParams();
    if (projectId) query.set('project_id', projectId);
    if (open) query.set('open', 'true');
    const text = query.toString();
    return request<Thread[]>(`/threads${text ? `?${text}` : ''}`);
  },
  get: (id: string) => request<ThreadDetail>(`/threads/${encodeURIComponent(id)}`),
  create: (input: CreateThread) => request<Thread>('/threads', { method: 'POST', body: JSON.stringify(input) }),
  /** Writes a message; it starts a turn. A thread that is closed or whose last turn has not answered answers 409. */
  post: (id: string, body: string, refs: MessageRef[] = []) =>
    request<PostedMessage>(`/threads/${encodeURIComponent(id)}/messages`, {
      method: 'POST',
      body: JSON.stringify({ body, refs }),
    }),
  close: (id: string) => request<Thread>(`/threads/${encodeURIComponent(id)}/close`, { method: 'POST' }),
};
