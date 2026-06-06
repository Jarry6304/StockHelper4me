/**
 * 各 toolkit 的因子欄定義(對映 V2 wireframe + cross_cores builders)。
 *
 * 每個 toolkit 一個 columns 列表:來自 ScreenRow 的哪些 key 該顯示為因子欄。
 * 不對應到的欄位忽略(對齊 spec CL2 — 表身吃 /screens 但不展示所有欄)。
 */

import type { ActiveToolkit, ScreenRow } from '$lib/api';

export interface FactorColumn {
  /** ScreenRow 的 key。 */
  key: string;
  /** Header 顯示名(短)。 */
  label: string;
  /** 解析數字:若為 null/undefined 顯示「—」。 */
  format?: 'percent' | 'int' | 'decimal' | 'auto';
  /** 數字單位後綴(顯示用)。 */
  suffix?: string;
}

/** detail JSONB 也可能包這些因子;先試頂層,後試 detail.{key}。 */
function findInRow(row: ScreenRow, key: string): unknown {
  if (row[key] !== undefined) return row[key];
  const detail = row.detail;
  if (detail && typeof detail === 'object' && detail !== null) {
    return (detail as Record<string, unknown>)[key];
  }
  return undefined;
}

export const TOOLKIT_FACTORS: Record<ActiveToolkit, FactorColumn[]> = {
  magic_formula: [
    { key: 'earnings_yield', label: 'EY%', format: 'decimal', suffix: '' },
    { key: 'return_on_capital', label: 'ROC', format: 'decimal' }
  ],
  f_score: [
    { key: 'f_score', label: 'F-score', format: 'int' }
  ],
  low_volatility: [
    { key: 'realized_vol_6m', label: '6M Vol', format: 'decimal' }
  ],
  mom_12_1: [
    { key: 'mom_12_1', label: '12-1 Mom', format: 'percent' }
  ],
  dividend_yield: [
    { key: 'dividend_yield', label: '殖利率%', format: 'decimal' }
  ],
  revenue_momentum: [
    { key: 'revenue_yoy_3m', label: '3M YoY%', format: 'decimal' }
  ],
  industry_adj_gp: [
    { key: 'industry_adj_gp', label: 'Adj-GP', format: 'decimal' }
  ],
  persistent_momentum: [
    { key: 'persistent_score', label: 'Score', format: 'decimal' }
  ],
  institutional_concert: [
    { key: 'concert_score', label: 'Concert', format: 'decimal' }
  ],
  long_term_low_vol: [
    { key: 'vol_5y', label: '5Y Vol', format: 'decimal' }
  ]
};

export function factorColumnsFor(toolkit: ActiveToolkit): FactorColumn[] {
  return TOOLKIT_FACTORS[toolkit] ?? [];
}

export function formatFactor(row: ScreenRow, col: FactorColumn): string {
  const raw = findInRow(row, col.key);
  if (raw === null || raw === undefined) return '—';
  const num = typeof raw === 'number' ? raw : Number(raw);
  if (Number.isNaN(num)) return '—';

  switch (col.format) {
    case 'int':
      return String(Math.round(num));
    case 'percent':
      return num.toFixed(1) + '%';
    case 'decimal':
      return num.toFixed(1);
    default:
      return num.toFixed(2);
  }
}

/** 短描述某 toolkit 的因子欄,給 ColumnGroupBanner 顯示用。 */
export function toolkitFactorSummary(toolkit: ActiveToolkit): string {
  return factorColumnsFor(toolkit)
    .map((c) => c.label)
    .join(' · ');
}
