/**
 * WaveDigest — V2 跨股表 WAVE 欄資料模組(2026-06-11 拍版 (a) 後接真端點)。
 *
 * 真實資料源:`GET /waves/summary`(批次,伺服端抽取;`fetchWaveDigests()`)。
 * fake 產生器保留兩用途(對齊 spec CL4 的 PH 角標語意):
 *   - `VITE_WAVE_PLACEHOLDER=1`:後端未起時的 dev fallback(整欄標 isPlaceholder)
 *   - vitest deterministic fixture
 * API 失敗 → 整欄 insufficient 視覺(isPlaceholder=false),不擋表格主體。
 *
 * Hash 策略(fake):對 stock_id 做 djb2(deterministic;同 stock_id 永遠同樣輸出)。
 */

import type { Certainty } from '$contracts/neely/Certainty';
import type { WaveSummaryRow } from '$contracts/fusion';
import { getWavesSummary, type WaveTimeframe } from '$lib/api/waves_summary';
import type { FetchOptions } from '$lib/api/client';

export type ResonanceLevel = 'strong' | 'basic' | 'divergence' | 'none';
export type WaveDirection = 'up' | 'down' | 'flat' | 'correction';

export interface WaveDigest {
  stockId: string;
  /** insufficient_data:該股 wave 引擎報「無法判斷」(WaveCell 顯示「— 無法判斷」)。 */
  insufficient: boolean;
  /** 簡短結構標籤(e.g. "5-3-5 ZZ·W4")。 */
  label: string;
  direction: WaveDirection;
  /** scenario_forest 長度。 */
  scenarioCount: number;
  certainty: Certainty;
  /** ≤10 個 sparkline 樣本點(歸一化 0..1;後端 monowave 尾段)。 */
  sparkline: number[];
  resonance: ResonanceLevel;
  /** 是否為 placeholder fake(真實端點資料為 false)。 */
  isPlaceholder: boolean;
}

// ── 真實端點 → WaveDigest ───────────────────────────────────────────────────

const DIRECTIONS: readonly WaveDirection[] = ['up', 'down', 'flat', 'correction'];
const CERTAINTIES: readonly Certainty[] = ['Primary', 'Possible', 'Rare', 'MissingWaveBundle'];
const RESONANCES: readonly ResonanceLevel[] = ['strong', 'basic', 'divergence', 'none'];

function asEnum<T extends string>(v: unknown, pool: readonly T[], fallback: T): T {
  return pool.includes(v as T) ? (v as T) : fallback;
}

/** insufficient 退化值(API 失敗 / 該股無資料時的 cell 狀態)。 */
export function insufficientDigest(stockId: string): WaveDigest {
  return {
    stockId,
    insufficient: true,
    label: '',
    direction: 'flat',
    scenarioCount: 0,
    certainty: 'Possible',
    sparkline: [],
    resonance: 'none',
    isPlaceholder: false
  };
}

/** /waves/summary row → WaveDigest(純映射,enum 防衛性收斂)。 */
export function digestFromRow(row: WaveSummaryRow): WaveDigest {
  if (row.insufficient) return insufficientDigest(row.stock_id);
  return {
    stockId: row.stock_id,
    insufficient: false,
    label: row.label,
    direction: asEnum(row.direction, DIRECTIONS, 'flat'),
    scenarioCount: row.scenario_count,
    certainty: asEnum(row.certainty, CERTAINTIES, 'Possible'),
    sparkline: row.sparkline,
    resonance: asEnum(row.resonance, RESONANCES, 'none'),
    isPlaceholder: false
  };
}

export interface FetchWaveDigestsArgs {
  stockIds: string[];
  date: Date | string;
  timeframe?: WaveTimeframe;
}

/**
 * 批次取 WAVE digest(V2 表格唯一入口)。
 *
 * - `VITE_WAVE_PLACEHOLDER=1` → fake(整欄 PH 角標)
 * - API 失敗 → 整欄 insufficient(不 throw,表格主體照常)
 */
