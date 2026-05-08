"""
scripts/probe_finmind_datasets.py
==================================
PR #21-B smoke test 跑出 HTTP 400 / 422 後,用本 script 探:

1. 列 FinMind /data_list 全部 dataset(自動篩 GovernmentBank / ShortSale / SBL 相關)
2. 對 candidate dataset 名 + 真實名稱跑 1 個 segment 看 status code + 回欄位

依賴:`FINMIND_TOKEN` 環境變數 + aiohttp。

用法:
    python scripts/probe_finmind_datasets.py
"""
from __future__ import annotations

import asyncio
import os
import sys

import aiohttp


FINMIND_BASE_URL = "https://api.finmindtrade.com/api/v4/data"
FINMIND_DATALIST_URL = "https://api.finmindtrade.com/api/v4/datalist"


CANDIDATE_DATASETS = [
    # gov_bank candidates
    "TaiwanStockGovernmentBankBuySell",
    "TaiwanStockTotalGovernmentBankBuySell",
    "TaiwanStockEightLargeBankBuySell",
    # SBL daily aggregate candidates
    "TaiwanStockShortSaleSecuritiesLending",
    "TaiwanStockShortSaleBalance",
    "TaiwanDailyShortSaleBalances",
    "TaiwanStockShortSale",
    "TaiwanStockSecuritiesLending",        # 既有(對照,trade-level)
]


async def list_datasets(session: aiohttp.ClientSession, token: str) -> list[str]:
    """從 FinMind /datalist 拿全部 dataset 名(篩 chip / margin / SBL 相關)。"""
    params = {"token": token}
    try:
        async with session.get(FINMIND_DATALIST_URL, params=params, timeout=30) as resp:
            print(f"[/datalist] status={resp.status}")
            if resp.status != 200:
                text = await resp.text()
                print(f"  body: {text[:300]}")
                return []
            data = await resp.json()
            items = data.get("data", []) if isinstance(data, dict) else []
            return [
                str(item) if isinstance(item, str) else item.get("table_id", str(item))
                for item in items
            ]
    except Exception as e:
        print(f"  error: {e}")
        return []


async def probe(
    session: aiohttp.ClientSession,
    token: str,
    dataset: str,
    *,
    with_data_id: bool = True,
) -> None:
    """跑 1 個 segment 看 status + 回欄位。"""
    params = {
        "dataset":    dataset,
        "start_date": "2025-01-01",
        "end_date":   "2025-01-31",
        "token":      token,
    }
    if with_data_id:
        params["data_id"] = "2330"
    label = f"{dataset:50} (data_id={'2330' if with_data_id else 'NONE'})"
    try:
        async with session.get(FINMIND_BASE_URL, params=params, timeout=30) as resp:
            text = await resp.text()
            if resp.status == 200:
                # Parse and show fields
                import json
                body = json.loads(text)
                rows = body.get("data", [])
                if rows:
                    fields = sorted(rows[0].keys())
                    print(f"  ✅ {label} → 200 OK, {len(rows)} rows, fields={fields}")
                    print(f"      sample row: {rows[0]}")
                else:
                    print(f"  ⚠️  {label} → 200 OK but 0 rows")
            else:
                # Show error message
                print(f"  ❌ {label} → {resp.status}: {text[:200]}")
    except Exception as e:
        print(f"  💥 {label} → exception: {e}")


async def main() -> int:
    token = os.environ.get("FINMIND_TOKEN")
    if not token:
        print("FINMIND_TOKEN env var not set")
        return 1

    async with aiohttp.ClientSession() as session:
        # 1. List available datasets matching keywords
        print("=" * 80)
        print("Phase 1: /datalist 全部 dataset(篩 GovernmentBank / ShortSale / SBL / Margin / Lending)")
        print("=" * 80)
        all_datasets = await list_datasets(session, token)
        if all_datasets:
            keywords = ["governmentbank", "shortsale", "sbl", "lending", "marginpurchase",
                        "totalmargin", "securities"]
            matches = [
                d for d in all_datasets
                if any(kw in d.lower() for kw in keywords)
            ]
            print(f"  total datasets: {len(all_datasets)}")
            print(f"  matching keywords: {len(matches)}")
            for m in sorted(matches):
                print(f"    - {m}")
        print()

        # 2. Probe candidate names (with stock_id)
        print("=" * 80)
        print("Phase 2: probe candidate datasets WITH data_id=2330")
        print("=" * 80)
        for d in CANDIDATE_DATASETS:
            await probe(session, token, d, with_data_id=True)
            await asyncio.sleep(0.5)
        print()

        # 3. Probe candidate names (without stock_id, for market-level testing)
        print("=" * 80)
        print("Phase 3: probe candidate datasets WITHOUT data_id (for market-level)")
        print("=" * 80)
        for d in ["TaiwanStockGovernmentBankBuySell", "TaiwanStockTotalGovernmentBankBuySell"]:
            await probe(session, token, d, with_data_id=False)
            await asyncio.sleep(0.5)
        print()

    return 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
