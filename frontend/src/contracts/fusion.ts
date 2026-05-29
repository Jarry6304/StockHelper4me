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
