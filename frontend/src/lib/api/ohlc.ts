import { apiGet, toIsoDate, type FetchOptions } from './client';

export interface OhlcRow {
  date: string;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number | null;
}

export interface OhlcResponse {
  stock_id: string;
  rows: OhlcRow[];
}

export interface GetOhlcArgs {
  stockId: string;
  from: Date | string;
  to: Date | string;
}

export function getOhlc(args: GetOhlcArgs, opts?: FetchOptions): Promise<OhlcResponse> {
  const { stockId, from, to } = args;
  const qs = new URLSearchParams({ from: toIsoDate(from), to: toIsoDate(to) });
  return apiGet<OhlcResponse>(`/stocks/${encodeURIComponent(stockId)}/ohlc?${qs}`, opts);
}
