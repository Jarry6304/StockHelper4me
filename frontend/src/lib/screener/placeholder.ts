/**
 * Placeholder WaveDigest generator — V2 跨股表 WAVE 欄專用(原型階段)。
 *
 * 對齊 plan Phase 4 + spec CL4:
 *   - 沒有 wave-summary 批次端點,本層產 deterministic placeholder
 *   - 每個 placeholder 在 DOM 標 data-placeholder="true",dev mode UI 加角標
 *   - **未來接真實端點只動 getWaveDigest() 1 個 module**(對齊 D2 取捨)
 *
 * Hash 策略:對 stock_id 做 djb2(deterministic;同 stock_id 永遠回同樣 placeholder)。
 */

import type { Certainty } from '$contracts/neely/Certainty';

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
  /** 6-10 個 sparkline 樣本點(歸一化 0..1)。 */
  sparkline: number[];
  resonance: ResonanceLevel;
  /** 是否為原型 placeholder(production 接真實端點時應為 false)。 */
  isPlaceholder: boolean;
}

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

export function getWaveDigest(stockId: string): WaveDigest {
  const seed = djb2(stockId);
  const rng = lcg(seed);

  // ~5% 命中 insufficient_data(對齊 plan Phase 4「random 5% insufficient」)
  const insufficient = rng() < 0.05;
  if (insufficient) {
    return {
      stockId,
      insufficient: true,
      label: '',
      direction: 'flat',
      scenarioCount: 0,
      certainty: 'Possible',
      sparkline: [],
      resonance: 'none',
      isPlaceholder: true
    };
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

export function getWaveDigests(stockIds: string[]): WaveDigest[] {
  return stockIds.map(getWaveDigest);
}
