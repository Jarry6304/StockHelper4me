/**
 * Traditional (Frost & Prechter EWP) chart helpers — 與 Neely 並排不整合。
 *
 * Traditional 自有 vertical:
 *   - 用 `pivot_series`(High/Low 交替 pivot 點)作 price 線(非 monowave_series)
 *   - 用 `scenario.wave_tree.children` 取 wave marker(每個 child 有 start/end_date
 *     + start/end_price 直接給座標,不像 Neely 要 join monowave_series)
 *   - invalidation_triggers shape `{kind, price, note}`(不是 Neely 的 trigger_type 子型別)
 *   - 不分 Track2(那是 Neely fusion-only)
 */

import type { FibZone } from '$contracts/neely/FibZone';
import type { TraditionalForestOutput, TraditionalScenario } from '$lib/api/traditional';
import type {
  ClosePoint,
  PlotlyAnnotation,
  PlotlyLayout,
  PlotlyShape,
  PlotlyTrace
} from './plotly-build';

export interface TradPivot {
  date: string;
  kind: 'High' | 'Low';
  price: number;
  bar_index: number;
}

export interface TradInvalidationTrigger {
  kind: string;
  price: number;
  note?: string;
  rule_reference?: string;
}

export interface TradWaveNode {
  label: string;
  start: string;
  end: string;
  children?: TradWaveNode[];
  start_price?: number;
  end_price?: number;
}

export interface TradBuildOptions {
  pivots: TradPivot[];
  selectedScenario: TraditionalScenario | null;
  asOf?: string;
  layers?: { fib: boolean; waveMarkers: boolean; invalidation: boolean };
  xRangeDaysBack?: number;
  xRangeDaysForward?: number;
  /** user 點了「全部」preset → autorange(computeTradXRange 回 null)。 */
  forceAutorange?: boolean;
  /** user 點了顯式範圍 preset → 純 asOf 錨定窗,不做形態擴窗/錨定。 */
  explicitRange?: boolean;
  /** 後復權收盤序列 — 淡色時間背景線(pivot 折線太稀疏,單獨看沒有時間感)。 */
  closeSeries?: ClosePoint[];
}

const COL_BG = '#0d1626';
const COL_PANEL = '#162439';
const COL_INK = '#c9d8ee';
const COL_WAVE = '#56d4f0';
const COL_FIB = '#f3b14e';
const COL_INVAL = '#ff6a7a';
const COL_GRID = 'rgba(120, 160, 210, 0.10)';

const DEFAULT_LAYERS = { fib: true, waveMarkers: true, invalidation: true };

function isoDate(d: Date): string {
  return `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, '0')}-${String(d.getUTCDate()).padStart(2, '0')}`;
}

export function computeTradXRange(opts: TradBuildOptions): [string, string] | null {
  if (opts.forceAutorange) return null;
  const anchor = opts.asOf;
  if (!anchor) return null;
  const t = Date.parse(anchor);
  if (Number.isNaN(t)) return null;
  const back = opts.xRangeDaysBack ?? 365;
  const forward = opts.xRangeDaysForward ?? 90;
  let fromMs = t - back * 86400000;
  let toMs = t + forward * 86400000;

  // 顯式 preset(user 點了範圍鈕)→ 純 asOf 錨定,不做形態擴窗/錨定
  if (opts.explicitRange) return [isoDate(new Date(fromMs)), isoDate(new Date(toMs))];

  const tree = opts.selectedScenario?.wave_tree as TradWaveNode | undefined;
  const ws = tree?.start ? Date.parse(tree.start) : Number.NaN;
  const we = tree?.end ? Date.parse(tree.end) : Number.NaN;

  // 整段形態都在預設窗外(stale)→ 窗錨定形態本身,不硬拉到 asOf
  // (老形態 + asOf 同框會把 x 軸攤成 2+ 年、刻度壓扁 — user 回報「沒有時間軸」主因)
  if (!Number.isNaN(we) && we < fromMs) {
    const fromAnchor = !Number.isNaN(ws) ? ws : we;
    let f = fromAnchor - 30 * 86400000;
    const t2 = we + 90 * 86400000;
    // corrective 形態常只跨數週 → 錨定窗會剩 3-4 個月(user 回報 x 軸太短);
    // 保底 12 個月跨度,向過去補滿(形態維持在窗的右半)
    const MIN_SPAN = 365 * 86400000;
    if (t2 - f < MIN_SPAN) f = t2 - MIN_SPAN;
    return [isoDate(new Date(f)), isoDate(new Date(t2))];
  }

  // 部分重疊:維持擴窗,把窗口拉大包含整個 wave_tree
  if (!Number.isNaN(ws) && ws < fromMs) fromMs = ws - 30 * 86400000;
  if (!Number.isNaN(we) && we > toMs) toMs = we + 30 * 86400000;

  return [isoDate(new Date(fromMs)), isoDate(new Date(toMs))];
}

