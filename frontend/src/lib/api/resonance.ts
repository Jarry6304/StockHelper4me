import type { ResonanceFusion } from '$contracts/fusion';
import { apiGet, toIsoDate, type FetchOptions } from './client';
import type { Timeframe } from './neely';

export interface GetResonanceArgs {
  stockId: string;
  asOf: Date | string;
  timeframe?: Timeframe;
}

export function getResonance(
  args: GetResonanceArgs,
  opts?: FetchOptions
): Promise<ResonanceFusion> {
  const { stockId, asOf, timeframe = 'daily' } = args;
  const qs = new URLSearchParams({ as_of: toIsoDate(asOf), timeframe });
  return apiGet<ResonanceFusion>(`/stocks/${encodeURIComponent(stockId)}/resonance?${qs}`, opts);
}
