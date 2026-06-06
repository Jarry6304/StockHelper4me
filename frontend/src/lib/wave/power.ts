/**
 * Power rating / certainty helpers — scenario 排序 + 顯示。
 *
 * 對齊 spec L1-L2:
 *   - L1 forest 無 primary;排序 ≠ 選定
 *   - L2 排序鍵 = power_rating(enum)+ passed/deferred 計數 + Certainty
 *   - **無 % / 無分數**(全是 enum / 整數)
 */

import type { Certainty } from '$contracts/neely/Certainty';
import type { Monowave } from '$contracts/neely/Monowave';
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

/**
 * scenario 結尾距 as_of 多少天(正值 = 過去,負值 = 未來投影,Infinity = 無 wave_tree)。
 *
 * Neely forest 含跨度多年的 historical scenario(e.g. anchor 在 2022 的 5-wave Impulse),
 * production data 一口氣回傳「最早到最新」會讓畫面只看舊資料 → 用 recency 當 default
 * selection tier 的第一個鍵。
 */
export function scenarioRecencyDays(scenario: Scenario, asOf: string | null): number {
  if (!asOf) return Number.POSITIVE_INFINITY;
  const end = scenario.wave_tree?.end;
  if (!end) return Number.POSITIVE_INFINITY;
  const t = Date.parse(end);
  const asOfTime = Date.parse(asOf);
  if (Number.isNaN(t) || Number.isNaN(asOfTime)) return Number.POSITIVE_INFINITY;
  return (asOfTime - t) / 86400000;
}

/**
 * 從 monowave_series 取最後一筆 end_price 作為「最新觀察價」(對齊 neely_core 的
 * data_range.end)— 用來判 scenario 是否已觸發 invalidation。
 */
export function extractCurrentPriceFromMonowaves(monowaves: Monowave[]): number | null {
  if (!monowaves || monowaves.length === 0) return null;
  const last = monowaves[monowaves.length - 1];
  if (!last || typeof last.end_price !== 'number') return null;
  return last.end_price;
}

/**
 * 判斷 scenario 是否已被當前價格觸發 invalidation。
 *
 * 對齊 spec § Invalidation Triggers:
 *   - `PriceBreakBelow X` + currentPrice < X → invalidated
 *   - `PriceBreakAbove X` + currentPrice > X → invalidated
 *   - 只看 `on_trigger === 'InvalidateScenario'` 的 trigger
 *     (WeakenScenario / PromoteAlternative 不算)
 *   - trigger 價格 = 0 視為 spec placeholder 忽略(Compaction Combination
 *     scenarios 常出現)
 */
export function isScenarioInvalidated(
  scenario: Scenario,
  currentPrice: number | null
): boolean {
  if (currentPrice === null || !Number.isFinite(currentPrice)) return false;
  for (const trigger of scenario.invalidation_triggers) {
    if (trigger.on_trigger !== 'InvalidateScenario') continue;
    const tt = trigger.trigger_type;
    if (typeof tt !== 'object' || tt === null) continue;
    if ('PriceBreakBelow' in tt && typeof tt.PriceBreakBelow === 'number') {
      if (tt.PriceBreakBelow > 0 && currentPrice < tt.PriceBreakBelow) return true;
    }
    if ('PriceBreakAbove' in tt && typeof tt.PriceBreakAbove === 'number') {
      if (tt.PriceBreakAbove > 0 && currentPrice > tt.PriceBreakAbove) return true;
    }
  }
  return false;
}

/**
 * Recency tier 階梯化(對齊 user「我要看現在」的 UX 預期):
 *   - tier 3:≤ 60 天 → very recent,最即時
 *   - tier 2:61-180 天 → recent,仍 actionable
 *   - tier 1:181-365 天 → moderate,結構意義仍在
 *   - tier 0:> 365 天 → old,通常是 historical anchor
 *
 * tier 3 內最弱的 Neutral 仍會勝過 tier 2 內最強的 StrongBullish — 因為「現在發生的事」
 * 優先於「半年前的強訊號」對 UI default focus 而言。user 想看舊強訊號可手動點選 list。
 */
export function recencyTier(days: number): number {
  if (!Number.isFinite(days)) return 0;
  if (days <= 60) return 3;
  if (days <= 180) return 2;
  if (days <= 365) return 1;
  return 0;
}

export interface PickOptions {
  /** 退守參數;預設沿用 365 仍接受。tier 化後此值少用,留 backward-compat。 */
  windowDays?: number;
  /**
   * 當前實際價格(用來過濾 invalidation triggered scenario)。null = 不過濾。
   * 通常從 `extractCurrentPriceFromMonowaves(neely.monowave_series)` 取。
   */
  currentPrice?: number | null;
}

/**
 * 選 default 顯示用 scenario(對齊 spec L1「forest 無 primary」— 此非「答案」只是
 * UI 預設焦點)。
 *
 * 排序鍵(對齊 user 反饋「畫面只顯示舊資料 wave 進入選中」):
 *   1. 未 invalidated 優先(被當前價格觸發 invalidation 的 scenario 推到最後)
 *   2. recency tier DESC(very recent > recent > moderate > old)
 *   3. power_rank DESC(同 tier 內偏好強訊號)
 *   4. rules_passed_count DESC
 *   5. days ASC(最後 tiebreaker,取最新)
 *
 * 2330 2026-06 production case 對比:
 *   舊版(只看 recency+power)→ c5-mw194-mw198 StrongBullish 2025-Q3(fib 早穿過)
 *   新版(invalidation 過濾 + tier 化)→ c3-mw236-mw238 Zigzag Up 2026-05(fib 真實對應現價)
 */
export function pickDefaultScenario(
  scenarios: Scenario[],
  asOf: string | null,
  optionsOrWindowDays: PickOptions | number = {}
): Scenario | null {
  if (scenarios.length === 0) return null;

  // backward-compat:第 3 參數舊版用 number windowDays
  const options: PickOptions =
    typeof optionsOrWindowDays === 'number'
      ? { windowDays: optionsOrWindowDays }
      : optionsOrWindowDays;
  const windowDays = options.windowDays ?? 365;
  const currentPrice = options.currentPrice ?? null;

  if (!asOf && currentPrice === null) {
    return sortScenarios(scenarios)[0] ?? null;
  }

  const annotated = scenarios.map((s) => ({
    s,
    invalidated: isScenarioInvalidated(s, currentPrice),
    days: scenarioRecencyDays(s, asOf)
  }));

  annotated.sort((a, b) => {
    if (a.invalidated !== b.invalidated) return a.invalidated ? 1 : -1;

    const ta = recencyTier(a.days);
    const tb = recencyTier(b.days);
    if (ta !== tb) return tb - ta;

    const pa = powerRank(a.s.power_rating);
    const pb = powerRank(b.s.power_rating);
    if (pa !== pb) return pb - pa;

    if (a.s.rules_passed_count !== b.s.rules_passed_count) {
      return b.s.rules_passed_count - a.s.rules_passed_count;
    }
    return a.days - b.days;
  });

  // windowDays 仍接受但不主導(對 < windowDays 的 scenario 給輕微 bonus 透過 tier 已覆蓋)
  void windowDays;

  return annotated[0]?.s ?? null;
}