export function computeTradFibProjectionRange(
  opts: TradBuildOptions
): [string, string] | null {
  if (!opts.selectedScenario?.wave_tree) return null;
  const start = (opts.selectedScenario.wave_tree as TradWaveNode).end;
  if (!start) return null;

  const forward = opts.xRangeDaysForward ?? 90;
  const anchorIso = opts.asOf ?? (opts.pivots.length > 0
    ? opts.pivots[opts.pivots.length - 1].date
    : null);
  if (!anchorIso) return null;
  const t = Date.parse(anchorIso);
  if (Number.isNaN(t)) return null;
  const end = isoDate(new Date(t + forward * 86400000));
  if (Date.parse(end) <= Date.parse(start)) return null;
  return [start, end];
}

/** 從 wave_tree.children 平展 (date, price, label) 點序列。 */
export function flattenTradWaveTree(
  node: TradWaveNode
): Array<{ date: string; price: number; label: string }> {
  if (!node || !node.children || node.children.length === 0) return [];
  const points: Array<{ date: string; price: number; label: string }> = [];
  // 第一個 child 的 start 當 root anchor
  const first = node.children[0];
  if (typeof first.start_price === 'number') {
    points.push({ date: first.start, price: first.start_price, label: '' });
  }
  for (const child of node.children) {
    if (typeof child.end_price === 'number') {
      points.push({ date: child.end, price: child.end_price, label: child.label || '' });
    }
  }
  return points;
}

export function buildTradTraces(opts: TradBuildOptions): PlotlyTrace[] {
  const layers = opts.layers ?? DEFAULT_LAYERS;
  const traces: PlotlyTrace[] = [];

  // 收盤背景線(最底層)— 同 Neely 圖,真實日線密度給 pivot zigzag 時間軸感
  if (opts.closeSeries && opts.closeSeries.length > 0) {
    traces.push({
      type: 'scatter',
      mode: 'lines',
      x: opts.closeSeries.map((p) => p.date),
      y: opts.closeSeries.map((p) => p.close),
      name: '收盤(後復權)',
      line: { color: '#8aa0bf', width: 1 },
      opacity: 0.35,
      hoverinfo: 'skip',
      showlegend: false
    });
  }

  // 主價格線 — 由 pivot_series 連線(High/Low 交替形成 ZigZag-like 路徑)
  if (opts.pivots.length > 0) {
    traces.push({
      type: 'scatter',
      mode: 'lines',
      x: opts.pivots.map((p) => p.date),
      y: opts.pivots.map((p) => p.price),
      line: { color: COL_WAVE, width: 1.6 },
      name: 'pivot 序列',
      hovertemplate: '%{x|%Y-%m-%d}<br>%{y:.2f} (pivot)<extra></extra>',
      showlegend: false
    });
  }

  // 選中 scenario 的 wave_tree(粗線高亮)
  if (layers.waveMarkers && opts.selectedScenario?.wave_tree) {
    const points = flattenTradWaveTree(opts.selectedScenario.wave_tree as TradWaveNode);
    if (points.length > 0) {
      traces.push({
        type: 'scatter',
        mode: 'lines',
        x: points.map((p) => p.date),
        y: points.map((p) => p.price),
        line: { color: COL_WAVE, width: 2.6 },
        opacity: 0.9,
        showlegend: false,
        hoverinfo: 'skip'
      });
      traces.push({
        type: 'scatter',
        mode: 'markers+text',
        x: points.map((p) => p.date),
        y: points.map((p) => p.price),
        text: points.map((p) => p.label),
        textposition: 'middle center',
        marker: {
          size: 18,
          color: COL_WAVE,
          line: { color: COL_INK, width: 0.5 }
        },
        showlegend: false,
        hovertemplate: '<b>%{text}</b><br>%{x|%Y-%m-%d}<br>%{y:.2f}<extra></extra>'
      });
    }
  }

  return traces;
}

