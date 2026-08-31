"""判讀 JSON 驗證 — chain 階段 4(m3Spec/wave_judgment_loop.md §2/§10)。

判讀(人/LLM)只能在 dossier 候選集內選;`no_fit` 是合法輸出。驗證失敗
raise `JudgmentValidationError`,`legal_keys` 附上該 timeframe 的合法
anchor_key 清單(判讀者修正用)。schema 手寫(repo 無 jsonschema dep);
`.claude/skills/neely-judgment/references/output-schema.json` 與本模組
以測試互鎖。
"""

from __future__ import annotations

from datetime import date, datetime
from typing import Any

__all__ = ["JudgmentValidationError", "validate_judgment"]

_TIMEFRAMES = ("daily", "weekly", "monthly")
_CONFIDENCE_CLASSES = ("single", "contested", "no_fit")
_ROLES = ("preferred", "alternate")


class JudgmentValidationError(ValueError):
    """驗證拒絕;`legal_keys` = dossier 該 timeframe 的合法 anchor_key。"""

    def __init__(self, message: str, *, legal_keys: list[str] | None = None):
        super().__init__(message)
        self.legal_keys = legal_keys or []


def _reject(msg: str, legal_keys: list[str] | None = None) -> None:
    raise JudgmentValidationError(msg, legal_keys=legal_keys)


def _parse_date(v: Any, field: str) -> date:
    if isinstance(v, date):
        return v
    try:
        return datetime.fromisoformat(str(v)).date()
    except (TypeError, ValueError):
        _reject(f"{field} 不是合法日期:{v!r}")
        raise AssertionError  # unreachable


