"""
inspect_db.py
-------------
快速檢視 data/tw_stock.db 的入庫狀態。
顯示每張表筆數，並對 price_adjustment_events 按 stock_id × event_type 統計。

執行：
    python scripts/inspect_db.py            # 全部表筆數
    python scripts/inspect_db.py 2330       # 加印特定股票的事件分布
"""

from __future__ import annotations

import sqlite3
import sys
from pathlib import Path

DB_PATH = Path(__file__).resolve().parent.parent / "data" / "tw_stock.db"

# 主要關注的表
KEY_TABLES = [
    "stock_info",
    "trading_calendar",
    "market_index_tw",
    "price_adjustment_events",
    "_dividend_policy_staging",
    "price_daily",
    "price_limit",
    "price_daily_fwd",
    "price_weekly_fwd",
    "price_monthly_fwd",
    "institutional_daily",
    "margin_daily",
    "foreign_holding",
    "holding_shares_per",
    "valuation_daily",
    "day_trading",
    "index_weight_daily",
    "monthly_revenue",
    "financial_statement",
    "market_index_us",
    "exchange_rate",
    "institutional_market_daily",
    "market_margin_maintenance",
    "fear_greed_index",
    "api_sync_progress",
]


def main(argv: list[str]) -> int:
    if not DB_PATH.exists():
        print(f"ERROR: DB 不存在: {DB_PATH}")
        return 2

    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row
    cur = conn.cursor()

    # 列出所有實際存在的表
    cur.execute("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
    existing = {row["name"] for row in cur.fetchall()}

    print(f"=== {DB_PATH} ===")
    print(f"{'table':<30s} {'rows':>10s}")
    print("-" * 42)
    for t in KEY_TABLES:
        if t not in existing:
            print(f"{t:<30s} {'(no table)':>10s}")
            continue
        n = cur.execute(f"SELECT COUNT(*) FROM {t}").fetchone()[0]
        print(f"{t:<30s} {n:>10d}")

    # 沒列在 KEY_TABLES 但存在的表
    extras = sorted(existing - set(KEY_TABLES))
    if extras:
        print()
        print("--- 其他表 ---")
        for t in extras:
            n = cur.execute(f"SELECT COUNT(*) FROM {t}").fetchone()[0]
            print(f"{t:<30s} {n:>10d}")

    # 特定股票的事件分布
    if len(argv) > 1:
        stock_id = argv[1]
        if "price_adjustment_events" in existing:
            print()
            print(f"--- price_adjustment_events for {stock_id} ---")
            rows = cur.execute(
                "SELECT event_type, COUNT(*) AS n FROM price_adjustment_events "
                "WHERE stock_id = ? GROUP BY event_type ORDER BY event_type",
                (stock_id,),
            ).fetchall()
            if not rows:
                print(f"  (no events)")
            for r in rows:
                print(f"  {r['event_type']:<24s} {r['n']:>6d}")

            print()
            print(f"--- price_adjustment_events 明細 for {stock_id} ---")
            print(f"  {'date':<12s} {'event_type':<20s} {'cash_div':>10s} {'stock_div':>10s} {'AF':>8s}")
            rows = cur.execute(
                "SELECT date, event_type, cash_dividend, stock_dividend, adjustment_factor "
                "FROM price_adjustment_events WHERE stock_id = ? ORDER BY date",
                (stock_id,),
            ).fetchall()
            for r in rows:
                cash = f"{r['cash_dividend']:.4f}" if r['cash_dividend'] is not None else "NULL"
                stk  = f"{r['stock_dividend']:.4f}" if r['stock_dividend'] is not None else "NULL"
                af   = f"{r['adjustment_factor']:.4f}" if r['adjustment_factor'] is not None else "NULL"
                print(f"  {r['date']:<12s} {r['event_type']:<20s} {cash:>10s} {stk:>10s} {af:>8s}")

        # _dividend_policy_staging 的 detail JSON 是否非空
        if "_dividend_policy_staging" in existing:
            print()
            print(f"--- _dividend_policy_staging for {stock_id}（detail 是否有資料）---")
            rows = cur.execute(
                "SELECT date, "
                "CASE WHEN detail IS NULL OR detail = '' THEN 'EMPTY' "
                "     ELSE substr(detail, 1, 80) || '...' END AS detail_preview "
                "FROM _dividend_policy_staging WHERE stock_id = ? ORDER BY date",
                (stock_id,),
            ).fetchall()
            if not rows:
                print("  (no rows)")
            for r in rows:
                print(f"  {r['date']:<12s} {r['detail_preview']}")

        # Phase 3：price_daily 最近 5 筆 + 最早 / 最新 / 筆數
        if "price_daily" in existing:
            print()
            print(f"--- price_daily for {stock_id}（範圍 + 最近 5 筆）---")
            stat = cur.execute(
                "SELECT COUNT(*) AS n, MIN(date) AS d_min, MAX(date) AS d_max "
                "FROM price_daily WHERE stock_id = ?",
                (stock_id,),
            ).fetchone()
            if stat["n"] == 0:
                print("  (no rows)")
            else:
                print(f"  rows={stat['n']}  range={stat['d_min']} ~ {stat['d_max']}")
                print(f"  {'date':<12s} {'open':>9s} {'high':>9s} {'low':>9s} "
                      f"{'close':>9s} {'volume':>12s} {'turnover':>14s}")
                rows = cur.execute(
                    "SELECT date, open, high, low, close, volume, turnover "
                    "FROM price_daily WHERE stock_id = ? "
                    "ORDER BY date DESC LIMIT 5",
                    (stock_id,),
                ).fetchall()
                for r in rows:
                    o = f"{r['open']:.2f}"     if r['open']     is not None else "NULL"
                    h = f"{r['high']:.2f}"     if r['high']     is not None else "NULL"
                    l = f"{r['low']:.2f}"      if r['low']      is not None else "NULL"
                    c = f"{r['close']:.2f}"    if r['close']    is not None else "NULL"
                    v = f"{r['volume']:,}"     if r['volume']   is not None else "NULL"
                    t = f"{r['turnover']:,.0f}" if r['turnover'] is not None else "NULL"
                    print(f"  {r['date']:<12s} {o:>9s} {h:>9s} {l:>9s} {c:>9s} {v:>12s} {t:>14s}")

        # Phase 3：price_limit 最近 5 筆 + 範圍
        if "price_limit" in existing:
            print()
            print(f"--- price_limit for {stock_id}（範圍 + 最近 5 筆）---")
            stat = cur.execute(
                "SELECT COUNT(*) AS n, MIN(date) AS d_min, MAX(date) AS d_max "
                "FROM price_limit WHERE stock_id = ?",
                (stock_id,),
            ).fetchone()
            if stat["n"] == 0:
                print("  (no rows)")
            else:
                print(f"  rows={stat['n']}  range={stat['d_min']} ~ {stat['d_max']}")
                print(f"  {'date':<12s} {'limit_up':>10s} {'limit_down':>12s}")
                rows = cur.execute(
                    "SELECT date, limit_up, limit_down "
                    "FROM price_limit WHERE stock_id = ? "
                    "ORDER BY date DESC LIMIT 5",
                    (stock_id,),
                ).fetchall()
                for r in rows:
                    lu = f"{r['limit_up']:.2f}"   if r['limit_up']   is not None else "NULL"
                    ld = f"{r['limit_down']:.2f}" if r['limit_down'] is not None else "NULL"
                    print(f"  {r['date']:<12s} {lu:>10s} {ld:>12s}")

        # sync 進度
        if "api_sync_progress" in existing:
            print()
            print(f"--- api_sync_progress for {stock_id} 與 __ALL__ ---")
            rows = cur.execute(
                "SELECT api_name, stock_id, status, COUNT(*) AS segments "
                "FROM api_sync_progress "
                "WHERE stock_id IN (?, '__ALL__') "
                "GROUP BY api_name, stock_id, status ORDER BY api_name, stock_id",
                (stock_id,),
            ).fetchall()
            for r in rows:
                print(f"  {r['api_name']:<24s} {r['stock_id']:<10s} "
                      f"{r['status']:<20s} segments={r['segments']}")

    conn.close()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