export function buildTradShapes(opts: TradBuildOptions): PlotlyShape[] {
  const layers = opts.layers ?? DEFAULT_LAYERS;
  const shapes: PlotlyShape[] = [];

  // Fib 投影 — 從 wave_tree.end 往未來畫
  if (layers.fib && opts.selectedScenario?.expected_fib_zones) {
    const fibRange = computeTradFibProjectionRange(opts);
    const zones = (opts.selectedScenario.expected_fib_zones as FibZone[]) ?? [];
    for (const fz of zones) {
      const mid = (fz.low + fz.high) / 2;
      if (fibRange) {
        shapes.push({
          type: 'line',
          x0: fibRange[0],
          x1: fibRange[1],
          y0: mid,
          y1: mid,
          line: { color: COL_FIB, width: 1, dash: 'dash' },
          opacity: 0.55
        });
        if (Math.abs(fz.high - fz.low) > 0.001) {
          shapes.push({
            type: 'rect',
            x0: fibRange[0],
            x1: fibRange[1],
            y0: fz.low,
            y1: fz.high,
            fillcolor: COL_FIB,
            opacity: 0.05,
            line: { width: 0 },
            layer: 'below'
          });
        }
      } else {
        shapes.push({
          type: 'line',
          xref: 'paper',
          x0: 0,
          x1: 1,
          y0: mid,
          y1: mid,
          line: { color: COL_FIB, width: 1, dash: 'dash' },
          opacity: 0.55
        });
      }
    }
    if (fibRange && zones.length > 0) {
      shapes.push({
        type: 'line',
        yref: 'paper',
        x0: fibRange[0],
        x1: fibRange[0],
        y0: 0,
        y1: 1,
        line: { color: COL_FIB, width: 1, dash: 'dot' },
        opacity: 0.4
      });
    }
  }

  // 失效線(traditional 的 trigger shape {kind, price})
  if (layers.invalidation && opts.selectedScenario?.invalidation_triggers) {
    const triggers = opts.selectedScenario.invalidation_triggers as unknown as TradInvalidationTrigger[];
    for (const t of triggers) {
      if (
        (t.kind === 'PriceBreakBelow' || t.kind === 'PriceBreakAbove') &&
        typeof t.price === 'number' &&
        t.price > 0
      ) {
        shapes.push({
          type: 'line',
          xref: 'paper',
          x0: 0,
          x1: 1,
          y0: t.price,
          y1: t.price,
          line: { color: COL_INVAL, width: 1.4, dash: 'dash' },
          opacity: 0.85
        });
      }
    }
  }

  // as_of 垂直線
  if (opts.asOf) {
    shapes.push({
      type: 'line',
      yref: 'paper',
      x0: opts.asOf,
      x1: opts.asOf,
      y0: 0,
      y1: 1,
      line: { color: '#52688c', width: 1, dash: 'dot' },
      opacity: 0.55
    });
  }

  return shapes;
}

