import { request } from './client';
import type { Dependency, CreateDependency } from '../types';

export const dependencies = {
  list: (itemId: string) =>
    request<Dependency[]>(`/items/${itemId}/dependencies`),

  // The server rejects cycles with a 400 — callers should surface ApiError.message.
  create: (itemId: string, data: CreateDependency) =>
    request<Dependency>(`/items/${itemId}/dependencies`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),

  remove: (itemId: string, depId: string) =>
    request<void>(`/items/${itemId}/dependencies/${depId}`, {
      method: 'DELETE',
    }),
};
