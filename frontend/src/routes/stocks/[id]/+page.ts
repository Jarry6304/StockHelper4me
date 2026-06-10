import { getOhlc, getResonance, getWaves, NotFoundError, ScenarioForestOverflowError } from '$lib/api';
import type { OhlcRow, WavesResponse } from '$lib/api';
import type { ResonanceFusion } from '$contracts/fusion';
import type { Timeframe } from '$lib/api/neely';
import type { PageLoad } from './$types';

export type LoadError =
  | { kind: 'not_found' }
  | { kind: 'overflow'; message: string }
  | { kind: 'network'; message: string }
  | null;

export interface PageLoadResult {
  stockId: string;
  asOf: string;
  timeframe: Timeframe;
  initialState: 'overview' | 'detail';
  waves: WavesResponse | null;
  resonance: ResonanceFusion | null;
  /** 後復權收盤序列(兩張波浪圖的時間背景線);失敗 → null 降級無背景,不擋卡片。 */
  ohlc: OhlcRow[] | null;
  error: LoadError;
}

function todayIso(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

function isoAddDays(iso: string, days: number): string {
  const t = Date.parse(iso);
  const d = new Date(t + days * 86400000);
  return `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, '0')}-${String(d.getUTCDate()).padStart(2, '0')}`;
}

/** 背景線回看天數 — 蓋住引擎窗(日線 1500 交易日 ≈ 6 年)與 stale 形態錨定窗。 */
const OHLC_LOOKBACK_DAYS = 2200;

function parseTimeframe(raw: string | null): Timeframe {
  if (raw === 'weekly' || raw === 'monthly' || raw === 'quarterly') return raw;
  return 'daily';
}

export const load: PageLoad = async ({ params, url, fetch: _fetch }): Promise<PageLoadResult> => {
  // 不直接用 SvelteKit fetch (apiGet 內部用全域 fetch),保持 client.ts 簡潔。
  void _fetch;
  const stockId = params.id;
  const asOf = url.searchParams.get('as_of') ?? todayIso();
  const timeframe = parseTimeframe(url.searchParams.get('timeframe'));
  const stateParam = url.searchParams.get('state');
  const initialState = stateParam === 'detail' ? 'detail' : 'overview';

  let waves: WavesResponse | null = null;
  let resonance: ResonanceFusion | null = null;
  let error: LoadError = null;

  // 收盤背景線與 waves 互相獨立 → 並行抓;失敗降級 null(不擋卡片)
  const ohlcPromise: Promise<OhlcRow[] | null> = getOhlc({
    stockId,
    from: isoAddDays(asOf, -OHLC_LOOKBACK_DAYS),
    to: asOf
  })
    .then((r) => r.rows)
    .catch(() => null);

  try {
    waves = await getWaves({ stockId, asOf, timeframe });
  } catch (err) {
    if (err instanceof NotFoundError) {
      error = { kind: 'not_found' };
    } else if (err instanceof ScenarioForestOverflowError) {
      error = {
        kind: 'overflow',
        message: '情境森林過大(>250)— 資料完整性保險絲觸發,請通報後端校準引擎參數。'
      };
    } else if (err instanceof Error) {
      error = { kind: 'network', message: err.message };
    } else {
      error = { kind: 'network', message: '無法連線到 API' };
    }
  }

  // Track2 帶為加值資訊;載入失敗不阻斷整個畫面,降級為 null
  try {
    resonance = await getResonance({ stockId, asOf, timeframe });
  } catch {
    resonance = null;
  }

  const ohlc = await ohlcPromise;

  return { stockId, asOf, timeframe, initialState, waves, resonance, ohlc, error };
};
