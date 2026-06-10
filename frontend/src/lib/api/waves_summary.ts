import { apiGet, toIsoDate, type FetchOptions } from './client';
import type { WaveSummaryRow, WavesSummary } from '$contracts/fusion';

/**
 * GET /waves/summary — V2 跨股表 WAVE 欄批次摘要(拍版 (a),v2-wave-endpoint)。
 *
 * 一次回整頁(≤100 檔)wave digest;伺服端抽取,不送 forest(CL5 summary-only)。
 */

export type WaveTimeframe = 'daily' | 'weekly' | 'monthly';

export interface GetWavesSummaryArgs {
  stockIds: string[];
  date: Date | string;
  timeframe?: WaveTimeframe;
}

export function getWavesSummary(
  args: GetWavesSummaryArgs,
  opts?: FetchOptions
): Promise<WavesSummary> {
  const { stockIds, date, timeframe = 'daily' } = args;
  const qs = new URLSearchParams({
    stock_ids: stockIds.join(','),
    date: toIsoDate(date),
    timeframe
  });
  return apiGet<WavesSummary>(`/waves/summary?${qs}`, opts);
}

export type { WaveSummaryRow, WavesSummary };
