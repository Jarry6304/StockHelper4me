/**
 * Plotly layout 構造 — 純函式,易測;不引入 Plotly runtime。
 *
 * 對映 plan Phase 3「Plotly 圖層策略」表:
 *   - close 線:scatter mode=lines
 *   - 波標:scatter mode=markers+text
 *   - fib 帶:shapes type='line' xref=paper
 *   - Track2 帶:shapes type='rect' xref=paper(右側 domain 0.78-1.0)
 *   - 失效線:shapes type='line' xref=paper
 *   - as_of 垂直:shapes type='line' yref=paper
 */

import type { FibZone } from '$contracts/neely/FibZone';
import type { Monowave } from '$contracts/neely/Monowave';
import type { Scenario } from '$contracts/neely/Scenario';
import type { Trigger } from '$contracts/neely/Trigger';
import { directionColor, flattenWaveTree, shortLabel, type WavePoint } from './coords';

/** K 棒背景最小點型(OhlcRow 結構性相容;volume 不需要)。 */
export interface OhlcPoint {
  date: string;
  open: number;
  high: number;
  low: number;
  close: number;
}

/**
 * K 棒列防衛過濾:OHLC 任一非有限數或 ≤ 0 的列丟棄。
 * (FinMind 無成交日有 0 值列;後端 /ohlc 已濾,這層防舊版後端 / dirty 資料。)
 */
export function sanitizeOhlc(rows?: OhlcPoint[] | null): OhlcPoint[] {
  if (!rows) return [];
  return rows.filter((r) =>
    [r.open, r.high, r.low, r.close].every(
      (v) => typeof v === 'number' && Number.isFinite(v) && v > 0
    )
  );
}

export interface BuildOptions {
  monowaves: Monowave[];
  /** State 1 用 live-only 聯集(collectLiveFibZones),State 2 用 selected scenario.expected_fib_zones。 */
  fibZones: FibZone[];
  /** 後復權 OHLCV(/stocks/{id}/ohlc)— K 棒時間背景;未給 → 不畫。 */
  ohlcSeries?: OhlcPoint[];
  /** State 2 用 — 選中 scenario(若有)。 */
  selectedScenario?: Scenario | null;
  /** as_of(垂直虛線)。 */
  asOf?: string;
  /** 失效線(State 2 用)。 */
  invalidationTriggers?: Trigger[];
  /** Track2 帶(State 2 — 右側獨立軌)。 */
  track2Bands?: Array<{ low: number; high: number; horizon: string }>;
  /** 圖層 toggle。 */
  layers?: {
    fib: boolean;
    waveMarkers: boolean;
    track2: boolean;
    invalidation: boolean;
  };
  /**
   * 預設 x 軸 clip(避免 production 一次回傳跨年 monowave 把 chart 攤成 4 年掃描)。
   * 若 `asOf` 有給,自動設 `[asOf - xRangeDaysBack, asOf + xRangeDaysForward]`;
   * user 仍可手動 pan/zoom 看更早歷史(Plotly 預設 dragmode=pan)。
   * 顯式給 `[from, to]` 蓋過 auto。
   */
  xRange?: [string, string] | null;
  xRangeDaysBack?: number;
  xRangeDaysForward?: number;
}

export interface PlotlyTrace {
  type: string;
  mode?: string;
  x: (string | number)[];
  /** scatter 用;candlestick 走 open/high/low/close。 */
  y?: number[];
  open?: number[];
  high?: number[];
  low?: number[];
  close?: number[];
  increasing?: Record<string, unknown>;
  decreasing?: Record<string, unknown>;
  text?: string[];
  textposition?: string;
  hoverinfo?: string;
  hovertemplate?: string;
  name?: string;
  marker?: Record<string, unknown>;
  line?: Record<string, unknown>;
  showlegend?: boolean;
  opacity?: number;
  customdata?: unknown[];
}

export interface PlotlyShape {
  type: 'line' | 'rect';
  xref?: string;
  yref?: string;
  x0: string | number;
  x1: string | number;
  y0: number;
  y1: number;
  line?: Record<string, unknown>;
  fillcolor?: string;
  opacity?: number;
  layer?: string;
}

export interface PlotlyAnnotation {
  x: string | number;
  y: string | number;
  xref?: string;
  yref?: string;
  xanchor?: 'left' | 'center' | 'right';
  yanchor?: 'top' | 'middle' | 'bottom';
  text: string;
  showarrow?: boolean;
  font?: Record<string, unknown>;
  bgcolor?: string;
  bordercolor?: string;
  borderwidth?: number;
  borderpad?: number;
  align?: string;
}