export function buildTradAnnotations(opts: TradBuildOptions): PlotlyAnnotation[] {
  const ann: PlotlyAnnotation[] = [];
  const layers = opts.layers ?? DEFAULT_LAYERS;

  if (layers.fib && opts.selectedScenario?.expected_fib_zones) {
    const fibRange = computeTradFibProjectionRange(opts);
    const zones = (opts.selectedScenario.expected_fib_zones as FibZone[]) ?? [];
    for (const fz of zones) {
      const mid = (fz.low + fz.high) / 2;
      if (fibRange) {
        ann.push({
          x: fibRange[0],
          xanchor: 'right',
          y: mid,
          text: `${fz.label} · ${mid.toFixed(1)}`,
          showarrow: false,
          font: { color: COL_FIB, size: 9, family: 'IBM Plex Mono, monospace' },
          align: 'right'
        });
      } else {
        ann.push({
          x: 0.005,
          xref: 'paper',
          y: mid,
          text: `${fz.label} · ${mid.toFixed(1)}`,
          showarrow: false,
          font: { color: COL_FIB, size: 9, family: 'IBM Plex Mono, monospace' },
          align: 'left'
        });
      }
    }
  }

  return ann;
}

/**
 * 可視 x 窗內的 y 軸範圍(同 plotly-build.ts::computeYRange 的理由:Plotly autorange
 * 看全部資料 — 6 年收盤背景線/pivot 會把 y 軸撐到數千跨度,窗內折線壓扁)。
 *
 * 候選:closeSeries / pivot(窗內)+ 選中 wave_tree 點(窗內)+ fib 帶(投影與窗
 * 重疊時)+ 失效線(price > 0)。候選不足 → null(退回 autorange)。
 */
export function computeTradYRange(
  opts: TradBuildOptions,
  xRange: [string, string] | null
): [number, number] | null {
  const layers = opts.layers ?? DEFAULT_LAYERS;
  const inWindow = (d: string) => !xRange || (d >= xRange[0] && d <= xRange[1]);
  const ys: number[] = [];

  for (const p of opts.closeSeries ?? []) {
    if (inWindow(p.date)) ys.push(p.close);
  }
  for (const pv of opts.pivots) {
    if (inWindow(pv.date)) ys.push(pv.price);
  }
  if (layers.waveMarkers && opts.selectedScenario?.wave_tree) {
    for (const pt of flattenTradWaveTree(opts.selectedScenario.wave_tree as TradWaveNode)) {
      if (inWindow(pt.date)) ys.push(pt.price);
    }
  }
  if (layers.fib && opts.selectedScenario?.expected_fib_zones) {
    const fr = computeTradFibProjectionRange(opts);
    const overlaps = !xRange || !fr || (fr[0] <= xRange[1] && fr[1] >= xRange[0]);
    if (overlaps) {
      for (const fz of (opts.selectedScenario.expected_fib_zones as FibZone[]) ?? []) {
        ys.push(fz.low);
        ys.push(fz.high);
      }
    }
  }
  if (layers.invalidation) {
    for (const t of extractTradInvalidationLines(opts.selectedScenario)) {
      ys.push(t.price);
    }
  }

  if (ys.length < 2) return null;
  const lo = Math.min(...ys);
  const hi = Math.max(...ys);
  if (!Number.isFinite(lo) || !Number.isFinite(hi) || hi < lo) return null;
  const pad = hi > lo ? (hi - lo) * 0.06 : Math.max(1, lo * 0.01);
  return [lo - pad, hi + pad];
}

