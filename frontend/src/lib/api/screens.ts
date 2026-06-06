import { apiGet, toIsoDate, type FetchOptions } from './client';

/**
 * 啟用的 toolkit(對映 V2 wireframe tabs / web_api.routers.screens._ALLOWED)。
 *
 * wave_impulse / monthly_trigger:MCP-only,本端口不提供(對齊 spec CL3)。
 */
export const ACTIVE_TOOLKITS = [
  'magic_formula',
  'f_score',
  'low_volatility',
  'mom_12_1',
  'dividend_yield',
  'revenue_momentum',
  'industry_adj_gp',
  'persistent_momentum',
  'institutional_concert',
  'long_term_low_vol'
] as const;
export type ActiveToolkit = (typeof ACTIVE_TOOLKITS)[number];

/** UI 顯示但 disabled 的 toolkit(MCP-only,對齊 V2 wireframe)。 */
export const DISABLED_TOOLKITS = ['wave_impulse'] as const;
export type DisabledToolkit = (typeof DISABLED_TOOLKITS)[number];

export type Toolkit = ActiveToolkit | DisabledToolkit;

export interface ScreenRow {
  stock_id: string;
  combined_rank: number;
  is_top_n?: boolean | null;
  /**
   * 後端 v4.35 把實體欄位 `is_top_30` 改名 `is_top_n`(本欄)— 概念名稱仍稱
   * "top 30"。view layer 對外仍叫 top30(對齊 V2 wireframe + DualTrackResult 語意層)。
   */
  is_top_30?: boolean | null;
  /** 因子欄(依 toolkit 不同):EY / ROC / F-score / 12-1 momentum ... */
  [factorKey: string]: unknown;
}

export interface ScreenResponse {
  toolkit: string;
  ranking_date: string | null;
  top_n: number;
  offset: number;
  rows: ScreenRow[];
}

export interface GetScreenArgs {
  toolkit: ActiveToolkit;
  date: Date | string;
  topN?: number;
  offset?: number;
}

export function getScreen(args: GetScreenArgs, opts?: FetchOptions): Promise<ScreenResponse> {
  const { toolkit, date, topN = 30, offset = 0 } = args;
  const qs = new URLSearchParams({
    date: toIsoDate(date),
    top_n: String(topN),
    offset: String(offset)
  });
  return apiGet<ScreenResponse>(`/screens/${encodeURIComponent(toolkit)}?${qs}`, opts);
}
