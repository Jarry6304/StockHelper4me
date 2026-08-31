import type { NeelyCoreOutput } from '$contracts/neely/NeelyCoreOutput';
import { apiGet, toIsoDate, type FetchOptions } from './client';
import type { Timeframe } from './neely';
import type { TraditionalForestOutput } from './traditional';

/**
 * `/stocks/{id}/waves` 邊緣組裝 `{ neely, traditional, dossier }` 並排
 * (v4.39 additive:dossier 為判讀證據卷宗,wave_judgment_loop §4)。
 *
 * 重要:as_of 僅作用於 neely 側(structural_snapshots 有 snapshot_date),
 * traditional 永遠取 latest computed_at(自有表無 snapshot_date)。L3 鐵則 —
 * UI 不可宣稱兩側為同一 as_of。
 */

/** dossier 候選(§4 三區;前端只消費身分區 — 錨定/高亮對映用)。 */
export interface DossierCandidate {
  id: string | null;
  anchor_key: string;
  pattern_type: string | null;
  structure_label: string | null;
  degree_level: number | null;
  span: { start: string | null; end: string | null };
  age_bars: number | null;
  evidence?: { robust?: boolean | null; ch6_status?: string; passed_rules?: string[] };
  forward?: {
    invalidation_triggers?: Array<Record<string, unknown>>;
    expected_fib_zones?: Array<Record<string, unknown>>;
  };
  is_invalidated?: boolean;
}

/** active judgment 摘要(dossier.active_judgment[tf];§5 列的附載投影)。 */
export interface ActiveJudgmentSummary {
  id: number;
  as_of: string | null;
  judged_by: string | null;
  accepted: Array<{ role: string; anchor_key: string }> | null;
  degree_read: unknown;
  confidence_class: string | null;
  invalidation: Record<string, unknown> | null;
  status: string | null;
  assumption_hash: string | null;
  engine_version: string | null;
}

export interface DossierTimeframeSection {
  snapshot_ref: { snapshot_date: string | null; params_hash: string | null } | null;
  candidates: DossierCandidate[];
  historical?: { count: number; note: string };
  truncated?: boolean;
}

export interface WaveDossier {
  stock_id: string;
  as_of: string;
  engine?: { neely?: string | null; assumption_hash?: string | null };
  timeframes: Record<string, DossierTimeframeSection>;
  active_judgment: Record<string, ActiveJudgmentSummary | null>;
  quality_caveat?: Record<string, unknown>;
}

export interface WavesResponse {
  neely: NeelyCoreOutput | null;
  traditional: TraditionalForestOutput | null;
  /** v4.39 additive;舊後端 / 組裝失敗 → 缺鍵或 null,UI 降級無判讀功能。 */
  dossier?: WaveDossier | null;
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
