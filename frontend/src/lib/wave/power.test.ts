import { describe, expect, it } from 'vitest';
import type { FibZone } from '$contracts/neely/FibZone';
import type { Monowave } from '$contracts/neely/Monowave';
import type { Scenario } from '$contracts/neely/Scenario';
import type { Trigger } from '$contracts/neely/Trigger';
import {
  collectLiveFibZones,
  extractCurrentPriceFromMonowaves,
  isScenarioInvalidated,
  pickDefaultScenario,
  powerAbsLevel,
  powerDirection,
  powerLabel,
  powerRank,
  recencyTier,
  scenarioPrimaryCertainty,
  scenarioRecencyDays,
  sortScenarios,
  topNScenarios
} from './power';

function mkScenario(
  p: Partial<Scenario> & {
    waveEnd?: string;
    waveStart?: string;
    invalidationTriggers?: Trigger[];
  } = {}
): Scenario {
  return {
    id: p.id ?? 'S1',
    wave_tree: {
      label: '',
      start: p.waveStart ?? '',
      end: p.waveEnd ?? '',
      children: []
    },
    pattern_type: 'Impulse' as never,
    initial_direction: 'Up',
    compacted_base_label: { label: 'Five', certainty: 'Primary' } as never,
    structure_label: p.structure_label ?? '5-3-5 Zigzag',
    complexity_level: 'Simple',
    power_rating: p.power_rating ?? 'Bullish',
    max_retracement: null,
    post_pattern_behavior: 'Continuation' as never,
    passed_rules: [],
    deferred_rules: [],
    rules_passed_count: p.rules_passed_count ?? 0,
    deferred_rules_count: p.deferred_rules_count ?? 0,
    invalidation_triggers: p.invalidationTriggers ?? p.invalidation_triggers ?? [],
    expected_fib_zones: p.expected_fib_zones ?? [],
    structural_facts: {} as never,
    advisory_findings: [],
    in_triangle_context: false,
    awaiting_l_label: false,
    monowave_structure_labels: p.monowave_structure_labels ?? [],
    round_state: 'Round1' as never,
    pattern_isolation_anchors: [],
    triplexity_detected: false
  };
}

function mkTrigger(kind: 'PriceBreakBelow' | 'PriceBreakAbove', price: number): Trigger {
  return {
    trigger_type: { [kind]: price } as never,
    on_trigger: 'InvalidateScenario',
    rule_reference: { Ch5_Essential: 3 } as never,
    neely_page: 'test'
  };
}

function mkMonowave(end_date: string, end_price: number): Monowave {
  return {
    start_date: '2020-01-01',
    end_date,
    start_price: 0,
    end_price,
    direction: 'Up',
    bar_indices: [0, 0]
  };
}

describe('powerRank', () => {
  it('StrongBullish=3, Neutral=0, StrongBearish=-3', () => {
    expect(powerRank('StrongBullish')).toBe(3);
    expect(powerRank('Bullish')).toBe(2);
    expect(powerRank('Neutral')).toBe(0);
    expect(powerRank('StrongBearish')).toBe(-3);
  });
});

describe('powerAbsLevel', () => {
  it('|3| = Strong / |2|or|1| = Avg / 0 = Weak', () => {
    expect(powerAbsLevel('StrongBullish')).toBe('Strong');
    expect(powerAbsLevel('StrongBearish')).toBe('Strong');
    expect(powerAbsLevel('Bullish')).toBe('Avg');
    expect(powerAbsLevel('SlightBearish')).toBe('Avg');
    expect(powerAbsLevel('Neutral')).toBe('Weak');
  });
});

describe('powerDirection', () => {
  it('bullish / bearish / neutral', () => {
    expect(powerDirection('Bullish')).toBe('bullish');
    expect(powerDirection('Bearish')).toBe('bearish');
    expect(powerDirection('Neutral')).toBe('neutral');
  });
});

describe('powerLabel', () => {
  it('中文標籤', () => {
    expect(powerLabel('StrongBullish')).toBe('強多');
    expect(powerLabel('Neutral')).toBe('中性');
    expect(powerLabel('StrongBearish')).toBe('強空');
  });
});

