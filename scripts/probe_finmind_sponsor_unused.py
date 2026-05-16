"""
scripts/probe_finmind_sponsor_unused.py
========================================
v3.12(2026-05-16):user 升 FinMind sponsor tier 後,列出 catalog 內**未被
collector.toml 使用**的 dataset,並對每個跑 1 個 segment 看 status + 回欄位,
評估哪些值得加進 v3 pipeline(可能延伸 PR #21-C / #M3-x)。

依賴:`FINMIND_TOKEN` 環境變數 + aiohttp + tomllib(Py 3.11+)。

用法:
    python scripts/probe_finmind_sponsor_unused.py
    python scripts/probe_finmind_sponsor_unused.py --filter taiwan   # 只 probe 含 'taiwan' 的
    python scripts/probe_finmind_sponsor_unused.py --max 50          # 上限 probe 數量
"""
from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
import tomllib
from pathlib import Path

import aiohttp


FINMIND_BASE_URL = "https://api.finmindtrade.com/api/v4/data"
FINMIND_DATALIST_URL = "https://api.finmindtrade.com/api/v4/datalist"
COLLECTOR_TOML = Path(__file__).parent.parent / "config" / "collector.toml"
RATE_LIMIT_SEC = 2.25  # 對齊 rate_limiter 1600 reqs/h


async def list_all_datasets(session: aiohttp.ClientSession, token: str) -> list[str]:
    """從 FinMind /datalist 拉全 catalog(sponsor tier 可看到的全部)。"""
    params = {"token": token}
    async with session.get(FINMIND_DATALIST_URL, params=params, timeout=30) as resp:
        if resp.status != 200:
            text = await resp.text()
            print(f"[/datalist] status={resp.status} body={text[:300]}", file=sys.stderr)
            return []
        data = await resp.json()
        items = data.get("data", []) if isinstance(data, dict) else []
        return sorted({
            str(item) if isinstance(item, str) else item.get("table_id", str(item))
            for item in items
        })


def used_datasets_from_collector() -> set[str]:
    """從 collector.toml 抽出所有已用 dataset 名(含 enabled=false)。"""
    raw = tomllib.loads(COLLECTOR_TOML.read_text(encoding="utf-8"))
    return {entry["dataset"] for entry in raw.get("api", []) if "dataset" in entry}


async def probe(
    session: aiohttp.ClientSession,
    token: str,
    dataset: str,
    *,
    with_data_id: bool = True,
) -> dict:
    """跑 1 個 segment;回 status + row count + fields + sample。"""
    params = {
        "dataset":    dataset,
        "start_date": "2025-01-01",
        "end_date":   "2025-01-31",
        "token":      token,
    }
    if with_data_id:
        params["data_id"] = "2330"
    try:
        async with session.get(FINMIND_BASE_URL, params=params, timeout=30) as resp:
            text = await resp.text()
            if resp.status == 200:
                body = json.loads(text)
                rows = body.get("data", [])
                return {
                    "status": 200,
                    "rows":   len(rows),
                    "fields": sorted(rows[0].keys()) if rows else [],
                    "sample": rows[0] if rows else None,
                }
            else:
                return {"status": resp.status, "body": text[:200]}
    except Exception as e:
        return {"status": -1, "error": str(e)}


def _fmt(d: dict, label: str) -> str:
    if d["status"] == 200:
        if d["rows"] > 0:
            return f"  ✅ {label:50} → 200, {d['rows']:4d} rows, fields={d['fields']}"
        return f"  ⚠️  {label:50} → 200 OK but 0 rows"
    elif d["status"] == 400 and "level" in d.get("body", "").lower():
        return f"  🔒 {label:50} → 400 (tier insufficient): {d['body'][:80]}"
    else:
        return f"  ❌ {label:50} → {d['status']}: {d.get('body') or d.get('error', '')[:120]}"


async def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--filter", default="", help="只 probe 名稱(lower-case)含此 substring 的 dataset")
    parser.add_argument("--max", type=int, default=0, help="上限 probe 數量(0 = 全部)")
    args = parser.parse_args()

    token = os.environ.get("FINMIND_TOKEN")
    if not token:
        print("FINMIND_TOKEN env var not set", file=sys.stderr)
        return 1

    async with aiohttp.ClientSession() as session:
        # Phase 1:catalog + diff
        print("=" * 80)
        print("Phase 1: catalog + diff vs collector.toml")
        print("=" * 80)
        catalog = await list_all_datasets(session, token)
        used = used_datasets_from_collector()
        if not catalog:
            return 2
        unused = [d for d in catalog if d not in used]
        already_used = sorted(d for d in catalog if d in used)
        not_in_catalog = sorted(d for d in used if d not in catalog)
        print(f"  catalog total:      {len(catalog)}")
        print(f"  collector.toml 已用: {len(used)}")
        print(f"  unused(可探):      {len(unused)}")
        if not_in_catalog:
            print(f"  collector.toml 內但 catalog 沒列(可能 tier 不足看不到):")
            for d in not_in_catalog:
                print(f"    ⚠️  {d}")
        if args.filter:
            unused = [d for d in unused if args.filter.lower() in d.lower()]
            print(f"  filter='{args.filter}':{len(unused)} datasets match")
        if args.max > 0:
            unused = unused[:args.max]
            print(f"  --max={args.max} → probe 前 {len(unused)} 筆")
        print()

        # Phase 2:probe each unused
        print("=" * 80)
        print(f"Phase 2: probe {len(unused)} unused datasets(rate-limited {RATE_LIMIT_SEC}s/req)")
        print(f"預估時間:{len(unused) * RATE_LIMIT_SEC / 60:.1f} 分鐘")
        print("=" * 80)
        accessible: list[tuple[str, dict]] = []
        for i, dataset in enumerate(unused, 1):
            # 先帶 data_id;若 400/422 再試不帶
            d = await probe(session, token, dataset, with_data_id=True)
            print(f"[{i:3d}/{len(unused)}] {_fmt(d, dataset)}")
            if d["status"] in (400, 422):
                await asyncio.sleep(RATE_LIMIT_SEC)
                d2 = await probe(session, token, dataset, with_data_id=False)
                print(f"           {_fmt(d2, dataset + ' (no data_id)')}")
                if d2["status"] == 200 and d2["rows"] > 0:
                    accessible.append((dataset, d2))
            elif d["status"] == 200 and d["rows"] > 0:
                accessible.append((dataset, d))
            await asyncio.sleep(RATE_LIMIT_SEC)

        # Phase 3:summary
        print()
        print("=" * 80)
        print(f"Summary: {len(accessible)} unused datasets 回 200 + 有資料(候選加入 v3 pipeline)")
        print("=" * 80)
        for dataset, info in accessible:
            print(f"  - {dataset}({info['rows']} rows,fields: {', '.join(info['fields'])})")
            if info.get("sample"):
                print(f"    sample: {info['sample']}")

    return 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
