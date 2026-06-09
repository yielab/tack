import { request } from './client';

export interface HealthInfo {
  status: string;
  version: string;
  migrations_applied: number;
}

export interface DbStats {
  tables: Record<string, number>;
}

/** Health / debug endpoints (read-only). */
export const system = {
  health: () => request<HealthInfo>('/health'),
  dbStats: () => request<DbStats>('/debug/db-stats'),
};