describe('sortScenarios', () => {
  it('依 power_rank DESC,然後 passed_count DESC', () => {
    const list = [
      mkScenario({ id: 'A', power_rating: 'Bullish', rules_passed_count: 5 }),
      mkScenario({ id: 'B', power_rating: 'StrongBullish', rules_passed_count: 3 }),
      mkScenario({ id: 'C', power_rating: 'Bullish', rules_passed_count: 8 }),
      mkScenario({ id: 'D', power_rating: 'Neutral', rules_passed_count: 10 })
    ];
    const sorted = sortScenarios(list);
    expect(sorted.map((s) => s.id)).toEqual(['B', 'C', 'A', 'D']);
  });

  it('不修改原 array', () => {
    const list = [
      mkScenario({ id: 'A', power_rating: 'Bullish' }),
      mkScenario({ id: 'B', power_rating: 'StrongBullish' })
    ];
    const before = list.map((s) => s.id).join(',');
    sortScenarios(list);
    expect(list.map((s) => s.id).join(',')).toBe(before);
  });
});

describe('topNScenarios', () => {
  it('取前 N 條(按 power 排序)', () => {
    const list = Array.from({ length: 10 }, (_, i) =>
      mkScenario({ id: `S${i}`, power_rating: i % 2 === 0 ? 'Bullish' : 'Neutral' })
    );
    const top3 = topNScenarios(list, 3);
    expect(top3).toHaveLength(3);
    // 前 3 應該都是 Bullish(power_rank=2)
    expect(top3.every((s) => s.power_rating === 'Bullish')).toBe(true);
  });
});

describe('scenarioRecencyDays', () => {
  it('近期結尾 → 正小數字', () => {
    const s = mkScenario({ waveEnd: '2026-06-01' });
    expect(scenarioRecencyDays(s, '2026-06-06')).toBeCloseTo(5);
  });

  it('結尾在 as_of 之後(未來投影)→ 負數', () => {
    const s = mkScenario({ waveEnd: '2026-09-01' });
    const days = scenarioRecencyDays(s, '2026-06-06');
    expect(days).toBeLessThan(0);
  });

  it('無 wave_tree.end → Infinity', () => {
    const s = mkScenario({ waveEnd: '' });
    expect(scenarioRecencyDays(s, '2026-06-06')).toBe(Number.POSITIVE_INFINITY);
  });

  it('asOf=null → Infinity', () => {
    const s = mkScenario({ waveEnd: '2026-06-01' });
    expect(scenarioRecencyDays(s, null)).toBe(Number.POSITIVE_INFINITY);
  });
});

describe('pickDefaultScenario', () => {
  it('優先選 1 年內結尾的 scenario,不被舊高 power 蓋過(regression: 2022 stale)', () => {
    const list = [
      mkScenario({ id: 'OLD', power_rating: 'StrongBullish', waveEnd: '2022-03-15' }),
      mkScenario({ id: 'RECENT', power_rating: 'SlightBullish', waveEnd: '2026-05-20' })
    ];
    const picked = pickDefaultScenario(list, '2026-06-06');
    expect(picked?.id).toBe('RECENT');
  });

  it('同一 recency tier 內按 power 排序', () => {
    const list = [
      mkScenario({ id: 'WEAK_RECENT', power_rating: 'SlightBullish', waveEnd: '2026-05-15' }),
      mkScenario({ id: 'STRONG_RECENT', power_rating: 'StrongBullish', waveEnd: '2026-04-30' })
    ];
    expect(pickDefaultScenario(list, '2026-06-06')?.id).toBe('STRONG_RECENT');
  });

  it('全部 stale → 仍按 power 排序取最強(graceful fallback)', () => {
    const list = [
      mkScenario({ id: 'OLD_WEAK', power_rating: 'SlightBullish', waveEnd: '2021-03-15' }),
      mkScenario({ id: 'OLD_STRONG', power_rating: 'StrongBullish', waveEnd: '2022-03-15' })
    ];
    expect(pickDefaultScenario(list, '2026-06-06')?.id).toBe('OLD_STRONG');
  });

  it('asOf=null → 退回 power-only sort', () => {
    const list = [
      mkScenario({ id: 'A', power_rating: 'Bullish', waveEnd: '2024-01-01' }),
      mkScenario({ id: 'B', power_rating: 'StrongBullish', waveEnd: '2022-01-01' })
    ];
    expect(pickDefaultScenario(list, null)?.id).toBe('B');
  });

  it('空 list → null', () => {
    expect(pickDefaultScenario([], '2026-06-06')).toBeNull();
  });

  it('windowDays=180 (半年) 可調', () => {
    const list = [
      mkScenario({ id: 'M9', power_rating: 'SlightBullish', waveEnd: '2025-09-01' }), // 9 個月前 → tier-out
      mkScenario({ id: 'M3', power_rating: 'SlightBearish', waveEnd: '2026-03-01' }) // 3 個月前 → tier-in
    ];
    expect(pickDefaultScenario(list, '2026-06-06', 180)?.id).toBe('M3');
  });
});

