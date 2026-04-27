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

        # Phase 4：price_daily_fwd 最近 5 筆（後復權）
        if "price_daily_fwd" in existing:
            print()
            print(f"--- price_daily_fwd for {stock_id}（範圍 + 最近 5 筆 後復權）---")
            stat = cur.execute(
                "SELECT COUNT(*) AS n, MIN(date) AS d_min, MAX(date) AS d_max "
                "FROM price_daily_fwd WHERE stock_id = ?",
                (stock_id,),
            ).fetchone()
            if stat["n"] == 0:
                print("  (no rows — Phase 4 尚未跑過或 stock 不在處理清單)")
            else:
                print(f"  rows={stat['n']}  range={stat['d_min']} ~ {stat['d_max']}")
                print(f"  {'date':<12s} {'open':>9s} {'high':>9s} {'low':>9s} "
                      f"{'close':>9s} {'volume':>12s}")
                rows = cur.execute(
                    "SELECT date, open, high, low, close, volume "
                    "FROM price_daily_fwd WHERE stock_id = ? "
                    "ORDER BY date DESC LIMIT 5",
                    (stock_id,),
                ).fetchall()
                for r in rows:
                    o = f"{r['open']:.2f}"     if r['open']     is not None else "NULL"
                    h = f"{r['high']:.2f}"     if r['high']     is not None else "NULL"
                    l = f"{r['low']:.2f}"      if r['low']      is not None else "NULL"
                    c = f"{r['close']:.2f}"    if r['close']    is not None else "NULL"
                    v = f"{r['volume']:,}"     if r['volume']   is not None else "NULL"
                    print(f"  {r['date']:<12s} {o:>9s} {h:>9s} {l:>9s} {c:>9s} {v:>12s}")

        # Phase 4：週K / 月K 簡報
        for tbl, kcol in (("price_weekly_fwd", "week"), ("price_monthly_fwd", "month")):
            if tbl in existing:
                print()
                print(f"--- {tbl} for {stock_id}（最近 3 筆）---")
                stat = cur.execute(
                    f"SELECT COUNT(*) AS n FROM {tbl} WHERE stock_id = ?",
                    (stock_id,),
                ).fetchone()
                if stat["n"] == 0:
                    print("  (no rows)")
                else:
                    print(f"  rows={stat['n']}")
                    print(f"  {'year':>4s} {kcol:>6s} {'open':>9s} {'high':>9s} "
                          f"{'low':>9s} {'close':>9s} {'volume':>14s}")
                    rows = cur.execute(
                        f"SELECT year, {kcol}, open, high, low, close, volume "
                        f"FROM {tbl} WHERE stock_id = ? "
                        f"ORDER BY year DESC, {kcol} DESC LIMIT 3",
                        (stock_id,),
                    ).fetchall()
                    for r in rows:
                        o = f"{r['open']:.2f}"   if r['open']   is not None else "NULL"
                        h = f"{r['high']:.2f}"   if r['high']   is not None else "NULL"
                        l = f"{r['low']:.2f}"    if r['low']    is not None else "NULL"
                        c = f"{r['close']:.2f}"  if r['close']  is not None else "NULL"
                        v = f"{r['volume']:,}"   if r['volume'] is not None else "NULL"
                        print(f"  {r['year']:>4d} {r[kcol]:>6d} {o:>9s} {h:>9s} "
                              f"{l:>9s} {c:>9s} {v:>14s}")

        # Phase 4 後復權正確性驗證：raw vs fwd 對比 + 累積 AF 比值
        # 預期：fwd_close(D) = raw_close(D) × ∏(AF for events where date > D)
        # 即「事件日當天當日 AF 不算」（除息日 raw 已是除息後價）
        if (
            "price_daily" in existing
            and "price_daily_fwd" in existing
            and "price_adjustment_events" in existing
        ):
            print()
            print(f"--- 後復權驗證 for {stock_id}（理論 vs 實際）---")
            events = cur.execute(
                "SELECT date, adjustment_factor "
                "FROM price_adjustment_events WHERE stock_id = ? ORDER BY date",
                (stock_id,),
            ).fetchall()
            if not events:
                print("  (無除權事件，fwd 應該完全等於 raw)")
            else:
                # 取一些關鍵驗證點：最早日 + 第一次事件前一日 + 第一次事件當日 + 最近日
                first_event_date = events[0]["date"]
                check_dates = cur.execute(
                    "SELECT date FROM price_daily WHERE stock_id = ? "
                    "AND date IN ("
                    "  (SELECT MIN(date) FROM price_daily WHERE stock_id = ?), "
                    "  (SELECT MAX(date) FROM price_daily WHERE stock_id = ? AND date < ?), "
                    "  ?, "
                    "  (SELECT MAX(date) FROM price_daily WHERE stock_id = ?)"
                    ") ORDER BY date",
                    (stock_id, stock_id, stock_id, first_event_date,
                     first_event_date, stock_id),
                ).fetchall()

                print(f"  累積事件數：{len(events)}")
                print(f"  {'date':<12s} {'raw_close':>10s} {'fwd_close':>10s} "
                      f"{'fwd/raw':>9s} {'theoretical':>12s} {'match':>6s}")

                for cd in check_dates:
                    d = cd["date"]
                    raw = cur.execute(
                        "SELECT close FROM price_daily WHERE stock_id = ? AND date = ?",
                        (stock_id, d),
                    ).fetchone()
                    fwd = cur.execute(
                        "SELECT close FROM price_daily_fwd WHERE stock_id = ? AND date = ?",
                        (stock_id, d),
                    ).fetchone()
                    if not raw or not fwd or raw["close"] is None or fwd["close"] is None:
                        continue
                    raw_c = raw["close"]
                    fwd_c = fwd["close"]
                    actual_ratio = fwd_c / raw_c if raw_c else float("nan")

                    # 理論值：累乘 D 之後（不含當日）所有事件的 AF
                    theo = 1.0
                    for ev in events:
                        if ev["date"] > d:
                            theo *= ev["adjustment_factor"]
                    diff_pct = abs(actual_ratio - theo) / theo * 100 if theo else 0
                    # 0.05% 以內視為 OK（浮點 round 誤差約 0.01%）；
                    # 0.5% 是「除息日當日 AF 重複計算」的典型 bug 量級
                    ok = "OK" if diff_pct < 0.05 else "FAIL"
                    print(f"  {d:<12s} {raw_c:>10.2f} {fwd_c:>10.2f} "
                          f"{actual_ratio:>9.4f} {theo:>12.4f} {ok:>6s}")

        # Phase 5：institutional_daily 5 類法人最近 3 筆
        if "institutional_daily" in existing:
            print()
            print(f"--- institutional_daily for {stock_id}（最近 3 筆 5 類法人）---")
            stat = cur.execute(
                "SELECT COUNT(*) AS n, MIN(date) AS d_min, MAX(date) AS d_max "
                "FROM institutional_daily WHERE stock_id = ?",
                (stock_id,),
            ).fetchone()
            if stat["n"] == 0:
                print("  (no rows)")
            else:
                print(f"  rows={stat['n']}  range={stat['d_min']} ~ {stat['d_max']}")
                rows = cur.execute(
                    "SELECT date, foreign_buy, foreign_sell, "
                    "foreign_dealer_self_buy, foreign_dealer_self_sell, "
                    "investment_trust_buy, investment_trust_sell, "
                    "dealer_buy, dealer_sell, "
                    "dealer_hedging_buy, dealer_hedging_sell "
                    "FROM institutional_daily WHERE stock_id = ? "
                    "ORDER BY date DESC LIMIT 3",
                    (stock_id,),
                ).fetchall()
                fmt = lambda v: f"{v:>12,d}" if v is not None else f"{'NULL':>12s}"
                for r in rows:
                    print(f"  {r['date']}")
                    print(f"    外資       buy={fmt(r['foreign_buy'])} sell={fmt(r['foreign_sell'])}")
                    print(f"    外資自營商 buy={fmt(r['foreign_dealer_self_buy'])} sell={fmt(r['foreign_dealer_self_sell'])}")
                    print(f"    投信       buy={fmt(r['investment_trust_buy'])} sell={fmt(r['investment_trust_sell'])}")
                    print(f"    自營(自行) buy={fmt(r['dealer_buy'])} sell={fmt(r['dealer_sell'])}")
                    print(f"    自營(避險) buy={fmt(r['dealer_hedging_buy'])} sell={fmt(r['dealer_hedging_sell'])}")

        # Phase 6 全市場資料（不分 stock_id，只看資料量 + 最近 3 筆）
        # 這幾張表沒 stock_id 欄位，跟 stock_id 引數無關，但跟著一起印方便驗證
        for tbl, cols in (
            ("market_index_us",            "stock_id, date, open, high, low, close, volume"),
            ("exchange_rate",               "date, currency, rate"),
            ("institutional_market_daily",  "date, foreign_buy, foreign_sell, "
                                            "foreign_dealer_self_buy, foreign_dealer_self_sell, "
                                            "investment_trust_buy, investment_trust_sell, "
                                            "dealer_buy, dealer_sell, "
                                            "dealer_hedging_buy, dealer_hedging_sell"),
            ("market_margin_maintenance",   "date, ratio"),
            ("fear_greed_index",            "date, score, label"),
        ):
            if tbl not in existing:
                continue
            print()
            print(f"--- {tbl}（最近 3 筆）---")
            stat = cur.execute(f"SELECT COUNT(*) AS n FROM {tbl}").fetchone()
            if stat["n"] == 0:
                print("  (no rows)")
                continue
            print(f"  rows={stat['n']}")
            try:
                rows = cur.execute(
                    f"SELECT {cols} FROM {tbl} ORDER BY date DESC LIMIT 3"
                ).fetchall()
                for r in rows:
                    items = [f"{k}={r[k]}" for k in r.keys()]
                    print("  " + ", ".join(items))
            except sqlite3.OperationalError as e:
                print(f"  (查詢失敗：{e})")

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
