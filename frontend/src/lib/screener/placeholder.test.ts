import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  digestFromRow,
  fetchWaveDigests,
  getWaveDigest,
  insufficientDigest
} from './placeholder';
import type { WaveSummaryRow } from '$contracts/fusion';

afterEach(() => {
  vi.restoreAllMocks();
});

describe('getWaveDigest', () => {
  it('deterministic — 同 stock_id 永遠回同樣結果', () => {
    const a = getWaveDigest('2330');
    const b = getWaveDigest('2330');
    expect(a).toEqual(b);
  });

  it('不同 stock_id 通常不同', () => {
    const samples = ['2330', '3030', '3363', '1101', '2454', '6415', '2379', '2451', '1312'];
    const labels = samples.map((s) => getWaveDigest(s).label);
    const uniq = new Set(labels);
    // 至少 50% 不同(deterministic 但分散)
    expect(uniq.size).toBeGreaterThanOrEqual(samples.length / 2);
  });

  it('isPlaceholder 永遠 true(原型階段)', () => {
    const d = getWaveDigest('2330');
    expect(d.isPlaceholder).toBe(true);
  });

  it('insufficient=true → label/sparkline/scenarioCount 為空', () => {
    // 搜出一個會 insufficient 的 stock_id(從廣集合掃)
    let found = false;
    for (let i = 0; i < 1000; i++) {
      const d = getWaveDigest(`TEST${i}`);
      if (d.insufficient) {
        found = true;
        expect(d.scenarioCount).toBe(0);
        expect(d.sparkline).toHaveLength(0);
        expect(d.resonance).toBe('none');
        break;
      }
    }
    expect(found).toBe(true);
  });

  it('insufficient 落 ~5% 區間(N=5000,deterministic 樣本)', () => {
    let insufCount = 0;
    const N = 5000;
    for (let i = 0; i < N; i++) {
      if (getWaveDigest(`SAMPLE${i}`).insufficient) insufCount++;
    }
    const rate = insufCount / N;
    // 預期 ~5%,允許 hash 分布不均做寬鬆 1%-10%
    expect(rate).toBeGreaterThan(0.01);
    expect(rate).toBeLessThan(0.10);
  });

  it('sparkline 內所有點落 [0, 1]', () => {
    for (let i = 0; i < 50; i++) {
      const d = getWaveDigest(`T${i}`);
      if (d.insufficient) continue;
      for (const p of d.sparkline) {
        expect(p).toBeGreaterThanOrEqual(0);
        expect(p).toBeLessThanOrEqual(1);
      }
    }
  });

  it('Certainty 從 4 個值取樣 (Primary/Possible/Rare/MissingWaveBundle)', () => {
    const seen = new Set<string>();
    for (let i = 0; i < 200; i++) {
      seen.add(getWaveDigest(`C${i}`).certainty);
    }
    // 至少看到 3 個 enum 值(MissingWaveBundle 3% 機率,200 樣本可能 miss 但 Primary/Possible/Rare 應該都出)
    expect(seen.size).toBeGreaterThanOrEqual(3);
  });
});

function realRow(overrides: Partial<WaveSummaryRow> = {}): WaveSummaryRow {
  return {
    stock_id: '2330',
    insufficient: false,
    label: 'Impulse·W3',
    direction: 'up',
    scenario_count: 4,
    certainty: 'Primary',
    sparkline: [0, 1, 0.5],
    resonance: 'strong',
    staleness_days: 0,
    ...overrides
  };
}

describe('digestFromRow(/waves/summary → WaveDigest)', () => {
  it('正常 row 全欄映射,isPlaceholder=false', () => {
    const d = digestFromRow(realRow());
    expect(d).toEqual({
      stockId: '2330',
      insufficient: false,
      label: 'Impulse·W3',
      direction: 'up',
      scenarioCount: 4,
      certainty: 'Primary',
      sparkline: [0, 1, 0.5],
      resonance: 'strong',
      isPlaceholder: false
    });
  });

  it('insufficient row → insufficientDigest 退化值', () => {
    const d = digestFromRow(realRow({ insufficient: true }));
    expect(d).toEqual(insufficientDigest('2330'));
    expect(d.isPlaceholder).toBe(false);
  });

  it('未知 enum 值防衛性收斂(direction/certainty/resonance fallback)', () => {
    const d = digestFromRow(
      realRow({ direction: 'sideways??', certainty: 'Sure', resonance: 'mega' })
    );
    expect(d.direction).toBe('flat');
    expect(d.certainty).toBe('Possible');
    expect(d.resonance).toBe('none');
  });
});

describe('fetchWaveDigests', () => {
  it('200 → Map keyed by stock_id;後端漏列補 insufficient', async () => {
    globalThis.fetch = vi.fn(async () => {
      return new Response(
        JSON.stringify({
          as_of: '2026-06-11',
          timeframe: 'daily',
          rows: [realRow()]
        }),
        { status: 200, headers: { 'content-type': 'application/json' } }
      );
    }) as typeof fetch;

    const map = await fetchWaveDigests({ stockIds: ['2330', '9999'], date: '2026-06-11' });
    expect(map.get('2330')?.label).toBe('Impulse·W3');
    expect(map.get('9999')?.insufficient).toBe(true);
  });

  it('API 失敗 → 整欄 insufficient(不 throw)', async () => {
    globalThis.fetch = vi.fn(async () => {
      throw new TypeError('network down');
    }) as typeof fetch;

    const map = await fetchWaveDigests({ stockIds: ['2330'], date: '2026-06-11' });
    expect(map.get('2330')?.insufficient).toBe(true);
    expect(map.get('2330')?.isPlaceholder).toBe(false);
  });

  it('空 stockIds → 空 Map,不打 API', async () => {
    const spy = vi.fn();
    globalThis.fetch = spy as unknown as typeof fetch;
    const map = await fetchWaveDigests({ stockIds: [], date: '2026-06-11' });
    expect(map.size).toBe(0);
    expect(spy).not.toHaveBeenCalled();
  });
});
