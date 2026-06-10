"""波浪域 tools — Neely / Traditional 雙引擎 + wave screen + 雙軌共振。

實作層:mcp_server/_forecast.py / _traditional.py / _screens.py +
src/fusion/dual_track/。註冊見 server.py mcp.tool() 區塊。
"""

from __future__ import annotations

from typing import Any

from mcp_server.tools._shared import _MAX_TOP_N, _clamp, _parse_date, _read_materialized_snapshot


def traditional_wave_forest(stock_id: str, timeframe: str = "daily") -> dict[str, Any]:
    """傳統派(Frost & Prechter EWP)波浪 forest — 與 Neely **並排、不整合**。

    讀 traditional_snapshots(獨立 vertical 自有表),回 forest 摘要:
    `top_scenarios`(依 preference_score 降序,**forest 不選 primary**)+ diagnostics + caveat。

    Args:
        stock_id: 股票代號(例 "3363")
        timeframe: "daily" / "weekly" / "monthly"(預設 daily;該表為 latest-per-(stock,tf))
    """
    from mcp_server._traditional import compute_traditional_forest

    return compute_traditional_forest(stock_id, timeframe)


def neely_forecast(
    stock_id: str,
    date: str,
) -> dict[str, Any]:
    """Neely 預測:4 個時間框架(月 / 季 / 半年 / 年)+ 上漲機率 + 價位區間(plan §Tool 1)。

    內部:撈 Neely scenario_forest 取 top 5 by power_rating → Fibonacci 投影
    分 4 時間框架 → 跨 cores 加權算 prob_up → invalidation_price 從 triggers 抽。

    輸出只回結論(~2 KB / ~500 tokens),不回 raw scenario_forest。

    Args:
        stock_id: 股票代號(例 "2330")
        date: 查詢日 ISO 字串

    Returns:
        {
          "stock_id": "2330",
          "as_of": "2026-05-13",
          "current_price": 1234.5,
          "primary_scenario": {label, pattern_type, power_rating, wave_count},
          "scenario_count": int,
          "forecasts": {
            "1_month":   {"prob_up": 0.62, "range_high": [...], "range_low": [...]},
            "1_quarter": {...},
            "6_month":   {...},
            "1_year":    {...}
          },
          "key_levels": {"support": [...], "resistance": [...]},
          "invalidation_price": float | None
        }
    """
    from mcp_server._forecast import compute_neely_forecast

    return compute_neely_forecast(stock_id, _parse_date(date))


