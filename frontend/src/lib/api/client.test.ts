import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  ApiError,
  NetworkError,
  NotFoundError,
  ScenarioForestOverflowError,
  apiGet,
  getBaseUrl,
  toIsoDate
} from './client';

const realFetch = globalThis.fetch;

afterEach(() => {
  globalThis.fetch = realFetch;
  vi.restoreAllMocks();
});

function mockJson(status: number, body: unknown): typeof fetch {
  return vi.fn(async () => {
    return new Response(JSON.stringify(body), {
      status,
      headers: { 'Content-Type': 'application/json' }
    });
  }) as unknown as typeof fetch;
}

describe('apiGet', () => {
  it('200 → parse JSON 回 body', async () => {
    globalThis.fetch = mockJson(200, { ok: true, value: 42 });
    const res = await apiGet<{ ok: boolean; value: number }>('/x');
    expect(res).toEqual({ ok: true, value: 42 });
  });

  it('404 → NotFoundError', async () => {
    globalThis.fetch = mockJson(404, { detail: 'not_found' });
    await expect(apiGet('/x')).rejects.toBeInstanceOf(NotFoundError);
  });

  it('422 → ScenarioForestOverflowError', async () => {
    globalThis.fetch = mockJson(422, { detail: 'forest_overflow: 300' });
    await expect(apiGet('/x')).rejects.toBeInstanceOf(ScenarioForestOverflowError);
  });

  it('500 → ApiError (generic)', async () => {
    globalThis.fetch = mockJson(500, { detail: 'internal' });
    await expect(apiGet('/x')).rejects.toBeInstanceOf(ApiError);
    await expect(apiGet('/x')).rejects.not.toBeInstanceOf(NotFoundError);
  });

  it('fetch reject → NetworkError', async () => {
    globalThis.fetch = vi.fn(async () => {
      throw new TypeError('fetch failed');
    }) as unknown as typeof fetch;
    await expect(apiGet('/x')).rejects.toBeInstanceOf(NetworkError);
  });

  it('組合 baseUrl + path', async () => {
    const mock = mockJson(200, {});
    globalThis.fetch = mock;
    await apiGet('/stocks/2330/waves');
    const call = (mock as ReturnType<typeof vi.fn>).mock.calls[0]?.[0];
    expect(typeof call).toBe('string');
    expect(call).toContain('/stocks/2330/waves');
  });
});

describe('toIsoDate', () => {
  it('Date → YYYY-MM-DD', () => {
    expect(toIsoDate(new Date(2026, 5, 6))).toBe('2026-06-06'); // 月份 0-index
  });

  it('已是 string → passthrough', () => {
    expect(toIsoDate('2026-06-06')).toBe('2026-06-06');
  });
});

describe('getBaseUrl', () => {
  it('預設 /api(無 env override)', () => {
    expect(getBaseUrl()).toBe('/api');
  });
});
