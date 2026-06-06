import type { NeelyCoreOutput } from '$contracts/neely/NeelyCoreOutput';
import { apiGet, toIsoDate, type FetchOptions } from './client';
import type { Timeframe } from './neely';
import type { TraditionalForestOutput } from './traditional';

/**
 * `/stocks/{id}/waves` 邊緣組裝 `{ neely, traditional }` 並排。
 *
 * 重要:as_of 僅作用於 neely 側(structural_snapshots 有 snapshot_date),
 * traditional 永遠取 latest computed_at(自有表無 snapshot_date)。L3 鐵則 —
 * UI 不可宣稱兩側為同一 as_of。
 */
export interface WavesResponse {
  neely: NeelyCoreOutput | null;
  traditional: TraditionalForestOutput | null;
}

export interface GetWavesArgs {
  stockId: string;
  asOf: Date | string;
  timeframe?: Timeframe;
}

export function getWaves(args: GetWavesArgs, opts?: FetchOptions): Promise<WavesResponse> {
  const { stockId, asOf, timeframe = 'daily' } = args;
  const qs = new URLSearchParams({ as_of: toIsoDate(asOf), timeframe });
  return apiGet<WavesResponse>(`/stocks/${encodeURIComponent(stockId)}/waves?${qs}`, opts);
}