def scan_wave_impulse(
    date: str,
    timeframe: str = "daily",
    top_n: int = 30,
    include_observe: bool = True,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Wave Impulse Cross-Stock Screen — r3 post-correction entry pivot。

    r3 重新定位(production verify 2026-05-27 揭露 neely_core forest 不 emit
    incomplete Impulse,r1/r2「找 W3 早段」設計不可行):
    - 改抓「3-wave Zigzag/Flat 向下修正剛完成」訊號 → 預期新 impulse 啟動,
      多頭反轉買點(對齊 NEoWave「A-B-C 結束後啟動新 impulse」)
    - 完整 5 波 Impulse → 反轉警示 observe
    - 上漲修正剛完成 → 空頭 setup observe(TW 多單略過)

    讀 wave_impulse_screen_derived(cross_cores Phase 8 第 12 個 builder):
    - Picker:`_pick_recent_correction` 找最近 RECENT_DAYS=14 內完成的
      Zigzag/Flat scenario
    - 過 R/R 1.5 + direction=down(向下修正)門檻才入 top_stocks
    - cross_tf_aligned:同股 daily+weekly 同向 CORRECTION_DONE_DOWN → 排名加分

    Phase enum(r3):
    - CORRECTION_DONE_DOWN:向下 3-wave 修正剛完成 → candidate(多頭反轉買點)
    - CORRECTION_DONE_UP:向上 3-wave 修正剛完成 → observe(空頭 setup)
    - CORRECTION_ONGOING:修正中 > RECENT_DAYS → observe
    - IMPULSE_COMPLETE:完整 5 波 → observe(反轉警示,該獲利了結)

    Args:
        date:            ISO 字串(例 "2026-05-27")
        timeframe:       daily / weekly / monthly(預設 daily)
        top_n:           top_stocks + observe_stocks 各取 N(預設 30)
        include_observe: 是否回 observe section(預設 True)

    Returns:
        {
          as_of, timeframe, top_n, ranking_date,
          top_stocks: [{stock_id, name, industry, rank, phase, wave_number,
                        pattern_kind, direction, effective_degree, structure_label,
                        confidence_level (strict/loose), entry_price, target_price,
                        invalidation_price, rr_ratio, cross_tf_aligned,
                        is_candidate}, ...],   # CORRECTION_DONE_DOWN candidates
          observe_stocks: [...]                # IMPULSE_COMPLETE / DONE_UP / ONGOING
          cross_tf_aligned_count, narrative, caveat
        }

    Refs:
      - Glenn Neely (1990). Mastering Elliott Wave, Ch6/7 corrective pattern 收尾
      - r3 pivot rationale 見 src/cross_cores/wave_impulse_screen.py docstring
    """
    from mcp_server._screens import compute_wave_impulse_scan

    return compute_wave_impulse_scan(
        _parse_date(date),
        timeframe=timeframe, top_n=_clamp(top_n, 1, _MAX_TOP_N),
        include_observe=include_observe,
        database_url=database_url,
    )


# ────────────────────────────────────────────────────────────
# Dual-Track Resonance(v1.0,2026-05-25)
#
# 對齊 m3Spec/dual_track_resonance.md §一(三層平面)+ §八(模組對應)。
# 軌道一(結構)+ 軌道二(統計)+ 關係層(A-3 失效閘門 / A-1 三級共振 /
# cross_stock 旁路升振 / T1/T2 時間反向標註)。
# ────────────────────────────────────────────────────────────


def dual_track_resonance(
    stock_id: str,
    date: str,
    primary_horizon: int = 63,
    primary_confidence: float = 0.80,
    cross_stock_table: str = "magic_formula_ranked_derived",
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """雙軌共振決策判定 — 個股「會怎麼走」的判斷呈現層。

    對齊 m3Spec/dual_track_resonance.md §一~§五。

    Returns:
        {
          "stock_id", "as_of",
          "track1": {                    # 軌道一(結構,非校準)
              "has_snapshot", "snapshot_date",
              "pattern_type", "power_rating", "direction",
              "effective_degree", "wave_count",
              "fib_lines": [{price, low, high, label, source_ratio}],
              "invalidation_price", "invalidated",
              "fallback_to_flat_union", "notes"
          },
          "track2": {                    # 軌道二(統計,校準)
              "current_price", "primary_horizon", "primary_confidence",
              "primary_band": {horizon_days, confidence, lower, upper, point,
                                source_core, width_ratio, is_overly_wide},
              "horizons": {21: band, 63: band, 126: band},  # 多 horizon T2
              "notes"
          },
          "is_top_30": bool,             # cross_stock 旁路升振狀態
          "is_top_30_source": str,       # ranked_derived 來源表
          "is_top_30_date": str,         # 對齊 as_of 取的 ranking_date
          "findings": [{                 # 逐 fib 線判定
              "fib_line": {...},
              "level": "divergence" | "basic" | "strong",
              "band_covers": bool,       # ② 軌道二涵蓋帶含此線
              "median_close": bool,      # ③ 中位數貼近(<2% of current)
              "cross_stock_boost": bool, # is_top_30 升振狀態
              "t1_horizon": int,         # T1 命中時最緊 horizon
              "t2_profile": {21, 63, 126: divergence/basic/basic_median_close},
              "notes"
          }],
          "single_track_mode": bool,     # A-3 閘門觸發,軌道一退場
          "notes"
        }

    判定規則(§三):
        - divergence:軌道二帶未涵蓋該 fib 線(或無 band / band 過寬)
        - basic:軌道二涵蓋帶包含該 fib 線
        - strong:basic + 中位數貼近 + is_top_30 命中(三條件齊備)

    cross_stock 升振(§四):
        - 並聯不擋路(全檔照跑)、命中升振、未命中不扣分、不仲裁分歧

    A-3 失效閘門(§四):
        - 現價跌破 invalidation_price → single_track_mode=True,共振判定跳過
    """
    from fusion.dual_track import resonance as _dual_resonance
    from fusion.raw._db import ALLOWED_RANKED_TABLES, get_connection

    as_of = _parse_date(date)
    # SQL identifier 注入防護:cross_stock_table 由 caller(LLM)控,非白名單表名
    # 早擋回 graceful error(library 層 fetch_is_top_30 也會 raise,此處讓 LLM 拿
    # 乾淨訊息而非 500/propagated ValueError)。
    if cross_stock_table not in ALLOWED_RANKED_TABLES:
        return {
            "stock_id": stock_id,
            "as_of": as_of.isoformat(),
            "error": f"cross_stock_table {cross_stock_table!r} 不在白名單",
            "allowed": sorted(ALLOWED_RANKED_TABLES),
        }
    # v4.32 Golden L3:預設參數 → 讀物化 resonance_fusion(daily);非預設 → compute fallback。
    # 物化 row = resonance().to_dict() 全量,讀到即直接回(免 30s timeout / fib cap 路徑)。
    if (
        primary_horizon == 63
        and primary_confidence == 0.80
        and cross_stock_table == "magic_formula_ranked_derived"
    ):
        doc = _read_materialized_snapshot(
            stock_id, as_of, "resonance_fusion", timeframe="daily",
            database_url=database_url,
        )
        if doc is not None:
            return doc

    # v4.28+ 安全網:30s statement timeout — v4.28 batch query 後既有 path 應 < 1s;
    # 若未來其他未知瓶頸(forecast_log 表規模 / index plan regression / scenario forest
    # 突增等)再次拖垮,LLM 拿明確 timeout error 而非 hang 4 分鐘。SET LOCAL 限本
    # transaction,conn close 後不影響其他 query;只在 MCP tool wrapper 設,不動
    # library code(dashboard / direct Python caller / test 不受影響)。
    conn = get_connection(database_url)
    try:
        with conn.cursor() as cur:
            cur.execute("SET LOCAL statement_timeout = '30s'")
        result = _dual_resonance(
            stock_id=stock_id,
            as_of=as_of,
            primary_horizon=primary_horizon,
            primary_confidence=primary_confidence,
            cross_stock_table=cross_stock_table,
            conn=conn,
        )
        return result.to_dict()
    except Exception as exc:
        # 只攔 statement_timeout 觸發的 QueryCanceled(graceful error response 給 LLM)
        # 其他 exception(connection lost / SQL error)正常 propagate 讓 caller 知道
        # 真實問題,避免靜默誤判 timeout。Lazy class-name match 不 hard-import psycopg
        # (對齊 _db.py:30 既有 lazy import pattern)。
        if type(exc).__name__ == "QueryCanceled":
            return {
                "stock_id": stock_id,
                "as_of": as_of.isoformat(),
                "error": "dual_track_resonance computation exceeded 30s timeout",
                "diagnostics": (
                    "Possible causes: forecast_log table size grew / index plan "
                    "regression / unexpected scenario forest size. Try EXPLAIN ANALYZE "
                    "on the batch query in fusion/dual_track/track2.py::fetch_bands_batch."
                ),
                "exception_type": "QueryCanceled",
            }
        raise
    finally:
        conn.close()
