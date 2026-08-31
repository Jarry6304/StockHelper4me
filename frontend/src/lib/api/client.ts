/**
 * API client base — thin fetch wrapper(對齊 plan Phase 2)。
 *
 * 設定:
 * - dev: `/api` 走 vite proxy → http://localhost:8000(對齊 vite.config.ts)
 * - prod: `VITE_API_BASE_URL` 設成絕對 URL,或 SvelteKit adapter-static 同源托管
 *
 * 不負責任何商業邏輯 — 只是 URL 組裝 / fetch / JSON parse / 錯誤分類。
 */

const DEFAULT_BASE = '/api';

export function getBaseUrl(): string {
  const env = import.meta.env.VITE_API_BASE_URL;
  if (typeof env === 'string' && env.length > 0) return env.replace(/\/+$/, '');
  return DEFAULT_BASE;
}

// ── error 分類 ────────────────────────────────────────────────────────────

export class ApiError extends Error {
  status: number;
  detail?: unknown;
  constructor(status: number, message: string, detail?: unknown) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.detail = detail;
  }
}

/** /neely/forest N > 250 完整性保險絲。 */
export class ScenarioForestOverflowError extends ApiError {
  constructor(detail: unknown) {
    super(422, 'scenario_forest size exceeds fuse cap', detail);
    this.name = 'ScenarioForestOverflowError';
  }
}

/** 對應某 (stock, core, as_of, ...) 無 row。 */
export class NotFoundError extends ApiError {
  constructor(detail: unknown) {
    super(404, 'not_found', detail);
    this.name = 'NotFoundError';
  }
}

/** Network / fetch 失敗(server 沒回 / DNS 等)。 */
export class NetworkError extends ApiError {
  constructor(cause: unknown) {
    super(0, 'network_error', cause);
    this.name = 'NetworkError';
  }
}

// ── core fetch ─────────────────────────────────────────────────────────────

export interface FetchOptions {
  signal?: AbortSignal;
}

export async function apiGet<T = unknown>(path: string, opts: FetchOptions = {}): Promise<T> {
  const url = `${getBaseUrl()}${path}`;
  let res: Response;
  try {
    res = await fetch(url, {
      method: 'GET',
      signal: opts.signal,
      headers: { Accept: 'application/json' }
    });
  } catch (cause) {
    throw new NetworkError(cause);
  }

  if (res.ok) {
    return (await res.json()) as T;
  }

  // 嘗試 parse detail (FastAPI HTTPException 寫進 {detail: ...})
  let detail: unknown = undefined;
  try {
    detail = await res.json();
  } catch {
    detail = await res.text().catch(() => undefined);
  }

  switch (res.status) {
    case 404:
      throw new NotFoundError(detail);
    case 422:
      throw new ScenarioForestOverflowError(detail);
    default:
      throw new ApiError(res.status, `HTTP ${res.status}`, detail);
  }
}

/**
 * POST(v4.39 判讀寫入用;唯一寫端點 POST /judgments)。
 *
 * 422 這裡不映 ScenarioForestOverflowError(那是 /neely/forest 專屬)—
 * 回傳 ApiError 帶 detail,caller(judgments.ts)自行解讀拒絕原因。
 */
export async function apiPost<T = unknown>(
  path: string,
  body: unknown,
  opts: FetchOptions = {}
): Promise<T> {
  const url = `${getBaseUrl()}${path}`;
  let res: Response;
  try {
    res = await fetch(url, {
      method: 'POST',
      signal: opts.signal,
      headers: { Accept: 'application/json', 'Content-Type': 'application/json' },
      body: JSON.stringify(body)
    });
  } catch (cause) {
    throw new NetworkError(cause);
  }

  if (res.ok) {
    return (await res.json()) as T;
  }

  let detail: unknown = undefined;
  try {
    detail = await res.json();
  } catch {
    detail = await res.text().catch(() => undefined);
  }
  if (res.status === 404) throw new NotFoundError(detail);
  throw new ApiError(res.status, `HTTP ${res.status}`, detail);
}

// ── helper: ISO date string ───────────────────────────────────────────────

export function toIsoDate(d: Date | string): string {
  if (typeof d === 'string') return d;
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  return `${yyyy}-${mm}-${dd}`;
}
