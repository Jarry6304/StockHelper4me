"""dual_track · 關係層(共振判定 + cross_stock 升振 + T1/T2)。

對齊 m3Spec/dual_track_resonance.md §三 + §四 + §五 + §十一。

公開 API:
    `resonance(stock_id, as_of, ...)` — 一次回傳完整 DualTrackResult。

判定流程:
    1. 取現價(price_daily authoritative source)
    2. 讀軌道一(structural_snapshots → primary scenario + fib_lines + invalidation)
    3. A-3 失效閘門:current_price 跌破 invalidation → single_track_mode=True
    4. 讀軌道二(forecast_log filtered → multi-horizon bands)
    5. 取 cross_stock is_top_30(對齊 as_of)
    6. 逐 fib 線判定 level:
        - divergence:band 不涵蓋 / band 過寬 / 無 band → 兩軌該線各自呈現
        - basic:band 涵蓋 fib_line.price
        - strong:basic + median 貼近 + is_top_30(三條件齊備)
    7. T1 標註(命中時最緊 horizon)+ T2 多 horizon 剖面
"""

from __future__ import annotations

from datetime import date
from typing import Any

from fusion.raw._db import (
    fetch_cross_stock_ranked,
    fetch_latest_close,
    get_connection,
)

from fusion.dual_track._shared import (
    ALL_HORIZONS,
    DEFAULT_CROSS_STOCK_TABLE,
    DualTrackResult,
    FibLine,
    FibLineResonance,
    MEDIAN_CLOSE_TOLERANCE,
    PRIMARY_CONFIDENCE,
    PRIMARY_HORIZON_DAYS,
    Track1View,
    Track2Band,
    Track2View,
)
from fusion.dual_track.track1 import read_track1
from fusion.dual_track.track2 import read_track2


__all__ = ["resonance", "judge_fib_line", "fetch_is_top_30"]


# ─── Level judgement(A-1)────────────────────────────────────────────────────


def _is_band_covers(band: Track2Band | None, fib_price: float) -> bool:
    """② band 涵蓋 fib 線。band 過寬 → 視為 not covers(防呆)。"""
    if band is None:
        return False
    if band.is_overly_wide:
        return False
    return band.covers(fib_price)


def _is_median_close(
    band: Track2Band | None,
    fib_price: float,
    current_price: float | None,
    tolerance: float = MEDIAN_CLOSE_TOLERANCE,
) -> bool:
    """③ band.point(中位數)貼近 fib_line.price。

    距離 / 現價 < tolerance(預設 2%)→ True。current_price 缺則 fallback 用
    fib_price 做分母(較保守)。
    """
    if band is None:
        return False
    denom = float(current_price) if current_price and float(current_price) > 0 else float(fib_price)
    if denom <= 0:
        return False
    return abs(float(band.point) - float(fib_price)) / denom < tolerance


