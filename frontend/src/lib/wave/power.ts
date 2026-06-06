/**
 * Power rating / certainty helpers — scenario 排序 + 顯示。
 *
 * 對齊 spec L1-L2:
 *   - L1 forest 無 primary;排序 ≠ 選定
 *   - L2 排序鍵 = power_rating(enum)+ passed/deferred 計數 + Certainty
 *   - **無 % / 無分數**(全是 enum / 整數)
 */

import type { Certainty } from '$contracts/neely/Certainty';
import type { PowerRating } from '$contracts/neely/PowerRating';
import type { Scenario } from '$contracts/neely/Scenario';

// PowerRating enum 排序鍵(高位優先,負為 bearish)。
const POWER_RANK: Record<PowerRating, number> = {
  StrongBullish: 3,
  Bullish: 2,
  SlightBullish: 1,
  Neutral: 0,
  SlightBearish: -1,
  Bearish: -2,
  StrongBearish: -3
};

// Certainty 排序(Primary 最強)。
const CERTAINTY_RANK: Record<Certainty, number> = {
  Primary: 3,
  Possible: 2,
  Rare: 1,
  MissingWaveBundle: 0
};

export function powerRank(p: PowerRating): number {
  return POWER_RANK[p] ?? 0;
}

/** 絕對強度(忽略多空,只看 strong/avg/weak)。 */
export function powerAbsLevel(p: PowerRating): 'Strong' | 'Avg' | 'Weak' {
  const r = Math.abs(powerRank(p));
  if (r >= 3) return 'Strong';
  if (r >= 1) return 'Avg';
  return 'Weak';
}

export function powerDirection(p: PowerRating): 'bullish' | 'bearish' | 'neutral' {
  const r = powerRank(p);
  if (r > 0) return 'bullish';
  if (r < 0) return 'bearish';
  return 'neutral';
}

/** 中文標籤(顯示用)。 */
export function powerLabel(p: PowerRating): string {
  switch (p) {
    case 'StrongBullish':
      return '強多';
    case 'Bullish':
      return '多';
    case 'SlightBullish':
      return '偏多';
    case 'Neutral':
      return '中性';
    case 'SlightBearish':
      return '偏空';
    case 'Bearish':
      return '空';
    case 'StrongBearish':
      return '強空';
  }
}

/** 排序:power_rank DESC,passed_count DESC,deferred_count ASC(對齊 spec L2)。 */
export function sortScenarios(scenarios: Scenario[]): Scenario[] {
  return [...scenarios].sort((a, b) => {
    const pa = powerRank(a.power_rating);
    const pb = powerRank(b.power_rating);
    if (pa !== pb) return pb - pa;
    if (a.rules_passed_count !== b.rules_passed_count) {
      return b.rules_passed_count - a.rules_passed_count;
    }
    return a.deferred_rules_count - b.deferred_rules_count;
  });
}

/** 取 top N(用於 State 1 前瞻淡線)。**不是** primary 選擇。 */
export function topNScenarios(scenarios: Scenario[], n: number): Scenario[] {
  return sortScenarios(scenarios).slice(0, n);
}

/** Certainty 從 monowave_structure_labels 取最強(或最末)的標 — 因為 forest 無 primary。 */
export function scenarioPrimaryCertainty(scenario: Scenario): Certainty | null {
  // 從 monowave_structure_labels 走遍找最強 Certainty(對齊 wireframe ScenarioCard 顯示)。
  let best: Certainty | null = null;
  for (const mw of scenario.monowave_structure_labels) {
    for (const lbl of mw.labels) {
      if (!best || (CERTAINTY_RANK[lbl.certainty] ?? -1) > (CERTAINTY_RANK[best] ?? -1)) {
        best = lbl.certainty;
      }
    }
  }
  return best;
}
