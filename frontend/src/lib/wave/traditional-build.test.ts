import { describe, expect, it } from 'vitest';
import type { TraditionalScenario } from '$lib/api/traditional';
import {
  buildTradLayout,
  buildTradShapes,
  buildTradTraces,
  computeTradFibProjectionRange,
  computeTradXRange,
  extractTradInvalidationLines,
  flattenTradWaveTree,
  sortTradScenarios,
  tradRecencyDays,
  tradRecencyTier,
  type TradPivot,
  type TradWaveNode
} from './traditional-build';

function mkPivot(date: string, price: number, kind: 'High' | 'Low' = 'High'): TradPivot {
  return { date, price, kind, bar_index: 0 };
}

function mkTradScenario(p: Partial<TraditionalScenario> = {}): TraditionalScenario {
  return {
    id: p.id ?? 'T1',
    structure_label: p.structure_label ?? 'A-B-C (Flat)',
    preference_score: p.preference_score ?? 0,
    pattern_type: p.pattern_type ?? 'Flat',
    wave_tree: p.wave_tree,
    expected_fib_zones: p.expected_fib_zones ?? [],
    invalidation_triggers: p.invalidation_triggers ?? [],
    ...p
  } as TraditionalScenario;
}

describe('flattenTradWaveTree', () => {
  it('從 children 展平 (date, price, label)', () => {
    const tree: TradWaveNode = {
      label: 'Flat',
      start: '2025-01-01',
      end: '2025-03-31',
      start_price: 100,
      end_price: 95,
      children: [
        { label: 'A', start: '2025-01-01', end: '2025-01-31', start_price: 100, end_price: 110, children: [] },
        { label: 'B', start: '2025-01-31', end: '2025-02-28', start_price: 110, end_price: 105, children: [] },
        { label: 'C', start: '2025-02-28', end: '2025-03-31', start_price: 105, end_price: 95, children: [] }
      ]
    };
    const pts = flattenTradWaveTree(tree);
    expect(pts).toEqual([
      { date: '2025-01-01', price: 100, label: '' },
      { date: '2025-01-31', price: 110, label: 'A' },
      { date: '2025-02-28', price: 105, label: 'B' },
      { date: '2025-03-31', price: 95, label: 'C' }
    ]);
  });

  it('無 children → 空陣列', () => {
    const tree: TradWaveNode = {
      label: '',
      start: '2025-01-01',
      end: '2025-03-31',
      children: []
    };
    expect(flattenTradWaveTree(tree)).toEqual([]);
  });
});

describe('computeTradFibProjectionRange', () => {
  it('從 wave_tree.end 往未來投影', () => {
    const range = computeTradFibProjectionRange({
      pivots: [mkPivot('2026-06-01', 2400)],
      asOf: '2026-06-06',
      selectedScenario: mkTradScenario({
        wave_tree: { label: 'A', start: '2026-04-01', end: '2026-05-07', children: [] }
      })
    });
    expect(range).toBeDefined();
    expect(range?.[0]).toBe('2026-05-07');
    expect(range?.[1]).toMatch(/^2026-09-/); // 默認 +90 天
  });

  it('無 selectedScenario → null', () => {
    expect(
      computeTradFibProjectionRange({
        pivots: [],
        asOf: '2026-06-06',
        selectedScenario: null
      })
    ).toBeNull();
  });
});

describe('computeTradXRange', () => {
  it('從 asOf 算前後窗', () => {
    const range = computeTradXRange({
      pivots: [],
      asOf: '2026-06-06',
      selectedScenario: null,
      xRangeDaysBack: 365,
      xRangeDaysForward: 90
    });
    expect(range?.[0]).toMatch(/^2025-06-/);
    expect(range?.[1]).toMatch(/^2026-09-/);
  });

  it('無 asOf → null(autorange)', () => {
    expect(computeTradXRange({ pivots: [], selectedScenario: null })).toBeNull();
  });
});

