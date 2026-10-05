import { request } from './client';
import type { components } from './schema.gen';

type FactoryMetrics = components['schemas']['FactoryMetrics'];

export const metrics = {
  factoryMetrics: (projectId: string, since?: string) =>
    request<FactoryMetrics>(
      `/projects/${projectId}/metrics/factory${since ? `?since=${encodeURIComponent(since)}` : ''}`,
    ),
};
