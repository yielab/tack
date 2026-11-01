import { request } from './client';
import type { components } from './schema.gen';

type Schemas = components['schemas'];

/** An agent credential as Tack shows it: never the secret, never its hash. */
export type AgentCredential = Schemas['AgentCredential'];
/** One scope: a project list (or every project) times what may be done there. */
export type CredentialScope = Schemas['Scope'];
export type CredentialCapability = Schemas['Capability'];
/** A credential with its secret, which an issue or a rotate answers once. */
export type IssuedCredential = Schemas['IssuedCredential'];

export const credentials = {
  list: () => request<AgentCredential[]>('/agent-credentials'),
  issue: (name: string, scopes: CredentialScope[]) =>
    request<IssuedCredential>('/agent-credentials', {
      method: 'POST',
      body: JSON.stringify({ name, scopes }),
    }),
  rotate: (id: string) => request<IssuedCredential>(`/agent-credentials/${id}/rotate`, { method: 'POST' }),
  revoke: (id: string) => request<AgentCredential>(`/agent-credentials/${id}/revoke`, { method: 'POST' }),
};