export interface PlotlyLayout {
  paper_bgcolor: string;
  plot_bgcolor: string;
  font: { family: string; size: number; color: string };
  margin: { l: number; r: number; t: number; b: number };
  hovermode: string;
  dragmode: string;
  shapes: PlotlyShape[];
  annotations: PlotlyAnnotation[];
  xaxis: Record<string, unknown>;
  yaxis: Record<string, unknown>;
  showlegend: boolean;
}

const COL_BG = '#0d1626';
const COL_PANEL = '#162439';
const COL_INK = '#c9d8ee';
const COL_WAVE = '#56d4f0';
const COL_FIB = '#f3b14e';
const COL_INVAL = '#ff6a7a';
const COL_TRACK2 = '#a08cff';
const COL_GRID = 'rgba(120, 160, 210, 0.10)';

const DEFAULT_LAYERS = { fib: true, waveMarkers: true, track2: true, invalidation: true };

export function buildTraces(opts: BuildOptions): PlotlyTrace[] {
  const layers = opts.layers ?? DEFAULT_LAYERS;
  const traces: PlotlyTrace[] = [];

  // 0. K 棒背景(最底層)— 真實日線時間密度,給 monowave 折線「時間軸感」。
  //    與引擎同源(後復權),價位才對齊 monowave/fib;台股慣例紅漲綠跌、調暗
  //    讓波浪線維持主視覺。缺列日不畫(不像折線會直接連到數月後)。
  const candles = sanitizeOhlc(opts.ohlcSeries);
  if (candles.length > 0) {
    traces.push({
      type: 'candlestick',
      x: candles.map((r) => r.date),
      open: candles.map((r) => r.open),
      high: candles.map((r) => r.high),
      low: candles.map((r) => r.low),
      close: candles.map((r) => r.close),
      name: 'K棒(後復權)',
      increasing: { line: { color: '#b8636f', width: 1 }, fillcolor: '#b8636f55' },
      decreasing: { line: { color: '#3f8f6b', width: 1 }, fillcolor: '#3f8f6b55' },
      opacity: 0.6,
      showlegend: false
    });
  }

  // 1. 主價格線 — 從 monowave_series 的 (start_date → end_date) 串接 close 推估。
  //    對齊 L7:wave 不帶 y 座標,price 來自 monowave_series。
  const priceX: string[] = [];
  const priceY: number[] = [];
  for (let i = 0; i < opts.monowaves.length; i++) {
    const mw = opts.monowaves[i];
    if (i === 0) {
      priceX.push(mw.start_date);
      priceY.push(mw.start_price);
    }
    priceX.push(mw.end_date);
    priceY.push(mw.end_price);
  }
  traces.push({
    type: 'scatter',
    mode: 'lines',
    x: priceX,
    y: priceY,
    name: '價格',
    line: { color: COL_WAVE, width: 1.6 },
    hovertemplate: '%{x|%Y-%m-%d}<br>%{y:.2f}<extra></extra>',
    showlegend: false
  });

  // 2. 波標 markers — 從 selected scenario(State 2)或 forest 第一條(State 1 樣本)取。
  if (layers.waveMarkers && opts.selectedScenario) {
    const waveTree = opts.selectedScenario.wave_tree;
    const points = flattenWaveTree(waveTree, opts.monowaves);
    addWaveMarkers(traces, points);
  }

  return traces;
}

function addWaveMarkers(traces: PlotlyTrace[], points: WavePoint[]): void {
  if (points.length === 0) return;
  const xs = points.map((p) => p.date);
  const ys = points.map((p) => p.price);
  const texts = points.map((p) => shortLabel(p.label));
  const colors = points.map((p) => directionColor(p.direction));

  traces.push({
    type: 'scatter',
    mode: 'markers+text',
    x: xs,
    y: ys,
    text: texts,
    textposition: 'middle center',
    marker: {
      size: 18,
      color: colors,
      line: { color: COL_INK, width: 0.5 }
    },
    name: '波標',
    showlegend: false,
    hovertemplate: '<b>%{text}</b><br>%{x|%Y-%m-%d}<br>%{y:.2f}<extra></extra>'
  });
}

/**
 * Fib 投影區域 — 從 wave_tree.end(或 monowave_series 末)往未來投影到 asOf+xRangeDaysForward。
 *
 * 修正 user 反饋「fib 帶應該畫在未來」:expected_fib_zones / flat_fib_zones 本質是
 * **forward projection**,不該往過去畫滿整個 chart。返回 null 時 fallback 走 paper-anchored
 * (相容無 asOf / 無 monowaves 退化場景)。
 */