describe('buildTradTraces', () => {
  it('pivots 變成 lines trace', () => {
    const traces = buildTradTraces({
      pivots: [
        mkPivot('2026-01-01', 100, 'Low'),
        mkPivot('2026-02-01', 120, 'High'),
        mkPivot('2026-03-01', 110, 'Low')
      ],
      selectedScenario: null
    });
    expect(traces[0].mode).toBe('lines');
    expect(traces[0].y).toEqual([100, 120, 110]);
  });

  it('selectedScenario 帶 wave_tree → 多 1 條粗線 + markers', () => {
    const traces = buildTradTraces({
      pivots: [mkPivot('2026-01-01', 100)],
      selectedScenario: mkTradScenario({
        wave_tree: {
          label: 'Flat',
          start: '2026-01-01',
          end: '2026-03-01',
          start_price: 100,
          end_price: 95,
          children: [
            { label: 'A', start: '2026-01-01', end: '2026-02-01', start_price: 100, end_price: 110, children: [] },
            { label: 'B', start: '2026-02-01', end: '2026-02-15', start_price: 110, end_price: 105, children: [] },
            { label: 'C', start: '2026-02-15', end: '2026-03-01', start_price: 105, end_price: 95, children: [] }
          ]
        } as TradWaveNode
      })
    });
    // pivots line + wave highlight line + markers trace
    expect(traces).toHaveLength(3);
    expect(traces[2].mode).toBe('markers+text');
    expect(traces[2].text).toEqual(['', 'A', 'B', 'C']);
  });
});

describe('buildTradShapes', () => {
  it('Fib 投影:有 selectedScenario+expected_fib_zones → 從 wave_tree.end 投影', () => {
    const shapes = buildTradShapes({
      pivots: [mkPivot('2026-06-01', 2400)],
      asOf: '2026-06-06',
      selectedScenario: mkTradScenario({
        wave_tree: { label: 'A', start: '2026-04-01', end: '2026-05-07', children: [] } as TradWaveNode,
        expected_fib_zones: [{ label: '.382', low: 2100, high: 2150, source_ratio: 0.382 }]
      })
    });
    const fibLines = shapes.filter(
      (s) => s.type === 'line' && typeof s.x0 === 'string' && s.x0.startsWith('2026-05')
    );
    expect(fibLines.length).toBeGreaterThan(0);
  });

  it('Invalidation:{kind: PriceBreakBelow, price: X} → 紅虛線', () => {
    const shapes = buildTradShapes({
      pivots: [],
      selectedScenario: mkTradScenario({
        invalidation_triggers: [
          { kind: 'PriceBreakBelow', price: 1500, note: '' } as never
        ] as never
      })
    });
    const inval = shapes.find((s) => s.y0 === 1500 && s.y1 === 1500);
    expect(inval).toBeDefined();
  });

  it('Invalidation price=0(placeholder)→ 忽略', () => {
    const shapes = buildTradShapes({
      pivots: [],
      selectedScenario: mkTradScenario({
        invalidation_triggers: [
          { kind: 'PriceBreakBelow', price: 0, note: '' } as never
        ] as never
      })
    });
    expect(shapes.find((s) => s.y0 === 0 && s.y1 === 0)).toBeUndefined();
  });
});

describe('buildTradLayout', () => {
  it('xRange 從 asOf 算', () => {
    const layout = buildTradLayout({
      pivots: [],
      selectedScenario: null,
      asOf: '2026-06-06'
    });
    expect(layout.xaxis.range).toBeDefined();
  });
});

describe('tradRecencyTier', () => {
  it('tier 階梯化', () => {
    expect(tradRecencyTier(0)).toBe(3);
    expect(tradRecencyTier(60)).toBe(3);
    expect(tradRecencyTier(61)).toBe(2);
    expect(tradRecencyTier(180)).toBe(2);
    expect(tradRecencyTier(181)).toBe(1);
    expect(tradRecencyTier(365)).toBe(1);
    expect(tradRecencyTier(366)).toBe(0);
    expect(tradRecencyTier(1320)).toBe(0);
  });
});

