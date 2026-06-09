import { request } from './client';
import type {
  CustomField,
  CreateCustomField,
  UpdateCustomField,
  CustomFieldValue,
} from '../types';

export const customFields = {
  // ─── Definitions (per project) ───────────────────────────────────────────

  /** Field definitions for a project. */
  list: (projectId: string) =>
    request<CustomField[]>(`/projects/${projectId}/custom-fields`),

  get: (fieldId: string) =>
    request<CustomField>(`/custom-fields/${fieldId}`),

  create: (projectId: string, data: CreateCustomField) =>
    request<CustomField>(`/projects/${projectId}/custom-fields`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),

  update: (fieldId: string, data: UpdateCustomField) =>
    request<CustomField>(`/custom-fields/${fieldId}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    }),

  remove: (fieldId: string) =>
    request<void>(`/custom-fields/${fieldId}`, { method: 'DELETE' }),

  // ─── Values (per item) ───────────────────────────────────────────────────

  /** All custom-field values set on an item. */
  listValues: (itemId: string) =>
    request<CustomFieldValue[]>(`/items/${itemId}/custom-fields`),

  getValue: (itemId: string, fieldId: string) =>
    request<CustomFieldValue>(`/items/${itemId}/custom-fields/${fieldId}`),

  /** Set (PUT) a field's value on an item. Body is the raw value. */
  setValue: (itemId: string, fieldId: string, value: unknown) =>
    request<CustomFieldValue>(`/items/${itemId}/custom-fields/${fieldId}`, {
      method: 'PUT',
      body: JSON.stringify(value),
    }),

  /** Clear a field's value on an item. */
  clearValue: (itemId: string, fieldId: string) =>
    request<void>(`/items/${itemId}/custom-fields/${fieldId}`, {
      method: 'DELETE',
    }),
};
