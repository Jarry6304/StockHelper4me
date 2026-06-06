import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  getMarketClimate,
  getNeelyForest,
  getOhlc,
  getResonance,
  getScreen,
  getTraditionalForest,
  getWaves,
  healthCheck,
  ScenarioForestOverflowError
} from './index';

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

function getCalledUrl(fetchMock: typeof fetch): string {
  const call = (fetchMock as ReturnType<typeof vi.fn>).mock.calls[0]?.[0];
  return String(call);
}

describe('healthCheck', () => {
  it('GET /health', async () => {
    const m = mockJson(200, { status: 'ok', service: 'golden-l3-api' });
    globalThis.fetch = m;
    const r = await healthCheck();
    expect(r.status).toBe('ok');
    expect(getCalledUrl(m)).toContain('/health');
  });
});

describe('getNeelyForest', () => {
  it('組 stock_id + as_of + timeframe query', async () => {
    const m = mockJson(200, { stock_id: '2330', scenario_forest: [] });
    globalThis.fetch = m;
    await getNeelyForest({ stockId: '2330', asOf: '2026-06-06', timeframe: 'daily' });
    const url = getCalledUrl(m);
    expect(url).toContain('/stocks/2330/neely/forest');
    expect(url).toContain('as_of=2026-06-06');
    expect(url).toContain('timeframe=daily');
  });

  it('422 → ScenarioForestOverflowError', async () => {
    globalThis.fetch = mockJson(422, { detail: 'forest_overflow' });
    await expect(
      getNeelyForest({ stockId: '2330', asOf: '2026-06-06' })
    ).rejects.toBeInstanceOf(ScenarioForestOverflowError);
  });
});

describe('getTraditionalForest', () => {
  it('GET /stocks/{id}/traditional/forest?timeframe=', async () => {
    const m = mockJson(200, { stock_id: '2330', scenario_forest: [] });
    globalThis.fetch = m;
    await getTraditionalForest({ stockId: '2330', timeframe: 'weekly' });
    const url = getCalledUrl(m);
    expect(url).toContain('/stocks/2330/traditional/forest');
    expect(url).toContain('timeframe=weekly');
  });
});

describe('getWaves', () => {
  it('回 { neely, traditional } 並排', async () => {
    const m = mockJson(200, { neely: null, traditional: null });
    globalThis.fetch = m;
    const r = await getWaves({ stockId: '3030', asOf: '2026-06-06' });
    expect(r).toHaveProperty('neely');
    expect(r).toHaveProperty('traditional');
  });
});

describe('getResonance', () => {
  it('GET /stocks/{id}/resonance', async () => {
    const m = mockJson(200, { single_track_mode: false, findings: [] });
    globalThis.fetch = m;
    await getResonance({ stockId: '2330', asOf: '2026-06-06' });
    expect(getCalledUrl(m)).toContain('/stocks/2330/resonance');
  });
});

describe('getOhlc', () => {
  it('組 from + to', async () => {
    const m = mockJson(200, { stock_id: '2330', rows: [] });
    globalThis.fetch = m;
    await getOhlc({ stockId: '2330', from: '2026-01-01', to: '2026-06-06' });
    const url = getCalledUrl(m);
    expect(url).toContain('/stocks/2330/ohlc');
    expect(url).toContain('from=2026-01-01');
    expect(url).toContain('to=2026-06-06');
  });
});

describe('getScreen', () => {
  it('預設 top_n=30 offset=0', async () => {
    const m = mockJson(200, { toolkit: 'magic_formula', ranking_date: '2026-06-06', top_n: 30, offset: 0, rows: [] });
    globalThis.fetch = m;
    await getScreen({ toolkit: 'magic_formula', date: '2026-06-06' });
    const url = getCalledUrl(m);
    expect(url).toContain('/screens/magic_formula');
    expect(url).toContain('top_n=30');
    expect(url).toContain('offset=0');
    expect(url).toContain('date=2026-06-06');
  });

  it('支援自訂 topN / offset', async () => {
    const m = mockJson(200, { toolkit: 'f_score', ranking_date: '', top_n: 10, offset: 5, rows: [] });
    globalThis.fetch = m;
    await getScreen({ toolkit: 'f_score', date: '2026-06-06', topN: 10, offset: 5 });
    const url = getCalledUrl(m);
    expect(url).toContain('top_n=10');
    expect(url).toContain('offset=5');
  });
});

describe('getMarketClimate', () => {
  it('GET /market/climate', async () => {
    const m = mockJson(200, {
      as_of: '2026-06-06',
      overall_climate: 'neutral',
      climate_score: 0,
      components: {},
      systemic_risks: [],
      narrative: ''
    });
    globalThis.fetch = m;
    await getMarketClimate({ asOf: '2026-06-06' });
    expect(getCalledUrl(m)).toContain('/market/climate');
  });
});