describe('extractCurrentPriceFromMonowaves', () => {
  it('回最後一筆 end_price', () => {
    const mws = [mkMonowave('2026-06-01', 2400), mkMonowave('2026-06-05', 2425)];
    expect(extractCurrentPriceFromMonowaves(mws)).toBe(2425);
  });

  it('空陣列 → null', () => {
    expect(extractCurrentPriceFromMonowaves([])).toBeNull();
  });
});

describe('isScenarioInvalidated', () => {
  it('PriceBreakBelow + current < trigger → true', () => {
    const s = mkScenario({
      invalidationTriggers: [mkTrigger('PriceBreakBelow', 1000)]
    });
    expect(isScenarioInvalidated(s, 900)).toBe(true);
  });

  it('PriceBreakBelow + current > trigger → false', () => {
    const s = mkScenario({
      invalidationTriggers: [mkTrigger('PriceBreakBelow', 1000)]
    });
    expect(isScenarioInvalidated(s, 1100)).toBe(false);
  });

  it('PriceBreakAbove + current > trigger → true', () => {
    const s = mkScenario({
      invalidationTriggers: [mkTrigger('PriceBreakAbove', 2327.5)]
    });
    expect(isScenarioInvalidated(s, 2425)).toBe(true);
  });

  it('PriceBreakAbove + current < trigger → false', () => {
    const s = mkScenario({
      invalidationTriggers: [mkTrigger('PriceBreakAbove', 2327.5)]
    });
    expect(isScenarioInvalidated(s, 2300)).toBe(false);
  });

  it('trigger price=0 視為 placeholder,不算 invalidated', () => {
    const s = mkScenario({
      invalidationTriggers: [mkTrigger('PriceBreakBelow', 0)]
    });
    expect(isScenarioInvalidated(s, 100)).toBe(false);
  });

  it('current=null → 不過濾', () => {
    const s = mkScenario({
      invalidationTriggers: [mkTrigger('PriceBreakBelow', 1000)]
    });
    expect(isScenarioInvalidated(s, null)).toBe(false);
  });

  it('WeakenScenario trigger 不算 invalidation', () => {
    const t: Trigger = {
      trigger_type: { PriceBreakBelow: 1000 } as never,
      on_trigger: 'WeakenScenario',
      rule_reference: { Ch5_Essential: 3 } as never,
      neely_page: 'test'
    };
    const s = mkScenario({ invalidationTriggers: [t] });
    expect(isScenarioInvalidated(s, 900)).toBe(false);
  });
});

describe('recencyTier', () => {
  it('tier 3:≤ 60 天', () => {
    expect(recencyTier(0)).toBe(3);
    expect(recencyTier(30)).toBe(3);
    expect(recencyTier(60)).toBe(3);
  });

  it('tier 2:61-180 天', () => {
    expect(recencyTier(61)).toBe(2);
    expect(recencyTier(100)).toBe(2);
    expect(recencyTier(180)).toBe(2);
  });

  it('tier 1:181-365 天', () => {
    expect(recencyTier(181)).toBe(1);
    expect(recencyTier(242)).toBe(1);
    expect(recencyTier(365)).toBe(1);
  });

  it('tier 0:> 365 天', () => {
    expect(recencyTier(366)).toBe(0);
    expect(recencyTier(1320)).toBe(0);
  });
});

