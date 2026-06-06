import { apiGet, type FetchOptions } from './client';
import type { Timeframe } from './neely';

/**
 * Traditional (Frost & Prechter EWP) forest passthrough。
 *
 * 沒有 contracts/ 型別 — Rust traditional_core 的 output 未被 ts-rs 包(spec 自己一條
 * 獨立 vertical 解耦);本介面用 `unknown` + 寬鬆 shape 描述,等之後 ts-rs 補上再
 * 換成嚴格型別。
 */
export interface TraditionalScenario {
  preference_score?: number | null;
  structure_label?: string | null;
  pattern_type?: string | null;
  wave_tree?: unknown;
  expected_fib_zones?: unknown[];
  invalidation_triggers?: unknown[];
  [key: string]: unknown;
}

export interface TraditionalForestOutput {
  stock_id?: string;
  timeframe?: string;
  scenario_forest?: TraditionalScenario[];
  monowave_series?: unknown[];
  diagnostics?: unknown;
  [key: string]: unknown;
}

export interface GetTraditionalForestArgs {
  stockId: string;
  timeframe?: Timeframe;
}

export function getTraditionalForest(
  args: GetTraditionalForestArgs,
  opts?: FetchOptions
): Promise<TraditionalForestOutput> {
  const { stockId, timeframe = 'daily' } = args;
  const qs = new URLSearchParams({ timeframe });
  return apiGet<TraditionalForestOutput>(
    `/stocks/${encodeURIComponent(stockId)}/traditional/forest?${qs}`,
    opts
  );
}