def judge_fib_line(
    *,
    fib_line: FibLine,
    primary_band: Track2Band | None,
    current_price: float | None,
    is_top_30: bool,
    all_bands: dict[int, Track2Band],
    median_tolerance: float = MEDIAN_CLOSE_TOLERANCE,
) -> FibLineResonance:
    """單一 fib 線共振判定(A-1)+ T1/T2 標註。

    對齊 m3Spec/dual_track_resonance.md §三 三級表 + §四 cross_stock 升振 + §五。
    """
    covers = _is_band_covers(primary_band, fib_line.price)
    median_close = _is_median_close(primary_band, fib_line.price, current_price, median_tolerance)
    notes: list[str] = []

    # 三級 + cross_stock 升振
    if not covers:
        level = "divergence"
        cross_stock_boost = False  # divergence 不被 is_top_30 仲裁
        if primary_band is None:
            notes.append("no primary band — divergence by absence")
        elif primary_band.is_overly_wide:
            notes.append(
                f"primary band overly wide (width/price={primary_band.width_ratio:.3f} "
                f"> threshold) — suppressed to divergence"
            )
        else:
            notes.append(
                f"primary band [{primary_band.lower:.2f}, {primary_band.upper:.2f}] "
                f"does not cover fib_line price {fib_line.price:.2f}"
            )
    else:
        # band 涵蓋 → 至少 basic
        if median_close and is_top_30:
            level = "strong"
            cross_stock_boost = True
            notes.append("strong: covers + median close + is_top_30")
        elif median_close:
            level = "basic"
            cross_stock_boost = False
            notes.append("basic: covers + median close,but is_top_30=False (no boost)")
        elif is_top_30:
            level = "basic"
            cross_stock_boost = False  # boost 僅在 median_close 時觸發
            notes.append("basic: covers,is_top_30=True 但 median 不貼近 (no strong boost)")
        else:
            level = "basic"
            cross_stock_boost = False

    # T1:標 primary horizon(命中時最緊 horizon 也可,但 primary 是主判定軸)
    t1_horizon: int | None = None
    if level in ("basic", "strong"):
        # 找「最緊」(width 最小)的 covers band horizon — 較精確的時間窗
        covers_horizons = [
            (h, b) for h, b in all_bands.items()
            if not b.is_overly_wide and b.covers(fib_line.price)
        ]
        if covers_horizons:
            # 取 width 最小者(較精確的「N 天內涵蓋」陳述)
            best_h, _ = min(covers_horizons, key=lambda hb: hb[1].upper - hb[1].lower)
            t1_horizon = best_h

    # T2:per-horizon 剖面(每 horizon 自己的 level,不套 cross_stock 升振 — 純資訊)
    t2_profile: dict[int, str] = {}
    for h, band in all_bands.items():
        h_covers = not band.is_overly_wide and band.covers(fib_line.price)
        h_median_close = _is_median_close(band, fib_line.price, current_price, median_tolerance)
        if not h_covers:
            t2_profile[h] = "divergence"
        elif h_median_close:
            t2_profile[h] = "basic_median_close"  # T2 不含 cross_stock 升振,純結構訊息
        else:
            t2_profile[h] = "basic"

    return FibLineResonance(
        fib_line=fib_line,
        level=level,
        band_covers=covers,
        median_close=median_close,
        cross_stock_boost=cross_stock_boost,
        t1_horizon=t1_horizon,
        t2_profile=t2_profile,
        notes=notes,
    )


# ─── Cross-stock 旁路升振 ────────────────────────────────────────────────────


def fetch_is_top_30(
    conn,
    *,
    stock_id: str,
    as_of: date,
    source_table: str = DEFAULT_CROSS_STOCK_TABLE,
    is_top_col: str = "is_top_30",
) -> tuple[bool, date | None]:
    """查 cross_stock ranked_derived 表,取對齊 as_of 的當下最新 ranking_date
    對該股的 is_top_30 旗標。

    對齊 m3Spec/dual_track_resonance.md §四「對齊預測 as_of,取 date <= as_of
    之當下最新期(ranked_derived PK 含 date,天然留歷史,無 lookahead)」。

    Returns:
        (is_top, ranking_date) — 無資料時 (False, None)
    """
    # 1. latest ranking_date ≤ as_of
    sql_date = f"""
        SELECT MAX(date) AS d FROM {source_table}
         WHERE market = 'TW' AND date <= %s
    """
    with conn.cursor() as cur:
        cur.execute(sql_date, (as_of,))
        row = cur.fetchone()
    ranking_date = row.get("d") if row else None
    if ranking_date is None:
        return False, None

    # 2. 該 stock_id 是否 is_top_30=TRUE
    sql_flag = f"""
        SELECT {is_top_col} AS flag FROM {source_table}
         WHERE market = 'TW' AND date = %s AND stock_id = %s
         LIMIT 1
    """
    with conn.cursor() as cur:
        cur.execute(sql_flag, (ranking_date, stock_id))
        row = cur.fetchone()
    flag = bool(row.get("flag")) if row else False
    return flag, ranking_date


# ─── Public API ──────────────────────────────────────────────────────────────


