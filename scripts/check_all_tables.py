"""
全表筆數體檢腳本：確認哪些表的資料真的有 commit。

從 repo 根目錄跑：
    python scripts/check_all_tables.py
"""
import sys
from pathlib import Path

# scripts/ 在 repo root 下一層，src/ 也在 root 下一層，所以是 parent.parent / "src"
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "src"))
from db import create_writer

TABLES_TO_CHECK = [
    # Phase 0 / 1
    "trading_calendar",
    "stock_info",
    "market_index_tw",
    # Phase 2
    "price_adjustment_events",
    # Phase 3
    "price_daily",
    "price_limit",
    # Phase 4
    "price_daily_fwd",
    "price_weekly_fwd",
    "price_monthly_fwd",
    # Phase 5
    "institutional_daily",
    "margin_daily",
    "foreign_holding",
    "holding_shares_per_legacy_v2",      # PR #R2 rename(原 holding_shares_per)
    "valuation_daily",
    "day_trading",
    "index_weight_daily",
    "monthly_revenue_legacy_v2",         # PR #R2 rename(原 monthly_revenue)
    "financial_statement_legacy_v2",     # PR #R2 rename(原 financial_statement)
    # Phase 6
    "market_index_us",
    "exchange_rate",
    "institutional_market_daily",
    "market_margin_maintenance",
    "fear_greed_index",
    # System
    "api_sync_progress",
    "stock_sync_status",
]

db = create_writer()
try:
    print(f"{'Table':<35} {'Rows':>10}")
    print("-" * 50)
    for t in TABLES_TO_CHECK:
        try:
            row = db.query_one(f'SELECT COUNT(*) AS cnt FROM "{t}"')
            cnt = row["cnt"]
            mark = "  ⚠️ EMPTY" if cnt == 0 else ""
            print(f"{t:<35} {cnt:>10}{mark}")
        except Exception as e:
            print(f"{t:<35} {'ERROR':>10}  ({type(e).__name__})")
finally:
    db.close()
