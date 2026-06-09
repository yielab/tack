import { request } from './client';
import type { Comment, CreateComment } from '../types';

export const comments = {
  list: (itemId: string) =>
    request<Comment[]>(`/items/${itemId}/comments`),

  create: (itemId: string, data: CreateComment) =>
    request<Comment>(`/items/${itemId}/comments`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),
};