export function computeFibProjectionRange(opts: BuildOptions): [string, string] | null {
  // start:優先 selected scenario 的 wave_tree.end(投影起點對齊形態結尾);否則 monowave 末筆
  let startDate: string | null = null;
  if (opts.selectedScenario?.wave_tree?.end) {
    startDate = opts.selectedScenario.wave_tree.end;
  } else if (opts.monowaves.length > 0) {
    startDate = opts.monowaves[opts.monowaves.length - 1].end_date;
  }
  if (!startDate) return null;

  // end:asOf + forward days(預設 90)
  let endDate: string | null = null;
  const forward = opts.xRangeDaysForward ?? 90;
  const anchorIso = opts.asOf ?? (opts.monowaves.length > 0
    ? opts.monowaves[opts.monowaves.length - 1].end_date
    : null);
  if (anchorIso) {
    const t = Date.parse(anchorIso);
    if (!Number.isNaN(t)) {
      const d = new Date(t + forward * 86400000);
      endDate = `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, '0')}-${String(d.getUTCDate()).padStart(2, '0')}`;
    }
  }
  if (!endDate) return null;

  // 防呆:end > start
  if (Date.parse(endDate) <= Date.parse(startDate)) return null;
  return [startDate, endDate];
}

export function buildShapes(opts: BuildOptions): PlotlyShape[] {
  const layers = opts.layers ?? DEFAULT_LAYERS;
  const shapes: PlotlyShape[] = [];

  // Fib 投影 — 從 wave_tree.end 往未來畫(對齊 user 反饋:fib 是 forward projection)
  if (layers.fib) {
    const fibRange = computeFibProjectionRange(opts);
    for (const fz of opts.fibZones) {
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
        // legacy fallback:無 asOf / 無 monowaves 退化用全寬 paper-anchored
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
        if (Math.abs(fz.high - fz.low) > 0.001) {
          shapes.push({
            type: 'rect',
            xref: 'paper',
            x0: 0,
            x1: 1,
            y0: fz.low,
            y1: fz.high,
            fillcolor: COL_FIB,
            opacity: 0.05,
            line: { width: 0 },
            layer: 'below'
          });
        }
      }
    }

    // 在投影起點畫一條垂直虛線標示「投影從這裡開始」(視覺分界)
    if (fibRange && opts.fibZones.length > 0) {
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

  // 失效水平線(State 2)
  if (layers.invalidation && opts.invalidationTriggers) {
    for (const t of opts.invalidationTriggers) {
      const tt = t.trigger_type;
      let price: number | null = null;
      if (typeof tt === 'object') {
        if ('PriceBreakBelow' in tt) price = tt.PriceBreakBelow;
        else if ('PriceBreakAbove' in tt) price = tt.PriceBreakAbove;
      }
      if (price === null) continue;
      shapes.push({
        type: 'line',
        xref: 'paper',
        x0: 0,
        x1: 1,
        y0: price,
        y1: price,
        line: { color: COL_INVAL, width: 1.4, dash: 'dash' },
        opacity: 0.85
      });
    }
  }

  // Track2 統計帶(右側獨立 domain x=0.78~1.0)
  if (layers.track2 && opts.track2Bands && opts.track2Bands.length > 0) {
    for (const band of opts.track2Bands) {
      shapes.push({
        type: 'rect',
        xref: 'paper',
        x0: 0.78,
        x1: 1.0,
        y0: band.low,
        y1: band.high,
        fillcolor: COL_TRACK2,
        opacity: 0.12,
        line: { width: 0 },
        layer: 'above'
      });
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

export function buildAnnotations(opts: BuildOptions): PlotlyAnnotation[] {
  const ann: PlotlyAnnotation[] = [];
  const layers = opts.layers ?? DEFAULT_LAYERS;

  if (layers.fib) {
    const fibRange = computeFibProjectionRange(opts);
    for (const fz of opts.fibZones) {
      const mid = (fz.low + fz.high) / 2;
      if (fibRange) {
        // 將標籤放在投影起點左側(資料軸 anchor=right),讓 fib 線往右投影、標籤往左留位
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
        // legacy paper-anchored
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

  if (layers.track2 && opts.track2Bands) {
    ann.push({
      x: 0.79,
      xref: 'paper',
      y: 1.04,
      yref: 'paper',
      text: 'Track2 統計帶(另軌)',
      showarrow: false,
      font: { color: COL_TRACK2, size: 9, family: 'IBM Plex Mono, monospace' }
    });
  }

  return ann;
}

/** 計算預設 x 軸 clip range(對應 BuildOptions.asOf + xRangeDays*)。 */
export function computeDefaultXRange(opts: BuildOptions): [string, string] | null {
  if (opts.xRange) return opts.xRange;
  if (!opts.asOf) return null;
  const asOfTime = Date.parse(opts.asOf);
  if (Number.isNaN(asOfTime)) return null;
  const back = opts.xRangeDaysBack ?? 365; // 預設 12 個月
  const forward = opts.xRangeDaysForward ?? 90; // 預設 3 個月投影 buffer
  const from = new Date(asOfTime - back * 86400000);
  const to = new Date(asOfTime + forward * 86400000);
  const iso = (d: Date) =>
    `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, '0')}-${String(d.getUTCDate()).padStart(2, '0')}`;
  return [iso(from), iso(to)];
}

/**
 * 計算可視 x 窗內的 y 軸範圍(Plotly autorange 看「全部資料」不看 x 裁窗 —
 * 收盤背景線一次塞 6 年後,y 軸會被歷史低價撐到數千跨度、把窗內折線壓扁)。
 *
 * 候選點:ohlcSeries 的 low/high / monowave 端點(窗內)+ 選中 scenario 波標(窗內)
 * + fib 帶(投影範圍與窗重疊時)+ 失效線 / Track2 帶(貼近現價的水平要素)。
 * 候選不足 → null(退回 autorange)。
 */
export function computeYRange(
  opts: BuildOptions,
  xRange: [string, string] | null
): [number, number] | null {
  const layers = opts.layers ?? DEFAULT_LAYERS;
  const inWindow = (d: string) => !xRange || (d >= xRange[0] && d <= xRange[1]);
  const ys: number[] = [];

  for (const r of sanitizeOhlc(opts.ohlcSeries)) {
    if (inWindow(r.date)) {
      ys.push(r.low);
      ys.push(r.high);
    }
  }
  for (const mw of opts.monowaves) {
    if (inWindow(mw.start_date)) ys.push(mw.start_price);
    if (inWindow(mw.end_date)) ys.push(mw.end_price);
  }
  if (layers.waveMarkers && opts.selectedScenario) {
    for (const pt of flattenWaveTree(opts.selectedScenario.wave_tree, opts.monowaves)) {
      if (inWindow(pt.date)) ys.push(pt.price);
    }
  }
  if (layers.fib && opts.fibZones.length > 0) {
    const fr = computeFibProjectionRange(opts);
    const overlaps = !xRange || !fr || (fr[0] <= xRange[1] && fr[1] >= xRange[0]);
    if (overlaps) {
      for (const fz of opts.fibZones) {
        ys.push(fz.low);
        ys.push(fz.high);
      }
    }
  }
  if (layers.invalidation && opts.invalidationTriggers) {
    for (const t of opts.invalidationTriggers) {
      const tt = t.trigger_type;
      if (typeof tt === 'object' && tt !== null) {
        const price = 'PriceBreakBelow' in tt ? tt.PriceBreakBelow
          : 'PriceBreakAbove' in tt ? tt.PriceBreakAbove : null;
        if (typeof price === 'number' && price > 0) ys.push(price);
      }
    }
  }
  if (layers.track2 && opts.track2Bands) {
    for (const b of opts.track2Bands) {
      ys.push(b.low);
      ys.push(b.high);
    }
  }

  if (ys.length < 2) return null;
  const lo = Math.min(...ys);
  const hi = Math.max(...ys);
  if (!Number.isFinite(lo) || !Number.isFinite(hi) || hi < lo) return null;
  const pad = hi > lo ? (hi - lo) * 0.06 : Math.max(1, lo * 0.01);
  return [lo - pad, hi + pad];
}

export function buildLayout(opts: BuildOptions): PlotlyLayout {
  const xRange = computeDefaultXRange(opts);
  const xaxis: Record<string, unknown> = {
    gridcolor: COL_GRID,
    zerolinecolor: COL_GRID,
    tickfont: { color: '#7d92b3' },
    // candlestick trace 會讓 Plotly 自動長出 rangeslider — 顯式關閉
    rangeslider: { visible: false }
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
  const yRange = computeYRange(opts, xRange);
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
    shapes: buildShapes(opts),
    annotations: buildAnnotations(opts),
    xaxis,
    yaxis,
    showlegend: false
  };
}
