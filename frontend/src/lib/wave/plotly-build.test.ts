import { describe, expect, it } from 'vitest';
import type { Monowave } from '$contracts/neely/Monowave';
import {
  buildAnnotations,
  buildLayout,
  buildShapes,
  buildTraces,
  computeDefaultXRange
} from './plotly-build';

function mkMonowave(
  start_date: string,
  end_date: string,
  start_price: number,
  end_price: number,
  direction: 'Up' | 'Down' | 'Neutral' = 'Up'
): Monowave {
  return { start_date, end_date, start_price, end_price, direction, bar_indices: [0, 0] };
}

describe('buildTraces', () => {
  it('產生 close 線 trace', () => {
    const mws = [
      mkMonowave('2026-01-01', '2026-01-05', 100, 110, 'Up'),
      mkMonowave('2026-01-05', '2026-01-10', 110, 105, 'Down')
    ];
    const traces = buildTraces({ monowaves: mws, fibZones: [] });
    expect(traces.length).toBeGreaterThanOrEqual(1);
    const price = traces[0];
    expect(price.type).toBe('scatter');
    expect(price.mode).toBe('lines');
    expect(price.x.length).toBe(3); // start, mid, end
    expect(price.y).toEqual([100, 110, 105]);
  });

  it('空 monowaves → 空 trace x/y', () => {
    const traces = buildTraces({ monowaves: [], fibZones: [] });
    expect(traces[0].x).toEqual([]);
    expect(traces[0].y).toEqual([]);
  });
});

describe('buildShapes', () => {
  it('fib zone → line + rect (有寬度時)', () => {
    const shapes = buildShapes({
      monowaves: [],
      fibZones: [{ label: '.382', low: 95, high: 110, source_ratio: 0.382 }]
    });
    const lines = shapes.filter((s) => s.type === 'line');
    const rects = shapes.filter((s) => s.type === 'rect');
    expect(lines.length).toBeGreaterThanOrEqual(1);
    expect(rects.length).toBeGreaterThanOrEqual(1);
  });

  it('layers.fib=false → 不產 fib shape', () => {
    const shapes = buildShapes({
      monowaves: [],
      fibZones: [{ label: '.382', low: 95, high: 110, source_ratio: 0.382 }],
      layers: { fib: false, waveMarkers: true, track2: true, invalidation: true }
    });
    expect(shapes).toHaveLength(0);
  });

  it('invalidation PriceBreakBelow → horizontal line', () => {
    const shapes = buildShapes({
      monowaves: [],
      fibZones: [],
      invalidationTriggers: [
        {
          trigger_type: { PriceBreakBelow: 318.0 },
          on_trigger: 'InvalidateScenario',
          rule_reference: 'Ch5_Essential' as never,
          neely_page: 'p.4-12'
        }
      ]
    });
    expect(shapes).toHaveLength(1);
    expect(shapes[0].type).toBe('line');
    expect(shapes[0].y0).toBe(318.0);
  });

  it('Track2 band → rect on right edge (x ref=paper)', () => {
    const shapes = buildShapes({
      monowaves: [],
      fibZones: [],
      track2Bands: [{ low: 100, high: 120, horizon: '21d' }]
    });
    const rect = shapes.find((s) => s.type === 'rect');
    expect(rect).toBeDefined();
    expect(rect?.xref).toBe('paper');
    expect(rect?.x0).toBe(0.78);
    expect(rect?.x1).toBe(1.0);
  });

  it('as_of → 垂直虛線 (y ref=paper)', () => {
    const shapes = buildShapes({
      monowaves: [],
      fibZones: [],
      asOf: '2026-06-06'
    });
    const vline = shapes.find((s) => s.x0 === '2026-06-06' && s.x1 === '2026-06-06');
    expect(vline).toBeDefined();
    expect(vline?.yref).toBe('paper');
  });
});

describe('buildAnnotations', () => {
  it('fib zone → 文字標籤', () => {
    const ann = buildAnnotations({
      monowaves: [],
      fibZones: [{ label: '.382', low: 95, high: 110, source_ratio: 0.382 }]
    });
    expect(ann).toHaveLength(1);
    expect(ann[0].text).toContain('.382');
  });
});

describe('buildLayout', () => {
  it('暗色主題對齊 wireframe', () => {
    const layout = buildLayout({ monowaves: [], fibZones: [] });
    expect(layout.paper_bgcolor).toBe('#162439');
    expect(layout.plot_bgcolor).toBe('#0d1626');
    expect(layout.font.family).toContain('IBM Plex Mono');
    expect(layout.showlegend).toBe(false);
    expect(layout.dragmode).toBe('pan');
  });

  it('無 asOf → xaxis.range 不設(autorange)', () => {
    const layout = buildLayout({ monowaves: [], fibZones: [] });
    expect(layout.xaxis.range).toBeUndefined();
  });

  it('有 asOf → xaxis.range 設成預設窗口', () => {
    const layout = buildLayout({ monowaves: [], fibZones: [], asOf: '2026-06-06' });
    expect(layout.xaxis.range).toBeDefined();
    const range = layout.xaxis.range as [string, string];
    // 預設 back=365, forward=90
    expect(range[0]).toMatch(/^2025-06-/); // ~1 年前
    expect(range[1]).toMatch(/^2026-09-/); // ~3 個月後投影 buffer
    expect(layout.xaxis.autorange).toBe(false);
  });

  it('顯式 xRange 蓋過 asOf 計算', () => {
    const layout = buildLayout({
      monowaves: [],
      fibZones: [],
      asOf: '2026-06-06',
      xRange: ['2020-01-01', '2030-01-01']
    });
    expect(layout.xaxis.range).toEqual(['2020-01-01', '2030-01-01']);
  });

  it('xRangeDaysBack 可調', () => {
    const layout = buildLayout({
      monowaves: [],
      fibZones: [],
      asOf: '2026-06-06',
      xRangeDaysBack: 30
    });
    const range = layout.xaxis.range as [string, string];
    expect(range[0]).toMatch(/^2026-05-/); // ~30 天前
  });
});

describe('computeDefaultXRange', () => {
  it('asOf=null → null', () => {
    expect(computeDefaultXRange({ monowaves: [], fibZones: [] })).toBeNull();
  });

  it('invalid asOf → null', () => {
    expect(
      computeDefaultXRange({ monowaves: [], fibZones: [], asOf: 'not-a-date' })
    ).toBeNull();
  });
});