describe('pickDefaultScenario invalidation filter + tier', () => {
  it('invalidated scenario 推到後面(即使最近)', () => {
    const list = [
      mkScenario({
        id: 'BROKEN_RECENT',
        waveEnd: '2026-05-20',
        power_rating: 'Bullish',
        invalidationTriggers: [mkTrigger('PriceBreakAbove', 2327.5)]
      }),
      mkScenario({
        id: 'VALID_RECENT',
        waveEnd: '2026-05-07',
        power_rating: 'Neutral',
        invalidationTriggers: [mkTrigger('PriceBreakBelow', 2040)]
      })
    ];
    const picked = pickDefaultScenario(list, '2026-06-06', { currentPrice: 2425 });
    expect(picked?.id).toBe('VALID_RECENT');
  });

  it('tier 化:tier 3 Neutral 勝過 tier 1 StrongBullish', () => {
    const list = [
      mkScenario({
        id: 'OLD_STRONG',
        waveEnd: '2025-10-07',
        power_rating: 'StrongBullish',
        invalidationTriggers: [mkTrigger('PriceBreakBelow', 1100)]
      }),
      mkScenario({
        id: 'NEW_WEAK',
        waveEnd: '2026-05-07',
        power_rating: 'Neutral',
        invalidationTriggers: [mkTrigger('PriceBreakBelow', 2000)]
      })
    ];
    const picked = pickDefaultScenario(list, '2026-06-06', { currentPrice: 2425 });
    expect(picked?.id).toBe('NEW_WEAK');
  });

  it('regression: 2330 2026-06 production case — 應選 c3-mw236-mw238(30d valid Neutral)而非 c5-mw194-mw198(242d valid StrongBullish)', () => {
    const c5mw194 = mkScenario({
      id: 'c5-mw194-mw198',
      waveEnd: '2025-10-07',
      power_rating: 'StrongBullish',
      invalidationTriggers: [mkTrigger('PriceBreakBelow', 1157.195)]
    });
    const c3mw236 = mkScenario({
      id: 'c3-mw236-mw238',
      waveEnd: '2026-05-07',
      power_rating: 'Neutral',
      invalidationTriggers: [mkTrigger('PriceBreakBelow', 2040)]
    });
    const c5mw29 = mkScenario({
      id: 'c5-mw29-mw33',
      waveEnd: '2022-10-26',
      power_rating: 'StrongBearish',
      invalidationTriggers: [mkTrigger('PriceBreakAbove', 528.03)] // 2425 ≫ 528 → invalidated
    });
    const c3mw239 = mkScenario({
      id: 'c3-mw239-mw241',
      waveEnd: '2026-05-20',
      power_rating: 'Neutral',
      invalidationTriggers: [mkTrigger('PriceBreakAbove', 2327.5)] // 2425 > 2327 → invalidated
    });

    const picked = pickDefaultScenario(
      [c5mw194, c3mw236, c5mw29, c3mw239],
      '2026-06-06',
      { currentPrice: 2425 }
    );
    expect(picked?.id).toBe('c3-mw236-mw238');
  });

  it('全部 invalidated → 仍按 tier+power 排,取最強', () => {
    const list = [
      mkScenario({
        id: 'A',
        waveEnd: '2026-05-20',
        power_rating: 'Bullish',
        invalidationTriggers: [mkTrigger('PriceBreakAbove', 2000)]
      }),
      mkScenario({
        id: 'B',
        waveEnd: '2026-05-25',
        power_rating: 'StrongBullish',
        invalidationTriggers: [mkTrigger('PriceBreakAbove', 2100)]
      })
    ];
    const picked = pickDefaultScenario(list, '2026-06-06', { currentPrice: 2425 });
    expect(picked?.id).toBe('B'); // StrongBullish 勝
  });

  it('currentPrice=null → 不過濾,退回 tier+power sort', () => {
    const list = [
      mkScenario({
        id: 'OLD_STRONG',
        waveEnd: '2025-10-07',
        power_rating: 'StrongBullish'
      }),
      mkScenario({
        id: 'NEW_WEAK',
        waveEnd: '2026-05-07',
        power_rating: 'Neutral'
      })
    ];
    const picked = pickDefaultScenario(list, '2026-06-06', { currentPrice: null });
    expect(picked?.id).toBe('NEW_WEAK'); // tier 3 vs tier 1,tier 勝
  });

  it('backward compat:第 3 參數可是 number windowDays', () => {
    const list = [
      mkScenario({ id: 'A', waveEnd: '2026-05-07', power_rating: 'Neutral' })
    ];
    const picked = pickDefaultScenario(list, '2026-06-06', 365);
    expect(picked?.id).toBe('A');
  });
});

