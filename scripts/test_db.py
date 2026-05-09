"""
測試 db.py 的完整流程:連線、init_schema、upsert、query、JSONB、DATE cast、
schema 變動容錯、_table_columns、close。
"""
import os
import sys
from pathlib import Path
from datetime import date, datetime

# 加路徑,讓 db.py 可以被 import
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

# 設定連線
os.environ["DATABASE_URL"] = "postgresql://twstock:twstock@localhost:5432/twstock"

from db import create_writer, PostgresWriter, SqliteWriter, DBWriter, SCHEMA_VERSION


def test_basic_flow():
    print("=" * 60)
    print("Test 1: Factory + Connection")
    print("=" * 60)
    db = create_writer()
    assert isinstance(db, PostgresWriter), f"期望 PostgresWriter,得到 {type(db)}"
    # Protocol 檢查
    assert isinstance(db, DBWriter), "PostgresWriter 不符合 DBWriter Protocol"
    print(f"  [OK] create_writer() 回傳 PostgresWriter,符合 DBWriter Protocol")

    print("\n" + "=" * 60)
    print("Test 2: init_schema (already initialized -> no-op)")
    print("=" * 60)
    db.init_schema()
    print(f"  [OK] init_schema 完成")

    print("\n" + "=" * 60)
    print("Test 3: schema_metadata 檢查")
    print("=" * 60)
    row = db.query_one("SELECT value FROM schema_metadata WHERE key = %s", ["schema_version"])
    print(f"  [OK] schema_version = {row['value']} (期望 {SCHEMA_VERSION})")
    assert row["value"] == SCHEMA_VERSION

    print("\n" + "=" * 60)
    print("Test 4: _table_columns / _table_column_types 快取")
    print("=" * 60)
    cols = db._table_columns("stock_info")
    print(f"  stock_info columns = {sorted(cols)}")
    assert "stock_id" in cols
    assert "detail" in cols  # JSONB 欄位

    types = db._table_column_types("stock_info")
    print(f"  stock_info types:")
    for c, t in sorted(types.items()):
        print(f"    {c:20s} -> {t}")
    assert types["detail"] == "jsonb"
    assert types["listing_date"] == "date"
    print(f"  [OK] 型別偵測正確")

    print("\n" + "=" * 60)
    print("Test 5: upsert 基本")
    print("=" * 60)
    rows = [
        {
            "market": "TW",
            "stock_id": "2330",
            "stock_name": "台積電",
            "market_type": "twse",
            "industry": "半導體",
            "listing_date": "1994-09-05",
            "par_value": 10.0,
            "detail": {"capital": 259000000000, "extra_info": "test"},
        },
        {
            "market": "TW",
            "stock_id": "0050",
            "stock_name": "元大台灣50",
            "market_type": "twse",
            "industry": "ETF",
            "listing_date": "2003-06-30",
            "par_value": 10.0,
            "detail": {"is_etf": True},
        },
    ]
    n = db.upsert("stock_info", rows, primary_keys=["market", "stock_id"])
    print(f"  [OK] upsert 寫入 {n} 筆")

    # 驗證 JSONB 寫入正確
    r = db.query_one(
        "SELECT detail FROM stock_info WHERE market = %s AND stock_id = %s",
        ["TW", "2330"],
    )
    print(f"  2330 detail = {r['detail']} (type={type(r['detail']).__name__})")
    assert isinstance(r["detail"], dict), "JSONB 應 deserialize 為 dict"
    assert r["detail"]["capital"] == 259000000000

    # 驗證 DATE 寫入正確
    r = db.query_one(
        "SELECT listing_date FROM stock_info WHERE stock_id = %s",
        ["2330"],
    )
    print(f"  2330 listing_date = {r['listing_date']} (type={type(r['listing_date']).__name__})")
    assert r["listing_date"] == date(1994, 9, 5), f"DATE 應為 date 物件"

    print("\n" + "=" * 60)
    print("Test 6: upsert ON CONFLICT DO UPDATE")
    print("=" * 60)
    # 改名再 upsert
    update_row = {
        "market": "TW",
        "stock_id": "2330",
        "stock_name": "台積電（改）",
        "detail": {"updated_test": True},
    }
    n = db.upsert("stock_info", [update_row], primary_keys=["market", "stock_id"])
    r = db.query_one(
        "SELECT stock_name, detail FROM stock_info WHERE stock_id = %s",
        ["2330"],
    )
    print(f"  [OK] 更新後 stock_name = {r['stock_name']}, detail = {r['detail']}")
    assert "改" in r["stock_name"]

    print("\n" + "=" * 60)
    print("Test 7: schema 變動容錯(API 給未知欄位)")
    print("=" * 60)
    bogus_row = {
        "market": "TW",
        "stock_id": "2317",
        "stock_name": "鴻海",
        "fake_unknown_field": "should_be_dropped",
        "another_bogus": 42,
    }
    n = db.upsert("stock_info", [bogus_row], primary_keys=["market", "stock_id"])
    print(f"  [OK] upsert 仍寫入 {n} 筆,未知欄位被略過(看 WARNING log)")

    print("\n" + "=" * 60)
    print("Test 8: 全欄位都是 PK 的表(trading_calendar)")
    print("=" * 60)
    n = db.upsert(
        "trading_calendar",
        [{"market": "TW", "date": "2026-04-28"}],
        primary_keys=["market", "date"],
    )
    print(f"  [OK] trading_calendar upsert {n} 筆 (應走 ON CONFLICT DO NOTHING)")
    # 重複插入應 no-op
    n = db.upsert(
        "trading_calendar",
        [{"market": "TW", "date": "2026-04-28"}],
        primary_keys=["market", "date"],
    )
    print(f"  [OK] 重複 upsert {n} 筆 (應為 0,因為 DO NOTHING)")

    print("\n" + "=" * 60)
    print("Test 9: financial_statement_legacy_v2 的 JSONB 欄位")
    print("=" * 60)
    fs_row = {
        "market": "TW",
        "stock_id": "2330",
        "date": "2026-03-31",
        "type": "income",
        "detail": {
            "Revenue": 7390000000000,
            "GrossProfit": 4250000000000,
            "OperatingIncome": 3150000000000,
        },
    }
    n = db.upsert(
        "financial_statement_legacy_v2",
        [fs_row],
        primary_keys=["market", "stock_id", "date", "type"],
    )
    # 用 JSONB 操作子驗證可查
    r = db.query_one(
        "SELECT detail->>'Revenue' AS revenue FROM financial_statement "
        "WHERE stock_id = %s AND type = %s",
        ["2330", "income"],
    )
    print(f"  [OK] JSONB ->>operator 取出 Revenue = {r['revenue']}")
    assert r["revenue"] == "7390000000000"

    print("\n" + "=" * 60)
    print("Test 10: NUMERIC 精度(par_value)")
    print("=" * 60)
    from decimal import Decimal
    r = db.query_one("SELECT par_value FROM stock_info WHERE stock_id = %s", ["2330"])
    print(f"  par_value = {r['par_value']} (type={type(r['par_value']).__name__})")
    assert isinstance(r["par_value"], Decimal), "NUMERIC 應 deserialize 為 Decimal"

    print("\n" + "=" * 60)
    print("Test 11: 清理 + close")
    print("=" * 60)
    db.update("DELETE FROM stock_info WHERE stock_id IN ('2330', '0050', '2317')", [])
    db.update("DELETE FROM trading_calendar WHERE date = %s", [date(2026, 4, 28)])
    db.update("DELETE FROM financial_statement WHERE stock_id = %s", ["2330"])
    db.close()
    print("  [OK] 清理完成,連線關閉")

    print("\n" + "=" * 60)
    print("ALL TESTS PASSED")
    print("=" * 60)


