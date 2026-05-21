"""
scripts/seed_all_market_sync_progress.py
=========================================
一次性 migration helper:dataset 從 per_stock 轉 all_market 後,api_sync_progress
對新轉的 dataset 沒有 ALL_MARKET_SENTINEL("__ALL__") 的進度紀錄。incremental
模式查 get_last_sync(name, "__ALL__") 會回 None → 誤判「從未同步」→ 從
backfill_start_date 重抓整段歷史(13 dataset × ~2700 日 ≈ 6 小時)。

本 script 對每個 all_market dataset,若無 __ALL__ 進度紀錄,就用 bronze
target_table 的 MAX(date) 補一筆 status='completed' 的 sentinel 紀錄。之後
incremental 只會抓 MAX(date) 之後的缺口。

冪等:已有 __ALL__ 紀錄的 dataset(price_limit 等既有 all_market entry,以及
本 script 已跑過的)自動略過。bronze 歷史資料完全不動。

用法:
    python scripts/seed_all_market_sync_progress.py --dry-run   # 先看會做什麼
    python scripts/seed_all_market_sync_progress.py             # 實際寫入
"""
from __future__ import annotations

import argparse
import sys
from datetime import date
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parent.parent
_SRC = _REPO_ROOT / "src"
if str(_SRC) not in sys.path:
    sys.path.insert(0, str(_SRC))

from api_client import ALL_MARKET_SENTINEL          # noqa: E402
from config_loader import load_collector_config     # noqa: E402
from db import create_writer                        # noqa: E402
from sync_tracker import SyncTracker                # noqa: E402

_ALL_MARKET_MODES = {"all_market", "all_market_no_id", "all_market_no_end"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dry-run", action="store_true",
        help="只列出會做什麼,不寫入 api_sync_progress",
    )
    args = parser.parse_args()

    db = create_writer()
    tracker = SyncTracker(db)
    cfg = load_collector_config(str(_REPO_ROOT / "config" / "collector.toml"))

    seeded, skipped = 0, 0
    print(f"{'[dry-run] ' if args.dry_run else ''}seed all_market sync progress")
    print("=" * 72)

    for api in cfg.apis:
        if api.param_mode not in _ALL_MARKET_MODES or not api.enabled:
            continue

        existing = tracker.get_last_sync(api.name, ALL_MARKET_SENTINEL)
        if existing is not None:
            print(f"  skip  {api.name:34} 已有 __ALL__ 進度(last_sync={existing})")
            skipped += 1
            continue

        try:
            row = db.query_one(f"SELECT MAX(date) AS d FROM {api.target_table}")
        except Exception as e:  # noqa: BLE001
            print(f"  skip  {api.name:34} 查 {api.target_table}.MAX(date) 失敗:{e}")
            skipped += 1
            continue

        max_date = row["d"] if row else None
        if max_date is None:
            print(f"  skip  {api.name:34} {api.target_table} 無資料(屬真正首次同步)")
            skipped += 1
            continue

        # clamp 到 today:target table 可能有未來日(除息日預先公告),seed 未來日
        # 會讓 incremental 從未來起算、跳過真實缺口。
        if not hasattr(max_date, "isoformat"):
            max_date = date.fromisoformat(str(max_date))
        md = min(max_date, date.today()).isoformat()
        if args.dry_run:
            print(f"  SEED  {api.name:34} → __ALL__ completed up to {md}")
        else:
            tracker.mark_progress(
                api.name, ALL_MARKET_SENTINEL, md, md,
                status="completed", record_count=0,
            )
            print(f"  seed  {api.name:34} → __ALL__ completed up to {md}")
        seeded += 1

    print("=" * 72)
    print(f"{'would seed' if args.dry_run else 'seeded'}: {seeded}    skipped: {skipped}")
    if args.dry_run and seeded:
        print("\n確認無誤後拿掉 --dry-run 實際寫入。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