export async function fetchWaveDigests(
  args: FetchWaveDigestsArgs,
  opts?: FetchOptions
): Promise<Map<string, WaveDigest>> {
  const { stockIds } = args;
  if (stockIds.length === 0) return new Map();

  if (import.meta.env?.VITE_WAVE_PLACEHOLDER === '1') {
    return new Map(stockIds.map((id) => [id, getWaveDigest(id)]));
  }

  try {
    const res = await getWavesSummary(
      { stockIds, date: args.date, timeframe: args.timeframe },
      opts
    );
    const out = new Map(res.rows.map((r) => [r.stock_id, digestFromRow(r)]));
    // 防後端漏列(理論上不會):缺檔補 insufficient
    for (const id of stockIds) {
      if (!out.has(id)) out.set(id, insufficientDigest(id));
    }
    return out;
  } catch {
    return new Map(stockIds.map((id) => [id, insufficientDigest(id)]));
  }
}

// ── fake 產生器(dev fallback + test fixture)────────────────────────────────

const LABELS = [
  '5-3-5 ZZ·W4',
  'Impulse·W3',
  'Flat·W5',
  'Triangle·B',
  'DoubleThree',
  'Zigzag·C',
  'Impulse·W1',
  '5-3-5 ZZ·C',
  'Flat·B-Failure',
  'Triangle·D'
];

const CERTAINTY_POOL: Array<{ value: Certainty; weight: number }> = [
  { value: 'Primary', weight: 50 },
  { value: 'Possible', weight: 35 },
  { value: 'Rare', weight: 12 },
  { value: 'MissingWaveBundle', weight: 3 }
];

const RESONANCE_POOL: Array<{ value: ResonanceLevel; weight: number }> = [
  { value: 'strong', weight: 20 },
  { value: 'basic', weight: 50 },
  { value: 'divergence', weight: 25 },
  { value: 'none', weight: 5 }
];

const DIRECTION_POOL: Array<{ value: WaveDirection; weight: number }> = [
  { value: 'up', weight: 35 },
  { value: 'down', weight: 25 },
  { value: 'flat', weight: 25 },
  { value: 'correction', weight: 15 }
];

/** djb2 hash — deterministic 32-bit。 */
function djb2(s: string): number {
  let h = 5381;
  for (let i = 0; i < s.length; i++) {
    h = ((h * 33) ^ s.charCodeAt(i)) >>> 0;
  }
  return h;
}

/** Linear congruential generator 衍生(seeded RNG)。 */
function lcg(seed: number) {
  let state = seed | 0;
  return () => {
    state = (state * 1664525 + 1013904223) | 0;
    return ((state >>> 0) / 0x1_0000_0000) % 1;
  };
}

function pickWeighted<T>(rng: () => number, pool: Array<{ value: T; weight: number }>): T {
  const totalWeight = pool.reduce((s, p) => s + p.weight, 0);
  let r = rng() * totalWeight;
  for (const { value, weight } of pool) {
    r -= weight;
    if (r <= 0) return value;
  }
  return pool[pool.length - 1].value;
}

/** fake digest(deterministic;dev fallback / test fixture 用)。 */
export function getWaveDigest(stockId: string): WaveDigest {
  const seed = djb2(stockId);
  const rng = lcg(seed);

  // ~5% 命中 insufficient_data(對齊 plan Phase 4「random 5% insufficient」)
  const insufficient = rng() < 0.05;
  if (insufficient) {
    return { ...insufficientDigest(stockId), isPlaceholder: true };
  }

  const label = LABELS[Math.floor(rng() * LABELS.length)];
  const direction = pickWeighted(rng, DIRECTION_POOL);
  const scenarioCount = 5 + Math.floor(rng() * 16); // 5..20
  const certainty = pickWeighted(rng, CERTAINTY_POOL);

  // sparkline:6-10 點,值 0..1
  const ptCount = 6 + Math.floor(rng() * 5);
  const sparkline: number[] = [];
  let cur = rng();
  for (let i = 0; i < ptCount; i++) {
    sparkline.push(cur);
    // 走步:±0.25,clamp 0..1
    const step = (rng() - 0.5) * 0.5;
    cur = Math.max(0, Math.min(1, cur + step));
  }
  // 根據 direction 調整尾段
  if (direction === 'up') sparkline[sparkline.length - 1] = Math.min(1, sparkline[0] + 0.3 + rng() * 0.3);
  if (direction === 'down') sparkline[sparkline.length - 1] = Math.max(0, sparkline[0] - 0.3 - rng() * 0.3);

  const resonance = pickWeighted(rng, RESONANCE_POOL);

  return {
    stockId,
    insufficient: false,
    label,
    direction,
    scenarioCount,
    certainty,
    sparkline,
    resonance,
    isPlaceholder: true
  };
}
