"""POST /judgments — 判讀寫入(wave_judgment_loop §2 階段 3-4)。

**web API 首個寫端點**(其餘全唯讀;2026-08-30 拍版:前端「選取→錨定」需要,
CORS 加 POST)。與 CLI `judgment submit` 走同一套驗證
(`fusion.judgment.validate_judgment` — 候選集約束 / confidence_class 一致性 /
as_of ≤ snapshot_date);422 回拒絕原因 + 合法 anchor_key 清單。
"""

from __future__ import annotations

from datetime import date
from typing import Any

from fastapi import APIRouter, Depends, HTTPException

from web_api.pool import db_conn

router = APIRouter(tags=["judgments"])


@router.post("/judgments", status_code=201)
def submit_judgment(judgment: dict[str, Any], conn: Any = Depends(db_conn)):
    from fusion.judgment import (
        JudgmentValidationError,
        build_dossier,
        insert_judgment,
        validate_judgment,
    )

    stock_id = str(judgment.get("stock_id") or "")
    as_of_raw = judgment.get("as_of")
    try:
        as_of = date.fromisoformat(str(as_of_raw)) if as_of_raw else date.today()
    except ValueError:
        raise HTTPException(status_code=422, detail={"error": f"as_of 不是合法日期:{as_of_raw!r}"})

    dossier = build_dossier(conn, stock_id=stock_id, as_of=as_of)
    try:
        row = validate_judgment(judgment, dossier)
    except JudgmentValidationError as e:
        raise HTTPException(
            status_code=422,
            detail={"error": str(e), "legal_anchor_keys": e.legal_keys},
        )
    new_id = insert_judgment(conn, row)
    return {
        "id": new_id,
        "stock_id": row["stock_id"],
        "timeframe": row["timeframe"],
        "confidence_class": row["confidence_class"],
        "status": row["status"],
    }
