"""Tests for refresh chain 15:30 market-close gate(`_is_before_market_close` +
`_run_refresh` 入口 gate)。

15:30 為 Asia/Taipei 盤後資料釋出 safety cutoff(法人 / 借券 / 處置等 chip
dataset 通常 15:00 後才上 FinMind)。之前跑 refresh 會 Bronze 拿不到當日新
資料,後續 Silver/Cross/Cores/Golden 全部白跑 → entry 直接 early return。
`--force` 繞過供 ad-hoc 補資料用。
"""

from __future__ import annotations

import asyncio
import sys
from datetime import datetime
from pathlib import Path
from unittest.mock import MagicMock
from zoneinfo import ZoneInfo

_REPO_ROOT = Path(__file__).resolve().parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)


_TP = ZoneInfo("Asia/Taipei")


def _import_main():
    import main as _main
    return _main


def test_before_market_close_at_0900():
    main = _import_main()
    now = datetime(2026, 6, 9, 9, 0, tzinfo=_TP)
    assert main._is_before_market_close(now) is True


def test_before_market_close_at_1529():
    main = _import_main()
    now = datetime(2026, 6, 9, 15, 29, tzinfo=_TP)
    assert main._is_before_market_close(now) is True


def test_at_market_close_1530_not_before():
    main = _import_main()
    now = datetime(2026, 6, 9, 15, 30, tzinfo=_TP)
    assert main._is_before_market_close(now) is False


def test_after_market_close_at_1600():
    main = _import_main()
    now = datetime(2026, 6, 9, 16, 0, tzinfo=_TP)
    assert main._is_before_market_close(now) is False


def test_after_market_close_at_2300():
    main = _import_main()
    now = datetime(2026, 6, 9, 23, 0, tzinfo=_TP)
    assert main._is_before_market_close(now) is False


def test_midnight_is_before():
    main = _import_main()
    now = datetime(2026, 6, 9, 0, 0, tzinfo=_TP)
    assert main._is_before_market_close(now) is True


def _make_refresh_args(*, force: bool = False):
    args = MagicMock()
    args.stocks = None
    args.skip_cores = True
    args.skip_bronze = False
    args.verbose = False
    args.config = "config/collector.toml"
    args.stock_list = "config/stock_list.toml"
    args.force = force
    return args


def test_run_refresh_skips_when_before_1530_without_force(monkeypatch, caplog):
    main = _import_main()
    monkeypatch.setattr(main, "_is_before_market_close", lambda: True)
    # 任一 step helper 被叫 → gate 失效;用 sentinel exception 抓
    monkeypatch.setattr(
        main, "_run_collector",
        lambda *a, **k: (_ for _ in ()).throw(AssertionError("Bronze should not run before 15:30")),
    )
    monkeypatch.setattr(
        main, "_run_silver",
        lambda *a, **k: (_ for _ in ()).throw(AssertionError("Silver should not run before 15:30")),
    )

    args = _make_refresh_args(force=False)
    with caplog.at_level("INFO"):
        asyncio.run(main._run_refresh(args, MagicMock(), MagicMock()))

    # 確認 log 含 skip 訊息
    log_text = "\n".join(rec.getMessage() for rec in caplog.records)
    assert "15:30" in log_text
    assert "skip" in log_text.lower() or "FinMind" in log_text


def test_run_refresh_proceeds_when_force_even_before_1530(monkeypatch):
    main = _import_main()
    monkeypatch.setattr(main, "_is_before_market_close", lambda: True)

    called = {"collector": False}

    async def _fake_collector(*a, **k):
        called["collector"] = True

    async def _fake_silver(*a, **k):
        pass

    async def _fake_cross(*a, **k):
        pass

    monkeypatch.setattr(main, "_run_collector", _fake_collector)
    monkeypatch.setattr(main, "_run_silver", _fake_silver)
    monkeypatch.setattr(main, "_run_cross_cores", _fake_cross)

    args = _make_refresh_args(force=True)
    args.skip_cores = True  # 跳過 cores / forecast / golden 簡化測試
    asyncio.run(main._run_refresh(args, MagicMock(), MagicMock()))
    assert called["collector"] is True, "--force 後 Bronze 仍須執行"


def test_run_refresh_proceeds_after_1530_without_force(monkeypatch):
    main = _import_main()
    monkeypatch.setattr(main, "_is_before_market_close", lambda: False)

    called = {"collector": False}

    async def _fake_collector(*a, **k):
        called["collector"] = True

    async def _fake_silver(*a, **k):
        pass

    async def _fake_cross(*a, **k):
        pass

    monkeypatch.setattr(main, "_run_collector", _fake_collector)
    monkeypatch.setattr(main, "_run_silver", _fake_silver)
    monkeypatch.setattr(main, "_run_cross_cores", _fake_cross)

    args = _make_refresh_args(force=False)
    args.skip_cores = True
    asyncio.run(main._run_refresh(args, MagicMock(), MagicMock()))
    assert called["collector"] is True, "15:30 後 gate 應放行"