def validate_judgment(judgment: dict[str, Any], dossier: dict[str, Any]) -> dict[str, Any]:
    """驗證判讀 JSON 並回傳可 INSERT 的 wave_judgments row(PIT 錨定欄由
    dossier 填:snapshot_date / params_hash / engine_version / assumption_hash;
    accepted 候選的 invalidation_triggers 併入 invalidation.recorded_triggers —
    J2 用**記錄的**觸發,不重算)。"""
    if not isinstance(judgment, dict):
        _reject("judgment 必須是 JSON object")

    # ── 基本欄位 ────────────────────────────────────────────
    stock_id = judgment.get("stock_id")
    if not stock_id or not isinstance(stock_id, str):
        _reject("stock_id 不可為空")
    if stock_id != dossier.get("stock_id"):
        _reject(f"stock_id 與 dossier 不符:{stock_id!r} vs {dossier.get('stock_id')!r}")

    timeframe = judgment.get("timeframe")
    if timeframe not in _TIMEFRAMES:
        _reject(f"timeframe 必須是 {_TIMEFRAMES},收到 {timeframe!r}")

    judged_by = judgment.get("judged_by")
    if not isinstance(judged_by, str) or not (
        judged_by == "human" or judged_by.startswith("llm:")
    ):
        _reject(f"judged_by 必須是 'human' 或 'llm:<model>',收到 {judged_by!r}")

    confidence_class = judgment.get("confidence_class")
    if confidence_class not in _CONFIDENCE_CLASSES:
        _reject(f"confidence_class 必須是 {_CONFIDENCE_CLASSES},收到 {confidence_class!r}")

    # ── dossier 對應 timeframe 段 ──────────────────────────
    section = (dossier.get("timeframes") or {}).get(timeframe) or {}
    snapshot_ref = section.get("snapshot_ref")
    if not snapshot_ref:
        _reject(f"dossier 無 {timeframe} snapshot(尚未跑 tw_cores),無法判讀")
    snapshot_date = _parse_date(snapshot_ref.get("snapshot_date"), "snapshot_ref.snapshot_date")

    # §11:as_of 必須 ≤ snapshot_date(判讀者看了盤中 → 拒絕)
    as_of = _parse_date(judgment.get("as_of"), "as_of")
    if as_of > snapshot_date:
        _reject(
            f"as_of {as_of.isoformat()} 晚於最新 snapshot {snapshot_date.isoformat()}"
            f"(判讀所見必須 ≤ snapshot_date)"
        )

    candidates = {c.get("anchor_key"): c for c in section.get("candidates") or []}
    legal_keys = sorted(k for k in candidates if k)

    # ── accepted:候選集約束 ───────────────────────────────
    accepted = judgment.get("accepted")
    if not isinstance(accepted, list):
        _reject("accepted 必須是 list(可為空 — no_fit)", legal_keys)
    roles: list[str] = []
    for i, a in enumerate(accepted):
        if not isinstance(a, dict):
            _reject(f"accepted[{i}] 必須是 object", legal_keys)
        key = a.get("anchor_key")
        role = a.get("role")
        if role not in _ROLES:
            _reject(f"accepted[{i}].role 必須是 {_ROLES},收到 {role!r}", legal_keys)
        if key not in candidates:
            _reject(
                f"accepted[{i}].anchor_key 不在 dossier 候選集內:{key!r}",
                legal_keys,
            )
        roles.append(role)

    n_preferred = roles.count("preferred")

    # ── confidence_class 一致性(§10 判讀驗證)─────────────
    no_fit_reason = judgment.get("no_fit_reason")
    if confidence_class == "no_fit":
        if accepted:
            _reject("no_fit 要求 accepted = []", legal_keys)
        if not no_fit_reason or not str(no_fit_reason).strip():
            _reject("no_fit 要求非空 no_fit_reason(缺什麼、引擎缺口)")
    else:
        if not accepted:
            _reject(f"{confidence_class} 要求至少 1 筆 accepted", legal_keys)
        if n_preferred != 1:
            _reject(f"{confidence_class} 要求恰 1 筆 preferred(收到 {n_preferred})", legal_keys)
        if confidence_class == "single":
            if len(accepted) != 1:
                _reject("single 要求 accepted 僅 1 筆(多筆 = contested)", legal_keys)
            preferred = candidates[accepted[0]["anchor_key"]]
            if (preferred.get("evidence") or {}).get("robust") is False:
                _reject(
                    "single 的 preferred 候選 robust=false(噪音門檻產物)— "
                    "降為 contested 或另選 robust 候選"
                )
        elif confidence_class == "contested" and len(accepted) < 2:
            _reject("contested 要求 preferred + 至少 1 筆 alternate", legal_keys)

    # ── invalidation(禁省略;no_fit 免)───────────────────
    invalidation = judgment.get("invalidation")
    if confidence_class != "no_fit":
        if not isinstance(invalidation, dict):
            _reject("invalidation 必須是 object({price_levels, time_limit_bar})")
        price_levels = invalidation.get("price_levels")
        if not isinstance(price_levels, list):
            _reject("invalidation.price_levels 必須是 list")
        if not price_levels and not invalidation.get("time_limit_bar"):
            _reject("invalidation 不可為空:至少一個具體價位或 time_limit_bar")
        for i, pl in enumerate(price_levels):
            if not isinstance(pl, dict) or not isinstance(pl.get("level"), (int, float)):
                _reject(f"invalidation.price_levels[{i}].level 必須是數字")
            if not pl.get("meaning"):
                _reject(f"invalidation.price_levels[{i}].meaning 不可為空")
    elif not isinstance(invalidation, dict):
        invalidation = {"price_levels": [], "time_limit_bar": None}

    # ── rationale ──────────────────────────────────────────
    rationale = judgment.get("rationale")
    if not isinstance(rationale, dict):
        _reject("rationale 必須是 object({rule_refs, emulation_considered, …})")
    if confidence_class != "no_fit" and not rationale.get("rule_refs"):
        _reject("rationale.rule_refs 不可為空(禁用「感覺」替代 rule_refs)")
    rationale = dict(rationale)
    if no_fit_reason:
        rationale["no_fit_reason"] = no_fit_reason  # 缺口表 = confidence_class='no_fit' 查詢

    # ── J2 用:accepted 候選的 recorded triggers 併入 ──────
    recorded_triggers = []
    for a in accepted:
        cand = candidates[a["anchor_key"]]
        recorded_triggers.append({
            "anchor_key": a["anchor_key"],
            "triggers": (cand.get("forward") or {}).get("invalidation_triggers") or [],
        })
    invalidation = dict(invalidation)
    invalidation["recorded_triggers"] = recorded_triggers

    return {
        "stock_id": stock_id,
        "timeframe": timeframe,
        "as_of": as_of,
        "judged_by": judged_by,
        "snapshot_date": snapshot_date,
        "params_hash": snapshot_ref.get("params_hash") or "",
        "engine_version": (dossier.get("engine") or {}).get("neely") or "",
        "assumption_hash": (dossier.get("engine") or {}).get("assumption_hash") or "",
        "accepted": accepted,
        "degree_read": judgment.get("degree_read"),
        "rationale": rationale,
        "invalidation": invalidation,
        "confidence_class": confidence_class,
        "status": "active",
        "supersedes_id": judgment.get("supersedes_id")
            or (rationale.get("prior_judgment_id") if isinstance(rationale, dict) else None),
        "diff_detail": None,
    }