def resonance(
    stock_id: str,
    as_of: date,
    *,
    primary_horizon: int = PRIMARY_HORIZON_DAYS,
    primary_confidence: float = PRIMARY_CONFIDENCE,
    horizons: tuple[int, ...] = ALL_HORIZONS,
    cross_stock_table: str = DEFAULT_CROSS_STOCK_TABLE,
    median_tolerance: float = MEDIAN_CLOSE_TOLERANCE,
    timeframe: str = "daily",
    database_url: str | None = None,
    conn: Any = None,
) -> DualTrackResult:
    """雙軌共振判定主入口。

    對齊 m3Spec/dual_track_resonance.md §一 完整流程:
    事實層(三表 + forecast_log)→ 讀法層(軌道一 / 二)→ 關係層(A-3 / A-1 /
    cross_stock 升振 / T1/T2)。

    Args:
        stock_id: 股票代號
        as_of: 判定日(上界,包含)
        primary_horizon: 主判定 horizon(預設 63)
        primary_confidence: 主判定 confidence(預設 0.80)
        horizons: T2 多 horizon 剖面取的 horizons(預設 21/63/126)
        cross_stock_table: ranked_derived 來源表(預設 magic_formula_ranked_derived)
        median_tolerance: ③ 中位數貼近容差(預設 2%)
        timeframe: structural_snapshots.timeframe(預設 daily)

    Returns:
        DualTrackResult — track1 / track2 / findings(per fib line)/ single_track_mode
    """
    own_conn = conn is None
    if own_conn:
        conn = get_connection(database_url)

    try:
        # 1. 現價(authoritative source = price_daily)
        price_info = fetch_latest_close(conn, stock_id=stock_id, as_of=as_of)
        current_price: float | None = price_info.get("close") if price_info else None

        # 2. 軌道一(含 A-3 失效閘門判定)
        track1 = read_track1(
            conn,
            stock_id=stock_id,
            as_of=as_of,
            current_price=current_price,
            timeframe=timeframe,
        )

        # 3. 軌道二(多 horizon)
        track2 = read_track2(
            conn,
            stock_id=stock_id,
            as_of=as_of,
            current_price=current_price,
            primary_horizon=primary_horizon,
            primary_confidence=primary_confidence,
            horizons=horizons,
        )

        # 4. cross_stock is_top_30(對齊 as_of)
        is_top_30, ranking_date = fetch_is_top_30(
            conn,
            stock_id=stock_id,
            as_of=as_of,
            source_table=cross_stock_table,
        )
    finally:
        if own_conn:
            conn.close()

    # 5. A-3 閘門:軌道一失效 → single_track_mode,不做共振
    single_track_mode = track1.invalidated
    findings: list[FibLineResonance] = []
    notes: list[str] = []

    if single_track_mode:
        notes.append(
            "A-3 invalidation gate triggered — track1 退場,軌道二單軌呈現,不顯示共振"
        )
    elif not track1.has_snapshot or not track1.fib_lines:
        notes.append("track1 unavailable (no snapshot or no fib lines) — 共振判定跳過")
    elif track2.primary_band is None:
        notes.append(
            f"track2 primary band (h={primary_horizon}, c={primary_confidence}) 缺 — "
            f"所有 fib 線將回 divergence(無法判定 ② 涵蓋)"
        )
        # 仍跑 judge_fib_line(會回 divergence + 多 horizon T2 profile)
        for fib in track1.fib_lines:
            findings.append(judge_fib_line(
                fib_line=fib,
                primary_band=track2.primary_band,
                current_price=current_price,
                is_top_30=is_top_30,
                all_bands=track2.horizons,
                median_tolerance=median_tolerance,
            ))
    else:
        # 完整路徑 — 逐 fib 線判定
        for fib in track1.fib_lines:
            findings.append(judge_fib_line(
                fib_line=fib,
                primary_band=track2.primary_band,
                current_price=current_price,
                is_top_30=is_top_30,
                all_bands=track2.horizons,
                median_tolerance=median_tolerance,
            ))

    # 6. 整體 notes(總覽 hint)
    if current_price is None:
        notes.append("current_price unavailable — A-3 / median_close 判定保守(fallback)")
    if is_top_30:
        notes.append(
            f"cross_stock 旁路升振:{stock_id} is_top_30=TRUE on {ranking_date} "
            f"(source={cross_stock_table})"
        )

    return DualTrackResult(
        stock_id=stock_id,
        as_of=as_of,
        track1=track1,
        track2=track2,
        is_top_30=is_top_30,
        is_top_30_source=cross_stock_table if ranking_date else None,
        is_top_30_date=ranking_date,
        findings=findings,
        single_track_mode=single_track_mode,
        notes=notes,
    )
