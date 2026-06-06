/**
 * Wave coordinate helpers — join wave_tree label 與 monowave_series 取 y 座標。
 *
 * L7:wave_tree 無 y → 由 join OHLC / monowave_series 取。
 */

import type { Monowave } from '$contracts/neely/Monowave';
import type { WaveNode } from '$contracts/neely/WaveNode';

export interface WavePoint {
  /** wave label(W1/W2/.../A/B/C 或 monowave 內部標)。 */
  label: string;
  /** ISO date。 */
  date: string;
  /** 取自 monowave_series 的 end_price 或 start_price。 */
  price: number;
  /** Up/Down/Neutral — 用來決定 marker 色。 */
  direction: 'Up' | 'Down' | 'Neutral';
}

/**
 * 從 wave_tree.children 平展出 WavePoint 序列。
 *
 * 對應 monowave_series 的方式:走 wave_tree.children,每個 child 的 `end` date
 * 對應 monowave_series 上某一條 leg 的 end_date,price 用該 leg 的 end_price。
 *
 * 若 wave_tree.children 為空,回單一 (start, end) 兩點。
 */
export function flattenWaveTree(node: WaveNode, monowaves: Monowave[]): WavePoint[] {
  if (!node) return [];
  const points: WavePoint[] = [];
  const byDate = new Map<string, Monowave>();
  for (const mw of monowaves) {
    byDate.set(mw.end_date, mw);
    byDate.set(mw.start_date, mw);
  }

  // root start anchor(W0)
  const rootStartMw = byDate.get(node.start);
  if (rootStartMw) {
    points.push({
      label: '',
      date: node.start,
      price: rootStartMw.start_date === node.start ? rootStartMw.start_price : rootStartMw.end_price,
      direction: rootStartMw.direction
    });
  }

  if (node.children.length === 0) {
    // 簡退:用 start/end + 簡單 label
    const endMw = byDate.get(node.end);
    if (endMw) {
      points.push({
        label: node.label,
        date: node.end,
        price: endMw.end_date === node.end ? endMw.end_price : endMw.start_price,
        direction: endMw.direction
      });
    }
    return points;
  }

  for (const child of node.children) {
    const mw = byDate.get(child.end);
    const price = mw
      ? mw.end_date === child.end
        ? mw.end_price
        : mw.start_price
      : Number.NaN;
    points.push({
      label: child.label || '',
      date: child.end,
      price,
      direction: mw?.direction ?? 'Neutral'
    });
  }

  return points;
}

/** Direction 對 marker fill 色(對映 wireframe V1 配色)。 */
export function directionColor(direction: WavePoint['direction']): string {
  switch (direction) {
    case 'Up':
      return '#56d4f0'; // --wave
    case 'Down':
      return '#2b87a3'; // --wave darker
    default:
      return '#8a99b3'; // --div(grey for ABC)
  }
}

/** Wave label 字串解析 — 從 "W1↑" / "W3:L5↑" 等取數字或字母。 */
export function shortLabel(label: string): string {
  if (!label) return '';
  // 取 W{n}: 後綴的數字,e.g. "W3:L5↑" → "3"
  const m = label.match(/W(\d+)/);
  if (m) return m[1];
  // 取 A/B/C 字母
  const abc = label.match(/[ABC]/);
  if (abc) return abc[0];
  return label.slice(0, 2);
}
