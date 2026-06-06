import type { LevelsFusion } from '$contracts/fusion';
import { apiGet, toIsoDate, type FetchOptions } from './client';

export interface GetLevelsArgs {
  stockId: string;
  asOf: Date | string;
}

export function getLevels(args: GetLevelsArgs, opts?: FetchOptions): Promise<LevelsFusion> {
  const { stockId, asOf } = args;
  const qs = new URLSearchParams({ as_of: toIsoDate(asOf) });
  return apiGet<LevelsFusion>(`/stocks/${encodeURIComponent(stockId)}/levels?${qs}`, opts);
}