describe('tradRecencyDays', () => {
  it('end > asOf 算負', () => {
    const s = mkTradScenario({ wave_tree: { end: '2026-06-15' } as TradWaveNode });
    expect(tradRecencyDays(s, '2026-06-06')).toBeLessThan(0);
  });

  it('end < asOf 算正', () => {
    const s = mkTradScenario({ wave_tree: { end: '2022-10-25' } as TradWaveNode });
    expect(tradRecencyDays(s, '2026-06-06')).toBeGreaterThan(1300);
  });

  it('無 asOf → Infinity', () => {
    const s = mkTradScenario({ wave_tree: { end: '2026-06-15' } as TradWaveNode });
    expect(tradRecencyDays(s, null)).toBe(Number.POSITIVE_INFINITY);
  });
});

describe('sortTradScenarios', () => {
  it('tier 優先(近期 tier 1+ 勝過歷史 tier 0,即使 pref 較低)', () => {
    const list = [
      mkTradScenario({
        id: 'OLD_HIGH_PREF',
        preference_score: 1,
        wave_tree: { end: '2023-08-24' } as TradWaveNode // ~1018d 前 → tier 0
      }),
      mkTradScenario({
        id: 'NEW_LOW_PREF',
        preference_score: 0,
        wave_tree: { end: '2026-05-15' } as TradWaveNode // ~22d 前 → tier 3
      })
    ];
    const sorted = sortTradScenarios(list, '2026-06-06');
    expect(sorted[0].id).toBe('NEW_LOW_PREF');
  });

  it('同 tier 內按 preference_score DESC + end DESC', () => {
    const list = [
      mkTradScenario({
        id: 'A',
        preference_score: 0,
        wave_tree: { end: '2026-05-01' } as TradWaveNode
      }),
      mkTradScenario({
        id: 'B',
        preference_score: 1,
        wave_tree: { end: '2026-04-15' } as TradWaveNode
      }),
      mkTradScenario({
        id: 'C',
        preference_score: 1,
        wave_tree: { end: '2026-05-15' } as TradWaveNode
      })
    ];
    const sorted = sortTradScenarios(list, '2026-06-06');
    expect(sorted.map((s) => s.id)).toEqual(['C', 'B', 'A']);
  });

  it('全部 tier 0 仍按 preference + end 排序(graceful)', () => {
    const list = [
      mkTradScenario({
        id: 'A',
        preference_score: 1,
        wave_tree: { end: '2023-07-10' } as TradWaveNode
      }),
      mkTradScenario({
        id: 'B',
        preference_score: 1,
        wave_tree: { end: '2023-08-24' } as TradWaveNode
      })
    ];
    const sorted = sortTradScenarios(list, '2026-06-06');
    expect(sorted[0].id).toBe('B'); // 較新
  });

  it('asOf=null 退回 pref + end sort', () => {
    const list = [
      mkTradScenario({
        id: 'OLD_HIGH',
        preference_score: 1,
        wave_tree: { end: '2023-01-01' } as TradWaveNode
      }),
      mkTradScenario({
        id: 'NEW_LOW',
        preference_score: 0,
        wave_tree: { end: '2026-05-01' } as TradWaveNode
      })
    ];
    const sorted = sortTradScenarios(list, null);
    expect(sorted[0].id).toBe('OLD_HIGH'); // pref 勝(無 tier 化)
  });

  it('regression: 2330 case — 全 tier 0,T1 為 pref=1+最新 end', () => {
    const list = [
      mkTradScenario({
        id: 'Flat-291-323',
        preference_score: 1,
        wave_tree: { end: '2023-08-24' } as TradWaveNode
      }),
      mkTradScenario({
        id: 'Flat-221-291',
        preference_score: 1,
        wave_tree: { end: '2023-07-10' } as TradWaveNode
      }),
      mkTradScenario({
        id: 'Zigzag-537-546',
        preference_score: 0,
        wave_tree: { end: '2024-07-26' } as TradWaveNode
      })
    ];
    const sorted = sortTradScenarios(list, '2026-06-06');
    expect(sorted[0].id).toBe('Flat-291-323');
    expect(sorted[1].id).toBe('Flat-221-291');
    expect(sorted[2].id).toBe('Zigzag-537-546');
  });
});