def test_factory_sqlite_fallback():
    """測試隱藏 flag 切到 SqliteWriter。"""
    print("\n" + "=" * 60)
    print("Test 12: TWSTOCK_USE_SQLITE=1 fallback")
    print("=" * 60)
    os.environ["TWSTOCK_USE_SQLITE"] = "1"
    os.environ["SQLITE_PATH"] = "/tmp/test_fallback.db"
    db = create_writer()
    assert isinstance(db, SqliteWriter), f"期望 SqliteWriter,得到 {type(db)}"
    print(f"  [OK] TWSTOCK_USE_SQLITE=1 切到 SqliteWriter")
    db.close()
    # 還原
    del os.environ["TWSTOCK_USE_SQLITE"]
    Path("/tmp/test_fallback.db").unlink(missing_ok=True)


def test_no_db_url_raises():
    """測試 DATABASE_URL 未設且非 SQLite 模式時拋錯。"""
    print("\n" + "=" * 60)
    print("Test 13: 缺 DATABASE_URL 拋錯")
    print("=" * 60)
    saved_url = os.environ.pop("DATABASE_URL", None)
    saved_sqlite = os.environ.pop("TWSTOCK_USE_SQLITE", None)
    try:
        try:
            create_writer()
            assert False, "應該拋錯"
        except RuntimeError as e:
            print(f"  [OK] 預期錯誤:{str(e)[:80]}...")
    finally:
        if saved_url:
            os.environ["DATABASE_URL"] = saved_url
        if saved_sqlite:
            os.environ["TWSTOCK_USE_SQLITE"] = saved_sqlite


if __name__ == "__main__":
    import logging
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(name)s: %(message)s")
    test_basic_flow()
    test_factory_sqlite_fallback()
    test_no_db_url_raises()
    print("\n=== ALL DONE ===")
