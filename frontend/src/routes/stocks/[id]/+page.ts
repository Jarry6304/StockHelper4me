import { getResonance, getWaves, NotFoundError, ScenarioForestOverflowError } from '$lib/api';
import type { WavesResponse } from '$lib/api';
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
  error: LoadError;
}

function todayIso(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

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

  return { stockId, asOf, timeframe, initialState, waves, resonance, error };
};
