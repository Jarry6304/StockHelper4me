"""tools/ 域檔共用 helper — 入參解析 / 安全上界 / section 錯誤 / 物化讀取。

只放被 ≥ 2 個域檔使用的共用碼;域檔間禁止互相 import(本檔是唯一下沉點,
依賴圖見 CLAUDE.md「常見任務」對應規格)。
"""

from __future__ import annotations

import logging
from datetime import date as Date
from typing import Any

_logger = logging.getLogger(__name__)


def _parse_date(value: str | Date) -> Date:
    """ISO 字串 → date。已是 date 直接 pass through。"""
    if isinstance(value, Date):
        return value
    return Date.fromisoformat(value)


def _clamp(value: int, lo: int, hi: int) -> int:
    """夾 value 到 [lo, hi]。MCP 工具入口防 LLM 傳極端 top_n / lookback_days 觸發
    runaway query(API 唯讀無 auth;clamp 把查詢成本上界釘死)。"""
    return max(lo, min(int(value), hi))


# MCP 工具參數安全上界(防 runaway query;合法呼叫遠在界內)
_MAX_TOP_N = 500
_MAX_LOOKBACK_DAYS = 365
# 重工具讀連線的 runaway 安全網(毫秒);clamp 已釘死成本,此為非預期慢查詢 backstop
_READ_TIMEOUT_MS = 30_000


def _section_error(label: str, exc: Exception) -> dict[str, Any]:
    """Consolidated 工具 section graceful degradation 的統一錯誤回值 + server log。

    回應行為不變(仍回 {"error","section"},LLM 看得到型別+訊息);但 server 端
    `logger.exception` 留完整 traceback,讓 programming bug(AttributeError 等)
    不再與「該股無資料」無法區分。"""
    _logger.exception("consolidated section %s failed", label)
    return {"error": f"{type(exc).__name__}: {exc}", "section": label}


def _read_materialized_snapshot(
    stock_id: str,
    as_of,
    core_name: str,
    *,
    timeframe: str | None = None,
    database_url: str | None = None,
) -> dict[str, Any] | None:
    """讀已物化 Golden L3 fusion row(stock_levels / dual_track_resonance /
    market_context 改讀物化共用)。回 snapshot dict 或 None(→ caller compute fallback)。

    Robust:連線失敗 / 無 row / snapshot 非 dict → 一律回 None(→ compute fallback)。
    `isinstance(snapshot, dict)` 守門確保只回真實物化 dict(避免 mock conn 的 truthy 回值
    被誤當成物化結果;對齊既有測試僅 mock compute path 的行為)。
    """
    from fusion.materialize.read import (
        extract_snapshot_with_provenance,
        fetch_fusion_doc,
    )
    from fusion.raw import get_connection

    try:
        conn = get_connection(database_url)
        try:
            row = fetch_fusion_doc(
                conn, stock_id=stock_id, as_of=as_of,
                core_name=core_name, timeframe=timeframe,
            )
        finally:
            conn.close()
    except Exception:  # noqa: BLE001 — 連線 / 查詢失敗 → compute fallback
        return None
    # 物化命中 → 附 _provenance(snapshot_date / staleness_days)揭露新鮮度;
    # snapshot 非 dict / 無 row → None → caller 走 compute fallback。
    return extract_snapshot_with_provenance(row, as_of)
