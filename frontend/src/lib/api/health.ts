import { apiGet, type FetchOptions } from './client';

export interface HealthResponse {
  status: string;
  service: string;
}

export function healthCheck(opts?: FetchOptions): Promise<HealthResponse> {
  return apiGet<HealthResponse>('/health', opts);
}