describe('computeTradXRange — 形態錨定 / 擴窗 / preset', () => {
  it('selectedScenario 整段在預設窗外(stale)→ 窗錨定形態本身,不硬拉到 asOf', () => {
    const range = computeTradXRange({
      pivots: [],
      asOf: '2026-06-06',
      xRangeDaysBack: 365,
      xRangeDaysForward: 90,
      selectedScenario: mkTradScenario({
        wave_tree: { start: '2023-03-24', end: '2023-08-24' } as TradWaveNode
      })
    });
    expect(range?.[0]).toBe('2023-02-22'); // start − 30d
    expect(range?.[1]).toBe('2023-11-22'); // end + 90d(不再硬拉到 asOf+90 攤成 3 年)
  });

  it('部分重疊(start 在窗外、end 在窗內)→ 維持擴窗包含整個 wave_tree', () => {
    const range = computeTradXRange({
      pivots: [],
      asOf: '2026-06-06',
      xRangeDaysBack: 365,
      xRangeDaysForward: 90,
      selectedScenario: mkTradScenario({
        wave_tree: { start: '2024-12-01', end: '2026-01-15' } as TradWaveNode
      })
    });
    expect(range?.[0]).toBe('2024-11-01'); // start − 30d
    expect(range?.[1].startsWith('2026-09')).toBe(true); // asOf + 90d 保留
  });

  it('selectedScenario 在預設窗內 → 保留預設範圍', () => {
    const range = computeTradXRange({
      pivots: [],
      asOf: '2026-06-06',
      xRangeDaysBack: 365,
      xRangeDaysForward: 90,
      selectedScenario: mkTradScenario({
        wave_tree: { start: '2026-04-01', end: '2026-05-01' } as TradWaveNode
      })
    });
    expect(range?.[0].startsWith('2025-06')).toBe(true);
    expect(range?.[1].startsWith('2026-09')).toBe(true);
  });

  it('無 selectedScenario → 純 asOf-anchored', () => {
    const range = computeTradXRange({
      pivots: [],
      asOf: '2026-06-06',
      selectedScenario: null
    });
    expect(range?.[0].startsWith('2025-06')).toBe(true);
  });

  it('explicitRange(preset)→ 純 asOf 錨定,stale 形態不影響窗', () => {
    const range = computeTradXRange({
      pivots: [],
      asOf: '2026-06-06',
      xRangeDaysBack: 180,
      xRangeDaysForward: 120,
      explicitRange: true,
      selectedScenario: mkTradScenario({
        wave_tree: { start: '2023-03-24', end: '2023-08-24' } as TradWaveNode
      })
    });
    expect(range?.[0]).toBe('2025-12-08'); // asOf − 180d
    expect(range?.[1]).toBe('2026-10-04'); // asOf + 120d
  });

  it('forceAutorange(「全部」preset)→ null(交給 Plotly autorange)', () => {
    expect(
      computeTradXRange({
        pivots: [],
        asOf: '2026-06-06',
        forceAutorange: true,
        selectedScenario: null
      })
    ).toBeNull();
  });
});

describe('extractTradInvalidationLines', () => {
  it('過濾 invalid kind / price=0', () => {
    const s = mkTradScenario({
      invalidation_triggers: [
        { kind: 'PriceBreakBelow', price: 100 },
        { kind: 'PriceBreakBelow', price: 0 },
        { kind: 'TimeExceeds', price: 0 },
        { kind: 'PriceBreakAbove', price: 200 }
      ] as never
    });
    const lines = extractTradInvalidationLines(s);
    expect(lines).toHaveLength(2);
    expect(lines[0].price).toBe(100);
    expect(lines[1].price).toBe(200);
  });

  it('null → []', () => {
    expect(extractTradInvalidationLines(null)).toEqual([]);
  });
});
