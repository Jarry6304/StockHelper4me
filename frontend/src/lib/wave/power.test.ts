import { describe, expect, it } from 'vitest';
import type { Scenario } from '$contracts/neely/Scenario';
import {
  pickDefaultScenario,
  powerAbsLevel,
  powerDirection,
  powerLabel,
  powerRank,
  scenarioPrimaryCertainty,
  scenarioRecencyDays,
  sortScenarios,
  topNScenarios
} from './power';

function mkScenario(p: Partial<Scenario> & { waveEnd?: string; waveStart?: string } = {}): Scenario {
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
    invalidation_triggers: [],
    expected_fib_zones: [],
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
