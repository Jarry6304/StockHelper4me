import { ACTIVE_TOOLKITS, NotFoundError, getScreen, type ActiveToolkit, type ScreenRow } from '$lib/api';
import { fetchWaveDigests, type WaveDigest } from '$lib/screener/placeholder';
import { error as svelteError } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export type LoadError =
  | { kind: 'not_found' }
  | { kind: 'network'; message: string }
  | null;

export interface PageLoadResult {
  toolkit: ActiveToolkit;
  date: string;
  topN: number;
  rows: ScreenRow[];
  rankingDate: string | null;
  /** WAVE 欄資料(/waves/summary 批次;依賴 rows 的 stock_id,故在 screen 後抓)。 */
  waveDigests: Map<string, WaveDigest>;
  error: LoadError;
}

function todayIso(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

function isActiveToolkit(s: string): s is ActiveToolkit {
  return (ACTIVE_TOOLKITS as readonly string[]).includes(s);
}

export const load: PageLoad = async ({ params, url }): Promise<PageLoadResult> => {
  const raw = params.toolkit;
  if (!isActiveToolkit(raw)) {
    throw svelteError(404, `unknown toolkit "${raw}". allowed: ${[...ACTIVE_TOOLKITS].join(', ')}`);
  }
  const toolkit: ActiveToolkit = raw;
  const date = url.searchParams.get('date') ?? todayIso();
  const topN = parseInt(url.searchParams.get('top_n') ?? '30', 10) || 30;

  let rows: ScreenRow[] = [];
  let rankingDate: string | null = null;
  let waveDigests: Map<string, WaveDigest> = new Map();
  let error: LoadError = null;

  try {
    const res = await getScreen({ toolkit, date, topN });
    rows = res.rows;
    rankingDate = res.ranking_date;
    // stock_ids 來自 screen rows → 必然第二跳;失敗整欄 insufficient,不擋表格主體
    waveDigests = await fetchWaveDigests({
      stockIds: rows.map((r) => r.stock_id),
      date
    });
  } catch (err) {
    if (err instanceof NotFoundError) {
      error = { kind: 'not_found' };
    } else if (err instanceof Error) {
      error = { kind: 'network', message: err.message };
    } else {
      error = { kind: 'network', message: '無法連線到 API' };
    }
  }

  return { toolkit, date, topN, rows, rankingDate, waveDigests, error };
};
