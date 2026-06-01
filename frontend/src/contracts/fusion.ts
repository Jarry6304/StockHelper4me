/* tslint:disable */
/* eslint-disable */
/**
/* This file was automatically generated from pydantic models by running pydantic2ts.
/* Do not modify it by hand - just update the pydantic models and then re-run the script
*/

export interface ClimateComponent {
  score: number;
  fact_count?: number | null;
  active_disposition_stocks?: number | null;
  escalations_60d?: number | null;
  announced_14d?: number | null;
}
export interface ClimateFusion {
  as_of: string;
  overall_climate: string;
  climate_score: number;
  components: {
    [k: string]: ClimateComponent;
  };
  systemic_risks: string[];
  narrative: string;
}
export interface FibLine {
  price: number;
  low: number;
  high: number;
  label: string | null;
  source_ratio: number | null;
}
export interface FibLineResonance {
  fib_line: FibLine;
  level: string;
  band_covers: boolean;
  median_close: boolean;
  cross_stock_boost: boolean;
  t1_horizon: number | null;
  t2_profile: {
    [k: string]: string;
  };
  notes: string[];
}
export interface Level {
  price: number;
  low: number;
  high: number;
  sources: string[];
  strength: number;
  member_count: number;
}
export interface LevelsFusion {
  stock_id: string;
  as_of: string;
  source_point_count: number;
  level_count_total: number;
  level_count: number;
  levels: Level[];
}
export interface PriceBar {
  date: string;
  open?: number | null;
  high?: number | null;
  low?: number | null;
  close?: number | null;
  volume?: number | null;
}
export interface PriceSeries {
  stock_id: string;
  rows: PriceBar[];
}
export interface ResonanceFusion {
  stock_id: string;
  as_of: string;
  track1: Track1View;
  track2: Track2View;
  is_top_30: boolean;
  is_top_30_source: string | null;
  is_top_30_date: string | null;
  findings: FibLineResonance[];
  single_track_mode: boolean;
  notes: string[];
}
export interface Track1View {
  stock_id: string;
  as_of: string;
  snapshot_date: string | null;
  has_snapshot: boolean;
  pattern_type: string | null;
  power_rating: string | null;
  direction: string;
  effective_degree: string | null;
  wave_count: number;
  fib_lines: FibLine[];
  invalidation_price: number | null;
  invalidated: boolean;
  fallback_to_flat_union: boolean;
  notes: string[];
}
export interface Track2View {
  stock_id: string;
  as_of: string;
  current_price: number | null;
  primary_horizon: number;
  primary_confidence: number;
  primary_band: Track2Band | null;
  horizons: {
    [k: string]: Track2Band;
  };
  notes: string[];
}
export interface Track2Band {
  horizon_days: number;
  confidence: number;
  lower: number;
  upper: number;
  point: number;
  source_core: string;
  width_ratio: number | null;
  is_overly_wide: boolean;
}
export interface ScreenResponse {
  toolkit: string;
  ranking_date: string | null;
  top_n: number;
  offset: number;
  rows: ScreenRowBase[];
}
export interface ScreenRowBase {
  stock_id: string;
  market: string;
  date: string;
  stock_name?: string | null;
  industry_category?: string | null;
  universe_size?: number | null;
  excluded_reason?: string | null;
  rank?: number | null;
  is_top_n: boolean;
}
export interface ScreenRowDividendYield {
  stock_id: string;
  market: string;
  date: string;
  stock_name?: string | null;
  industry_category?: string | null;
  universe_size?: number | null;
  excluded_reason?: string | null;
  rank?: number | null;
  is_top_n: boolean;
  dividend_yield_pct?: number | null;
  return_12m_pct?: number | null;
  payout_years_5y?: number | null;
}
export interface ScreenRowFScore {
  stock_id: string;
  market: string;
  date: string;
  stock_name?: string | null;
  industry_category?: string | null;
  universe_size?: number | null;
  excluded_reason?: string | null;
  rank?: number | null;
  is_top_n: boolean;
  f_score?: number | null;
  profitability?: number | null;
  leverage?: number | null;
  efficiency?: number | null;
}
export interface ScreenRowIndustryAdjGp {
  stock_id: string;
  market: string;
  date: string;
  stock_name?: string | null;
  industry_category?: string | null;
  universe_size?: number | null;
  excluded_reason?: string | null;
  rank?: number | null;
  is_top_n: boolean;
  gross_profitability?: number | null;
  industry?: string | null;
  industry_median_gp?: number | null;
  industry_adj_gp?: number | null;
}
export interface ScreenRowInstitutionalConcert {
  stock_id: string;
  market: string;
  date: string;
  stock_name?: string | null;
  industry_category?: string | null;
  universe_size?: number | null;
  excluded_reason?: string | null;
  rank?: number | null;
  is_top_n: boolean;
  concert_days?: number | null;
  foreign_cumulative_20d?: number | null;
  shares_outstanding?: number | null;
  cumulative_pct?: number | null;
}
export interface ScreenRowLongTermLowVol {
  stock_id: string;
  market: string;
  date: string;
  stock_name?: string | null;
  industry_category?: string | null;
  universe_size?: number | null;
  excluded_reason?: string | null;
  rank?: number | null;
  is_top_n: boolean;
  std_36m?: number | null;
}
export interface ScreenRowLowVolatility {
  stock_id: string;
  market: string;
  date: string;
  stock_name?: string | null;
  industry_category?: string | null;
  universe_size?: number | null;
  excluded_reason?: string | null;
  rank?: number | null;
  is_top_n: boolean;
  std_252d?: number | null;
}
export interface ScreenRowMagicFormula {
  stock_id: string;
  market: string;
  date: string;
  stock_name?: string | null;
  industry_category?: string | null;
  universe_size?: number | null;
  excluded_reason?: string | null;
  rank?: number | null;
  is_top_n: boolean;
  ebit_ttm?: number | null;
  market_cap?: number | null;
  total_debt?: number | null;
  cash?: number | null;
  enterprise_value?: number | null;
  invested_capital?: number | null;
  earnings_yield?: number | null;
  roic?: number | null;
  ey_rank?: number | null;
  roic_rank?: number | null;
}
export interface ScreenRowMom12_1 {
  stock_id: string;
  market: string;
  date: string;
  stock_name?: string | null;
  industry_category?: string | null;
  universe_size?: number | null;
  excluded_reason?: string | null;
  rank?: number | null;
  is_top_n: boolean;
  return_12m_1m?: number | null;
}
export interface ScreenRowPersistentMomentum {
  stock_id: string;
  market: string;
  date: string;
  stock_name?: string | null;
  industry_category?: string | null;
  universe_size?: number | null;
  excluded_reason?: string | null;
  rank?: number | null;
  is_top_n: boolean;
  return_6m?: number | null;
  return_12m_1m?: number | null;
  persistent_months?: number | null;
}
export interface ScreenRowRevenueMomentum {
  stock_id: string;
  market: string;
  date: string;
  stock_name?: string | null;
  industry_category?: string | null;
  universe_size?: number | null;
  excluded_reason?: string | null;
  rank?: number | null;
  is_top_n: boolean;
  revenue_yoy_latest?: number | null;
  consecutive_positive?: number | null;
}
export interface StockRef {
  stock_id: string;
  stock_name?: string | null;
  industry_category?: string | null;
}
