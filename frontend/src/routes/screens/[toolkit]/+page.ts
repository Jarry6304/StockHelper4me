import { ACTIVE_TOOLKITS, NotFoundError, getScreen, type ActiveToolkit, type ScreenRow } from '$lib/api';
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
  let error: LoadError = null;

  try {
    const res = await getScreen({ toolkit, date, topN });
    rows = res.rows;
    rankingDate = res.ranking_date;
  } catch (err) {
    if (err instanceof NotFoundError) {
      error = { kind: 'not_found' };
    } else if (err instanceof Error) {
      error = { kind: 'network', message: err.message };
    } else {
      error = { kind: 'network', message: '無法連線到 API' };
    }
  }

  return { toolkit, date, topN, rows, rankingDate, error };
};
