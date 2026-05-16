"""
scripts/probe_finmind_sponsor_unused.py
========================================
v3.13(2026-05-16):從 FinMind catalog 探未被 collector.toml 使用的 dataset。

設計變更 vs v3.12:
- `/datalist` endpoint **不是** dataset catalog(回某 dataset 的 data_id 清單,
  e.g. 股票代碼,doc 明文)。改走 **422 enum parser**:故意送 invalid dataset
  名給 `/data`,FinMind 回 422 error message 內夾完整 allowed enum,regex 抽出
  (對齊 CLAUDE.md v1.20 第 4 條:「改從 422 error message 撈 91 個 backer-tier
  dataset enum」)。
- 全部 emoji 改 ASCII 標籤([WARN] [OK] [LOCK] [FAIL] [CRASH]),Windows
  cp950 console 不再 UnicodeEncodeError;額外在開頭 reconfigure stdout/stderr
  為 utf-8 雙保險。

依賴:`FINMIND_TOKEN` 環境變數 + aiohttp + tomllib(Py 3.11+)。

用法:
    python scripts/probe_finmind_sponsor_unused.py
    python scripts/probe_finmind_sponsor_unused.py --filter taiwan   # 只 probe 含 'taiwan' 的
    python scripts/probe_finmind_sponsor_unused.py --max 5           # 上限(避免 burst 觸發 ban)
"""
from __future__ import annotations

import argparse
import asyncio
import json
import os
import re
import sys
import tomllib
from pathlib import Path

import aiohttp


# Windows cp950 console UTF-8 修法(對齊 scripts/run_av3.ps1)
if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    except AttributeError:
        pass  # Python < 3.7 fallback


FINMIND_BASE_URL = "https://api.finmindtrade.com/api/v4/data"
COLLECTOR_TOML = Path(__file__).parent.parent / "config" / "collector.toml"
RATE_LIMIT_SEC = 2.25  # 對齊 rate_limiter 1600 reqs/h 上限
INVALID_PROBE_NAME = "__INVALID_PROBE_FOR_ENUM__"


async def discover_catalog(session: aiohttp.ClientSession, token: str) -> list[str]:
    """
    送 invalid dataset 名給 /data,接 422 → 解 error message 內的 allowed enum。

    FinMind 422 body 範例(觀察自 CLAUDE.md v1.20 + v1.21-I):
        {"msg": "dataset is invalid, please use one of the following: ['TaiwanStockInfo',
                 'TaiwanStockPrice', 'TaiwanStockPriceTick', ...]", "status": 422}
    """
    params = {
        "dataset":    INVALID_PROBE_NAME,
        "data_id":    "2330",
        "start_date": "2025-01-01",
        "end_date":   "2025-01-31",
        "token":      token,
    }
    async with session.get(FINMIND_BASE_URL, params=params, timeout=30) as resp:
        text = await resp.text()
        if resp.status != 422:
            print(f"[CRASH] expected 422 invalid dataset enum, got {resp.status}", file=sys.stderr)
            print(f"        body[:500]: {text[:500]}", file=sys.stderr)
            return []
        # 解 enum:'...', 'TaiwanStockX', 'TaiwanStockY', ...
        # 兩種 quote 都接(單引號 / 雙引號)
        names = re.findall(r"['\"]([A-Za-z][A-Za-z0-9_]+)['\"]", text)
        # 過濾掉明顯不是 dataset 名的(short keys like "msg", "status")
        return sorted({n for n in names if len(n) >= 6 and n[0].isupper()})


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
            return f"  [OK]    {label:55} rows={d['rows']:4d} fields={d['fields']}"
        return f"  [WARN]  {label:55} 200 but 0 rows"
    elif d["status"] == 400 and "level" in d.get("body", "").lower():
        return f"  [LOCK]  {label:55} 400 tier insufficient: {d['body'][:80]}"
    elif d["status"] == 403 and "ip" in d.get("body", "").lower():
        return f"  [BAN]   {label:55} 403 IP banned: {d['body'][:80]}"
    else:
        return f"  [FAIL]  {label:55} {d['status']}: {d.get('body') or d.get('error', '')[:120]}"


async def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--filter", default="", help="只 probe 名稱(lower-case)含此 substring 的 dataset")
    parser.add_argument("--max", type=int, default=0, help="上限 probe 數量(0 = 全部;建議 --max 5 避免 burst 觸發 ban)")
    args = parser.parse_args()

    token = os.environ.get("FINMIND_TOKEN")
    if not token:
        print("[CRASH] FINMIND_TOKEN env var not set", file=sys.stderr)
        return 1

    async with aiohttp.ClientSession() as session:
        # Phase 1:走 422 enum parser 拿真 catalog,diff vs collector.toml
        print("=" * 80)
        print("Phase 1: discover catalog via 422 enum parser + diff vs collector.toml")
        print("=" * 80)
        catalog = await discover_catalog(session, token)
        used = used_datasets_from_collector()
        if not catalog:
            print("[CRASH] discover_catalog 拿不到 enum,abort", file=sys.stderr)
            return 2
        unused = [d for d in catalog if d not in used]
        not_in_catalog = sorted(d for d in used if d not in catalog)
        print(f"  catalog total:      {len(catalog)}")
        print(f"  collector.toml 已用: {len(used)}")
        print(f"  unused(可探):      {len(unused)}")
        if not_in_catalog:
            print(f"  [WARN] collector.toml 內但 catalog 沒列(tier 不足看不到 OR 大小寫不對):")
            for d in not_in_catalog:
                print(f"         - {d}")
        if args.filter:
            unused = [d for d in unused if args.filter.lower() in d.lower()]
            print(f"  filter='{args.filter}':{len(unused)} datasets match")
        if args.max > 0:
            unused = unused[:args.max]
            print(f"  --max={args.max} -> probe 前 {len(unused)} 筆")
        print()

        # Phase 2:probe each unused
        # 等 RATE_LIMIT_SEC 後再開始(discover_catalog 也算 1 req)
        await asyncio.sleep(RATE_LIMIT_SEC)
        print("=" * 80)
        print(f"Phase 2: probe {len(unused)} unused datasets(rate-limited {RATE_LIMIT_SEC}s/req)")
        print(f"預估時間:{len(unused) * RATE_LIMIT_SEC / 60:.1f} 分鐘")
        print("=" * 80)
        accessible: list[tuple[str, dict]] = []
        for i, dataset in enumerate(unused, 1):
            # 先帶 data_id;若 400/422 再試不帶
            d = await probe(session, token, dataset, with_data_id=True)
            print(f"[{i:3d}/{len(unused)}] {_fmt(d, dataset)}")
            # IP banned 直接中止,避免雪上加霜
            if d["status"] == 403 and "ip" in d.get("body", "").lower():
                print("[CRASH] IP banned, abort probe loop", file=sys.stderr)
                break
            # fallback no-data_id 條件:400/422(param 拒絕)or 200 OK 但 0 rows
            # (可能 dataset 是 market-level,帶 data_id 被 silently filter 成空)
            needs_fallback = (
                d["status"] in (400, 422)
                or (d["status"] == 200 and d["rows"] == 0)
            )
            if needs_fallback:
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
            print(f"  - {dataset}(rows={info['rows']},fields: {', '.join(info['fields'])})")
            if info.get("sample"):
                print(f"    sample: {info['sample']}")

    return 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
