import { ApiError, apiPost, type FetchOptions } from './client';
import type { DossierCandidate } from './waves';

/**
 * POST /judgments — 「選取 → 錨定」判讀寫入(wave_judgment_loop §2/§5;
 * web API 唯一寫端點)。驗證在伺服端(fusion.judgment.validate_judgment,
 * 候選集約束 / confidence_class 一致性 / as_of ≤ snapshot_date);422 →
 * JudgmentRejectedError 帶拒絕原因 + 合法 anchor_key 清單。
 */

export interface JudgmentSubmission {
  stock_id: string;
  timeframe: string;
  as_of: string;
  judged_by: string; // 'human' | 'llm:<model>'
  confidence_class: 'single' | 'contested' | 'no_fit';
  accepted: Array<{ role: 'preferred' | 'alternate'; anchor_key: string }>;
  invalidation: {
    price_levels: Array<{ level: number; meaning: string }>;
    time_limit_bar: string | null;
  };
  rationale: Record<string, unknown> & { rule_refs?: string[] };
  no_fit_reason?: string | null;
  degree_read?: unknown;
  supersedes_id?: number | null;
}

export interface JudgmentAccepted {
  id: number;
  stock_id: string;
  timeframe: string;
  confidence_class: string;
  status: string;
}

export class JudgmentRejectedError extends Error {
  legalAnchorKeys: string[];
  constructor(message: string, legalAnchorKeys: string[]) {
    super(message);
    this.name = 'JudgmentRejectedError';
    this.legalAnchorKeys = legalAnchorKeys;
  }
}

export async function postJudgment(
  judgment: JudgmentSubmission,
  opts?: FetchOptions
): Promise<JudgmentAccepted> {
  try {
    return await apiPost<JudgmentAccepted>('/judgments', judgment, opts);
  } catch (err) {
    if (err instanceof ApiError && err.status === 422) {
      const d = err.detail as { detail?: { error?: string; legal_anchor_keys?: string[] } } | undefined;
      const inner = d?.detail;
      throw new JudgmentRejectedError(
        inner?.error ?? '判讀被伺服端驗證拒絕',
        inner?.legal_anchor_keys ?? []
      );
    }
    throw err;
  }
}

/**
 * 「選取 → 錨定」一鍵組裝:single/preferred 判讀,invalidation 預填候選的
 * InvalidateScenario price triggers(J2 用**記錄的**觸發 — 伺服端會再從
 * dossier 拷貝 recorded_triggers),rationale.rule_refs 預填候選 passed_rules
 * (人按下錨定 = 接受畫面上的引擎證據;禁空 rule_refs)。
 */
export function buildAnchorJudgment(args: {
  stockId: string;
  timeframe: string;
  snapshotDate: string; // dossier.timeframes[tf].snapshot_ref.snapshot_date
  candidate: DossierCandidate;
}): JudgmentSubmission {
  const { stockId, timeframe, snapshotDate, candidate } = args;
  const priceLevels: Array<{ level: number; meaning: string }> = [];
  for (const trig of candidate.forward?.invalidation_triggers ?? []) {
    const onTrigger = trig['on_trigger'];
    if (onTrigger !== 'InvalidateScenario') continue;
    const tt = trig['trigger_type'];
    if (!tt || typeof tt !== 'object') continue;
    const rule = typeof trig['rule_reference'] === 'string' ? trig['rule_reference'] : null;
    const below = (tt as Record<string, unknown>)['PriceBreakBelow'];
    if (typeof below === 'number') {
      priceLevels.push({ level: below, meaning: rule ?? '引擎 InvalidateScenario(跌破)' });
    }
    const above = (tt as Record<string, unknown>)['PriceBreakAbove'];
    if (typeof above === 'number') {
      priceLevels.push({ level: above, meaning: rule ?? '引擎 InvalidateScenario(漲破)' });
    }
  }
  return {
    stock_id: stockId,
    timeframe,
    as_of: snapshotDate,
    judged_by: 'human',
    confidence_class: 'single',
    accepted: [{ role: 'preferred', anchor_key: candidate.anchor_key }],
    invalidation: { price_levels: priceLevels, time_limit_bar: null },
    rationale: {
      rule_refs: candidate.evidence?.passed_rules ?? [],
      note: 'ui:anchor-selection(V1 卡「選取→錨定」;接受引擎證據原樣)'
    }
  };
}
