import type { NeelyCoreOutput } from '$contracts/neely/NeelyCoreOutput';
import { apiGet, toIsoDate, type FetchOptions } from './client';

export type Timeframe = 'daily' | 'weekly' | 'monthly' | 'quarterly';

export interface GetNeelyForestArgs {
  stockId: string;
  asOf: Date | string;
  timeframe?: Timeframe;
}

export function getNeelyForest(
  args: GetNeelyForestArgs,
  opts?: FetchOptions
): Promise<NeelyCoreOutput> {
  const { stockId, asOf, timeframe = 'daily' } = args;
  const qs = new URLSearchParams({ as_of: toIsoDate(asOf), timeframe });
  return apiGet<NeelyCoreOutput>(`/stocks/${encodeURIComponent(stockId)}/neely/forest?${qs}`, opts);
}