describe('collectLiveFibZones', () => {
  function mkZone(label: string, low: number, high: number): FibZone {
    return { label, low, high, source_ratio: 0.382 };
  }

  it('只收 live(≤180d,tier ≥ 2)scenario 的 zones', () => {
    const list = [
      mkScenario({
        id: 'LIVE',
        waveEnd: '2026-05-01',
        expected_fib_zones: [mkZone('.382', 2100, 2150)]
      }),
      mkScenario({
        id: 'OLD',
        waveEnd: '2022-03-15',
        expected_fib_zones: [mkZone('.618', 500, 520)] // 2022 價位 — 不該進今天的雲層
      })
    ];
    const zones = collectLiveFibZones(list, '2026-06-06');
    expect(zones).toHaveLength(1);
    expect(zones[0].label).toBe('.382');
  });

  it('去重 key = label|low|high(跨 scenario 同 zone 只留一份)', () => {
    const list = [
      mkScenario({
        id: 'A',
        waveEnd: '2026-05-01',
        expected_fib_zones: [mkZone('.382', 2100, 2150)]
      }),
      mkScenario({
        id: 'B',
        waveEnd: '2026-05-20',
        expected_fib_zones: [mkZone('.382', 2100, 2150), mkZone('1.618', 2600, 2680)]
      })
    ];
    expect(collectLiveFibZones(list, '2026-06-06')).toHaveLength(2);
  });

  it('全 stale forest → 空陣列(呼叫端隱藏雲層 + 提示)', () => {
    const list = [
      mkScenario({ id: 'OLD1', waveEnd: '2022-03-15', expected_fib_zones: [mkZone('.382', 500, 520)] }),
      mkScenario({ id: 'OLD2', waveEnd: '2023-08-24', expected_fib_zones: [mkZone('.618', 600, 620)] })
    ];
    expect(collectLiveFibZones(list, '2026-06-06')).toEqual([]);
  });

  it('181-365d(tier 1)不算 live', () => {
    const list = [
      mkScenario({ waveEnd: '2025-09-01', expected_fib_zones: [mkZone('.5', 1900, 1950)] })
    ];
    expect(collectLiveFibZones(list, '2026-06-06')).toEqual([]);
  });

  it('asOf=null → 無法判 recency → 空(不畫雲層)', () => {
    const list = [
      mkScenario({ waveEnd: '2026-05-01', expected_fib_zones: [mkZone('.382', 1, 2)] })
    ];
    expect(collectLiveFibZones(list, null)).toEqual([]);
  });
});

describe('scenarioPrimaryCertainty', () => {
  it('多 monowave label 中取最強 Certainty', () => {
    const s = mkScenario({
      monowave_structure_labels: [
        {
          monowave_index: 0,
          classified_index: 0,
          labels: [
            { label: 'F3', certainty: 'Possible' } as never,
            { label: 'L5', certainty: 'Rare' } as never
          ],
          pass1_only_labels: []
        },
        {
          monowave_index: 1,
          classified_index: 1,
          labels: [{ label: 'C3', certainty: 'Primary' } as never],
          pass1_only_labels: []
        }
      ]
    });
    expect(scenarioPrimaryCertainty(s)).toBe('Primary');
  });

  it('全空 → null', () => {
    expect(scenarioPrimaryCertainty(mkScenario({ monowave_structure_labels: [] }))).toBeNull();
  });
});