export function buildTradLayout(opts: TradBuildOptions): PlotlyLayout {
  const xRange = computeTradXRange(opts);
  const xaxis: Record<string, unknown> = {
    gridcolor: COL_GRID,
    zerolinecolor: COL_GRID,
    tickfont: { color: '#7d92b3' }
  };
  if (xRange) {
    xaxis.range = xRange;
    xaxis.autorange = false;
  }

  const yaxis: Record<string, unknown> = {
    gridcolor: COL_GRID,
    zerolinecolor: COL_GRID,
    tickfont: { color: '#7d92b3' }
  };
  const yRange = computeTradYRange(opts, xRange);
  if (yRange) {
    yaxis.range = yRange;
    yaxis.autorange = false;
  }

  return {
    paper_bgcolor: COL_PANEL,
    plot_bgcolor: COL_BG,
    font: { family: 'IBM Plex Mono, monospace', size: 11, color: COL_INK },
    margin: { l: 40, r: 30, t: 20, b: 36 },
    hovermode: 'x',
    dragmode: 'pan',
    shapes: buildTradShapes(opts),
    annotations: buildTradAnnotations(opts),
    xaxis,
    yaxis,
    showlegend: false
  };
}

/** 算 scenario 結尾距 asOf 天數;無資料 → Infinity。 */
export function tradRecencyDays(
  scenario: TraditionalScenario,
  asOf: string | null
): number {
  if (!asOf) return Number.POSITIVE_INFINITY;
  const tree = scenario.wave_tree as TradWaveNode | undefined;
  if (!tree?.end) return Number.POSITIVE_INFINITY;
  const t = Date.parse(tree.end);
  const a = Date.parse(asOf);
  if (Number.isNaN(t) || Number.isNaN(a)) return Number.POSITIVE_INFINITY;
  return (a - t) / 86400000;
}

/** Recency tier 階梯化(對齊 Neely 的 power.ts recencyTier):3=≤60d / 2=≤180d / 1=≤365d / 0=>365d。 */
export function tradRecencyTier(days: number): number {
  if (!Number.isFinite(days)) return 0;
  if (days <= 60) return 3;
  if (days <= 180) return 2;
  if (days <= 365) return 1;
  return 0;
}

/**
 * 排序 traditional scenarios:
 *   1. recency tier DESC(近期優先;asOf=null 時跳過 tier 排序)
 *   2. preference_score DESC
 *   3. wave_tree.end DESC(同 tier+同分時取最新)
 *
 * 對齊 user 反饋「為什麼不是近一年內到現在」— 把近期形態頂到前面;若全 forest
 * 都是歷史形態(tier 0),仍按 preference_score 排,但呼叫端應顯示「近期無形態」警示。
 */
export function sortTradScenarios<T extends TraditionalScenario>(
  scenarios: T[],
  asOf: string | null = null
): T[] {
  return [...scenarios].sort((a, b) => {
    if (asOf) {
      const ta = tradRecencyTier(tradRecencyDays(a, asOf));
      const tb = tradRecencyTier(tradRecencyDays(b, asOf));
      if (ta !== tb) return tb - ta;
    }
    const pa = typeof a.preference_score === 'number' ? a.preference_score : 0;
    const pb = typeof b.preference_score === 'number' ? b.preference_score : 0;
    if (pa !== pb) return pb - pa;
    const ea = (a.wave_tree as TradWaveNode | undefined)?.end ?? '';
    const eb = (b.wave_tree as TradWaveNode | undefined)?.end ?? '';
    return ea > eb ? -1 : ea < eb ? 1 : 0;
  });
}

export interface TradInvalidationLine {
  kind: 'PriceBreakBelow' | 'PriceBreakAbove' | string;
  price: number;
  note?: string;
}

/** 篩 valid invalidation triggers(price > 0 且 kind 為兩種價格類型)。 */
export function extractTradInvalidationLines(
  scenario: TraditionalScenario | null
): TradInvalidationLine[] {
  if (!scenario?.invalidation_triggers) return [];
  const triggers = scenario.invalidation_triggers as unknown as TradInvalidationTrigger[];
  return triggers.filter(
    (t) =>
      (t.kind === 'PriceBreakBelow' || t.kind === 'PriceBreakAbove') &&
      typeof t.price === 'number' &&
      t.price > 0
  );
}
