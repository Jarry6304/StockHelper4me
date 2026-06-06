import { describe, expect, it } from 'vitest';
import { factorColumnsFor, formatFactor, toolkitFactorSummary } from './factors';

describe('factorColumnsFor', () => {
  it('magic_formula → [EY%, ROC]', () => {
    const cols = factorColumnsFor('magic_formula');
    expect(cols).toHaveLength(2);
    expect(cols.map((c) => c.label)).toEqual(['EY%', 'ROC']);
  });

  it('f_score → [F-score]', () => {
    expect(factorColumnsFor('f_score')).toHaveLength(1);
  });
});

describe('formatFactor', () => {
  it('int → 四捨五入', () => {
    const v = formatFactor({ stock_id: '', combined_rank: 1, f_score: 7.6 }, {
      key: 'f_score',
      label: 'F',
      format: 'int'
    });
    expect(v).toBe('8');
  });

  it('decimal → toFixed(1)', () => {
    const v = formatFactor({ stock_id: '', combined_rank: 1, ey: 9.234 }, {
      key: 'ey',
      label: 'EY',
      format: 'decimal'
    });
    expect(v).toBe('9.2');
  });

  it('percent → toFixed(1) + %', () => {
    const v = formatFactor({ stock_id: '', combined_rank: 1, m: 12.3 }, {
      key: 'm',
      label: 'M',
      format: 'percent'
    });
    expect(v).toBe('12.3%');
  });

  it('null → —', () => {
    const v = formatFactor({ stock_id: '', combined_rank: 1, ey: null }, {
      key: 'ey',
      label: 'EY',
      format: 'decimal'
    });
    expect(v).toBe('—');
  });

  it('detail JSONB fallback', () => {
    const v = formatFactor(
      {
        stock_id: '',
        combined_rank: 1,
        detail: { custom_factor: 42.5 } as Record<string, unknown>
      },
      { key: 'custom_factor', label: 'Custom', format: 'decimal' }
    );
    expect(v).toBe('42.5');
  });
});

describe('toolkitFactorSummary', () => {
  it('magic_formula → "EY% · ROC"', () => {
    expect(toolkitFactorSummary('magic_formula')).toBe('EY% · ROC');
  });
});
