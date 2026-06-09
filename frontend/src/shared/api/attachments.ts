import { request, requestForm, requestBlob, apiUrl } from './client';
import type { Attachment } from '../types';

export const attachments = {
  list: (itemId: string) =>
    request<Attachment[]>(`/items/${itemId}/attachments`),

  /** Upload a file via multipart/form-data (max 50 MB, enforced server-side). */
  upload: (itemId: string, file: File) => {
    const form = new FormData();
    form.append('file', file);
    return requestForm<Attachment>(`/items/${itemId}/attachments`, form);
  },

  /** Fetch raw bytes for an attachment (for download/preview). */
  download: (attachmentId: string) =>
    requestBlob(`/attachments/${attachmentId}`),

  /** Direct URL to an attachment, e.g. for an `<a download>` href. */
  downloadUrl: (attachmentId: string) => apiUrl(`/attachments/${attachmentId}`),

  remove: (attachmentId: string) =>
    request<void>(`/attachments/${attachmentId}`, { method: 'DELETE' }),
};
